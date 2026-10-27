//! JNI entry points; app classes must use the activity's class loader.
use super::reminders::Outgoing;

const NOTIFICATIONS: &str = "dev.dioxus.main.AhdNotifications";
const INBOX: &str = "dev.dioxus.main.AhdInbox";

#[cfg(target_os = "android")]
mod android {
    use jni::{
        JNIEnv, JavaVM,
        objects::{JClass, JObject, JString, JValue, JValueOwned},
    };
    fn checked<T>(env: &mut JNIEnv<'_>, value: jni::errors::Result<T>) -> Option<T> {
        if env.exception_check().unwrap_or(true) {
            let _ = env.exception_clear();
            None
        } else {
            value.ok()
        }
    }
    pub(super) fn call(
        class: &str,
        name: &str,
        signature: &str,
        texts: Option<(&str, &str)>,
        activity_arg: bool,
    ) -> Option<String> {
        let context = ndk_context::android_context();
        // SAFETY: ndk-context owns the live process VM and activity.
        let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        env.with_local_frame(16, |env| {
            Ok::<_, jni::errors::Error>((|| {
                // SAFETY: this borrowed activity remains valid for the app lifetime.
                let activity = unsafe { JObject::from_raw(context.context().cast()) };
                let value = env.call_method(
                    &activity,
                    "getClassLoader",
                    "()Ljava/lang/ClassLoader;",
                    &[],
                );
                let loader = checked(env, value)?.l().ok()?;
                let value = env.new_string(class);
                let class_name = checked(env, value)?;
                let value = env.call_method(
                    &loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(class_name.as_ref())],
                );
                let class = JClass::from(checked(env, value)?.l().ok()?);
                let mut args = Vec::new();
                if activity_arg {
                    args.push(JValue::Object(&activity));
                }
                let strings;
                if let Some((channel, json)) = texts {
                    let value = env.new_string(channel);
                    let channel = checked(env, value)?;
                    let value = env.new_string(json);
                    let json = checked(env, value)?;
                    strings = (channel, json);
                    args.push(JValue::Object(strings.0.as_ref()));
                    args.push(JValue::Object(strings.1.as_ref()));
                }
                let value = env.call_static_method(class, name, signature, &args);
                match checked(env, value)? {
                    JValueOwned::Bool(value) => Some((value != 0).to_string()),
                    JValueOwned::Object(value) if !value.is_null() => {
                        let string = JString::from(value);
                        let value = env.get_string(&string);
                        checked(env, value).map(Into::into)
                    }
                    _ => None,
                }
            })())
        })
        .ok()
        .flatten()
    }
}
fn call_on(
    class: &str,
    name: &str,
    signature: &str,
    texts: Option<(&str, &str)>,
    activity: bool,
) -> Option<String> {
    #[cfg(target_os = "android")]
    {
        android::call(class, name, signature, texts, activity)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (class, name, signature, texts, activity);
        None
    }
}
fn call(
    name: &str,
    signature: &str,
    texts: Option<(&str, &str)>,
    activity: bool,
) -> Option<String> {
    call_on(NOTIFICATIONS, name, signature, texts, activity)
}
pub(crate) fn permission_granted() -> bool {
    call(
        "permissionGranted",
        "(Landroid/content/Context;)Z",
        None,
        true,
    )
    .as_deref()
        == Some("true")
}
pub(crate) fn request_permission() {
    let _ = call("requestPermission", "(Landroid/app/Activity;)V", None, true);
}
pub(crate) fn open_settings() {
    let _ = call("openSettings", "(Landroid/app/Activity;)V", None, true);
}
pub(crate) fn schedule(channel: &str, entries: &[Outgoing]) -> bool {
    let Ok(json) = serde_json::to_string(entries) else {
        return false;
    };
    call(
        "schedule",
        "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z",
        Some((channel, &json)),
        true,
    )
    .as_deref()
        == Some("true")
}
pub(crate) fn show_now(channel: &str, entry: &Outgoing) -> bool {
    let Ok(json) = serde_json::to_string(entry) else {
        return false;
    };
    call(
        "showNow",
        "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z",
        Some((channel, &json)),
        true,
    )
    .as_deref()
        == Some("true")
}
pub(crate) fn take_open_target() -> Option<String> {
    call("takeOpenTarget", "()Ljava/lang/String;", None, false)
}
/// `{"device","owner"}` of the installation registered for server notices, if any.
pub(crate) fn inbox_state() -> Option<String> {
    call_on(
        INBOX,
        "state",
        "(Landroid/content/Context;)Ljava/lang/String;",
        None,
        true,
    )
}
/// Starts (or keeps) polling the server inbox for `config`, then polls once now.
pub(crate) fn inbox_configure(channel: &str, config: &super::inbox::Config) -> bool {
    let Ok(json) = serde_json::to_string(config) else {
        return false;
    };
    call_on(
        INBOX,
        "configure",
        "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z",
        Some((channel, &json)),
        true,
    )
    .as_deref()
        == Some("true")
}
pub(crate) fn inbox_stop() {
    let _ = call_on(INBOX, "stop", "(Landroid/content/Context;)Z", None, true);
}
pub(crate) fn inbox_poll_now() {
    let _ = call_on(INBOX, "pollNow", "(Landroid/content/Context;)Z", None, true);
}
