use crate::api;
#[cfg(target_arch = "wasm32")]
use crate::models::PasskeyChallenge;
#[cfg(any(target_arch = "wasm32", test))]
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
#[cfg(target_arch = "wasm32")]
use serde_json::{Value, json};

pub async fn login(username: &str) -> api::Result<()> {
    login_impl(username).await
}

pub async fn register(label: &str) -> api::Result<()> {
    register_impl(label).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn login_impl(username: &str) -> api::Result<()> {
    let _ = username;
    Err(api::ApiError::browser(
        "messages.passkeys_are_available_only_in_a_browser",
    ))
}

#[cfg(not(target_arch = "wasm32"))]
async fn register_impl(label: &str) -> api::Result<()> {
    let _ = label;
    Err(api::ApiError::browser(
        "messages.passkeys_are_available_only_in_a_browser",
    ))
}

#[cfg(target_arch = "wasm32")]
async fn login_impl(username: &str) -> api::Result<()> {
    let challenge: PasskeyChallenge = api::json(
        "POST",
        "/auth/passkey/options",
        Some(json!({ "username": username })),
    )
    .await?;
    let options = credential_request_options(challenge.options)?;
    let credential = get_credential(&options).await?;
    let response = assertion_json(&credential)?;
    api::send(
        "POST",
        "/auth/passkey/verify",
        Some(json!({ "challenge_id": challenge.challenge_id, "response": response })),
    )
    .await
}

#[cfg(target_arch = "wasm32")]
async fn register_impl(label: &str) -> api::Result<()> {
    let challenge: PasskeyChallenge = api::json(
        "POST",
        "/me/passkeys/options",
        Some(json!({ "label": label })),
    )
    .await?;
    let options = credential_creation_options(challenge.options)?;
    let credential = create_credential(&options).await?;
    let response = attestation_json(&credential)?;
    api::send(
        "POST",
        "/me/passkeys/verify",
        Some(json!({ "challenge_id": challenge.challenge_id, "response": response })),
    )
    .await
}

#[cfg(any(target_arch = "wasm32", test))]
fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(any(target_arch = "wasm32", test))]
fn b64_decode(text: &str) -> api::Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(text.as_bytes())
        .map_err(|_| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))
}

#[cfg(target_arch = "wasm32")]
fn credential_creation_options(options: Value) -> api::Result<web_sys::CredentialCreationOptions> {
    let public_key = public_key_options(options, &["excludeCredentials"])?;
    let wrapper = web_sys::CredentialCreationOptions::new();
    js_sys::Reflect::set(
        wrapper.as_ref(),
        &wasm_bindgen::JsValue::from_str("publicKey"),
        &public_key,
    )
    .map_err(|_| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))?;
    Ok(wrapper)
}

#[cfg(target_arch = "wasm32")]
fn credential_request_options(options: Value) -> api::Result<web_sys::CredentialRequestOptions> {
    let public_key = public_key_options(options, &["allowCredentials"])?;
    let wrapper = web_sys::CredentialRequestOptions::new();
    js_sys::Reflect::set(
        wrapper.as_ref(),
        &wasm_bindgen::JsValue::from_str("publicKey"),
        &public_key,
    )
    .map_err(|_| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))?;
    Ok(wrapper)
}

#[cfg(target_arch = "wasm32")]
fn public_key_options(
    options: Value,
    credential_lists: &[&str],
) -> api::Result<wasm_bindgen::JsValue> {
    use wasm_bindgen::JsValue;

    let js = value_to_js(&options)?;
    let public_key = js_sys::Reflect::get(&js, &JsValue::from_str("publicKey"))
        .map_err(|_| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))?;
    if public_key.is_undefined() || public_key.is_null() {
        return Err(api::ApiError::browser(
            "messages.the_passkey_challenge_was_invalid",
        ));
    }
    set_base64_field(&public_key, "challenge")?;

    let user = js_sys::Reflect::get(&public_key, &JsValue::from_str("user"))
        .map_err(|_| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))?;
    if !user.is_undefined() && !user.is_null() {
        set_base64_field(&user, "id")?;
    }

    for list_name in credential_lists {
        let value = js_sys::Reflect::get(&public_key, &JsValue::from_str(list_name))
            .map_err(|_| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))?;
        if value.is_undefined() || value.is_null() {
            continue;
        }
        let entries = js_sys::Array::from(&value);
        for entry in entries.iter() {
            set_base64_field(&entry, "id")?;
        }
    }
    Ok(public_key)
}

#[cfg(target_arch = "wasm32")]
fn set_base64_field(object: &wasm_bindgen::JsValue, field: &str) -> api::Result<()> {
    use wasm_bindgen::JsValue;

    let key = JsValue::from_str(field);
    let text = js_sys::Reflect::get(object, &key)
        .map_err(|_| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))?
        .as_string()
        .ok_or_else(|| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))?;
    let bytes = b64_decode(&text)?;
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    js_sys::Reflect::set(object, &key, &array.buffer())
        .map_err(|_| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn value_to_js(value: &Value) -> api::Result<wasm_bindgen::JsValue> {
    js_sys::JSON::parse(&value.to_string())
        .map_err(|_| api::ApiError::browser("messages.the_passkey_challenge_was_invalid"))
}

#[cfg(target_arch = "wasm32")]
async fn create_credential(
    options: &web_sys::CredentialCreationOptions,
) -> api::Result<web_sys::PublicKeyCredential> {
    let credentials = credentials_container()?;
    let promise = credentials
        .create_with_options(options)
        .map_err(|error| js_error(error, true))?;
    credential_from_js(
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|error| js_error(error, true))?,
    )
}

#[cfg(target_arch = "wasm32")]
async fn get_credential(
    options: &web_sys::CredentialRequestOptions,
) -> api::Result<web_sys::PublicKeyCredential> {
    let credentials = credentials_container()?;
    let promise = credentials
        .get_with_options(options)
        .map_err(|error| js_error(error, false))?;
    credential_from_js(
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|error| js_error(error, false))?,
    )
}

#[cfg(target_arch = "wasm32")]
fn credentials_container() -> api::Result<web_sys::CredentialsContainer> {
    let window = web_sys::window().ok_or_else(|| {
        api::ApiError::browser("messages.passkeys_are_unavailable_in_this_browser")
    })?;
    Ok(window.navigator().credentials())
}

#[cfg(target_arch = "wasm32")]
fn credential_from_js(value: wasm_bindgen::JsValue) -> api::Result<web_sys::PublicKeyCredential> {
    use wasm_bindgen::JsCast;

    if value.is_null() || value.is_undefined() {
        return Err(api::ApiError::browser(
            "messages.passkey_verification_was_cancelled",
        ));
    }
    value
        .dyn_into::<web_sys::PublicKeyCredential>()
        .map_err(|_| {
            api::ApiError::browser("messages.the_browser_returned_an_invalid_passkey_response")
        })
}

#[cfg(target_arch = "wasm32")]
fn attestation_json(credential: &web_sys::PublicKeyCredential) -> api::Result<Value> {
    use wasm_bindgen::JsCast;

    let response = credential
        .response()
        .dyn_into::<web_sys::AuthenticatorAttestationResponse>()
        .map_err(|_| {
            api::ApiError::browser("messages.the_browser_returned_an_invalid_passkey_response")
        })?;
    let mut response_json = json!({
        "clientDataJSON": buffer_b64(response.client_data_json()),
        "attestationObject": buffer_b64(response.attestation_object()),
    });
    let transports = response.get_transports();
    if transports.length() > 0 {
        response_json["transports"] = Value::Array(
            transports
                .iter()
                .filter_map(|value| value.as_string().map(Value::String))
                .collect(),
        );
    }
    Ok(json!({
        "id": credential.id(),
        "rawId": buffer_b64(credential.raw_id()),
        "type": credential.type_(),
        "response": response_json,
        "clientExtensionResults": client_extension_results(credential)?,
    }))
}

#[cfg(target_arch = "wasm32")]
fn assertion_json(credential: &web_sys::PublicKeyCredential) -> api::Result<Value> {
    use wasm_bindgen::JsCast;

    let response = credential
        .response()
        .dyn_into::<web_sys::AuthenticatorAssertionResponse>()
        .map_err(|_| {
            api::ApiError::browser("messages.the_browser_returned_an_invalid_passkey_response")
        })?;
    let user_handle = response
        .user_handle()
        .map(buffer_b64)
        .map(Value::String)
        .unwrap_or(Value::Null);
    Ok(json!({
        "id": credential.id(),
        "rawId": buffer_b64(credential.raw_id()),
        "type": credential.type_(),
        "response": {
            "clientDataJSON": buffer_b64(response.client_data_json()),
            "authenticatorData": buffer_b64(response.authenticator_data()),
            "signature": buffer_b64(response.signature()),
            "userHandle": user_handle,
        },
        "clientExtensionResults": client_extension_results(credential)?,
    }))
}

#[cfg(target_arch = "wasm32")]
fn buffer_b64(buffer: js_sys::ArrayBuffer) -> String {
    let array = js_sys::Uint8Array::new(&buffer);
    b64(&array.to_vec())
}

#[cfg(target_arch = "wasm32")]
fn client_extension_results(credential: &web_sys::PublicKeyCredential) -> api::Result<Value> {
    use wasm_bindgen::JsCast;

    let method = js_sys::Reflect::get(
        credential.as_ref(),
        &wasm_bindgen::JsValue::from_str("getClientExtensionResults"),
    )
    .map_err(|_| {
        api::ApiError::browser("messages.the_browser_returned_an_invalid_passkey_response")
    })?;
    let Some(method) = method.dyn_ref::<js_sys::Function>() else {
        return Ok(json!({}));
    };
    let result = method.call0(credential.as_ref()).map_err(|_| {
        api::ApiError::browser("messages.the_browser_returned_an_invalid_passkey_response")
    })?;
    let text = js_sys::JSON::stringify(&result)
        .ok()
        .and_then(|text| text.as_string())
        .unwrap_or_else(|| "{}".to_owned());
    serde_json::from_str(&text).map_err(|_| {
        api::ApiError::browser("messages.the_browser_returned_an_invalid_passkey_response")
    })
}

#[cfg(target_arch = "wasm32")]
fn js_error(value: wasm_bindgen::JsValue, registering: bool) -> api::ApiError {
    let name = js_sys::Reflect::get(&value, &wasm_bindgen::JsValue::from_str("name"))
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default();
    api::ApiError::browser(passkey_error_key(&name, registering))
}

#[cfg(any(target_arch = "wasm32", test))]
fn passkey_error_key(name: &str, registering: bool) -> &'static str {
    match name {
        "AbortError" | "NotAllowedError" => "messages.passkey_verification_was_cancelled",
        "SecurityError" => "messages.passkeys_are_not_allowed_for_this_page",
        // Registration lists existing credentials; a match means this one is enrolled.
        "InvalidStateError" if registering => "messages.this_passkey_is_already_registered",
        _ => "messages.passkeys_are_unavailable_in_this_browser",
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub async fn subscribe_push(public_key: &str) -> api::Result<()> {
    subscribe_push_impl(public_key).await
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub async fn unsubscribe_push() -> api::Result<()> {
    unsubscribe_push_impl().await
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
async fn subscribe_push_impl(_: &str) -> api::Result<()> {
    Err(api::ApiError::browser(
        "messages.push_available_only_in_a_browser",
    ))
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
async fn unsubscribe_push_impl() -> api::Result<()> {
    Err(api::ApiError::browser(
        "messages.push_available_only_in_a_browser",
    ))
}

#[cfg(target_arch = "wasm32")]
async fn subscribe_push_impl(public_key: &str) -> api::Result<()> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;
    if public_key.trim().is_empty() {
        return Err(api::ApiError::browser(
            "messages.push_unavailable_on_this_server",
        ));
    }
    let window = web_sys::window()
        .ok_or_else(|| api::ApiError::browser("messages.push_available_only_in_a_browser"))?;
    let navigator = window.navigator();
    let sw_container = js_sys::Reflect::get(&navigator, &"serviceWorker".into())
        .ok()
        .filter(|value| !value.is_undefined() && !value.is_null())
        .ok_or_else(|| api::ApiError::browser("messages.push_available_only_in_a_browser"))?;
    // Push without visible notification support is useless; probe before registering.
    if js_sys::Reflect::get(&window, &"PushManager".into())
        .ok()
        .is_none_or(|value| value.is_undefined() || value.is_null())
    {
        return Err(api::ApiError::browser(
            "messages.push_unavailable_in_this_browser",
        ));
    }
    // Ask for the notification permission explicitly; "default" leaves the subscribe
    // call to reject with NotAllowedError, which users can't distinguish from a real
    // "denied" state.
    let permission = request_notification_permission(&window).await?;
    if permission != "granted" {
        return Err(api::ApiError::browser("messages.push_permission_denied"));
    }
    // Root scope so the worker controls /dashboard, not just /assets.
    let register_options = js_sys::Object::new();
    js_sys::Reflect::set(&register_options, &"scope".into(), &"/".into()).unwrap();
    let register = js_sys::Reflect::get(&sw_container, &"register".into())
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let register_promise: js_sys::Promise = register
        .call2(
            &sw_container,
            &"/assets/push-sw.js".into(),
            &register_options,
        )
        .map_err(|error| subscribe_error(&error))?
        .dyn_into()
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let registration = JsFuture::from(register_promise)
        .await
        .map_err(|error| subscribe_error(&error))?;
    // Subscribe needs an active worker; wait for installation to finish, otherwise
    // Chromium rejects with InvalidStateError and the page looks frozen on reload.
    wait_for_active(&registration).await?;
    let push_manager = js_sys::Reflect::get(&registration, &"pushManager".into())
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    // Reuse an existing matching subscription to avoid a redundant permission round-trip.
    let existing = js_sys::Reflect::get(&push_manager, &"getSubscription".into())
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok());
    if let Some(get_sub) = existing {
        let promise: js_sys::Promise = get_sub
            .call0(&push_manager)
            .map_err(|error| subscribe_error(&error))?
            .dyn_into()
            .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
        let current = JsFuture::from(promise)
            .await
            .map_err(|error| subscribe_error(&error))?;
        if !current.is_null() && !current.is_undefined() {
            return post_subscription(&current).await;
        }
    }
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"userVisibleOnly".into(), &JsValue::TRUE).unwrap();
    js_sys::Reflect::set(
        &options,
        &"applicationServerKey".into(),
        &decode_base64url(public_key)?,
    )
    .unwrap();
    let subscribe = js_sys::Reflect::get(&push_manager, &"subscribe".into())
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let promise: js_sys::Promise = subscribe
        .call1(&push_manager, &options)
        .map_err(|error| subscribe_error(&error))?
        .dyn_into()
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let subscription = JsFuture::from(promise)
        .await
        .map_err(|error| subscribe_error(&error))?;
    post_subscription(&subscription).await
}

#[cfg(target_arch = "wasm32")]
async fn post_subscription(value: &wasm_bindgen::JsValue) -> api::Result<()> {
    let json: String = js_sys::JSON::stringify(value)
        .ok()
        .and_then(|value| value.as_string())
        .ok_or_else(|| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    api::send("POST", "/me/notifications/push", Some(value)).await
}

/// Awaits every pending state so `pushManager.subscribe` sees an active worker.
#[cfg(target_arch = "wasm32")]
async fn wait_for_active(registration: &wasm_bindgen::JsValue) -> api::Result<()> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    if !js_sys::Reflect::get(registration, &"active".into())
        .ok()
        .is_none_or(|value| value.is_null() || value.is_undefined())
    {
        return Ok(());
    }
    // The service worker container exposes `ready`, a Promise for the active
    // registration under the current page's scope. Awaiting it is the race-free
    // way to wait through "installing" and "waiting" to "activated".
    let window = web_sys::window()
        .ok_or_else(|| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let sw_container = js_sys::Reflect::get(&window.navigator(), &"serviceWorker".into())
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let ready: js_sys::Promise = js_sys::Reflect::get(&sw_container, &"ready".into())
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?
        .dyn_into()
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    JsFuture::from(ready)
        .await
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    Ok(())
}

/// Prompts for the notification permission, awaiting both the Promise and legacy
/// callback forms, and returns "granted", "denied" or "default".
#[cfg(target_arch = "wasm32")]
async fn request_notification_permission(window: &web_sys::Window) -> api::Result<String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let notification = js_sys::Reflect::get(window, &"Notification".into())
        .ok()
        .filter(|value| !value.is_undefined() && !value.is_null())
        .ok_or_else(|| api::ApiError::browser("messages.push_unavailable_in_this_browser"))?;
    if let Ok(current) = js_sys::Reflect::get(&notification, &"permission".into())
        && let Some(current) = current.as_string()
        && current == "granted"
    {
        return Ok(current);
    }
    let request = js_sys::Reflect::get(&notification, &"requestPermission".into())
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
        .ok_or_else(|| api::ApiError::browser("messages.push_unavailable_in_this_browser"))?;
    let outcome = request
        .call0(&notification)
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let outcome = if let Ok(promise) = outcome.clone().dyn_into::<js_sys::Promise>() {
        JsFuture::from(promise)
            .await
            .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?
    } else {
        outcome
    };
    Ok(outcome.as_string().unwrap_or_else(|| "default".to_owned()))
}

/// Maps a Push subscribe/unsubscribe rejection to the catalog key users see.
#[cfg(target_arch = "wasm32")]
fn subscribe_error(error: &wasm_bindgen::JsValue) -> api::ApiError {
    let key = match js_error_name(error).as_deref() {
        Some("NotAllowedError") => "messages.push_permission_denied",
        Some("InvalidAccessError") => "messages.push_unavailable_on_this_server",
        Some("NotSupportedError") => "messages.push_unavailable_in_this_browser",
        _ => "messages.push_subscribe_failed",
    };
    api::ApiError::browser(key)
}

#[cfg(target_arch = "wasm32")]
fn js_error_name(error: &wasm_bindgen::JsValue) -> Option<String> {
    js_sys::Reflect::get(error, &"name".into())
        .ok()
        .and_then(|value| value.as_string())
}

#[cfg(target_arch = "wasm32")]
async fn unsubscribe_push_impl() -> api::Result<()> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let navigator = web_sys::window()
        .ok_or_else(|| api::ApiError::browser("messages.push_available_only_in_a_browser"))?
        .navigator();
    let sw_container = js_sys::Reflect::get(&navigator, &"serviceWorker".into())
        .ok()
        .filter(|value| !value.is_undefined() && !value.is_null())
        .ok_or_else(|| api::ApiError::browser("messages.push_available_only_in_a_browser"))?;
    // Look up only an existing registration; `ready` would hang if none exists.
    let get_registration = js_sys::Reflect::get(&sw_container, &"getRegistration".into())
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
        .ok_or_else(|| api::ApiError::browser("messages.push_unsubscribe_failed"))?;
    let promise: js_sys::Promise = get_registration
        .call0(&sw_container)
        .map_err(|_| api::ApiError::browser("messages.push_unsubscribe_failed"))?
        .dyn_into()
        .map_err(|_| api::ApiError::browser("messages.push_unsubscribe_failed"))?;
    let registration = JsFuture::from(promise)
        .await
        .map_err(|_| api::ApiError::browser("messages.push_unsubscribe_failed"))?;
    if registration.is_null() || registration.is_undefined() {
        return Ok(());
    }
    let push_manager = js_sys::Reflect::get(&registration, &"pushManager".into())
        .map_err(|_| api::ApiError::browser("messages.push_unsubscribe_failed"))?;
    let get_sub = js_sys::Reflect::get(&push_manager, &"getSubscription".into())
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
        .ok_or_else(|| api::ApiError::browser("messages.push_unsubscribe_failed"))?;
    let promise: js_sys::Promise = get_sub
        .call0(&push_manager)
        .map_err(|_| api::ApiError::browser("messages.push_unsubscribe_failed"))?
        .dyn_into()
        .map_err(|_| api::ApiError::browser("messages.push_unsubscribe_failed"))?;
    let subscription = JsFuture::from(promise).await.ok();
    if let Some(subscription) =
        subscription.filter(|value| !value.is_null() && !value.is_undefined())
    {
        let endpoint = js_sys::Reflect::get(&subscription, &"endpoint".into())
            .map_err(|_| api::ApiError::browser("messages.push_unsubscribe_failed"))?
            .as_string()
            .unwrap_or_default();
        if let Ok(unsubscribe) = js_sys::Reflect::get(&subscription, &"unsubscribe".into())
            .and_then(|v| v.dyn_into::<js_sys::Function>())
        {
            let _ = unsubscribe.call0(&subscription);
        }
        if !endpoint.is_empty() {
            return api::send(
                "DELETE",
                "/me/notifications/push",
                Some(serde_json::json!({"endpoint": endpoint})),
            )
            .await;
        }
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn decode_base64url(text: &str) -> api::Result<wasm_bindgen::JsValue> {
    let mut normalized = text.replace("-", "+").replace("_", "/");
    while !normalized.len().is_multiple_of(4) {
        normalized.push('=');
    }
    let window = web_sys::window()
        .ok_or_else(|| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let decoded: String = window
        .atob(&normalized)
        .map_err(|_| api::ApiError::browser("messages.push_subscribe_failed"))?;
    let bytes: Vec<u8> = decoded.chars().map(|c| c as u8).collect();
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    Ok(array.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_state_means_already_registered_only_during_registration() {
        assert_eq!(
            passkey_error_key("InvalidStateError", true),
            "messages.this_passkey_is_already_registered"
        );
        assert_eq!(
            passkey_error_key("InvalidStateError", false),
            "messages.passkeys_are_unavailable_in_this_browser"
        );
        for registering in [true, false] {
            assert_eq!(
                passkey_error_key("NotAllowedError", registering),
                "messages.passkey_verification_was_cancelled"
            );
            assert_eq!(
                passkey_error_key("SecurityError", registering),
                "messages.passkeys_are_not_allowed_for_this_page"
            );
        }
    }

    #[test]
    fn base64url_without_padding_roundtrips_binary_fields() {
        let bytes = [0, 1, 2, 250, 251, 252, 253, 254, 255];
        let encoded = b64(&bytes);
        assert!(!encoded.contains('='));
        assert_eq!(b64_decode(&encoded).unwrap(), bytes);
    }
}
