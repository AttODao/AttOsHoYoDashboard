//! Horizontal swipes between tabs on touch screens, in the browser and in the app.
//!
//! [`SwipeableTabs`] wraps the tab panels; it does not render the tab bar, so the existing
//! `role="tab"` buttons, keyboard handling and the stored tab state (`AppState` or the
//! `Page` in the URL) stay where they are and a swipe only calls `on_change` like a tap.
//! Policy (thresholds, edge guard, which index) is `app::swipe`; this module collects
//! touches with web-sys in the browser and through `mobile::swipe::SwipeListener` in the app.
//! Only touch events are observed, so mouse drags on a PC never switch tabs. The page keeps
//! native vertical scrolling: listeners are passive and a touch that scrolled the page,
//! started on a form control or inside a horizontally scrollable element, or started at a
//! screen edge (iOS back swipe) is ignored.
use crate::app::swipe::{Gesture, SwipeDirection, classify, step};
use dioxus::prelude::*;
use std::{cell::Cell, rc::Rc};

/// The props a gesture is judged against; updated on every render.
#[derive(Clone, Copy)]
struct Tabs {
    count: usize,
    index: usize,
    enabled: bool,
    on_change: EventHandler<usize>,
}

/// Swipe left for the next tab, right for the previous one. At either end the panel
/// bounces instead. The new panel slides in from the swipe direction unless the user
/// prefers reduced motion (`dashboard.css`, `.swipe-tabs[data-swipe-enter]`).
///
/// `id` must be unique on the page and stay the same while mounted. Pass `enabled: false`
/// where the tabs are not shown (e.g. the wide dashboard shows both games side by side).
/// Mark controls that need horizontal drags with `data-swipe-ignore`; form controls and
/// horizontally scrollable elements (`overflow-x: auto|scroll` with overflowing content)
/// are excluded automatically.
///
/// ```ignore
/// let index = TABS.iter().position(|tab| *tab == active()).unwrap_or(0);
/// rsx! {
///     SwipeableTabs {
///         id: "game-panels", count: TABS.len(), index,
///         on_change: move |next: usize| select(TABS[next]),
///         for tab in TABS { Panel { key: "{tab}", tab, active: active() == tab } }
///     }
/// }
/// ```
#[component]
pub(super) fn SwipeableTabs(
    id: String,
    count: usize,
    index: usize,
    on_change: EventHandler<usize>,
    #[props(default = true)] enabled: bool,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    let mut enter = use_horizontal_swipe_tabs(&id, count, index, enabled, on_change);
    rsx! {
        div {
            id,
            class: if class.is_empty() { "swipe-tabs".to_owned() } else { format!("swipe-tabs {class}") },
            "data-swipe-enabled": if enabled { "true" } else { "false" },
            "data-swipe-enter": enter(),
            onanimationend: move |event: AnimationEvent| {
                if event.animation_name().starts_with("ahd-swipe-") {
                    enter.set(None);
                }
            },
            {children}
        }
    }
}

/// Listens for swipes on the element with `id` and calls `on_change` with the neighbouring
/// tab index. Returns the running enter animation for `data-swipe-enter` (`next`,
/// `previous`, or `end-next`/`end-previous` for the bounce at either end), which the
/// caller clears on `animationend`. The element needs `data-swipe-enabled="true"`.
pub(super) fn use_horizontal_swipe_tabs(
    id: &str,
    count: usize,
    index: usize,
    enabled: bool,
    on_change: EventHandler<usize>,
) -> Signal<Option<&'static str>> {
    let enter = use_signal(|| None);
    let current = Tabs {
        count,
        index,
        enabled,
        on_change,
    };
    let tabs = use_hook(|| Rc::new(Cell::new(current)));
    tabs.set(current);
    let id = id.to_owned();
    let _listener = use_hook(move || {
        let on_gesture = move |gesture: Gesture| {
            let mut enter = enter;
            let tabs = tabs.get();
            if !tabs.enabled {
                return;
            }
            let Some(direction) = classify(&gesture) else {
                return;
            };
            match step(tabs.index, tabs.count, direction) {
                Some(next) => {
                    enter.set(Some(direction.name()));
                    tabs.on_change.call(next);
                }
                None => enter.set(Some(match direction {
                    SwipeDirection::Next => "end-next",
                    SwipeDirection::Previous => "end-previous",
                })),
            }
        };
        listen(id, on_gesture)
    });
    enter
}

#[cfg(feature = "mobile")]
fn listen(
    id: String,
    on_gesture: impl FnMut(Gesture) + 'static,
) -> Option<Rc<crate::mobile::swipe::SwipeListener>> {
    Some(Rc::new(crate::mobile::swipe::SwipeListener::attach(
        id, on_gesture,
    )))
}

#[cfg(all(target_arch = "wasm32", not(feature = "mobile")))]
fn listen(id: String, on_gesture: impl FnMut(Gesture) + 'static) -> Option<Rc<web::SwipeListener>> {
    web::SwipeListener::attach(id, on_gesture)
}

/// Native test builds have no DOM to listen on.
#[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
fn listen(id: String, on_gesture: impl FnMut(Gesture) + 'static) -> Option<Rc<()>> {
    let _ = (id, on_gesture);
    None
}

/// Browser touch collection; the app does the same in JavaScript (`mobile::swipe`).
#[cfg(all(target_arch = "wasm32", not(feature = "mobile")))]
mod web {
    use crate::app::swipe::{Gesture, IGNORE_SELECTOR};
    use std::{cell::RefCell, rc::Rc};
    use wasm_bindgen::{JsCast, closure::Closure};
    use web_sys::{Element, TouchEvent, Window};

    type Callback = Closure<dyn FnMut(TouchEvent)>;

    struct Start {
        x: f64,
        y: f64,
        at: f64,
        width: f64,
        top: f64,
        multi_touch: bool,
    }

    pub(super) struct SwipeListener {
        window: Window,
        start: Callback,
        end: Callback,
    }

    impl SwipeListener {
        pub(super) fn attach(
            id: String,
            on_gesture: impl FnMut(Gesture) + 'static,
        ) -> Option<Rc<Self>> {
            let window = web_sys::window()?;
            let touch: Rc<RefCell<Option<Start>>> = Rc::default();
            let start = {
                let (window, touch) = (window.clone(), touch.clone());
                Callback::new(move |event: TouchEvent| {
                    let mut touch = touch.borrow_mut();
                    if event.touches().length() != 1 {
                        if let Some(start) = touch.as_mut() {
                            start.multi_touch = true;
                        }
                        return;
                    }
                    *touch = begin(&window, &id, &event);
                })
            };
            let end = {
                let (window, mut on_gesture) = (window.clone(), on_gesture);
                Callback::new(move |event: TouchEvent| {
                    let Some(start) = touch.borrow_mut().take() else {
                        return;
                    };
                    if event.type_() != "touchend" {
                        return;
                    }
                    let Some(point) = event.changed_touches().get(0) else {
                        return;
                    };
                    on_gesture(Gesture {
                        dx: f64::from(point.client_x()) - start.x,
                        dy: f64::from(point.client_y()) - start.y,
                        ms: event.time_stamp() - start.at,
                        start_x: start.x,
                        width: start.width,
                        scrolled: (scroll_top(&window) - start.top).abs() > 1.0,
                        multi_touch: start.multi_touch || event.touches().length() > 0,
                    });
                })
            };
            let listener = Self { window, start, end };
            let options = web_sys::AddEventListenerOptions::new();
            options.set_passive(true);
            for (kind, callback) in listener.callbacks() {
                listener
                    .window
                    .add_event_listener_with_callback_and_add_event_listener_options(
                        kind,
                        callback.as_ref().unchecked_ref(),
                        &options,
                    )
                    .ok()?;
            }
            Some(Rc::new(listener))
        }

        fn callbacks(&self) -> [(&'static str, &Callback); 3] {
            [
                ("touchstart", &self.start),
                ("touchend", &self.end),
                ("touchcancel", &self.end),
            ]
        }
    }

    impl Drop for SwipeListener {
        fn drop(&mut self) {
            for (kind, callback) in self.callbacks() {
                let _ = self
                    .window
                    .remove_event_listener_with_callback(kind, callback.as_ref().unchecked_ref());
            }
        }
    }

    fn begin(window: &Window, id: &str, event: &TouchEvent) -> Option<Start> {
        let document = window.document()?;
        let area = document.get_element_by_id(id)?;
        if area.get_attribute("data-swipe-enabled").as_deref() != Some("true") {
            return None;
        }
        let target = event.target()?.dyn_into::<Element>().ok()?;
        if !area.contains(Some(&target))
            || document
                .query_selector("dialog[open]")
                .ok()
                .flatten()
                .is_some()
            || target.closest(IGNORE_SELECTOR).ok().flatten().is_some()
            || scrolls_horizontally(window, &target, &area)
        {
            return None;
        }
        let point = event.touches().get(0)?;
        Some(Start {
            x: f64::from(point.client_x()),
            y: f64::from(point.client_y()),
            at: event.time_stamp(),
            width: window.inner_width().ok()?.as_f64()?,
            top: scroll_top(window),
            multi_touch: false,
        })
    }

    /// Whether the touch starts inside an element that scrolls sideways itself
    /// (wide tables, scrolling tab bars); those keep their own horizontal drags.
    fn scrolls_horizontally(window: &Window, target: &Element, area: &Element) -> bool {
        let mut element = Some(target.clone());
        while let Some(current) = element {
            if &current == area {
                return false;
            }
            if current.scroll_width() > current.client_width() + 1 {
                let overflow = window
                    .get_computed_style(&current)
                    .ok()
                    .flatten()
                    .and_then(|style| style.get_property_value("overflow-x").ok())
                    .unwrap_or_default();
                if overflow == "auto" || overflow == "scroll" {
                    return true;
                }
            }
            element = current.parent_element();
        }
        false
    }

    fn scroll_top(window: &Window) -> f64 {
        window.scroll_y().unwrap_or(0.0)
    }
}
