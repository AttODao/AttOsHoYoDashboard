//! Best-effort cleanup of the retired browser push service worker.
#[cfg(any(target_arch = "wasm32", test))]
fn is_retired_push_worker(script_url: &str) -> bool {
    // Strip the query and fragment before inspecting the URL path.
    let url = script_url.split(['?', '#']).next().unwrap_or_default();
    let path = url.split_once("://").map_or(url, |(_, rest)| {
        rest.find('/').map_or("", |start| &rest[start..])
    });
    path.ends_with("/assets/push-sw.js")
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn retire() {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    fn get(object: &JsValue, field: &str) -> Option<JsValue> {
        js_sys::Reflect::get(object, &field.into())
            .ok()
            .filter(|value| !value.is_null() && !value.is_undefined())
    }

    async fn call(object: &JsValue, method: &str) -> Option<JsValue> {
        let function = get(object, method)?.dyn_into::<js_sys::Function>().ok()?;
        let promise = function
            .call0(object)
            .ok()?
            .dyn_into::<js_sys::Promise>()
            .ok()?;
        JsFuture::from(promise).await.ok()
    }

    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(container) = get(window.navigator().as_ref(), "serviceWorker") else {
            return;
        };
        let Some(registrations) = call(&container, "getRegistrations").await else {
            return;
        };
        for registration in js_sys::Array::from(&registrations).iter() {
            let worker = get(&registration, "active")
                .or_else(|| get(&registration, "waiting"))
                .or_else(|| get(&registration, "installing"));
            let script_url = worker
                .and_then(|worker| get(&worker, "scriptURL"))
                .and_then(|url| url.as_string());
            if script_url.as_deref().is_some_and(is_retired_push_worker) {
                let _ = call(&registration, "unregister").await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::is_retired_push_worker;

    #[test]
    fn matches_only_the_retired_worker_path() {
        assert!(is_retired_push_worker(
            "https://h.example/assets/push-sw.js"
        ));
        assert!(is_retired_push_worker(
            "https://h.example/assets/push-sw.js?v=1"
        ));
        assert!(is_retired_push_worker(
            "https://h.example/assets/push-sw.js#old"
        ));
        assert!(!is_retired_push_worker(
            "https://h.example/assets/other-sw.js"
        ));
        assert!(!is_retired_push_worker(""));
        assert!(!is_retired_push_worker(
            "https://h.example/?next=/assets/push-sw.js"
        ));
    }
}
