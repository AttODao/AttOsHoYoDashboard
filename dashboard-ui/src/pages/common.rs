use dioxus::prelude::*;

#[component]
pub(super) fn Field(id: String, label: String, children: Element) -> Element {
    rsx! { div { class: "field", label { r#for: "{id}", "{label}" } {children} } }
}

#[component]
pub(super) fn Metric(label: &'static str, value: String) -> Element {
    rsx! { div { class: "metric", dt { "{label}" } dd { "{value}" } } }
}
