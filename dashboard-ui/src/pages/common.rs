use super::icons::{Glyph, Icon};
use crate::app::actions::spawn_root;
use crate::app::data::load_page_data;
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::state::AppState;
use dioxus::prelude::*;

/// Localized title for a backend `game_kind`; unknown kinds get a generic name.
pub(super) fn game_name(game: &str, ui: Preferences) -> &'static str {
    match game {
        "genshin" => ui.text("dashboard.genshin_impact"),
        "starrail" => ui.text("dashboard.honkai_star_rail"),
        _ => ui.text("dashboard.other_game"),
    }
}

/// A failed refresh warns that the shown data may be out of date; with nothing shown
/// the same failure only says that the fetch failed (`fetch`). Other problems stay.
pub(super) fn refresh_problem(key: &'static str, shown: bool, fetch: &'static str) -> &'static str {
    if key.ends_with(".refresh_failed") && !shown {
        fetch
    } else {
        key
    }
}

/// A dashboard section reads as loading while its request runs and also while it is
/// still queued behind the other dashboard reads: no data and no problem yet.
pub(super) fn pending(loading: bool, shown: bool, failed: bool) -> bool {
    loading || (!shown && !failed)
}

#[component]
pub(super) fn Field(id: String, label: String, children: Element) -> Element {
    rsx! { div { class: "field", label { r#for: "{id}", "{label}" } {children} } }
}

#[component]
pub(super) fn Metric(label: &'static str, value: String) -> Element {
    rsx! { div { class: "metric", dt { "{label}" } dd { "{value}" } } }
}

/// A plain section with a title; wraps settings subsections.
#[component]
pub(super) fn Card(title: &'static str, children: Element) -> Element {
    rsx! {
        section { class: "panel page-stack",
            h2 { "{title}" }
            {children}
        }
    }
}

/// A failed load with the means to try again: re-requests the current page's data.
#[component]
pub(super) fn LoadError(state: Signal<AppState>, message: &'static str) -> Element {
    let ui = use_preferences();
    rsx! {
        div { class: "warning load-error", role: "alert",
            span { "{message}" }
            button { r#type: "button", onclick: move |_| {
                let page = state.read().page;
                spawn_root(async move { load_page_data(state, page).await; });
            }, {ui.text("common.retry")} }
        }
    }
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
    let icon = if shown() { Icon::Hide } else { Icon::Eye };
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
                Glyph { icon }
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

/// Browser DOM focus never evaluates JavaScript; the app passes the internal element ID
/// JSON-encoded to `document::eval` (see `mobile::dom`).
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
    #[cfg(feature = "mobile")]
    crate::mobile::dom::focus(id);
    let _ = id;
}

/// Scrolls the parent of `id` sideways just enough to show it, without moving the page
/// vertically (the app's settings tab bar scrolls horizontally).
pub(super) fn reveal_horizontally(id: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        && let Some(parent) = element.parent_element()
    {
        let (item, bar) = (
            element.get_bounding_client_rect(),
            parent.get_bounding_client_rect(),
        );
        if item.left() < bar.left() {
            parent.set_scroll_left(parent.scroll_left() - (bar.left() - item.left()).ceil() as i32);
        } else if item.right() > bar.right() {
            parent
                .set_scroll_left(parent.scroll_left() + (item.right() - bar.right()).ceil() as i32);
        }
    }
    #[cfg(feature = "mobile")]
    crate::mobile::dom::reveal_horizontally(id);
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
    #[cfg(feature = "mobile")]
    crate::mobile::dom::set_checked(id, checked);
    let _ = (id, checked);
}

/// Native modal dialog with a labelled heading; Escape, backdrop, the app's Back key and
/// focus restoration come from `use_modal`. `back_closes: false` keeps it open on Back
/// while it must stay (the app only; the browser's Back is unchanged).
#[component]
pub(super) fn ModalDialog(
    id: String,
    title: &'static str,
    onclose: EventHandler<()>,
    #[props(default = true)] back_closes: bool,
    children: Element,
) -> Element {
    let title_id = format!("{id}-title");
    use_modal(id.clone(), true, back_closes, onclose);
    rsx! {
        dialog { id, class: "modal-dialog", "aria-labelledby": title_id.clone(),
            div { class: "form-stack", h2 { id: title_id, "{title}" } {children} }
        }
    }
}

/// A yes/no confirmation in the shared native dialog.
#[component]
pub(super) fn ConfirmDialog(
    id: String,
    title: &'static str,
    message: String,
    confirm: String,
    #[props(default)] danger: bool,
    onconfirm: EventHandler<()>,
    onclose: EventHandler<()>,
) -> Element {
    let ui = use_preferences();
    rsx! {
        ModalDialog { id, title, onclose,
            p { "{message}" }
            div { class: "actions",
                crate::components::Button { variant: crate::components::ButtonVariant::Secondary, r#type: "button", onclick: move |_| onclose.call(()), {ui.text("admin.cancel")} }
                crate::components::Button {
                    variant: if danger { crate::components::ButtonVariant::Destructive } else { crate::components::ButtonVariant::Primary },
                    r#type: "button", onclick: move |_| onconfirm.call(()), "{confirm}"
                }
            }
        }
    }
}

/// Own the native modal, its browser listeners, scroll lock and focus restoration. In the app
/// it is also the topmost entry for Android's Back key (`mobile::back`) while `back_closes`.
pub(super) fn use_modal(id: String, active: bool, back_closes: bool, onclose: EventHandler<()>) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = back_closes;
        let modal = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None::<Modal>)));
        use_effect(use_reactive!(|active, id| {
            let mut modal = modal.borrow_mut();
            if active {
                if modal.is_none() {
                    let opened = Modal::open(&id, onclose);
                    let failed = opened.is_none();
                    *modal = opened;
                    drop(modal);
                    // The dialog could not be shown (not in the document, or already open
                    // without being modal): hand the state back, so the caller's open flag
                    // never claims a dialog that is not there and the next press retries.
                    if failed {
                        onclose.call(());
                    }
                }
            } else {
                modal.take();
            }
        }));
    }
    #[cfg(feature = "mobile")]
    {
        let modal = use_hook(|| {
            std::rc::Rc::new(std::cell::RefCell::new(None::<crate::mobile::dom::Modal>))
        });
        // Updated while rendering, not in an effect: a Back press right after the state
        // changed must already see the new rule.
        if let Some(open) = modal.borrow().as_ref() {
            open.set_back_closes(back_closes);
        }
        use_effect(use_reactive!(|active, id, back_closes| {
            let mut modal = modal.borrow_mut();
            if active {
                if modal.is_none() {
                    *modal = Some(crate::mobile::dom::Modal::open(&id, back_closes, onclose));
                }
            } else {
                modal.take();
            }
        }));
    }
    #[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
    let _ = (id, active, back_closes, onclose);
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
    use super::{Preferences, game_name, outside_bounds, pending, refresh_problem};

    #[test]
    fn sections_without_data_or_a_problem_read_as_loading() {
        assert!(pending(true, true, false), "a refresh in flight");
        assert!(
            pending(false, false, false),
            "queued behind the other reads"
        );
        assert!(!pending(false, true, false), "data shown");
        assert!(!pending(false, false, true), "the problem is shown instead");
    }

    #[test]
    fn a_failed_refresh_without_shown_data_only_says_the_fetch_failed() {
        for (refresh, fetch) in [
            ("dashboard.refresh_failed", "dashboard.status_fetch_failed"),
            ("history.refresh_failed", "history.fetch_failed"),
            ("challenges.refresh_failed", "challenges.fetch_failed"),
            ("calendar.refresh_failed", "calendar.fetch_failed"),
        ] {
            assert_eq!(refresh_problem(refresh, false, fetch), fetch);
            assert_eq!(refresh_problem(refresh, true, fetch), refresh);
        }
        assert_eq!(
            refresh_problem(
                "dashboard.status_identity_mismatch",
                false,
                "dashboard.status_fetch_failed"
            ),
            "dashboard.status_identity_mismatch"
        );
    }

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
