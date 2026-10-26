//! Web/app differences: what the Android app leaves out, and timers that work on both.

/// The Android app (`mobile` feature). The browser build and native tests are `false`.
/// The app leaves administration, API docs, account deletion, API tokens and password
/// change to the web version: pages are filtered in `app::state`, controls in `pages`.
pub const APP: bool = cfg!(feature = "mobile");

/// Waits on the browser timer, or the native runtime timer in the app.
pub async fn sleep_ms(ms: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(ms).await;
    #[cfg(all(feature = "mobile", not(target_arch = "wasm32")))]
    tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))).await;
    // Native tests never drive timers; never resolving keeps polling loops inert.
    #[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
    {
        let _ = ms;
        std::future::pending::<()>().await;
    }
}

/// Milliseconds since the Unix epoch by the device clock.
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(all(feature = "mobile", not(target_arch = "wasm32")))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1000.0)
    }
    #[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
    0.0
}
