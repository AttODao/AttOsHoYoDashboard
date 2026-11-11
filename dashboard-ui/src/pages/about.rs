//! About page: author, version, licenses and the source repository, read from Cargo
//! metadata at build time. Author, license and repository come from the UI crate and are
//! the same in web and app. The version is per distribution (docs/versioning.md): the app
//! shows its own version (`dashboard-ui/Cargo.toml`, release tag `vX.Y.Z`), the web build
//! shows the web version (the server crate in the root `Cargo.toml`, tag `web-vX.Y.Z`).

use super::icons::{Glyph, Icon};
use crate::app::preferences::use_preferences;
use dioxus::prelude::*;

const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
/// The app version: the UI crate is what the APK is built from.
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The root manifest (server crate), whose `[package] version` is the web version.
const WEB_MANIFEST: &str = include_str!("../../../Cargo.toml");
/// The last release whose `vX.Y.Z` tag shipped web and app together; later web
/// releases are tagged `web-vX.Y.Z`.
const LAST_SHARED_RELEASE: (u64, u64, u64) = (1, 8, 0);
const LICENSE: &str = env!("CARGO_PKG_LICENSE");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Third-party work shipped in the UI: name, SPDX license expression, upstream URL.
const THIRD_PARTY: [(&str, &str, &str); 4] = [
    (
        "Dioxus",
        "MIT OR Apache-2.0",
        "https://github.com/DioxusLabs/dioxus",
    ),
    (
        "Dioxus Components",
        "MIT",
        "https://github.com/DioxusLabs/components",
    ),
    (
        "Noto Sans JP",
        "OFL-1.1",
        "https://github.com/google/fonts/tree/main/ofl/notosansjp",
    ),
    (
        "Orbitron",
        "OFL-1.1",
        "https://github.com/theleagueof/orbitron",
    ),
];

/// `version = "…"` of the `[package]` table; other tables' `version` keys are ignored.
fn package_version(manifest: &str) -> Option<&str> {
    let mut in_package = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "version" {
            return value
                .trim()
                .strip_prefix('"')?
                .split_once('"')
                .map(|(v, _)| v);
        }
    }
    None
}

/// The version of the running distribution: the app's own or the web's.
fn version(app: bool) -> &'static str {
    if app {
        APP_VERSION
    } else {
        package_version(WEB_MANIFEST).unwrap_or("?")
    }
}

/// `MAJOR.MINOR.PATCH` of a version, ignoring any prerelease or build suffix.
fn version_core(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(Some(major)), Some(Some(minor)), Some(Some(patch)), None) => {
            Some((major, minor, patch))
        }
        _ => None,
    }
}

/// The tag that released this version of the distribution.
fn release_tag(app: bool, version: &str) -> String {
    let shared = version_core(version).is_some_and(|core| core <= LAST_SHARED_RELEASE);
    if app || shared {
        format!("v{version}")
    } else {
        format!("web-v{version}")
    }
}

/// Cargo joins several authors with `:`.
fn authors(raw: &str) -> String {
    raw.split(':')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The repository URL without scheme or trailing slash, as link text.
fn repository_label(url: &str) -> &str {
    let url = url.trim_end_matches('/');
    url.strip_prefix("https://").unwrap_or(url)
}

/// Cargo.lock at the release tag of this build.
fn lockfile_url(repository: &str, tag: &str) -> String {
    format!(
        "{}/src/tag/{tag}/Cargo.lock",
        repository.trim_end_matches('/')
    )
}

/// The license text on the default branch (tags up to v1.4.0 predate the LICENSE file).
fn license_url(repository: &str) -> String {
    format!(
        "{}/src/branch/main/LICENSE",
        repository.trim_end_matches('/')
    )
}

#[component]
pub(super) fn AboutPage() -> Element {
    let ui = use_preferences();
    let app = crate::platform::APP;
    let version = version(app);
    let tag = release_tag(app, version);
    rsx! {
        div { class: "page-stack about-page",
            h1 { {ui.text("about.title")} }
            section { class: "panel",
                dl { class: "about-facts",
                    div { dt { {ui.text("about.author")} } dd { {authors(AUTHORS)} } }
                    div { dt { {ui.text("about.version")} } dd { "{version}" } }
                    div { dt { {ui.text("about.license")} }
                        dd {
                            a { href: license_url(REPOSITORY), target: "_blank", rel: "noopener noreferrer",
                                "{LICENSE}"
                                Glyph { icon: Icon::External }
                            }
                        }
                    }
                    div { dt { {ui.text("about.source")} }
                        dd {
                            a { href: REPOSITORY, target: "_blank", rel: "noopener noreferrer",
                                for (index, part) in repository_label(REPOSITORY).split('/').enumerate() {
                                    if index > 0 {
                                        "/"
                                        wbr {}
                                    }
                                    "{part}"
                                }
                                Glyph { icon: Icon::External }
                            }
                        }
                    }
                }
            }
            section { class: "panel",
                h2 { {ui.text("about.third_party")} }
                ul { class: "item-list about-licenses",
                    for (name, license, url) in THIRD_PARTY {
                        li {
                            a { href: url, target: "_blank", rel: "noopener noreferrer", "{name}" }
                            span { class: "muted", "{license}" }
                        }
                    }
                }
                p { class: "muted", {ui.text("about.crates")} }
                p { class: "about-lockfile",
                    a { href: lockfile_url(REPOSITORY, &tag), target: "_blank", rel: "noopener noreferrer",
                        {ui.text("about.open_cargo_lock")}
                        Glyph { icon: Icon::External }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authors_are_joined_and_read_from_metadata() {
        assert_eq!(authors("AttODao"), "AttODao");
        assert_eq!(authors("A:B"), "A, B");
        assert_eq!(authors(AUTHORS), "AttODao");
    }
    #[test]
    fn project_license_is_the_declared_gpl() {
        assert_eq!(LICENSE, "GPL-3.0-or-later");
    }
    #[test]
    fn repository_links_and_labels_use_build_metadata() {
        assert_eq!(
            REPOSITORY,
            "https://git.attodao.cc/AttODao/AttOsHoYoDashboard"
        );
        assert_eq!(
            repository_label(REPOSITORY),
            "git.attodao.cc/AttODao/AttOsHoYoDashboard"
        );
        assert_eq!(repository_label("https://x.example/r/"), "x.example/r");
        assert_eq!(
            lockfile_url("https://x.example/r/", "web-v1.2.3"),
            "https://x.example/r/src/tag/web-v1.2.3/Cargo.lock"
        );
        assert_eq!(
            license_url("https://x.example/r/"),
            "https://x.example/r/src/branch/main/LICENSE"
        );
    }
    #[test]
    fn package_version_reads_only_the_package_table() {
        let manifest = r#"
[workspace]
members = ["dashboard-ui"]

[dependencies]
chrono = { version = "0.4" }
version = "9.9.9"

[package]
name = "x"
version = "1.8.1" # web
edition = "2024"

[dev-dependencies]
version = "0.0.1"
"#;
        assert_eq!(package_version(manifest), Some("1.8.1"));
        assert_eq!(
            package_version("[package]\nversion.workspace = true\n"),
            None
        );
        assert_eq!(
            package_version("[dependencies]\nversion = \"1.0.0\"\n"),
            None
        );
        assert_eq!(package_version(""), None);
    }
    #[test]
    fn app_shows_the_app_version_and_web_the_root_manifest_version() {
        assert_eq!(version(true), env!("CARGO_PKG_VERSION"));
        let web = version(false);
        assert!(
            version_core(web).is_some(),
            "root Cargo.toml version {web:?}"
        );
        assert_eq!(
            Some(web),
            include_str!("../../../Cargo.toml")
                .lines()
                .skip_while(|line| *line != "[package]")
                .find_map(|line| line.strip_prefix("version = \""))
                .and_then(|rest| rest.strip_suffix('"'))
        );
    }
    #[test]
    fn release_tags_follow_the_distribution() {
        assert_eq!(release_tag(true, "1.9.0"), "v1.9.0");
        assert_eq!(release_tag(true, "1.8.0"), "v1.8.0");
        assert_eq!(release_tag(false, "1.8.1"), "web-v1.8.1");
        assert_eq!(release_tag(false, "2.0.0"), "web-v2.0.0");
        // Up to 1.8.0 one `v` tag released both; there is no `web-v1.8.0`.
        assert_eq!(release_tag(false, "1.8.0"), "v1.8.0");
        assert_eq!(release_tag(false, "1.7.1"), "v1.7.1");
        assert_eq!(version_core("1.10.0-rc.1"), Some((1, 10, 0)));
        assert_eq!(version_core("1.8"), None);
        assert_eq!(version_core("?"), None);
    }
    #[test]
    fn third_party_licenses_have_secure_upstream_links() {
        for (_, license, url) in THIRD_PARTY {
            assert!(url.starts_with("https://"));
            assert!(!license.is_empty());
        }
    }
}
