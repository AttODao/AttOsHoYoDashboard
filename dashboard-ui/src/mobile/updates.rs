//! App update check against the Forgejo releases of this repository.
//!
//! Interface for the follow-up download/install/prompt work:
//!
//! - [`check`] (app only, async): calls `GET {base}/api/v1/repos/{owner}/{repo}/releases/latest`
//!   with [`Source::BUILT_IN`] and returns `Ok(`[`UpdateCheck`]`)` or a [`CheckError`]. It
//!   never panics and makes exactly one request; callers decide what to log or show.
//! - [`UpdateCheck`]: `update_available`, `current`/`latest` [`Version`], `release_notes`
//!   (the release body, Markdown), `release_url` (the release page), and `asset`: the
//!   [`Asset`] for this device (`name`, `download_url`, `size`, `checksum_url`) when the
//!   release has one. `update_available` is true only when `latest > current` *and* an
//!   asset for this device exists, so a release without an APK never prompts.
//! - [`evaluate`] is the pure decision (release JSON + current version + [`Target`]) that
//!   `check` uses; it is what the unit tests exercise with mocked API responses.
//!
//! The startup hook is `app::updates::use_update_check`, which runs `check` in the
//! background and publishes the result as `Signal<Option<UpdateCheck>>` context.
//!
//! Assets follow `ahd-{os}-{version}-{arch}.{ext}` (today `ahd-android-X.Y.Z-arm64.apk`,
//! built by `.forgejo/workflows/android.yml`); the checksum is `{asset}.sha256` or, if
//! absent, a release-wide `SHA256SUMS`.
use serde::Deserialize;
use std::cmp::Ordering;
use std::fmt;

/// Where releases are looked up. Build-time overrides: `AHD_UPDATE_FORGEJO_URL`
/// (`https://host`), `AHD_UPDATE_REPOSITORY` (`owner/repo`) and, for a private repository,
/// `AHD_UPDATE_TOKEN`. The token is compiled into the APK and can be extracted from it, so
/// it must be a read-only (`read:repository`) token of an account with access to nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) base_url: &'static str,
    pub(crate) owner: &'static str,
    pub(crate) repo: &'static str,
    pub(crate) token: Option<&'static str>,
}

const DEFAULT_BASE_URL: &str = "https://git.attodao.cc";
const DEFAULT_REPOSITORY: &str = "AttODao/AttOsHoYoDashboard";

impl Source {
    pub(crate) const BUILT_IN: Self = {
        let repository = match option_env!("AHD_UPDATE_REPOSITORY") {
            Some(value) => value,
            None => DEFAULT_REPOSITORY,
        };
        let (owner, repo) = split_repository(repository);
        Self {
            base_url: match option_env!("AHD_UPDATE_FORGEJO_URL") {
                Some(value) => value,
                None => DEFAULT_BASE_URL,
            },
            owner,
            repo,
            token: match option_env!("AHD_UPDATE_TOKEN") {
                Some(value) if !value.is_empty() => Some(value),
                _ => None,
            },
        }
    };

    /// The latest-release endpoint (Forgejo excludes drafts and prereleases there).
    pub(crate) fn latest_release_url(&self) -> String {
        format!(
            "{}/api/v1/repos/{}/{}/releases/latest",
            self.base_url.trim_end_matches('/'),
            self.owner,
            self.repo
        )
    }
}

/// Splits `owner/repo` at compile time; a value without exactly one `/` fails the build.
const fn split_repository(value: &'static str) -> (&'static str, &'static str) {
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut slash = None;
    while index < bytes.len() {
        if bytes[index] == b'/' {
            assert!(slash.is_none(), "AHD_UPDATE_REPOSITORY must be owner/repo");
            slash = Some(index);
        }
        index += 1;
    }
    let Some(slash) = slash else {
        panic!("AHD_UPDATE_REPOSITORY must be owner/repo");
    };
    assert!(
        slash > 0 && slash + 1 < bytes.len(),
        "AHD_UPDATE_REPOSITORY must be owner/repo"
    );
    let (owner, rest) = value.split_at(slash);
    let (_, repo) = rest.split_at(1);
    (owner, repo)
}

/// A semantic version (`MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]`, optional leading `v`).
/// Ordering follows SemVer 2.0 precedence; build metadata is ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Version {
    pub(crate) major: u64,
    pub(crate) minor: u64,
    pub(crate) patch: u64,
    pub(crate) pre: Vec<String>,
}

impl Version {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        let text = text.split_once('+').map_or(text, |(core, build)| {
            // Build metadata is ignored but must still be well formed.
            if identifiers_ok(build) { core } else { "" }
        });
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) if identifiers_ok(pre) => (core, Some(pre)),
            Some(_) => return None,
            None => (text, None),
        };
        let mut numbers = core.split('.').map(numeric);
        let (Some(Some(major)), Some(Some(minor)), Some(Some(patch)), None) = (
            numbers.next(),
            numbers.next(),
            numbers.next(),
            numbers.next(),
        ) else {
            return None;
        };
        let pre = pre.map_or_else(Vec::new, |pre| {
            pre.split('.').map(str::to_owned).collect::<Vec<_>>()
        });
        if pre
            .iter()
            .any(|part| part.bytes().all(|b| b.is_ascii_digit()) && numeric(part).is_none())
        {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
            pre,
        })
    }

    /// The running app's version (`dashboard-ui/Cargo.toml`, equal to the release tag).
    pub(crate) fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("the package version is SemVer")
    }

    pub(crate) fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

/// A number without leading zeros.
fn numeric(part: &str) -> Option<u64> {
    let valid = !part.is_empty()
        && part.bytes().all(|b| b.is_ascii_digit())
        && (part == "0" || !part.starts_with('0'));
    valid.then(|| part.parse().ok()).flatten()
}

fn identifiers_ok(text: &str) -> bool {
    text.split('.').all(|part| {
        !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => compare_pre(&self.pre, &other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_pre(left: &[String], right: &[String]) -> Ordering {
    for (a, b) in left.iter().zip(right) {
        let order = match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => a.cmp(b),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

/// The OS/architecture part of asset names, e.g. `android`/`arm64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) os: &'static str,
    pub(crate) arch: &'static str,
    pub(crate) extension: &'static str,
}

impl Target {
    /// This build's target; None where no release asset exists (web, desktop tests).
    pub(crate) const fn current() -> Option<Self> {
        if !cfg!(target_os = "android") {
            return None;
        }
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "arm") {
            "armv7"
        } else if cfg!(target_arch = "x86") {
            "x86"
        } else {
            return None;
        };
        Some(Self {
            os: "android",
            arch,
            extension: "apk",
        })
    }

    pub(crate) fn asset_name(&self, version: &Version) -> String {
        format!("ahd-{}-{version}-{}.{}", self.os, self.arch, self.extension)
    }
}

/// The downloadable file for this device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Asset {
    pub(crate) name: String,
    pub(crate) download_url: String,
    pub(crate) size: u64,
    /// `{name}.sha256` (one `sha256sum` line) or the release's `SHA256SUMS`.
    pub(crate) checksum_url: Option<String>,
}

/// Result of one successful check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateCheck {
    pub(crate) update_available: bool,
    pub(crate) current: Version,
    pub(crate) latest: Version,
    pub(crate) release_notes: String,
    pub(crate) release_url: String,
    pub(crate) asset: Option<Asset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckError {
    /// No connection, timeout, TLS or an unreadable body.
    Network,
    /// The API answered with another status (404: no release yet, 401/403: token).
    Status(u16),
    /// The body is not a Forgejo release.
    Malformed,
    /// The release is a draft or prerelease, or its tag is a SemVer prerelease.
    NotStable,
    /// The tag is not `vMAJOR.MINOR.PATCH`.
    BadTag(String),
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network => f.write_str("network error"),
            Self::Status(status) => write!(f, "HTTP {status}"),
            Self::Malformed => f.write_str("unexpected response"),
            Self::NotStable => f.write_str("latest release is a draft or prerelease"),
            Self::BadTag(tag) => write!(f, "tag {tag:?} is not a version"),
        }
    }
}

#[derive(Deserialize)]
struct ReleaseJson {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<AssetJson>,
}

#[derive(Deserialize)]
struct AssetJson {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Decides from a `releases/latest` body. `target` None (no asset for this build)
/// never reports an update.
pub(crate) fn evaluate(
    body: &str,
    current: &Version,
    target: Option<Target>,
) -> Result<UpdateCheck, CheckError> {
    let release: ReleaseJson = serde_json::from_str(body).map_err(|_| CheckError::Malformed)?;
    if release.draft || release.prerelease {
        return Err(CheckError::NotStable);
    }
    let latest =
        Version::parse(&release.tag_name).ok_or(CheckError::BadTag(release.tag_name.clone()))?;
    if latest.is_prerelease() {
        return Err(CheckError::NotStable);
    }
    let asset =
        target.and_then(|target| select_asset(&release.assets, &target.asset_name(&latest)));
    Ok(UpdateCheck {
        update_available: latest > *current && asset.is_some(),
        current: current.clone(),
        latest,
        release_notes: release.body.unwrap_or_default(),
        release_url: release.html_url.unwrap_or_default(),
        asset,
    })
}

fn select_asset(assets: &[AssetJson], name: &str) -> Option<Asset> {
    let find = |wanted: &str| assets.iter().find(|asset| asset.name == wanted);
    let file = find(name)?;
    let checksum = find(&format!("{name}.sha256")).or_else(|| find("SHA256SUMS"));
    Some(Asset {
        name: file.name.clone(),
        download_url: file.browser_download_url.clone(),
        size: file.size,
        checksum_url: checksum.map(|asset| asset.browser_download_url.clone()),
    })
}

/// Largest accepted API body; a release with long notes stays far below this.
#[cfg(feature = "mobile")]
const BODY_LIMIT: usize = 512 * 1024;

/// Asks Forgejo for the latest release (one request, 10 s connect / 30 s total).
#[cfg(feature = "mobile")]
pub(crate) async fn check() -> Result<UpdateCheck, CheckError> {
    let source = Source::BUILT_IN;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(super::http::USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| CheckError::Network)?;
    let mut request = client
        .get(source.latest_release_url())
        .header("Accept", "application/json");
    if let Some(token) = source.token {
        request = request.header("Authorization", format!("token {token}"));
    }
    let mut response = request.send().await.map_err(|_| CheckError::Network)?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(CheckError::Status(status));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| CheckError::Network)? {
        if body.len() + chunk.len() > BODY_LIMIT {
            return Err(CheckError::Malformed);
        }
        body.extend_from_slice(&chunk);
    }
    let body = String::from_utf8(body).map_err(|_| CheckError::Malformed)?;
    evaluate(&body, &Version::current(), Target::current())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const ARM64: Target = Target {
        os: "android",
        arch: "arm64",
        extension: "apk",
    };
    const X86_64: Target = Target {
        os: "android",
        arch: "x86_64",
        extension: "apk",
    };

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} parses"))
    }

    fn asset(name: &str) -> Value {
        json!({
            "id": 66,
            "name": name,
            "size": 19906252,
            "download_count": 1,
            "browser_download_url": format!("https://git.example/AttODao/AHD/releases/download/v1.8.0/{name}"),
        })
    }

    /// The shape of Forgejo's `releases/latest` answer (fields trimmed).
    fn release(tag: &str, assets: &[&str]) -> Value {
        json!({
            "id": 40,
            "tag_name": tag,
            "name": tag,
            "body": "- notes",
            "html_url": format!("https://git.example/AttODao/AHD/releases/tag/{tag}"),
            "draft": false,
            "prerelease": false,
            "author": {"login": "AttODao"},
            "assets": assets.iter().map(|name| asset(name)).collect::<Vec<_>>(),
        })
    }

    fn run(body: &Value, current: &str, target: Option<Target>) -> Result<UpdateCheck, CheckError> {
        evaluate(&body.to_string(), &version(current), target)
    }

    #[test]
    fn parses_versions_and_tags() {
        assert_eq!(
            version("v1.7.0"),
            Version {
                major: 1,
                minor: 7,
                patch: 0,
                pre: vec![]
            }
        );
        assert_eq!(version("1.8.0-rc.1+build.5").pre, ["rc", "1"]);
        assert_eq!(version("V10.0.3").to_string(), "10.0.3");
        for bad in [
            "",
            "v",
            "1.7",
            "1.7.0.1",
            "01.7.0",
            "1.07.0",
            "1.7.x",
            "1.7.0-",
            "1.7.0-rc..1",
            "1.7.0-01",
            "1.7.0+",
            "latest",
            "v1.7.0 beta",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn orders_by_semver_precedence() {
        let ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.1.0",
            "1.9.0",
            "1.10.0",
            "2.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(
                version(pair[0]) < version(pair[1]),
                "{} < {}",
                pair[0],
                pair[1]
            );
        }
        assert_eq!(version("1.7.0+a").cmp(&version("v1.7.0")), Ordering::Equal);
    }

    #[test]
    fn newer_release_with_this_devices_asset_is_an_update() {
        let body = release(
            "v1.8.0",
            &[
                "ahd-android-1.8.0-x86_64.apk",
                "ahd-android-1.8.0-arm64.apk",
                "ahd-android-1.8.0-arm64.apk.sha256",
                "SHA256SUMS",
            ],
        );
        let check = run(&body, "1.7.0", Some(ARM64)).unwrap();
        assert!(check.update_available);
        assert_eq!(check.current, version("1.7.0"));
        assert_eq!(check.latest, version("1.8.0"));
        assert_eq!(check.release_notes, "- notes");
        assert_eq!(
            check.release_url,
            "https://git.example/AttODao/AHD/releases/tag/v1.8.0"
        );
        let asset = check.asset.unwrap();
        assert_eq!(asset.name, "ahd-android-1.8.0-arm64.apk");
        assert_eq!(
            asset.download_url,
            "https://git.example/AttODao/AHD/releases/download/v1.8.0/ahd-android-1.8.0-arm64.apk"
        );
        assert_eq!(asset.size, 19906252);
        assert!(
            asset
                .checksum_url
                .unwrap()
                .ends_with("/ahd-android-1.8.0-arm64.apk.sha256")
        );
    }

    #[test]
    fn picks_the_asset_for_the_architecture_and_falls_back_to_sha256sums() {
        let body = release(
            "v1.8.0",
            &[
                "ahd-android-1.8.0-arm64.apk",
                "ahd-android-1.8.0-x86_64.apk",
                "SHA256SUMS",
            ],
        );
        let asset = run(&body, "1.7.0", Some(X86_64)).unwrap().asset.unwrap();
        assert_eq!(asset.name, "ahd-android-1.8.0-x86_64.apk");
        assert!(asset.checksum_url.unwrap().ends_with("/SHA256SUMS"));

        let unsigned = release("v1.8.0", &["ahd-android-1.8.0-arm64.apk"]);
        assert_eq!(
            run(&unsigned, "1.7.0", Some(ARM64))
                .unwrap()
                .asset
                .unwrap()
                .checksum_url,
            None
        );
    }

    #[test]
    fn missing_asset_never_prompts() {
        // Assets of another version, another architecture or a checksum alone do not match.
        let body = release(
            "v1.8.0",
            &[
                "ahd-android-1.7.0-arm64.apk",
                "ahd-android-1.8.0-x86_64.apk",
                "ahd-android-1.8.0-arm64.apk.sha256",
            ],
        );
        let check = run(&body, "1.7.0", Some(ARM64)).unwrap();
        assert!(!check.update_available);
        assert_eq!(check.asset, None);
        assert_eq!(check.latest, version("1.8.0"));
        // A build without a release target (web, native tests) only reports the version.
        let check = run(
            &release("v1.8.0", &["ahd-android-1.8.0-arm64.apk"]),
            "1.7.0",
            None,
        )
        .unwrap();
        assert!(!check.update_available);
    }

    #[test]
    fn same_or_older_release_is_not_an_update() {
        for (tag, current) in [
            ("v1.7.0", "1.7.0"),
            ("v1.6.0", "1.7.0"),
            ("v1.7.0", "1.8.0-rc.1"),
        ] {
            let check = run(
                &release(tag, &[&format!("ahd-android-{}-arm64.apk", &tag[1..])]),
                current,
                Some(ARM64),
            )
            .unwrap();
            assert_eq!(
                check.update_available,
                version(tag) > version(current),
                "{tag} vs {current}"
            );
        }
        assert!(
            !run(
                &release("v1.7.0", &["ahd-android-1.7.0-arm64.apk"]),
                "1.7.0",
                Some(ARM64)
            )
            .unwrap()
            .update_available
        );
        // A stable release is newer than a prerelease of the same version.
        assert!(
            run(
                &release("v1.8.0", &["ahd-android-1.8.0-arm64.apk"]),
                "1.8.0-rc.1",
                Some(ARM64)
            )
            .unwrap()
            .update_available
        );
    }

    #[test]
    fn drafts_and_prereleases_are_excluded() {
        let mut draft = release("v1.8.0", &["ahd-android-1.8.0-arm64.apk"]);
        draft["draft"] = json!(true);
        assert_eq!(
            run(&draft, "1.7.0", Some(ARM64)),
            Err(CheckError::NotStable)
        );
        let mut pre = release("v1.8.0", &["ahd-android-1.8.0-arm64.apk"]);
        pre["prerelease"] = json!(true);
        assert_eq!(run(&pre, "1.7.0", Some(ARM64)), Err(CheckError::NotStable));
        let tagged = release("v1.8.0-rc.1", &["ahd-android-1.8.0-rc.1-arm64.apk"]);
        assert_eq!(
            run(&tagged, "1.7.0", Some(ARM64)),
            Err(CheckError::NotStable)
        );
    }

    #[test]
    fn malformed_answers_are_errors() {
        assert_eq!(
            evaluate("not json", &version("1.7.0"), Some(ARM64)),
            Err(CheckError::Malformed)
        );
        assert_eq!(
            evaluate(
                r#"{"message":"Not Found","url":"https://git.example/api/swagger"}"#,
                &version("1.7.0"),
                Some(ARM64)
            ),
            Err(CheckError::Malformed)
        );
        assert_eq!(
            run(&release("nightly", &[]), "1.7.0", Some(ARM64)),
            Err(CheckError::BadTag("nightly".into()))
        );
        // Optional fields may be null or absent.
        let minimal = json!({"tag_name": "v1.8.0", "body": null});
        let check = run(&minimal, "1.7.0", Some(ARM64)).unwrap();
        assert_eq!((check.release_notes.as_str(), check.asset), ("", None));
    }

    #[test]
    fn built_in_source_points_at_this_repository() {
        assert_eq!(
            split_repository("AttODao/AttOsHoYoDashboard"),
            ("AttODao", "AttOsHoYoDashboard")
        );
        let source = Source {
            base_url: "https://git.example/",
            owner: "o",
            repo: "r",
            token: None,
        };
        assert_eq!(
            source.latest_release_url(),
            "https://git.example/api/v1/repos/o/r/releases/latest"
        );
        if option_env!("AHD_UPDATE_REPOSITORY").is_none()
            && option_env!("AHD_UPDATE_FORGEJO_URL").is_none()
        {
            assert_eq!(
                Source::BUILT_IN.latest_release_url(),
                "https://git.attodao.cc/api/v1/repos/AttODao/AttOsHoYoDashboard/releases/latest"
            );
        }
        assert_eq!(
            Target::current(),
            None,
            "native test builds have no release asset"
        );
        assert_eq!(
            ARM64.asset_name(&version("1.8.0")),
            "ahd-android-1.8.0-arm64.apk"
        );
    }
}
