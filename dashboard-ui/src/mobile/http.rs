//! Native API client for the app: one configured server, its session cookie kept in
//! app-private storage, and the Origin header the server requires for unsafe methods.
//!
//! The browser holds the session in an HttpOnly cookie; the WebView page cannot reach the
//! server's cookies, so requests go through Rust and this jar instead.
use super::session::{Jar, parse_origin};
use super::storage;
use std::sync::{Mutex, OnceLock};

/// Build-time server, e.g. `AHD_SERVER_URL=https://ahd.example.com`. The default suits
/// `adb reverse tcp:8080 tcp:8080` against a local development server.
const SERVER_URL: &str = match option_env!("AHD_SERVER_URL") {
    Some(url) => url,
    None => "http://localhost:8080",
};
const COOKIE_FILE: &str = "session-cookies.json";
/// Larger bodies are cut here; `api` rejects anything over its own limits.
const BODY_LIMIT: usize = 1024 * 1024 + 1;
const USER_AGENT: &str = concat!("AHD-Android/", env!("CARGO_PKG_VERSION"), " (Android)");

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
    if let Some(cookie) = jar().lock().map_err(|_| ())?.header() {
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
        let mut jar = jar().lock().map_err(|_| ())?;
        let mut changed = false;
        for line in &set_cookies {
            changed |= jar.apply(line);
        }
        if changed {
            save(&jar);
        }
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

fn jar() -> &'static Mutex<Jar> {
    static JAR: OnceLock<Mutex<Jar>> = OnceLock::new();
    JAR.get_or_init(|| Mutex::new(Jar::from_json(storage::read(COOKIE_FILE).as_deref())))
}

fn save(jar: &Jar) {
    match jar.to_json() {
        Some(text) => {
            let _ = storage::write(COOKIE_FILE, &text);
        }
        None => storage::remove(COOKIE_FILE),
    }
}
