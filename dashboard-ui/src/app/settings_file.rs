//! Browser file handling for the settings transfer page: saving an export as a download
//! and reading the chosen file. Only the web version shows that page; the app leaves it to
//! the browser because its WebView has no file chooser or download handler.
use serde_json::Value;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
/// The server's request body limit; larger files are refused before they are sent.
pub(super) const MAX_BYTES: u64 = 16 * 1024;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ReadError {
    TooLarge,
    Unreadable,
    NotJson,
}

impl ReadError {
    pub(super) fn message_key(self) -> &'static str {
        match self {
            Self::TooLarge => "transfer.file_too_large",
            Self::Unreadable => "transfer.file_unreadable",
            Self::NotJson => "transfer.file_not_json",
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
/// The chosen file's text as JSON; any JSON value is sent so the server can name the problem.
pub(super) fn parse(size: u64, text: Option<&str>) -> Result<Value, ReadError> {
    if size > MAX_BYTES {
        return Err(ReadError::TooLarge);
    }
    let text = text.ok_or(ReadError::Unreadable)?;
    serde_json::from_str(text).map_err(|_| ReadError::NotJson)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
/// `ahd-settings-YYYY-MM-DD.json` from the export's `exported_at` (its server-side date).
pub(super) fn file_name(file: &Value) -> String {
    let date = file
        .get("exported_at")
        .and_then(Value::as_str)
        .and_then(|at| at.get(..10))
        .filter(|date| {
            date.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
        });
    match date {
        Some(date) => format!("ahd-settings-{date}.json"),
        None => "ahd-settings.json".to_owned(),
    }
}

/// Offers `file` as a JSON download; false when the browser refused.
#[cfg(target_arch = "wasm32")]
pub(super) fn download(file: &Value) -> bool {
    use wasm_bindgen::JsCast;
    let Ok(text) = serde_json::to_string_pretty(file) else {
        return false;
    };
    let save = || -> Option<()> {
        let document = web_sys::window()?.document()?;
        let parts = js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(&(text.clone() + "\n")));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("application/json");
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options).ok()?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).ok()?;
        let link: web_sys::HtmlAnchorElement =
            document.create_element("a").ok()?.dyn_into().ok()?;
        link.set_href(&url);
        link.set_download(&file_name(file));
        link.click();
        let _ = web_sys::Url::revoke_object_url(&url);
        Some(())
    };
    save().is_some()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn download(file: &Value) -> bool {
    let _ = file;
    false
}

/// The file chosen in the `<input type=file>` with `input_id`; None when none is chosen.
#[cfg(target_arch = "wasm32")]
pub(super) async fn read_chosen(input_id: &str) -> Option<Result<Value, ReadError>> {
    use wasm_bindgen::JsCast;
    let input: web_sys::HtmlInputElement = web_sys::window()?
        .document()?
        .get_element_by_id(input_id)?
        .dyn_into()
        .ok()?;
    let file = input.files()?.get(0)?;
    let size = file.size() as u64;
    if size > MAX_BYTES {
        return Some(parse(size, None));
    }
    let text = wasm_bindgen_futures::JsFuture::from(file.text())
        .await
        .ok()
        .and_then(|text| text.as_string());
    Some(parse(size, text.as_deref()))
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn read_chosen(input_id: &str) -> Option<Result<Value, ReadError>> {
    let _ = input_id;
    None
}

/// Clears the file input so choosing the same file again fires `change`.
#[cfg(target_arch = "wasm32")]
pub(super) fn clear_chosen(input_id: &str) {
    use wasm_bindgen::JsCast;
    if let Some(input) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(input_id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
    {
        input.set_value("");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn clear_chosen(input_id: &str) {
    let _ = input_id;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chosen_files_must_be_small_readable_json() {
        assert_eq!(parse(10, Some("{\"format\": 1}")), Ok(json!({"format": 1})));
        assert_eq!(parse(10, Some("[]")), Ok(json!([])));
        assert_eq!(parse(MAX_BYTES + 1, Some("{}")), Err(ReadError::TooLarge));
        assert_eq!(parse(MAX_BYTES, Some("{}")), Ok(json!({})));
        assert_eq!(parse(10, None), Err(ReadError::Unreadable));
        assert_eq!(parse(10, Some("not json")), Err(ReadError::NotJson));
        assert_eq!(parse(10, Some("")), Err(ReadError::NotJson));
    }

    #[test]
    fn downloads_are_named_after_the_export_date() {
        assert_eq!(
            file_name(&json!({"exported_at": "2026-11-09T01:02:03Z"})),
            "ahd-settings-2026-11-09.json"
        );
        for file in [
            json!({}),
            json!({"exported_at": 1}),
            json!({"exported_at": "2026"}),
            json!({"exported_at": "../../etc/passwd"}),
            json!({"exported_at": "2026/11/09T00"}),
        ] {
            assert_eq!(file_name(&file), "ahd-settings.json", "{file}");
        }
    }
}
