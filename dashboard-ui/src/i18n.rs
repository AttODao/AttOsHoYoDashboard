//! Embedded UI catalogs. State stores message keys; rendering selects the current language.
use std::{collections::BTreeMap, sync::LazyLock};

type Catalog = BTreeMap<String, String>;

static JAPANESE: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../locales/ja.json")).expect("valid Japanese UI catalog")
});
static ENGLISH: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../locales/en.json")).expect("valid English UI catalog")
});

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Language {
    #[default]
    Japanese,
    English,
}

impl Language {
    pub(crate) fn text(self, key: &str) -> &'static str {
        let catalog = match self {
            Self::Japanese => &*JAPANESE,
            Self::English => &*ENGLISH,
        };
        lookup(catalog, &ENGLISH, key)
    }

    pub(crate) fn format(self, key: &str, arguments: &[(&str, &str)]) -> String {
        // Read only the template, never reinterpret braces inside inserted user data.
        let mut remaining = self.text(key);
        let mut output = String::new();
        while let Some((prefix, rest)) = remaining.split_once('{') {
            output.push_str(prefix);
            let Some((name, suffix)) = rest.split_once('}') else {
                output.push('{');
                remaining = rest;
                break;
            };
            if let Some((_, value)) = arguments.iter().find(|(key, _)| *key == name) {
                output.push_str(value);
            } else {
                output.push('{');
                output.push_str(name);
                output.push('}');
            }
            remaining = suffix;
        }
        output.push_str(remaining);
        output
    }
}

fn lookup<'a>(catalog: &'a Catalog, fallback: &'a Catalog, key: &str) -> &'a str {
    catalog
        .get(key)
        .or_else(|| fallback.get(key))
        .unwrap_or(&fallback["messages.unavailable"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, fs, path::Path};

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .map(|part| part.split_once('}').expect("closed placeholder").0)
            .collect()
    }

    #[test]
    fn catalogs_have_matching_nonempty_messages_and_placeholders() {
        assert_eq!(
            JAPANESE.keys().collect::<Vec<_>>(),
            ENGLISH.keys().collect::<Vec<_>>()
        );
        for (key, english) in &*ENGLISH {
            let japanese = &JAPANESE[key];
            assert!(!english.trim().is_empty(), "{key}");
            assert!(!japanese.trim().is_empty(), "{key}");
            assert_eq!(placeholders(japanese), placeholders(english), "{key}");
        }
    }

    #[test]
    fn missing_translation_falls_back_without_exposing_unknown_input() {
        let empty = Catalog::new();
        assert_eq!(lookup(&empty, &ENGLISH, "app.title"), "AGD");
        assert_eq!(
            Language::Japanese.text("untrusted-input"),
            ENGLISH["messages.unavailable"]
        );
    }

    #[test]
    fn interpolation_does_not_reinterpret_values_or_drop_missing_arguments() {
        assert_eq!(
            Language::Japanese.format("docs.version", &[("version", "1.2")]),
            "バージョン 1.2"
        );
        assert_eq!(
            Language::English.format("docs.version", &[("version", "{version}<script>")]),
            "Version {version}<script>"
        );
        assert_eq!(
            Language::English.format("docs.version", &[]),
            "Version {version}"
        );
    }

    #[test]
    fn stored_status_keys_translate_again_after_switching_language() {
        let key = "messages.the_username_or_password_is_incorrect";
        assert_eq!(
            Language::Japanese.text(key),
            "ユーザー名かパスワードが違います"
        );
        assert_eq!(
            Language::English.text(key),
            "The username or password is incorrect."
        );
        let key = "rewards.run_accepted_queued_not_completed";
        assert_eq!(
            Language::Japanese.text(key),
            "実行を受け付けました（処理待ち）"
        );
        assert_eq!(
            Language::English.text(key),
            "Run accepted; queued, not completed."
        );
    }

    #[test]
    fn application_message_keys_exist_and_copy_stays_out_of_rust() {
        fn check_sources(path: &Path, used: &mut BTreeSet<String>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    check_sources(&path, used);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let source = fs::read_to_string(&path).unwrap();
                    let production = source.split("#[cfg(test)]").next().unwrap();
                    // Keys have a catalog namespace; API paths, CSS and user data do not.
                    for literal in production.split('"').skip(1).step_by(2) {
                        let namespace = literal.split_once('.').map(|(prefix, _)| prefix);
                        if namespace.is_some_and(|prefix| {
                            ENGLISH
                                .keys()
                                .any(|key| key.starts_with(&format!("{prefix}.")))
                        }) {
                            assert!(
                                ENGLISH.contains_key(literal),
                                "{}: {literal}",
                                path.display()
                            );
                            used.insert(literal.to_owned());
                        }
                        assert!(
                            !literal
                                .chars()
                                .any(|ch| ('\u{3040}'..='\u{30ff}').contains(&ch)),
                            "UI copy in {}: {literal}",
                            path.display()
                        );
                    }
                }
            }
        }
        let mut used = BTreeSet::new();
        check_sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut used,
        );
        for key in ENGLISH.keys() {
            assert!(used.contains(key), "unused catalog key: {key}");
        }
    }
}
