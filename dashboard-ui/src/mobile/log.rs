//! Diagnostic lines for the app: Android's log (`adb logcat -s AHD`), stderr elsewhere.
//! Never pass secrets, tokens or response bodies.

/// Writes one informational line under the `AHD` tag.
pub(crate) fn info(message: &str) {
    write(4, message);
}

/// Writes one warning line under the `AHD` tag.
pub(crate) fn warn(message: &str) {
    write(5, message);
}

#[cfg(target_os = "android")]
fn write(priority: i32, message: &str) {
    use std::ffi::{CString, c_char, c_int};
    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    let Ok(text) = CString::new(message.replace('\0', " ")) else {
        return;
    };
    // SAFETY: both pointers are NUL-terminated strings that outlive the call.
    unsafe {
        __android_log_write(priority, c"AHD".as_ptr(), text.as_ptr());
    }
}

#[cfg(not(target_os = "android"))]
fn write(priority: i32, message: &str) {
    let level = if priority >= 5 { "WARN" } else { "INFO" };
    eprintln!("AHD {level}: {message}");
}
