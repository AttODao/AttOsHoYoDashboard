use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fmt;

#[cfg(any(target_arch = "wasm32", test))]
const API_PREFIX: &str = "/api/v1";
const OPENAPI_PATH: &str = "/openapi.json";
#[cfg(target_arch = "wasm32")]
const JSON_LIMIT: usize = 1024 * 1024;
#[cfg(target_arch = "wasm32")]
const ERROR_LIMIT: usize = 16 * 1024;

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Clone, PartialEq, Debug)]
pub struct ApiError {
    status: Option<u16>,
    code: String,
    message: &'static str,
}

impl ApiError {
    pub fn is_unauthorized(&self) -> bool {
        self.status == Some(401) || self.code == "unauthorized"
    }

    pub fn message_display(&self) -> &'static str {
        self.message
    }

    pub(crate) fn browser(message: &'static str) -> Self {
        Self {
            status: None,
            code: "browser_error".to_owned(),
            message,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn browser_only() -> Self {
        Self::browser("This action is available only in a browser.")
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn internal(code: &'static str, message: &'static str) -> Self {
        Self {
            status: None,
            code: code.to_owned(),
            message,
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn http(status: u16, code: String) -> Self {
        // Never retain an arbitrary server string in Debug, even if Display is static.
        let code = if matches!(
            code.as_str(),
            "unauthorized"
                | "forbidden"
                | "password_change_required"
                | "initial_setup_required"
                | "not_found"
                | "conflict"
                | "rate_limited"
                | "busy"
                | "unavailable"
                | "invalid_input"
                | "upstream_error"
        ) {
            code
        } else {
            "http_error".to_owned()
        };
        let message = safe_message(status, &code);
        Self {
            status: Some(status),
            code,
            message,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message_display())
    }
}

pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T> {
    request_json("GET", path, None).await
}

pub async fn send(method: &str, path: &str, body: Option<Value>) -> Result<()> {
    request_unit(method, path, body).await
}

#[cfg(target_arch = "wasm32")]
pub async fn json<T: DeserializeOwned>(method: &str, path: &str, body: Option<Value>) -> Result<T> {
    request_json(method, path, body).await
}

pub async fn openapi() -> Result<Value> {
    request_json_at("GET", OPENAPI_PATH, None, false).await
}

async fn request_unit(method: &str, path: &str, body: Option<Value>) -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (method, path, body);
        Err(ApiError::browser_only())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let response = fetch(method, &api_path(path)?, body).await?;
        let status = response.status();
        if (200..300).contains(&status) {
            return Ok(());
        }
        Err(error_from_response(response).await)
    }
}

async fn request_json<T: DeserializeOwned>(
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (method, path, body);
        Err(ApiError::browser_only())
    }
    #[cfg(target_arch = "wasm32")]
    {
        request_json_at(method, &api_path(path)?, body, true).await
    }
}

async fn request_json_at<T: DeserializeOwned>(
    method: &str,
    path: &str,
    body: Option<Value>,
    already_checked_path: bool,
) -> Result<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (method, path, body, already_checked_path);
        Err(ApiError::browser_only())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let url = if already_checked_path {
            path.to_owned()
        } else {
            public_path(path)?
        };
        let response = fetch(method, &url, body).await?;
        let status = response.status();
        if !(200..300).contains(&status) {
            return Err(error_from_response(response).await);
        }
        let text = response.text().await.map_err(|_| {
            ApiError::internal("network_error", "The server response could not be read.")
        })?;
        if text.is_empty() {
            return Err(ApiError::internal(
                "decode_error",
                "The server returned no JSON data.",
            ));
        }
        if text.len() > JSON_LIMIT {
            return Err(ApiError::internal(
                "response_too_large",
                "The server response was too large.",
            ));
        }
        serde_json::from_str(&text)
            .map_err(|_| ApiError::internal("decode_error", "The server returned unreadable JSON."))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn api_path(path: &str) -> Result<String> {
    checked_relative_path(path).map(|path| format!("{API_PREFIX}{path}"))
}

#[cfg(any(target_arch = "wasm32", test))]
fn public_path(path: &str) -> Result<String> {
    if path == OPENAPI_PATH {
        Ok(path.to_owned())
    } else {
        Err(ApiError::internal(
            "invalid_path",
            "The request path is not allowed.",
        ))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn checked_relative_path(path: &str) -> Result<&str> {
    if path.starts_with('/') && !path.starts_with("//") && !path.contains("://") {
        Ok(path)
    } else {
        Err(ApiError::internal(
            "invalid_path",
            "The request path is not allowed.",
        ))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn safe_message(status: u16, code: &str) -> &'static str {
    match code {
        "unauthorized" => "Please sign in again.",
        "forbidden" => "You do not have access to this action.",
        "password_change_required" => "Please change your password before continuing.",
        "initial_setup_required" => "Initial setup is required before continuing.",
        "not_found" => "The requested item was not found.",
        "conflict" => "The request conflicts with current server state.",
        "rate_limited" => "Too many attempts. Please try again later.",
        "busy" => "The server is busy. Please try again shortly.",
        "unavailable" => "This feature is not available right now.",
        "invalid_input" => "Please check the form and try again.",
        "upstream_error" => "The upstream service could not be reached safely.",
        _ if status >= 500 => "The server could not complete the request.",
        _ => "The request could not be completed.",
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch(method: &str, url: &str, body: Option<Value>) -> Result<gloo_net::http::Response> {
    use gloo_net::http::Request;
    use web_sys::RequestCredentials;

    let builder = match method {
        "GET" => Request::get(url),
        "POST" => Request::post(url),
        "PUT" => Request::put(url),
        "PATCH" => Request::patch(url),
        "DELETE" => Request::delete(url),
        _ => {
            return Err(ApiError::internal(
                "invalid_method",
                "The request method is not allowed.",
            ));
        }
    }
    .credentials(RequestCredentials::SameOrigin)
    .header("Accept", "application/json")
    .header("Content-Type", "application/json");

    match body {
        Some(body) => builder
            .json(&body)
            .map_err(|_| ApiError::internal("encode_error", "The request could not be encoded."))?
            .send()
            .await
            .map_err(|_| ApiError::internal("network_error", "The server could not be reached.")),
        None => builder
            .send()
            .await
            .map_err(|_| ApiError::internal("network_error", "The server could not be reached.")),
    }
}

#[cfg(target_arch = "wasm32")]
async fn error_from_response(response: gloo_net::http::Response) -> ApiError {
    let status = response.status();
    let text = match response.text().await {
        Ok(text) if text.len() <= ERROR_LIMIT => text,
        Ok(_) => String::new(),
        Err(_) => String::new(),
    };
    let code = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .or_else(|| value.get("code"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            match status {
                400 => "invalid_input",
                401 => "unauthorized",
                403 => "forbidden",
                404 => "not_found",
                409 => "conflict",
                429 => "rate_limited",
                503 => "unavailable",
                _ if status >= 500 => "server_error",
                _ => "request_failed",
            }
            .to_owned()
        });
    ApiError::http(status, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_paths_are_same_origin_and_prefixed() {
        assert_eq!(api_path("/me").unwrap(), "/api/v1/me");
        assert!(api_path("https://example.test/me").is_err());
        assert!(api_path("//example.test/me").is_err());
        assert!(api_path("me").is_err());
        assert_eq!(public_path("/openapi.json").unwrap(), "/openapi.json");
        assert!(public_path("/api/v1/me").is_err());
    }

    #[test]
    fn api_error_display_is_static_and_code_based() {
        let error = ApiError::http(401, "unauthorized".to_owned());
        assert!(error.is_unauthorized());
        assert_eq!(error.status, Some(401));
        assert_eq!(error.code, "unauthorized");
        assert_eq!(error.to_string(), "Please sign in again.");

        let error = ApiError::http(400, "invalid_input".to_owned());
        assert_eq!(error.to_string(), "Please check the form and try again.");
        let error = ApiError::http(500, "synthetic-secret-echo".to_owned());
        assert!(!format!("{error:?}").contains("synthetic-secret-echo"));
    }
}
