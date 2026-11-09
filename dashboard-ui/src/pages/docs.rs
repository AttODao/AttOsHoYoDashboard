use super::shell::{CONTENT_ID, Header, SkipLink, Status};
use crate::app::data::load_docs;
use crate::app::preferences::use_preferences;
use crate::app::state::AppState;
use crate::components::{Card, CardContent, CardDescription, CardHeader};
use dioxus::prelude::*;
use serde_json::Value;

#[component]
pub(in crate::app) fn DocsPage(mut state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    use_future(move || async move {
        load_docs(state).await;
    });
    let doc = state.read().openapi.clone();
    rsx! {
        SkipLink {}
        Header { state }
        main { class: "container docs-page", id: CONTENT_ID, tabindex: "-1",
            Status { state }
            div { class: "page-stack",
                h1 { {ui.text("docs.title")} }
                Card {
                    CardHeader { CardDescription { {ui.text("docs.description")} } }
                    CardContent {
                        if let Some(error) = &state.read().openapi_error { p { class: "warning", {ui.text(error)} } }
                        if let Some(doc) = doc {
                            OpenApiSummary { doc }
                        } else {
                            p { {ui.text("docs.loading")} }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn OpenApiSummary(doc: Value) -> Element {
    let ui = use_preferences();
    let title = doc
        .pointer("/info/title")
        .and_then(Value::as_str)
        .unwrap_or(ui.text("docs.default_title"))
        .to_owned();
    let version = doc
        .pointer("/info/version")
        .and_then(Value::as_str)
        .unwrap_or(ui.text("docs.unknown_version"))
        .to_owned();
    let paths: Vec<(String, Vec<String>)> = doc
        .get("paths")
        .and_then(Value::as_object)
        .map(|map| {
            let mut rows: Vec<_> = map
                .iter()
                .map(|(path, methods)| {
                    let methods = methods
                        .as_object()
                        .map(|obj| obj.keys().map(|k| k.to_uppercase()).collect())
                        .unwrap_or_default();
                    (path.clone(), methods)
                })
                .collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            rows
        })
        .unwrap_or_default();
    rsx! {
        div { class: "docs-summary",
            h2 { "{title}" }
            p { {ui.format("docs.version", &[("version", &version)])} }
            div { class: "table-wrap",
                table {
                    thead { tr { th { {ui.text("docs.path")} } th { {ui.text("docs.methods")} } } }
                    tbody { for (path, methods) in paths { tr { td { code { "{path}" } } td { {methods.join(", ")} } } } }
                }
            }
        }
    }
}
