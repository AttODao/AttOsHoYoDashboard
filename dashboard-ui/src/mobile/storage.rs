//! Small files in the app-private data directory (not readable by other apps).
use std::{
    io::{self, Write},
    path::PathBuf,
    sync::OnceLock,
};

fn data_dir() -> Option<&'static PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(locate).as_ref()
}

#[cfg(target_os = "android")]
fn locate() -> Option<PathBuf> {
    use jni::{
        JavaVM,
        objects::{JObject, JString},
    };
    let context = ndk_context::android_context();
    // SAFETY: ndk-context holds the process JavaVM and the activity for the app's lifetime.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    // SAFETY: as above; the activity reference stays valid while the app runs.
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    fn call<'local>(
        env: &mut jni::JNIEnv<'local>,
        target: &JObject,
        name: &str,
        signature: &str,
    ) -> Option<JObject<'local>> {
        let value = env.call_method(target, name, signature, &[]);
        if env.exception_check().unwrap_or(true) {
            let _ = env.exception_clear();
            return None;
        }
        value.ok()?.l().ok()
    }
    let dir = call(&mut env, &activity, "getFilesDir", "()Ljava/io/File;")?;
    let path = call(&mut env, &dir, "getAbsolutePath", "()Ljava/lang/String;")?;
    let path: String = env.get_string(&JString::from(path)).ok()?.into();
    Some(PathBuf::from(path))
}

/// Desktop runs of the mobile build (development only) use an explicit directory.
#[cfg(not(target_os = "android"))]
fn locate() -> Option<PathBuf> {
    std::env::var_os("AHD_APP_DATA_DIR").map(PathBuf::from)
}

pub(crate) fn read(name: &str) -> Option<String> {
    std::fs::read_to_string(data_dir()?.join(name)).ok()
}

/// Replaces the file atomically, readable by the app only.
pub(crate) fn write(name: &str, contents: &str) -> io::Result<()> {
    let dir = data_dir().ok_or_else(|| io::Error::other("no app data directory"))?;
    std::fs::create_dir_all(dir)?;
    let temporary = dir.join(format!(".{name}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temporary)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(temporary, dir.join(name))
}

pub(crate) fn remove(name: &str) {
    if let Some(dir) = data_dir() {
        let _ = std::fs::remove_file(dir.join(name));
    }
}
