//! Ephemeral administrator credentials. Never serialize or debug the result state.
use crate::{api, models::UserView};

const PASSWORD_LENGTH: usize = 24;
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode(bytes: &[u8; PASSWORD_LENGTH]) -> String {
    // Each uniform byte contributes exactly six bits: 24 * 6 = 144 bits.
    bytes
        .iter()
        .map(|byte| char::from(ALPHABET[usize::from(byte & 63)]))
        .collect()
}

fn generate_with(
    fill: impl FnOnce(&mut [u8; PASSWORD_LENGTH]) -> Result<(), ()>,
) -> Result<String, ()> {
    let mut bytes = [0; PASSWORD_LENGTH];
    fill(&mut bytes)?;
    Ok(encode(&bytes))
}

pub(super) fn generate() -> Result<String, ()> {
    generate_with(|bytes| {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .ok_or(())?
                .crypto()
                .map_err(|_| ())?
                .get_random_values_with_u8_array(bytes)
                .map_err(|_| ())?;
            Ok(())
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = bytes;
            Err(())
        }
    })
}

pub(super) async fn copy(secret: &str) -> Result<(), ()> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let navigator = web_sys::window().ok_or(())?.navigator();
        // Clipboard may be absent outside a secure context; absence is a failure.
        let clipboard = js_sys::Reflect::get(navigator.as_ref(), &"clipboard".into())
            .map_err(|_| ())?
            .dyn_into::<web_sys::Clipboard>()
            .map_err(|_| ())?;
        wasm_bindgen_futures::JsFuture::from(clipboard.write_text(secret))
            .await
            .map_err(|_| ())?;
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = secret;
        Err(())
    }
}

pub(super) async fn create(username: &str, secret: &str) -> api::Result<UserView> {
    #[cfg(target_arch = "wasm32")]
    {
        api::json(
            "POST",
            "/admin/users",
            Some(serde_json::json!({
                "username": username, "temporary_password": secret
            })),
        )
        .await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (username, secret);
        Err(api::ApiError::browser(
            "messages.this_action_is_available_only_in_a_browser",
        ))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyState {
    Ready,
    Pending,
    Copied,
    Failed,
}

impl CopyState {
    pub(super) fn after_write(outcome: Result<(), ()>) -> Self {
        if outcome.is_ok() {
            Self::Copied
        } else {
            Self::Failed
        }
    }
}

pub(super) struct CredentialResult {
    pub(super) username: String,
    pub(super) secret: String,
    pub(super) generation: u64,
    pub(super) copy: CopyState,
}

impl CredentialResult {
    // An API failure drops the candidate instead of exposing it to a result view.
    pub(super) fn after_success<E>(
        outcome: Result<String, E>,
        secret: String,
        generation: u64,
    ) -> Result<Self, E> {
        Ok(Self {
            username: outcome?,
            secret,
            generation,
            copy: CopyState::Ready,
        })
    }
}

pub(super) fn clear_stale(result: &mut Option<CredentialResult>, generation: u64) {
    if result
        .as_ref()
        .is_some_and(|result| result.generation != generation)
    {
        *result = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_has_exact_length_and_four_equal_preimages_per_symbol() {
        let mut counts = [0; 64];
        for byte in 0..=255u8 {
            let encoded = encode(&[byte; PASSWORD_LENGTH]);
            assert!(encoded.len() == 24 && (12..=256).contains(&encoded.len()));
            assert!(encoded.is_ascii());
            let index = ALPHABET
                .iter()
                .position(|symbol| *symbol == encoded.as_bytes()[0])
                .unwrap();
            counts[index] += 1;
            assert!(
                encoded
                    .bytes()
                    .all(|symbol| symbol == ALPHABET[usize::from(byte & 63)])
            );
        }
        assert!(counts.iter().all(|count| *count == 4));
    }

    #[test]
    fn entropy_failure_never_returns_a_candidate() {
        assert!(generate_with(|_| Err(())).is_err());
        assert!(
            generate_with(|bytes| {
                bytes.fill(255);
                Ok(())
            })
            .is_ok()
        );
    }

    #[test]
    fn clipboard_failure_is_not_success() {
        assert!(CopyState::after_write(Err(())) == CopyState::Failed);
        assert!(CopyState::after_write(Ok(())) == CopyState::Copied);
    }

    #[test]
    fn only_success_can_reveal_and_stale_generations_clear_result() {
        let failed =
            CredentialResult::after_success(Err::<String, _>(()), "SYNTHETIC-ONLY".into(), 4);
        assert!(failed.is_err());
        let mut result = CredentialResult::after_success(
            Ok::<_, ()>("mock-user".into()),
            "SYNTHETIC-ONLY".into(),
            4,
        )
        .ok();
        clear_stale(&mut result, 4);
        assert!(
            result
                .as_ref()
                .is_some_and(|value| value.copy == CopyState::Ready)
        );
        clear_stale(&mut result, 5);
        assert!(result.is_none());
    }
}
