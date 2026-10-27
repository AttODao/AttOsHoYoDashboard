//! About page: author, version, licenses and the source repository, read from the
//! UI crate's Cargo metadata at build time (web and app share the same values).

use super::common::Icon;
use crate::app::preferences::use_preferences;
use dioxus::prelude::*;

const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const VERSION: &str = env!("CARGO_PKG_VERSION");
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

/// Cargo joins several authors with `:`.
fn authors(raw: &str) -> String {
    raw.split(':')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The project license from Cargo metadata; `None` while the package declares none.
fn project_license(raw: &str) -> Option<&str> {
    let license = raw.trim();
    (!license.is_empty()).then_some(license)
}

/// The repository URL without scheme or trailing slash, as link text.
fn repository_label(url: &str) -> &str {
    let url = url.trim_end_matches('/');
    url.strip_prefix("https://").unwrap_or(url)
}

/// Cargo.lock at the release tag of this build.
fn lockfile_url(repository: &str, version: &str) -> String {
    format!(
        "{}/src/tag/v{version}/Cargo.lock",
        repository.trim_end_matches('/')
    )
}

#[component]
pub(super) fn AboutPage() -> Element {
    let ui = use_preferences();
    rsx! {
        div { class: "page-stack about-page",
            h1 { {ui.text("about.title")} }
            section { class: "panel",
                dl { class: "about-facts",
                    div { dt { {ui.text("about.author")} } dd { {authors(AUTHORS)} } }
                    div { dt { {ui.text("about.version")} } dd { "{VERSION}" } }
                    div { dt { {ui.text("about.license")} }
                        dd {
                            if let Some(license) = project_license(LICENSE) {
                                "{license}"
                            } else {
                                span { class: "muted about-unspecified", {ui.text("about.license_unspecified")} }
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
                                Icon { name: "external" }
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
                    a { href: lockfile_url(REPOSITORY, VERSION), target: "_blank", rel: "noopener noreferrer",
                        {ui.text("about.open_cargo_lock")}
                        Icon { name: "external" }
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
    fn unspecified_project_license_is_not_invented() {
        assert_eq!(project_license(""), None);
        assert_eq!(project_license(" MIT "), Some("MIT"));
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
            lockfile_url("https://x.example/r/", "1.2.3"),
            "https://x.example/r/src/tag/v1.2.3/Cargo.lock"
        );
        assert!(!VERSION.is_empty());
    }
    #[test]
    fn third_party_licenses_have_secure_upstream_links() {
        for (_, license, url) in THIRD_PARTY {
            assert!(url.starts_with("https://"));
            assert!(!license.is_empty());
        }
    }
}
