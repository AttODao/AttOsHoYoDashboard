//! Native API client for the app: one configured server, its session cookie sealed with an
//! Android Keystore key (Kotlin `AhdSessionStore` in `android/MainActivity.kt`), and the
//! Origin header the server requires for unsafe methods.
//!
//! The browser holds the session in an HttpOnly cookie; the WebView page cannot reach the
//! server's cookies, so requests go through Rust and this jar instead.
use super::session::{Jar, parse_origin};
use std::sync::{Mutex, OnceLock};

/// Build-time server, e.g. `AHD_SERVER_URL=https://ahd.example.com`. The default suits
/// `adb reverse tcp:8080 tcp:8080` against a local development server.
const SERVER_URL: &str = match option_env!("AHD_SERVER_URL") {
    Some(url) => url,
    None => "http://localhost:8080",
};
/// Larger bodies are cut here; `api` rejects anything over its own limits.
const BODY_LIMIT: usize = 1024 * 1024 + 1;
pub(crate) const USER_AGENT: &str =
    concat!("AHD-Android/", env!("CARGO_PKG_VERSION"), " (Android)");

pub(crate) struct Response {
    status: u16,
    body: Vec<u8>,
}

impl Response {
    pub(crate) fn status(&self) -> u16 {
        self.status
    }

    pub(crate) async fn text(&self) -> Result<String, ()> {
        String::from_utf8(self.body.clone()).map_err(|_| ())
    }
}

/// The configured server's origin, or None if `AHD_SERVER_URL` is unusable.
pub(crate) fn server_origin() -> Option<&'static str> {
    static ORIGIN: OnceLock<Option<String>> = OnceLock::new();
    ORIGIN.get_or_init(|| parse_origin(SERVER_URL)).as_deref()
}

fn client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                // Cookies and the Origin header must never follow a redirect elsewhere.
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(USER_AGENT)
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .ok()
        })
        .as_ref()
}

/// Sends one request to `path` (an absolute path checked by `api`) on the server.
pub(crate) async fn send(
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<Response, ()> {
    let origin = server_origin().ok_or(())?;
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| ())?;
    let mut request = client()
        .ok_or(())?
        .request(method, format!("{origin}{path}"))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Origin", origin);
    if let Some(cookie) = with_jar(|jar| jar.cookies.header())? {
        request = request.header("Cookie", cookie);
    }
    if let Some(body) = body {
        request = request.body(serde_json::to_vec(body).map_err(|_| ())?);
    }
    let mut response = request.send().await.map_err(|_| ())?;
    let set_cookies: Vec<String> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_owned))
        .collect();
    if !set_cookies.is_empty() {
        with_jar(|jar| {
            for line in &set_cookies {
                jar.unsaved |= jar.cookies.apply(line);
            }
        })?;
    }
    let status = response.status().as_u16();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        body.extend_from_slice(&chunk[..chunk.len().min(BODY_LIMIT - body.len())]);
        if body.len() >= BODY_LIMIT {
            break;
        }
    }
    Ok(Response { status, body })
}

/// The loaded jar, and whether it changed since the store last accepted it.
struct Loaded {
    cookies: Jar,
    unsaved: bool,
}

/// Runs `operation` on the jar, loading it from the session store first, and saves a
/// changed jar afterwards. A store that failed for now fails the request like a network
/// error and is asked again next time; a jar the store refused to save is saved again
/// after the next request, so the background jobs do not keep reading an older session.
fn with_jar<T>(operation: impl FnOnce(&mut Loaded) -> T) -> Result<T, ()> {
    static JAR: Mutex<Option<Loaded>> = Mutex::new(None);
    let mut slot = JAR.lock().map_err(|_| ())?;
    if slot.is_none() {
        *slot = Jar::from_store(store::load().as_deref()).map(|cookies| Loaded {
            cookies,
            unsaved: false,
        });
    }
    let Some(jar) = slot.as_mut() else {
        super::log::warn("the session store is unavailable");
        return Err(());
    };
    let result = operation(jar);
    if jar.unsaved {
        jar.unsaved = !store::save(jar.cookies.to_json().as_deref());
        if jar.unsaved {
            super::log::warn("could not save the session cookie");
        }
    }
    Ok(result)
}

/// The Keystore-sealed jar of the Kotlin `AhdSessionStore`, which also migrates the plain
/// `session-cookies.json` of earlier versions.
#[cfg(target_os = "android")]
mod store {
    const SESSION_STORE: &str = "dev.dioxus.main.AhdSessionStore";

    /// The jar's JSON (`{}` when signed out), or None when the store failed for now.
    pub(super) fn load() -> Option<String> {
        super::super::notify::call_on(
            SESSION_STORE,
            "read",
            "(Landroid/content/Context;)Ljava/lang/String;",
            &[],
            true,
        )
    }

    /// Seals the jar, or deletes it when it is empty.
    pub(super) fn save(json: Option<&str>) -> bool {
        let result = match json {
            Some(json) => super::super::notify::call_on(
                SESSION_STORE,
                "write",
                "(Landroid/content/Context;Ljava/lang/String;)Z",
                &[json],
                true,
            ),
            None => super::super::notify::call_on(
                SESSION_STORE,
                "clear",
                "(Landroid/content/Context;)Z",
                &[],
                true,
            ),
        };
        result.as_deref() == Some("true")
    }
}

/// Desktop runs of the mobile build (development only) have no Keystore: a plain file in
/// `AHD_APP_DATA_DIR`.
#[cfg(not(target_os = "android"))]
mod store {
    use super::super::storage;
    const COOKIE_FILE: &str = "session-cookies.json";

    pub(super) fn load() -> Option<String> {
        Some(storage::read(COOKIE_FILE).unwrap_or_else(|| "{}".to_owned()))
    }

    pub(super) fn save(json: Option<&str>) -> bool {
        match json {
            Some(text) => storage::write(COOKIE_FILE, text).is_ok(),
            None => {
                storage::remove(COOKIE_FILE);
                true
            }
        }
    }
}
