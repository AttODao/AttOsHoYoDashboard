//! One-shot Android browser sign-in callbacks and WebView event delivery.
use dioxus::prelude::*;

#[cfg(target_os = "android")]
mod android {
    use jni::{
        JNIEnv, JavaVM,
        objects::{JClass, JObject, JString, JValue},
    };

    fn checked<T>(env: &mut JNIEnv<'_>, value: jni::errors::Result<T>) -> Option<T> {
        if env.exception_check().unwrap_or(true) {
            let _ = env.exception_clear();
            None
        } else {
            value.ok()
        }
    }

    pub(super) fn take_callback() -> Option<String> {
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
                let value = env.new_string("dev.dioxus.main.AhdBrowserLogin");
                let class_name = checked(env, value)?;
                let value = env.call_method(
                    &loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(class_name.as_ref())],
                );
                let class = JClass::from(checked(env, value)?.l().ok()?);
                let value =
                    env.call_static_method(class, "takeCallback", "()Ljava/lang/String;", &[]);
                let value = checked(env, value)?.l().ok()?;
                if value.is_null() {
                    return None;
                }
                let string = JString::from(value);
                let value = env.get_string(&string);
                checked(env, value).map(Into::into)
            })())
        })
        .ok()
        .flatten()
    }
}

pub(crate) fn take_callback() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        android::take_callback()
    }
    #[cfg(not(target_os = "android"))]
    {
        None
    }
}

pub(crate) fn use_callback_events(mut on_event: impl FnMut() + 'static) {
    use_hook(move || {
        let mut eval = document::eval("window.__ahdBrowserLoginEvent = () => dioxus.send(true);");
        spawn(async move {
            while eval.recv::<bool>().await.is_ok() {
                on_event();
            }
        });
    });
}
