use crate::app::preferences::{Preferences, use_preferences};
use dioxus::prelude::*;

/// Localized title for a backend `game_kind`; unknown kinds get a generic name.
pub(super) fn game_name(game: &str, ui: Preferences) -> &'static str {
    match game {
        "genshin" => ui.text("dashboard.genshin_impact"),
        "starrail" => ui.text("dashboard.honkai_star_rail"),
        _ => ui.text("dashboard.other_game"),
    }
}

#[component]
pub(super) fn Field(id: String, label: String, children: Element) -> Element {
    rsx! { div { class: "field", label { r#for: "{id}", "{label}" } {children} } }
}

#[component]
pub(super) fn Metric(label: &'static str, value: String) -> Element {
    rsx! { div { class: "metric", dt { "{label}" } dd { "{value}" } } }
}

#[component]
pub(super) fn Icon(name: &'static str) -> Element {
    let path = match name {
        "close" => "M6 6l12 12M6 18 18 6",
        "check" => "M5 12.5l4.5 4.5L19 7.5",
        "link" => {
            "M10 13a4 4 0 0 0 6 0l4-4a4 4 0 0 0-6-6l-2 2M14 11a4 4 0 0 0-6 0l-4 4a4 4 0 0 0 6 6l2-2"
        }
        "settings" => "M4 7h16M4 17h16M8 4v6M16 14v6",
        "user" => "M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0M4 21v-2a8 8 0 0 1 16 0v2",
        "logout" => "M10 4H4v16h6M9 12h12m-4-4 4 4-4 4",
        "eye" => {
            "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0"
        }
        "hide" => {
            "M3 3l18 18M9.5 5.3A12 12 0 0 1 12 5c6.5 0 10 7 10 7a18 18 0 0 1-4 4.7M6 6.5A19 19 0 0 0 2 12s3.5 7 10 7a12 12 0 0 0 5-1.1M10 10a3 3 0 0 0 4 4"
        }
        _ => "M4 6h16M4 12h16M4 18h16",
    };
    rsx! { svg { view_box: "0 0 24 24", "aria-hidden": "true", "focusable": "false", path { d: path } } }
}

#[component]
pub(super) fn PasswordInput(
    id: String,
    mut value: Signal<String>,
    autocomplete: String,
    /// Space-separated IDs of hint and error text for this control.
    #[props(default)]
    describedby: Option<String>,
    #[props(default)] invalid: bool,
    #[props(default)] onblur: Option<EventHandler<()>>,
) -> Element {
    let ui = use_preferences();
    let preferences = use_context::<Signal<Preferences>>();
    let mut shown = use_signal(|| false);
    use_effect(move || {
        let _ = *preferences.read();
        shown.set(false);
    });
    use_effect(move || {
        if value.read().is_empty() {
            shown.set(false);
        }
    });
    #[cfg(target_arch = "wasm32")]
    let _listener = {
        let control_id = id.clone();
        use_hook(move || mask_on_visibility_change(shown, control_id))
    };
    let input_type = if shown() { "text" } else { "password" };
    let icon_name = if shown() { "hide" } else { "eye" };
    let eye_label = if shown() {
        ui.text("common.hide_password")
    } else {
        ui.text("common.show_password")
    };
    rsx! {
        div { class: "password-control",
            input { id: id.clone(), r#type: input_type, autocomplete: "{autocomplete}",
                "aria-describedby": describedby, "aria-invalid": invalid.then_some("true"),
                value: "{value}", oninput: move |event| value.set(event.value()),
                onblur: move |_| if let Some(handler) = &onblur { handler.call(()) },
            }
            button { class: "password-eye", r#type: "button", "aria-controls": id,
                "aria-label": eye_label,
                "aria-pressed": shown().to_string(), onclick: move |_| shown.toggle(),
                Icon { name: icon_name }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct VisibilityMask {
    window: web_sys::Window,
    document: web_sys::Document,
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
    focus_callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for VisibilityMask {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        let callback = self.callback.as_ref().unchecked_ref();
        let _ = self
            .window
            .remove_event_listener_with_callback("blur", callback);
        let _ = self
            .document
            .remove_event_listener_with_callback("visibilitychange", callback);
        let _ = self.document.remove_event_listener_with_callback(
            "focusout",
            self.focus_callback.as_ref().unchecked_ref(),
        );
    }
}

#[cfg(target_arch = "wasm32")]
fn mask_on_visibility_change(
    mut shown: Signal<bool>,
    id: String,
) -> Option<std::rc::Rc<VisibilityMask>> {
    use wasm_bindgen::JsCast;
    let window = web_sys::window()?;
    let document = window.document()?;
    let callback =
        wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| shown.set(false));
    if window
        .add_event_listener_with_callback("blur", callback.as_ref().unchecked_ref())
        .is_err()
    {
        return None;
    }
    if document
        .add_event_listener_with_callback("visibilitychange", callback.as_ref().unchecked_ref())
        .is_err()
    {
        let _ =
            window.remove_event_listener_with_callback("blur", callback.as_ref().unchecked_ref());
        return None;
    }
    let focus_document = document.clone();
    let focus_callback = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
        move |event: web_sys::Event| {
            let inside = focus_document
                .get_element_by_id(&id)
                .and_then(|input| input.parent_element())
                .is_some_and(|control| {
                    let target = event
                        .dyn_ref::<web_sys::FocusEvent>()
                        .and_then(|event| event.related_target())
                        .and_then(|target| target.dyn_into::<web_sys::Node>().ok());
                    control.contains(target.as_ref())
                });
            if !inside {
                shown.set(false);
            }
        },
    );
    let listener = std::rc::Rc::new(VisibilityMask {
        window,
        document,
        callback,
        focus_callback,
    });
    if listener
        .document
        .add_event_listener_with_callback(
            "focusout",
            listener.focus_callback.as_ref().unchecked_ref(),
        )
        .is_err()
    {
        return None;
    }
    Some(listener)
}

/// DOM focus never evaluates JavaScript; callers supply internal element IDs.
pub(super) fn focus_element(id: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(id))
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    }
    let _ = id;
}

/// A controlled checkbox only re-renders when its stored value changes, so a
/// rejected toggle would stay flipped in the DOM. Write the stored value back.
pub(super) fn reset_checked(id: &str, checked: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(input) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(id))
            .and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok())
        {
            input.set_checked(checked);
        }
    }
    let _ = (id, checked);
}

/// Native modal dialog with a labelled heading; Escape, backdrop and focus
/// restoration come from `use_modal`.
#[component]
pub(super) fn ModalDialog(
    id: String,
    title: &'static str,
    onclose: EventHandler<()>,
    children: Element,
) -> Element {
    let title_id = format!("{id}-title");
    use_modal(id.clone(), true, onclose);
    rsx! {
        dialog { id, class: "modal-dialog", "aria-labelledby": title_id.clone(),
            div { class: "form-stack", h2 { id: title_id, "{title}" } {children} }
        }
    }
}

/// Own the native modal, its browser listeners, scroll lock and focus restoration.
pub(super) fn use_modal(id: String, active: bool, onclose: EventHandler<()>) {
    #[cfg(target_arch = "wasm32")]
    {
        let modal = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None::<Modal>)));
        use_effect(use_reactive!(|active, id| {
            let mut modal = modal.borrow_mut();
            if active {
                if modal.is_none() {
                    *modal = Modal::open(&id, onclose);
                }
            } else {
                modal.take();
            }
        }));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (id, active, onclose);
}

#[cfg(target_arch = "wasm32")]
struct Modal {
    dialog: web_sys::HtmlDialogElement,
    previous_focus: Option<web_sys::HtmlElement>,
    styles: Vec<(web_sys::CssStyleDeclaration, &'static str, String, String)>,
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl Modal {
    fn open(id: &str, onclose: EventHandler<()>) -> Option<Self> {
        use wasm_bindgen::JsCast;
        let window = web_sys::window()?;
        let document = window.document()?;
        let dialog = document
            .get_element_by_id(id)?
            .dyn_into::<web_sys::HtmlDialogElement>()
            .ok()?;
        let previous_focus = document.active_element().and_then(|e| e.dyn_into().ok());
        dialog.show_modal().ok()?;
        let target_dialog = dialog.clone();
        let callback = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
            move |event: web_sys::Event| {
                match event.type_().as_str() {
                    "cancel" => {
                        event.prevent_default();
                        onclose.call(());
                    }
                    "close" => onclose.call(()),
                    "click" => {
                        // Backdrop clicks target the dialog; descendants must never dismiss it.
                        if event.target().as_ref() != Some(target_dialog.as_ref()) {
                            return;
                        }
                        if let Some(mouse) = event.dyn_ref::<web_sys::MouseEvent>() {
                            let rect = target_dialog.get_bounding_client_rect();
                            if outside_bounds(
                                f64::from(mouse.client_x()),
                                f64::from(mouse.client_y()),
                                rect.left(),
                                rect.top(),
                                rect.right(),
                                rect.bottom(),
                            ) {
                                onclose.call(());
                            }
                        }
                    }
                    _ => {}
                }
            },
        );
        let mut modal = Self {
            dialog,
            previous_focus,
            styles: Vec::new(),
            callback,
        };
        for event in ["cancel", "close", "click"] {
            if modal
                .dialog
                .add_event_listener_with_callback(event, modal.callback.as_ref().unchecked_ref())
                .is_err()
            {
                return None;
            }
        }
        if let (Some(body), Some(root)) = (
            document.body(),
            document
                .document_element()
                .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok()),
        ) {
            let gutter = window
                .inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or_default()
                - f64::from(root.client_width());
            if gutter > 0.0 {
                let padding = window
                    .get_computed_style(&body)
                    .ok()
                    .flatten()
                    .and_then(|style| style.get_property_value("padding-right").ok())
                    .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
                    .unwrap_or_default();
                modal.set_style(
                    body.style(),
                    "padding-right",
                    &format!("{}px", padding + gutter),
                );
            }
            modal.set_style(body.style(), "overflow", "hidden");
            modal.set_style(root.style(), "overflow", "hidden");
        }
        Some(modal)
    }

    fn set_style(&mut self, style: web_sys::CssStyleDeclaration, name: &'static str, value: &str) {
        let previous = style.get_property_value(name).unwrap_or_default();
        let priority = style.get_property_priority(name);
        self.styles.push((style.clone(), name, previous, priority));
        let _ = style.set_property(name, value);
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Modal {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        for event in ["cancel", "close", "click"] {
            let _ = self
                .dialog
                .remove_event_listener_with_callback(event, self.callback.as_ref().unchecked_ref());
        }
        self.dialog.close();
        for (style, name, value, priority) in self.styles.iter().rev() {
            if value.is_empty() {
                let _ = style.remove_property(name);
            } else {
                let _ = style.set_property_with_priority(name, value, priority);
            }
        }
        if let Some(element) = &self.previous_focus
            && element.is_connected()
        {
            let _ = element.focus();
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn outside_bounds(x: f64, y: f64, left: f64, top: f64, right: f64, bottom: f64) -> bool {
    x < left || x > right || y < top || y > bottom
}

#[cfg(test)]
mod tests {
    use super::{Preferences, game_name, outside_bounds};

    #[test]
    fn game_names_are_localized_and_unknown_kinds_get_a_generic_name() {
        let ui = Preferences::default();
        assert_eq!(
            game_name("genshin", ui),
            ui.text("dashboard.genshin_impact")
        );
        assert_eq!(
            game_name("starrail", ui),
            ui.text("dashboard.honkai_star_rail")
        );
        assert_eq!(game_name("zzz", ui), ui.text("dashboard.other_game"));
    }

    #[test]
    fn backdrop_coordinates_exclude_dialog_and_its_edges() {
        assert!(!outside_bounds(10.0, 20.0, 10.0, 20.0, 100.0, 200.0));
        assert!(!outside_bounds(100.0, 200.0, 10.0, 20.0, 100.0, 200.0));
        assert!(outside_bounds(9.0, 50.0, 10.0, 20.0, 100.0, 200.0));
        assert!(outside_bounds(50.0, 201.0, 10.0, 20.0, 100.0, 200.0));
    }
}
