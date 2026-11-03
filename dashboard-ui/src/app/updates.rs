//! Startup update check of the Android app (see `mobile::updates` for the interface).
use crate::mobile::{
    log,
    updates::{self, UpdateCheck},
};
use dioxus::prelude::*;

/// Starts one background check when the app launches and provides its outcome as
/// `Signal<Option<UpdateCheck>>` context: None until it finishes, and for good when it
/// fails (offline, API error), which is only logged. Startup never waits for it.
pub(super) fn use_update_check() {
    let mut result = use_context_provider(|| Signal::new(None::<UpdateCheck>));
    use_future(move || async move {
        match updates::check().await {
            Ok(check) => {
                log::info(&format!(
                    "update check: current {}, latest {}, update available: {}",
                    check.current, check.latest, check.update_available
                ));
                result.set(Some(check));
            }
            Err(error) => log::warn(&format!("update check failed: {error}")),
        }
    });
}
