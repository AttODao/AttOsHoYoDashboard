//! Downloads the release APK chosen by `updates::check` into `<cache>/updates/`,
//! verifies SHA-256 against SHA256SUMS (or `{asset}.sha256`), checks the package,
//! version and installed app's signer, then hands it to Android's PackageInstaller.
//! Any failure deletes temporary files and leaves the installed app untouched. "Install unknown
//! apps" is checked before downloading, and a verified APK whose session was cancelled is kept
//! for "Try again" until the dialog closes (or the next launch).
use super::updates::{UpdateCheck, Version};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) const SKIP_FILE: &str = "update-skip.json";
pub(crate) const MAX_PACKAGE_BYTES: u64 = 200 * 1024 * 1024;
const CHECKSUM_LIMIT: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateError {
    Download,
    Storage,
    NoChecksum,
    ChecksumMismatch,
    InvalidPackage,
    SignerMismatch,
    Install,
    InstallAborted,
}
impl UpdateError {
    pub(crate) fn message_key(self) -> &'static str {
        match self {
            Self::Download => "update.error_download",
            Self::Storage => "update.error_storage",
            Self::NoChecksum => "update.error_no_checksum",
            Self::ChecksumMismatch => "update.error_checksum",
            Self::InvalidPackage => "update.error_package",
            Self::SignerMismatch => "update.error_signer",
            Self::Install => "update.error_install",
            Self::InstallAborted => "update.install_aborted",
        }
    }
}
impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Download => "download failed",
            Self::Storage => "could not write the download",
            Self::NoChecksum => "no checksum for the asset",
            Self::ChecksumMismatch => "checksum mismatch",
            Self::InvalidPackage => "not a newer package of this app",
            Self::SignerMismatch => "signer differs from the installed app",
            Self::Install => "installer failed",
            Self::InstallAborted => "installation cancelled",
        })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Progress {
    pub(crate) received: u64,
    pub(crate) total: u64,
}
impl Progress {
    pub(crate) fn percent(self) -> Option<u8> {
        (self.total != 0)
            .then(|| ((u128::from(self.received) * 100 / u128::from(self.total)).min(100)) as u8)
    }
}
pub(crate) fn progress_text(progress: Progress) -> String {
    let mb = progress.received as f64 / 1_000_000.0;
    match progress.percent() {
        Some(percent) => format!(
            "{mb:.1} / {:.1} MB ({percent}%)",
            progress.total as f64 / 1_000_000.0
        ),
        None => format!("{mb:.1} MB"),
    }
}
pub(crate) fn expected_sha256(list: &str, asset_name: &str) -> Option<[u8; 32]> {
    let mut found = None;
    for line in list.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let bytes = line.as_bytes();
        if bytes.len() < 66
            || bytes[64] != b' '
            || !matches!(bytes[65], b' ' | b'*')
            || bytes.get(66..) != Some(asset_name.as_bytes())
        {
            continue;
        }
        let mut hash = [0u8; 32];
        let mut valid = true;
        for (out, pair) in hash.iter_mut().zip(bytes[..64].as_chunks::<2>().0) {
            let digit = |b: u8| (b as char).to_digit(16);
            match (digit(pair[0]), digit(pair[1])) {
                (Some(a), Some(b)) => *out = (a * 16 + b) as u8,
                _ => {
                    valid = false;
                    break;
                }
            }
        }
        if !valid {
            continue;
        }
        if found.is_some_and(|previous| previous != hash) {
            return None;
        }
        found = Some(hash);
    }
    found
}
#[derive(Deserialize)]
struct PackageInfo {
    package: String,
    #[serde(rename = "versionCode")]
    version_code: i64,
    #[serde(rename = "versionName")]
    version_name: String,
    signers: Vec<String>,
}
#[derive(Deserialize)]
struct Inspection {
    archive: Option<PackageInfo>,
    own: PackageInfo,
}
pub(crate) fn verify_package(inspection_json: &str, expected: &Version) -> Result<(), UpdateError> {
    let inspection: Inspection =
        serde_json::from_str(inspection_json).map_err(|_| UpdateError::InvalidPackage)?;
    let archive = inspection.archive.ok_or(UpdateError::InvalidPackage)?;
    if archive.package != inspection.own.package
        || Version::parse(&archive.version_name).as_ref() != Some(expected)
        || archive.version_code <= inspection.own.version_code
    {
        return Err(UpdateError::InvalidPackage);
    }
    let normalize = |signers: Vec<String>| {
        let mut signers: Vec<_> = signers
            .into_iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        signers.sort();
        signers.dedup();
        signers
    };
    let archive = normalize(archive.signers);
    let own = normalize(inspection.own.signers);
    if archive.is_empty() || own.is_empty() || archive != own {
        return Err(UpdateError::SignerMismatch);
    }
    Ok(())
}
pub(crate) fn skip_json(version: &Version) -> String {
    serde_json::json!({"version": version.to_string()}).to_string()
}
pub(crate) fn skipped_version(text: &str) -> Option<Version> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    Version::parse(value.get("version")?.as_str()?)
}
pub(crate) fn should_prompt(check: &UpdateCheck, skipped: Option<&Version>) -> bool {
    check.update_available && check.asset.is_some() && skipped != Some(&check.latest)
}

/// Streams a download to a file while hashing it.
pub(crate) struct Sink {
    file: std::fs::File,
    hasher: Sha256,
    received: u64,
    limit: u64,
    temp: TempFile,
}
impl Sink {
    pub(crate) fn create(path: PathBuf, limit: u64) -> Result<Self, UpdateError> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let temp = TempFile { path };
        let file = options
            .open(temp.path())
            .map_err(|_| UpdateError::Storage)?;
        Ok(Self {
            file,
            hasher: Sha256::new(),
            received: 0,
            limit,
            temp,
        })
    }
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Progress, UpdateError> {
        let received = self
            .received
            .checked_add(chunk.len() as u64)
            .filter(|n| *n <= self.limit)
            .ok_or(UpdateError::Download)?;
        self.file
            .write_all(chunk)
            .map_err(|_| UpdateError::Storage)?;
        self.hasher.update(chunk);
        self.received = received;
        Ok(Progress {
            received,
            total: self.limit,
        })
    }
    pub(crate) fn finish(
        self,
        expected_size: u64,
        expected: &[u8; 32],
    ) -> Result<TempFile, UpdateError> {
        if expected_size > 0 && self.received != expected_size {
            return Err(UpdateError::Download);
        }
        self.file.sync_all().map_err(|_| UpdateError::Storage)?;
        if self.hasher.finalize().as_slice() != expected {
            return Err(UpdateError::ChecksumMismatch);
        }
        Ok(self.temp)
    }
}
/// Deletes its path when dropped (also when the owning future is cancelled).
#[derive(Debug)]
pub(crate) struct TempFile {
    path: PathBuf,
}
impl TempFile {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn rename(self, to: PathBuf) -> Result<TempFile, UpdateError> {
        std::fs::rename(&self.path, &to).map_err(|_| UpdateError::Storage)?;
        Ok(TempFile { path: to })
    }
}
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InstallResult {
    Success,
    Aborted,
    Failed,
}
pub(crate) fn parse_install_result(text: &str) -> Option<InstallResult> {
    match text {
        "success" => Some(InstallResult::Success),
        "aborted" => Some(InstallResult::Aborted),
        "failed" => Some(InstallResult::Failed),
        _ => None,
    }
}

/// What tapping "Update now"/"Try again", or returning to the app while waiting for the install
/// permission, does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// "Install unknown apps" is off: explain and offer the settings screen, download nothing yet.
    AskInstallPermission,
    Download,
    /// A verified APK for this version is still kept (the last session was cancelled).
    Install,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    /// Android cannot reopen the app after the update (background activity launch is blocked), so
    /// the "updated" notification is the only hint: ask for it once, when the user starts from the
    /// prompt, and continue after the answer (a permission dialog left behind the installer would
    /// outlive the replaced app).
    pub(crate) request_notifications: bool,
    pub(crate) step: Step,
}
pub(crate) fn plan(
    from_prompt: bool,
    notifications_granted: bool,
    install_allowed: bool,
    verified_ready: bool,
) -> Plan {
    Plan {
        request_notifications: from_prompt && !notifications_granted,
        step: if !install_allowed {
            Step::AskInstallPermission
        } else if verified_ready {
            Step::Install
        } else {
            Step::Download
        },
    }
}
/// A cancelled session keeps the verified APK for "Try again"; a failed one discards it.
pub(crate) fn keeps_verified_package(result: InstallResult) -> bool {
    result == InstallResult::Aborted
}

#[cfg(feature = "mobile")]
pub(crate) enum Stage {
    Progress(Progress),
    Verifying,
}
#[cfg(feature = "mobile")]
pub(crate) struct Prepared {
    file: TempFile,
    version: Version,
}
#[cfg(feature = "mobile")]
impl Prepared {
    pub(crate) fn is_for(&self, version: &Version) -> bool {
        &self.version == version
    }
}
#[cfg(feature = "mobile")]
pub(crate) fn clear_downloads() {
    if let Some(cache) = super::storage::cache_dir() {
        let _ = std::fs::remove_dir_all(cache.join("updates"));
    }
}
#[cfg(feature = "mobile")]
pub(crate) async fn download_and_verify(
    asset: &super::updates::Asset,
    latest: &Version,
    mut on_stage: impl FnMut(Stage),
) -> Result<Prepared, UpdateError> {
    use std::time::Duration;
    let checksum_url = asset.checksum_url.as_ref().ok_or(UpdateError::NoChecksum)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(super::http::USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| UpdateError::Download)?;
    let request = |url: &str| {
        let request = client.get(url);
        match super::updates::Source::BUILT_IN.token {
            Some(token) => request.header("Authorization", format!("token {token}")),
            None => request,
        }
    };
    let mut response = request(checksum_url)
        .send()
        .await
        .map_err(|_| UpdateError::Download)?;
    if response.status().as_u16() != 200 {
        return Err(UpdateError::Download);
    }
    let mut list = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Download)? {
        if list.len() + chunk.len() > CHECKSUM_LIMIT {
            return Err(UpdateError::NoChecksum);
        }
        list.extend_from_slice(&chunk);
    }
    let list = std::str::from_utf8(&list).map_err(|_| UpdateError::NoChecksum)?;
    let expected = expected_sha256(list, &asset.name).ok_or(UpdateError::NoChecksum)?;
    let dir = super::storage::cache_dir()
        .ok_or(UpdateError::Storage)?
        .join("updates");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|_| UpdateError::Storage)?;
    let limit = if asset.size > 0 {
        asset.size.min(MAX_PACKAGE_BYTES)
    } else {
        MAX_PACKAGE_BYTES
    };
    let mut sink = Sink::create(dir.join("ahd-update.apk.part"), limit)?;
    let mut response = request(&asset.download_url)
        .send()
        .await
        .map_err(|_| UpdateError::Download)?;
    if response.status().as_u16() != 200 {
        return Err(UpdateError::Download);
    }
    let mut last = Progress {
        received: 0,
        total: asset.size,
    };
    on_stage(Stage::Progress(last));
    while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Download)? {
        let mut progress = sink.push(&chunk)?;
        progress.total = asset.size;
        if progress.percent() != last.percent() || progress.received - last.received >= 512 * 1024 {
            on_stage(Stage::Progress(progress));
            last = progress;
        }
    }
    on_stage(Stage::Verifying);
    let file = sink
        .finish(asset.size, &expected)?
        .rename(dir.join("ahd-update.apk"))?;
    // Keep ownership in the future, so cancellation still removes the APK.
    let path = file.path().to_string_lossy().into_owned();
    let json = tokio::task::spawn_blocking(move || {
        super::notify::call_on(
            "dev.dioxus.main.AhdUpdater",
            "inspect",
            "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
            &[&path],
            true,
        )
    })
    .await
    .map_err(|_| UpdateError::InvalidPackage)?
    .ok_or(UpdateError::InvalidPackage)?;
    verify_package(&json, latest)?;
    Ok(Prepared {
        file,
        version: latest.clone(),
    })
}
#[cfg(feature = "mobile")]
#[derive(serde::Serialize)]
pub(crate) struct InstallNotice<'a> {
    pub(crate) version: String,
    pub(crate) title: &'a str,
    pub(crate) body: String,
    pub(crate) channel: &'a str,
}
/// Commits an install session and hands the APK back: the caller keeps it while the system
/// installer runs, so a cancelled session can be retried without downloading again.
#[cfg(feature = "mobile")]
pub(crate) async fn install(
    prepared: Prepared,
    notice: &InstallNotice<'_>,
) -> (Option<Prepared>, Result<(), UpdateError>) {
    let Ok(json) = serde_json::to_string(notice) else {
        return (Some(prepared), Err(UpdateError::Install));
    };
    // The blocking operation owns the APK until PackageInstaller has copied it.
    let joined = tokio::task::spawn_blocking(move || {
        let path = prepared.file.path().to_string_lossy().into_owned();
        let result = super::notify::call_on(
            "dev.dioxus.main.AhdUpdater",
            "install",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z",
            &[&path, &json],
            true,
        );
        (prepared, result)
    })
    .await;
    match joined {
        Ok((prepared, result)) if result.as_deref() == Some("true") => (Some(prepared), Ok(())),
        Ok((prepared, _)) => (Some(prepared), Err(UpdateError::Install)),
        Err(_) => (None, Err(UpdateError::Install)),
    }
}
#[cfg(feature = "mobile")]
pub(crate) fn take_install_result() -> Option<InstallResult> {
    parse_install_result(&super::notify::call_on(
        "dev.dioxus.main.AhdUpdater",
        "takeResult",
        "()Ljava/lang/String;",
        &[],
        false,
    )?)
}
#[cfg(feature = "mobile")]
pub(crate) fn install_allowed() -> bool {
    super::notify::call_on(
        "dev.dioxus.main.AhdUpdater",
        "installAllowed",
        "(Landroid/content/Context;)Z",
        &[],
        true,
    )
    .as_deref()
        != Some("false")
}
#[cfg(feature = "mobile")]
pub(crate) fn open_install_settings() -> bool {
    super::notify::call_on(
        "dev.dioxus.main.AhdUpdater",
        "openInstallSettings",
        "(Landroid/app/Activity;)Z",
        &[],
        true,
    )
    .as_deref()
        == Some("true")
}
#[cfg(feature = "mobile")]
/// True when Android was asked (the answer arrives as an install event).
pub(crate) fn request_notification_permission() -> bool {
    super::notify::call_on(
        "dev.dioxus.main.AhdUpdater",
        "requestNotificationPermission",
        "(Landroid/app/Activity;)Z",
        &[],
        true,
    )
    .as_deref()
        == Some("true")
}
/// Fires on installer results and whenever the activity resumes (a missing result is normal).
#[cfg(feature = "mobile")]
pub(crate) fn use_install_events(mut on_event: impl FnMut() + 'static) {
    use dioxus::prelude::*;
    use_hook(move || {
        let mut eval = document::eval("window.__ahdUpdateEvent = () => dioxus.send(true);");
        spawn(async move {
            while eval.recv::<bool>().await.is_ok() {
                on_event();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn version() -> Version {
        Version::parse("1.8.0").unwrap()
    }
    #[test]
    fn checksum_formats_and_duplicates() {
        let a = "ab".repeat(32);
        let b = "12".repeat(32);
        assert_eq!(
            expected_sha256(&format!("{b}  other.apk\n{a}  app.apk\n"), "app.apk"),
            Some([0xab; 32])
        );
        assert_eq!(
            expected_sha256(&format!("{a} *app.apk\r\n"), "app.apk"),
            Some([0xab; 32])
        );
        assert_eq!(expected_sha256(&format!("{a}  other.apk"), "app.apk"), None);
        for hash in ["g".repeat(64), "a".repeat(63)] {
            assert_eq!(
                expected_sha256(&format!("{hash}  app.apk"), "app.apk"),
                None
            );
        }
        assert_eq!(
            expected_sha256(&format!("{a}  app.apk\n{b}  app.apk"), "app.apk"),
            None
        );
        assert_eq!(
            expected_sha256(&format!("{a}  app.apk\n{a} *app.apk"), "app.apk"),
            Some([0xab; 32])
        );
    }
    fn inspection() -> serde_json::Value {
        json!({"archive":{"package":"cc.attodao.ahd","versionCode":10800,"versionName":"1.8.0","signers":["aabb"]},"own":{"package":"cc.attodao.ahd","versionCode":10700,"versionName":"1.7.0","signers":["aabb"]}})
    }
    #[test]
    fn package_identity_and_version() {
        assert_eq!(
            verify_package(&inspection().to_string(), &version()),
            Ok(())
        );
        for (field, value) in [
            ("package", json!("other")),
            ("versionCode", json!(10700)),
            ("versionCode", json!(10600)),
            ("versionName", json!("1.8.1")),
        ] {
            let mut data = inspection();
            data["archive"][field] = value;
            assert_eq!(
                verify_package(&data.to_string(), &version()),
                Err(UpdateError::InvalidPackage)
            );
        }
        let mut data = inspection();
        data["archive"] = serde_json::Value::Null;
        assert_eq!(
            verify_package(&data.to_string(), &version()),
            Err(UpdateError::InvalidPackage)
        );
        assert_eq!(
            verify_package("garbage", &version()),
            Err(UpdateError::InvalidPackage)
        );
    }
    #[test]
    fn package_signers_are_nonempty_case_insensitive_sets() {
        let mut data = inspection();
        data["archive"]["signers"] = json!(["AABB", "aabb"]);
        assert_eq!(verify_package(&data.to_string(), &version()), Ok(()));
        for (side, signers) in [
            ("archive", json!(["different"])),
            ("archive", json!([])),
            ("own", json!([])),
        ] {
            let mut data = inspection();
            data[side]["signers"] = signers;
            assert_eq!(
                verify_package(&data.to_string(), &version()),
                Err(UpdateError::SignerMismatch)
            );
        }
    }
    #[test]
    fn skip_roundtrip_and_prompt_policy() {
        assert_eq!(skipped_version(&skip_json(&version())), Some(version()));
        for bad in ["garbage", "{}", "{\"version\":42}", "{\"version\":\"bad\"}"] {
            assert_eq!(skipped_version(bad), None);
        }
        let mut check = UpdateCheck {
            update_available: true,
            current: Version::parse("1.7.0").unwrap(),
            latest: version(),
            release_notes: String::new(),
            release_url: String::new(),
            asset: Some(super::super::updates::Asset {
                name: String::new(),
                download_url: String::new(),
                size: 0,
                checksum_url: None,
            }),
        };
        assert!(should_prompt(&check, None));
        assert!(!should_prompt(&check, Some(&version())));
        assert!(should_prompt(&check, Some(&check.current)));
        check.update_available = false;
        assert!(!should_prompt(&check, None));
        check.update_available = true;
        check.asset = None;
        assert!(!should_prompt(&check, None));
        assert_eq!(SKIP_FILE, "update-skip.json");
    }
    #[test]
    fn progress_formats_and_handles_unknown_size_and_overflow() {
        let p = Progress {
            received: 12_300_000,
            total: 19_900_000,
        };
        assert_eq!(p.percent(), Some(61));
        assert_eq!(progress_text(p), "12.3 / 19.9 MB (61%)");
        assert_eq!(
            progress_text(Progress {
                received: 1_260_000,
                total: 0
            }),
            "1.3 MB"
        );
        assert_eq!(Progress::default().percent(), None);
        assert_eq!(
            Progress {
                received: u64::MAX,
                total: 1
            }
            .percent(),
            Some(100)
        );
        assert_eq!(MAX_PACKAGE_BYTES, 209_715_200);
        assert_eq!(CHECKSUM_LIMIT, 65_536);
    }
    #[test]
    fn installer_results_and_error_messages() {
        for (text, result) in [
            ("success", InstallResult::Success),
            ("aborted", InstallResult::Aborted),
            ("failed", InstallResult::Failed),
        ] {
            assert_eq!(parse_install_result(text), Some(result));
        }
        assert_eq!(parse_install_result("unknown"), None);
        for error in [
            UpdateError::Download,
            UpdateError::Storage,
            UpdateError::NoChecksum,
            UpdateError::ChecksumMismatch,
            UpdateError::InvalidPackage,
            UpdateError::SignerMismatch,
            UpdateError::Install,
            UpdateError::InstallAborted,
        ] {
            assert!(error.message_key().starts_with("update."));
            assert!(!error.to_string().is_empty());
        }
    }
    #[test]
    fn start_plan_asks_for_notifications_only_from_the_prompt() {
        assert_eq!(
            plan(true, false, true, false),
            Plan {
                request_notifications: true,
                step: Step::Download
            }
        );
        assert!(!plan(true, true, true, false).request_notifications);
        // Retrying, returning from the settings screen or after the answer never asks again.
        assert!(!plan(false, false, true, false).request_notifications);
        assert!(!plan(false, false, true, true).request_notifications);
        assert!(plan(true, false, false, false).request_notifications);
    }
    #[test]
    fn start_plan_checks_the_install_permission_before_downloading() {
        for ready in [false, true] {
            for from_prompt in [false, true] {
                assert_eq!(
                    plan(from_prompt, true, false, ready).step,
                    Step::AskInstallPermission
                );
            }
        }
        assert_eq!(plan(false, true, true, true).step, Step::Install);
        assert_eq!(plan(true, false, true, true).step, Step::Install);
        assert_eq!(plan(true, true, true, false).step, Step::Download);
    }
    #[test]
    fn only_a_cancelled_session_keeps_the_verified_package() {
        assert!(keeps_verified_package(InstallResult::Aborted));
        assert!(!keeps_verified_package(InstallResult::Failed));
        assert!(!keeps_verified_package(InstallResult::Success));
    }
    fn directory(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ahd-update-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn sink_success_rename_and_drop() {
        let dir = directory("success");
        let path = dir.join("update.part");
        let mut sink = Sink::create(path.clone(), 100).unwrap();
        sink.push(b"hello").unwrap();
        let expected: [u8; 32] = Sha256::digest(b"hello").into();
        let file = sink.finish(5, &expected).unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read(file.path()).unwrap(), b"hello");
        let new = dir.join("ahd-update.apk");
        let file = file.rename(new.clone()).unwrap();
        assert!(!path.exists());
        assert!(new.exists());
        drop(file);
        assert!(!new.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn sink_errors_and_cancellation_delete_partial_files() {
        let dir = directory("errors");
        let path = dir.join("update.part");
        let mut sink = Sink::create(path.clone(), 100).unwrap();
        sink.push(b"hello").unwrap();
        assert_eq!(
            sink.finish(5, &[0; 32]).unwrap_err(),
            UpdateError::ChecksumMismatch
        );
        assert!(!path.exists());
        let mut sink = Sink::create(path.clone(), 4).unwrap();
        assert_eq!(sink.push(b"hello"), Err(UpdateError::Download));
        drop(sink);
        assert!(!path.exists());
        let mut sink = Sink::create(path.clone(), 100).unwrap();
        sink.push(b"hello").unwrap();
        assert_eq!(sink.finish(6, &[0; 32]).unwrap_err(), UpdateError::Download);
        assert!(!path.exists());
        let sink = Sink::create(path.clone(), 100).unwrap();
        drop(sink);
        assert!(!path.exists());
        let sink = Sink::create(path.clone(), 100).unwrap();
        let file = sink.finish(0, &Sha256::digest([]).into()).unwrap();
        assert_eq!(
            file.rename(dir.join("missing/new")).unwrap_err(),
            UpdateError::Storage
        );
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
