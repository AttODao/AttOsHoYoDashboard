//! Native API client for the app: the selected server (the build-time default or the user's
//! choice, `mobile::server`), its session cookie sealed with an Android Keystore key (Kotlin
//! `AhdSessionStore` in `android/MainActivity.kt`), and the Origin header the server requires
//! for unsafe methods.
//!
//! The browser holds the session in an HttpOnly cookie; the WebView page cannot reach the
//! server's cookies, so requests go through Rust and this jar instead.
use super::server::{self, SERVER_FILE, ServerError};
use super::session::Jar;
use super::storage;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Build-time default server, e.g. `AHD_SERVER_URL=https://ahd.example.com`. The fallback
/// suits `adb reverse tcp:8080 tcp:8080` against a local development server.
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

/// The selected server; loaded from `SERVER_FILE` on first use. Changed only by
/// `switch_server`, which holds the jar's lock while it does (lock order: jar, then origin;
/// never lock the jar while holding this).
static ORIGIN: Mutex<Option<Option<String>>> = Mutex::new(None);

/// The selected server's origin, or None when neither the saved choice nor
/// `AHD_SERVER_URL` is usable.
pub(crate) fn server_origin() -> Option<String> {
    let mut slot = ORIGIN.lock().ok()?;
    slot.get_or_insert_with(|| {
        server::configured(storage::read(SERVER_FILE).as_deref(), SERVER_URL)
    })
    .clone()
}

/// The build-time default server's origin (offered on the login page).
pub(crate) fn default_origin() -> Option<String> {
    super::session::parse_origin(SERVER_URL)
}

/// Checks that `origin` answers `/health` like an AHD server. Sends no cookie and no Origin.
pub(crate) async fn probe(origin: &str) -> Result<(), ServerError> {
    let request = client()
        .ok_or(ServerError::Unreachable)?
        .get(format!("{origin}/health"))
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(15));
    let response = request.send().await.map_err(|_| ServerError::Unreachable)?;
    let status = response.status().as_u16();
    let body = read_limited(response, 16 * 1024)
        .await
        .map_err(|_| ServerError::Unreachable)?;
    server::health_answer(status, &body)
}

/// Whether a session cookie for the current server is held.
pub(crate) fn has_session() -> bool {
    with_jar(|jar| jar.cookies.header().is_some()).unwrap_or(false)
}

/// Selects `origin`: deletes the current server's session cookie first (memory and the
/// Keystore-sealed file), then stores the choice. A cookie that cannot be deleted stops the
/// change, so it can never be sent to the new server. Responses still in flight from the old
/// server are not applied to the jar afterwards (see `send`).
pub(crate) fn switch_server(origin: &str) -> Result<(), ServerError> {
    let mut jar = JAR.lock().map_err(|_| ServerError::NotSaved)?;
    if !store::save(None) {
        super::log::warn("could not delete the session cookie for a server change");
        return Err(ServerError::NotSaved);
    }
    *jar = Some(Loaded {
        cookies: Jar::default(),
        unsaved: false,
    });
    let saved = match server::saved_value(origin, SERVER_URL) {
        Some(value) => storage::write(SERVER_FILE, &value).is_ok(),
        None => storage::remove(SERVER_FILE),
    };
    if !saved {
        return Err(ServerError::NotSaved);
    }
    let mut slot = ORIGIN.lock().map_err(|_| ServerError::NotSaved)?;
    *slot = Some(Some(origin.to_owned()));
    Ok(())
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
    send_to(method, path, body, None).await
}

/// `send` with an overall time limit, for best-effort calls such as the old server's logout
/// during a server change.
pub(crate) async fn send_within(
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
    limit: Duration,
) -> Result<Response, ()> {
    send_to(method, path, body, Some(limit)).await
}

async fn send_to(
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
    limit: Option<Duration>,
) -> Result<Response, ()> {
    // The origin and its cookie are read together under the jar's lock (lock order: jar, then
    // origin, as in `switch_server`), so a request never pairs one server with another's cookie.
    let (origin, cookie) = with_jar(|jar| (server_origin(), jar.cookies.header()))?;
    let origin = origin.ok_or(())?;
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| ())?;
    let mut request = client()
        .ok_or(())?
        .request(method, format!("{origin}{path}"))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Origin", &origin);
    if let Some(limit) = limit {
        request = request.timeout(limit);
    }
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    if let Some(body) = body {
        request = request.body(serde_json::to_vec(body).map_err(|_| ())?);
    }
    let response = request.send().await.map_err(|_| ())?;
    let set_cookies: Vec<String> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_owned))
        .collect();
    if !set_cookies.is_empty() {
        with_jar(|jar| {
            // The jar belongs to the selected server: never keep another server's cookie.
            if server_origin().as_deref() == Some(origin.as_str()) {
                for line in &set_cookies {
                    jar.unsaved |= jar.cookies.apply(line);
                }
            }
        })?;
    }
    let status = response.status().as_u16();
    let body = read_limited(response, BODY_LIMIT).await?;
    Ok(Response { status, body })
}

async fn read_limited(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, ()> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        body.extend_from_slice(&chunk[..chunk.len().min(limit - body.len())]);
        if body.len() >= limit {
            break;
        }
    }
    Ok(body)
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
static JAR: Mutex<Option<Loaded>> = Mutex::new(None);

fn with_jar<T>(operation: impl FnOnce(&mut Loaded) -> T) -> Result<T, ()> {
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
