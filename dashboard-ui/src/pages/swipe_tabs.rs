//! Horizontal swipes between tabs on touch screens, in the browser and in the app, drawn as
//! a pager: while the finger drags sideways the current panel follows it and the
//! neighbouring tab's panel moves in beside it; on release the panels settle on the
//! neighbour (the swipe completes) or snap back.
//!
//! [`SwipeableTabs`] wraps the tab panels; it does not render the tab bar, so the existing
//! `role="tab"` buttons, keyboard handling and the stored tab state (`AppState` or the
//! `Page` in the URL) stay where they are. The tab bar does not react during the drag; a
//! swipe that completes when the finger lifts calls `on_change` like a tap (as the
//! finished-touch swipes did), while the panels finish moving.
//! Policy (slop, direction lock, resistance, release, settle) is `app::swipe`; this module
//! collects touches and moves the panels with web-sys in the browser, and
//! `mobile::swipe::SwipeListener` does the same in the app's WebView script. Per-frame
//! movement never goes through a Dioxus render: the collectors set the panels' transforms
//! through the CSSOM (`style.setProperty`, allowed under `style-src 'self'`), throttled with
//! `requestAnimationFrame`, and report only the drag start/end and the completed swipe.
//!
//! Only touch events are observed, so mouse drags on a PC never swipe. The area's
//! `touch-action: pan-y pinch-zoom` (`dashboard.css`) leaves vertical scrolling and pinch
//! zoom to the browser and keeps horizontal drags from scrolling the page, so the browser
//! never has to wait for a listener to decide the direction. touchstart/touchend stay
//! passive; touchmove is not, and cancels only moves of a touch already locked as a pager
//! drag: without that, Chromium still runs the dropped horizontal scroll's fling and
//! swallows the next tap within about half a second (a tab tapped right after a swipe did
//! nothing). Vertical scrolls are never cancelled. A touch that scrolled the page, started
//! on a form control or inside a horizontally scrollable element, started at a screen edge
//! (iOS back swipe, Android back gesture) or while a dialog is open never moves the panels.
//!
//! The collectors mark the panels they move with `data-swipe-role` (`lead` = the current
//! panel, `peek` = the neighbour, made `inert` and `aria-hidden` while it is only a preview)
//! and the area with `data-swipe-drag` (empty while dragging, `settling`, `settled`). After
//! a completed swipe the marks stay until both the settle animation ended and the re-render
//! with the new tab happened ([`SwipeableTabs`] asks for the reset in an effect once `index`
//! changed), so the selected panel never flashes and a tab tapped during the settle wins.
use dioxus::prelude::*;
use std::{cell::Cell, rc::Rc};

/// The props the collectors' reports are handled with; updated on every render.
#[derive(Clone, Copy)]
struct Tabs {
    count: usize,
    enabled: bool,
    on_change: EventHandler<usize>,
    on_drag: Option<EventHandler<bool>>,
}

/// Swipe left for the next tab, right for the previous one; the panels follow the finger.
/// At either end the panel resists and returns.
///
/// Every panel the pager may show is a direct child with `data-swipe-page="{index}"`;
/// the neighbour of the selected one must already be in the DOM (hidden), or appear
/// once `on_drag(true)` fired (the settings tabs render their neighbours as inert previews
/// until `on_drag(false)`). `id` must be unique on the page and stay the same while mounted. Pass
/// `enabled: false` where the tabs are not shown (e.g. the wide dashboard shows both games
/// side by side). Mark controls that need horizontal drags with `data-swipe-ignore`; form
/// controls and horizontally scrollable elements (`overflow-x: auto|scroll` with
/// overflowing content) are excluded automatically.
///
/// ```ignore
/// let index = TABS.iter().position(|tab| *tab == active()).unwrap_or(0);
/// rsx! {
///     SwipeableTabs {
///         id: "game-panels", count: TABS.len(), index,
///         on_change: move |next: usize| select(TABS[next]),
///         for (position, tab) in TABS.iter().enumerate() {
///             Panel { key: "{tab}", tab, active: active() == *tab, "data-swipe-page": position }
///         }
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
    /// Called with `true` when a touch locks as a horizontal pager drag, and with `false`
    /// once the panels are at rest again (snapped back, or settled and re-rendered with
    /// the new tab).
    #[props(default)]
    on_drag: Option<EventHandler<bool>>,
    children: Element,
) -> Element {
    let pager = use_pager(
        &id,
        Tabs {
            count,
            enabled,
            on_change,
            on_drag,
        },
    );
    // A completed swipe (or a tap) re-rendered the panels: drop the drag positions once the
    // settle animation has ended, now that the new panel is the selected one.
    use_effect(use_reactive((&index,), move |_| pager.reset()));
    rsx! {
        div {
            id,
            class: if class.is_empty() { "swipe-tabs".to_owned() } else { format!("swipe-tabs {class}") },
            "data-swipe-enabled": if enabled { "true" } else { "false" },
            "data-swipe-index": index,
            "data-swipe-count": count,
            {children}
        }
    }
}

/// The collector of one pager, removed when the component unmounts.
#[derive(Clone)]
struct Pager(Option<Rc<Collector>>);

impl Pager {
    fn reset(&self) {
        if let Some(collector) = &self.0 {
            collector.reset();
        }
    }
}

fn use_pager(id: &str, current: Tabs) -> Pager {
    let tabs = use_hook(|| Rc::new(Cell::new(current)));
    tabs.set(current);
    let id = id.to_owned();
    use_hook(move || {
        let reports = Reports {
            drag: {
                let tabs = tabs.clone();
                Box::new(move |active| {
                    if let Some(on_drag) = tabs.get().on_drag {
                        on_drag.call(active);
                    }
                })
            },
            complete: Box::new(move |target| {
                let tabs = tabs.get();
                if tabs.enabled && target < tabs.count {
                    tabs.on_change.call(target);
                }
            }),
        };
        Pager(listen(id, reports))
    })
}

/// What the collectors report: a drag locked horizontally (`true`) and the panels at rest
/// again (`false`), and the tab a completed swipe arrived at. They run outside any Dioxus
/// scope (raw browser callbacks, the app's eval task) and only call event handlers.
struct Reports {
    drag: Box<dyn FnMut(bool)>,
    complete: Box<dyn FnMut(usize)>,
}

#[cfg(feature = "mobile")]
type Collector = crate::mobile::swipe::SwipeListener;

#[cfg(feature = "mobile")]
fn listen(id: String, reports: Reports) -> Option<Rc<Collector>> {
    Some(Rc::new(crate::mobile::swipe::SwipeListener::attach(
        id,
        reports.drag,
        reports.complete,
    )))
}

#[cfg(all(target_arch = "wasm32", not(feature = "mobile")))]
type Collector = web::SwipeListener;

#[cfg(all(target_arch = "wasm32", not(feature = "mobile")))]
fn listen(id: String, reports: Reports) -> Option<Rc<Collector>> {
    web::SwipeListener::attach(id, reports)
}

/// Native test builds have no DOM to listen on.
#[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
struct Collector;

#[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
impl Collector {
    fn reset(&self) {}
}

#[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
fn listen(id: String, reports: Reports) -> Option<Rc<Collector>> {
    let _ = (id, reports.drag, reports.complete);
    None
}

/// Browser touch collection and panel movement; the app does the same in JavaScript
/// (`mobile::swipe`).
#[cfg(all(target_arch = "wasm32", not(feature = "mobile")))]
mod web {
    use super::Reports;
    use crate::app::swipe::{
        IGNORE_SELECTOR, Lock, Release, SETTLE_EASING, Settle, VELOCITY_WINDOW_MS, drag_offset,
        lock, neighbour_offset, peek, settle, starts_inside,
    };
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};
    use wasm_bindgen::{JsCast, closure::Closure};
    use web_sys::{Element, HtmlElement, TouchEvent, Window};

    type Callback = Closure<dyn FnMut(TouchEvent)>;

    /// How long a settled swipe waits for the re-render that selects the new panel before
    /// the marks are dropped anyway (`on_change` did not change the tab).
    const SETTLED_FALLBACK_MS: i32 = 1000;

    struct Touch {
        x: f64,
        y: f64,
        at: f64,
        top: f64,
        multi_touch: bool,
        lock: Lock,
        dx: f64,
        offset: f64,
        index: usize,
        count: usize,
        page_width: f64,
        /// Recent (timestamp, clientX) pairs for the release speed.
        samples: VecDeque<(f64, f64)>,
    }

    struct Mark {
        element: HtmlElement,
        added_inert: bool,
        added_hidden: bool,
    }

    #[derive(Default)]
    struct State {
        touch: Option<Touch>,
        marks: Vec<Mark>,
        /// From the release until the marks are dropped.
        settling: bool,
        /// The settle animation is running.
        animating: bool,
        /// The tab changed (re-render) while the animation ran: reset when it ends.
        rendered: bool,
        /// `drag(true)` was reported and `drag(false)` is still due.
        reported: bool,
        /// The running settle ends on another tab (it cannot be caught; a snap back can).
        completing: bool,
        frame: bool,
        /// Bumped by every reset, so late timers of an older drag do nothing.
        generation: u64,
    }

    struct Shared {
        window: Window,
        id: String,
        state: RefCell<State>,
        reports: RefCell<Reports>,
    }

    pub(crate) struct SwipeListener {
        shared: Rc<Shared>,
        start: Callback,
        moved: Callback,
        end: Callback,
    }

    impl SwipeListener {
        pub(super) fn attach(id: String, reports: Reports) -> Option<Rc<Self>> {
            let window = web_sys::window()?;
            let shared = Rc::new(Shared {
                window,
                id,
                state: RefCell::default(),
                reports: RefCell::new(reports),
            });
            let start = {
                let shared = shared.clone();
                Callback::new(move |event: TouchEvent| shared.touch_start(&event))
            };
            let moved = {
                let shared = shared.clone();
                Callback::new(move |event: TouchEvent| shared.touch_move(&event))
            };
            let end = {
                let shared = shared.clone();
                Callback::new(move |event: TouchEvent| shared.touch_end(&event))
            };
            let listener = Self {
                shared,
                start,
                moved,
                end,
            };
            for (kind, callback) in listener.callbacks() {
                // Only touchmove may cancel (a locked pager drag); see the module comment.
                let options = web_sys::AddEventListenerOptions::new();
                options.set_passive(kind != "touchmove");
                listener
                    .shared
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

        /// Drops every drag mark after the re-render that selected the new panel, or once
        /// the running settle animation ends.
        pub(crate) fn reset(&self) {
            let animating = self
                .shared
                .state
                .try_borrow_mut()
                .map(|mut state| {
                    let animating = state.animating;
                    if animating {
                        state.rendered = true;
                    }
                    animating
                })
                .unwrap_or(false);
            if !animating {
                self.shared.reset(true);
            }
        }

        fn callbacks(&self) -> [(&'static str, &Callback); 4] {
            [
                ("touchstart", &self.start),
                ("touchmove", &self.moved),
                ("touchend", &self.end),
                ("touchcancel", &self.end),
            ]
        }
    }

    impl Drop for SwipeListener {
        fn drop(&mut self) {
            for (kind, callback) in self.callbacks() {
                let _ = self
                    .shared
                    .window
                    .remove_event_listener_with_callback(kind, callback.as_ref().unchecked_ref());
            }
            // The component is gone: nothing to report to.
            self.shared.reset(false);
        }
    }

    impl Shared {
        fn area(&self) -> Option<Element> {
            self.window.document()?.get_element_by_id(&self.id)
        }

        fn touch_start(&self, event: &TouchEvent) {
            let mut state = self.state.borrow_mut();
            if event.touches().length() != 1 {
                // A second finger (pinch): the current drag snaps back below.
                if let Some(touch) = state.touch.as_mut() {
                    touch.multi_touch = true;
                }
                return;
            }
            state.touch = None;
            if state.settling {
                if state.completing || !state.animating {
                    return;
                }
                // Catch a panel that is still snapping back.
                drop(state);
                self.reset(true);
                state = self.state.borrow_mut();
            }
            state.touch = self.begin(event);
        }

        fn begin(&self, event: &TouchEvent) -> Option<Touch> {
            let document = self.window.document()?;
            let area = document.get_element_by_id(&self.id)?;
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
                || scrolls_horizontally(&self.window, &target, &area)
            {
                return None;
            }
            let point = event.touches().get(0)?;
            let x = f64::from(point.client_x());
            let width = self.window.inner_width().ok()?.as_f64()?;
            if !starts_inside(x, width) {
                return None;
            }
            let number = |name: &str| area.get_attribute(name)?.parse::<usize>().ok();
            Some(Touch {
                x,
                y: f64::from(point.client_y()),
                at: event.time_stamp(),
                top: self.scroll_top(),
                multi_touch: false,
                lock: Lock::Undecided,
                dx: 0.0,
                offset: 0.0,
                index: number("data-swipe-index")?,
                count: number("data-swipe-count")?,
                page_width: 0.0,
                samples: VecDeque::from([(event.time_stamp(), x)]),
            })
        }

        fn touch_move(self: &Rc<Self>, event: &TouchEvent) {
            let mut state = self.state.borrow_mut();
            let Some(touch) = state.touch.as_mut() else {
                return;
            };
            let multi_touch = touch.multi_touch || event.touches().length() != 1;
            let Some(point) = event.touches().get(0) else {
                return;
            };
            let dx = f64::from(point.client_x()) - touch.x;
            let dy = f64::from(point.client_y()) - touch.y;
            let scrolled = (self.scroll_top() - touch.top).abs() > 1.0;
            let now = event.time_stamp();
            touch.samples.push_back((now, f64::from(point.client_x())));
            while touch.samples.len() > 2
                && touch
                    .samples
                    .get(1)
                    .is_some_and(|(at, _)| now - at > VELOCITY_WINDOW_MS)
            {
                touch.samples.pop_front();
            }
            let mut started = false;
            match touch.lock {
                Lock::Undecided if multi_touch => {
                    state.touch = None;
                    return;
                }
                Lock::Undecided => match lock(dx, dy, scrolled) {
                    Lock::Undecided => return,
                    Lock::Scroll => {
                        state.touch = None;
                        return;
                    }
                    Lock::Pager => {
                        let Some(area) = self.area() else {
                            state.touch = None;
                            return;
                        };
                        touch.lock = Lock::Pager;
                        touch.page_width = f64::from(area.client_width());
                        let _ = area.set_attribute("data-swipe-drag", "");
                        started = true;
                    }
                },
                Lock::Pager if multi_touch || scrolled => {
                    // A pinch or a page scroll takes over: the panels return.
                    touch.multi_touch |= multi_touch;
                    let release = Release {
                        dx: touch.dx,
                        multi_touch: touch.multi_touch,
                        scrolled,
                        offset: touch.offset,
                        page_width: touch.page_width,
                        reduced: self.reduced_motion(),
                        ..Release::default()
                    };
                    let (index, count) = (touch.index, touch.count);
                    state.touch = None;
                    let settle = settle(&release, index, count);
                    drop(state);
                    self.run_settle(index, settle);
                    return;
                }
                Lock::Pager | Lock::Scroll => {}
            }
            // The pager owns this touch: keep the browser's gesture detector from treating it
            // as a (dropped) horizontal scroll, whose fling would swallow the next tap.
            if event.cancelable() {
                event.prevent_default();
            }
            touch.dx = dx;
            state.reported |= started;
            self.request_frame(&mut state);
            drop(state);
            if started {
                (self.reports.borrow_mut().drag)(true);
            }
        }

        fn request_frame(self: &Rc<Self>, state: &mut State) {
            if state.frame {
                return;
            }
            state.frame = true;
            let shared = self.clone();
            let frame = Closure::once_into_js(move || shared.frame());
            if self
                .window
                .request_animation_frame(frame.unchecked_ref())
                .is_err()
            {
                state.frame = false;
            }
        }

        fn frame(self: &Rc<Self>) {
            let mut state = self.state.borrow_mut();
            state.frame = false;
            // A neighbour rendered only after the drag started (settings previews) is
            // picked up by the next frame, even while the finger rests.
            if self.draw(&mut state) {
                self.request_frame(&mut state);
            }
        }

        /// Places the current panel at the finger and the neighbour beside it. Returns
        /// whether the neighbour should be there but is not rendered yet.
        fn draw(&self, state: &mut State) -> bool {
            let Some(touch) = state.touch.as_mut() else {
                return false;
            };
            if touch.lock != Lock::Pager {
                return false;
            }
            let Some(area) = self.area() else {
                return false;
            };
            touch.offset = drag_offset(touch.dx, touch.index, touch.count);
            let (index, offset, width) = (touch.index, touch.offset, touch.page_width);
            let expected = peek(touch.dx, index, touch.count);
            let neighbour = expected.and_then(|next| Some((next, page(&area, next)?)));
            let missing = expected.is_some() && neighbour.is_none();
            let Some(lead) = page(&area, index) else {
                return false;
            };
            let keep: Vec<&HtmlElement> = std::iter::once(&lead)
                .chain(neighbour.as_ref().map(|(_, element)| element))
                .collect();
            state.marks.retain(|mark| {
                let kept = keep.iter().any(|element| **element == mark.element);
                if !kept {
                    unmark(mark);
                }
                kept
            });
            place(&mut state.marks, &lead, "lead", offset);
            if let Some((next, element)) = neighbour {
                place(
                    &mut state.marks,
                    &element,
                    "peek",
                    neighbour_offset(offset, index, next, width),
                );
            }
            missing
        }

        fn touch_end(self: &Rc<Self>, event: &TouchEvent) {
            let mut state = self.state.borrow_mut();
            let Some(touch) = state.touch.as_ref() else {
                return;
            };
            if touch.lock != Lock::Pager {
                state.touch = None;
                return;
            }
            let cancelled = event.type_() != "touchend";
            if let (false, Some(point)) = (cancelled, event.changed_touches().get(0))
                && let Some(touch) = state.touch.as_mut()
            {
                touch.dx = f64::from(point.client_x()) - touch.x;
                touch
                    .samples
                    .push_back((event.time_stamp(), f64::from(point.client_x())));
            }
            // Draw the released position now, so the settle starts exactly there.
            let _ = self.draw(&mut state);
            let Some(touch) = state.touch.take() else {
                return;
            };
            let velocity = match (touch.samples.front(), touch.samples.back()) {
                (Some((from_at, from_x)), Some((to_at, to_x))) if to_at > from_at => {
                    (to_x - from_x) / (to_at - from_at)
                }
                _ => 0.0,
            };
            let release = Release {
                dx: touch.dx,
                ms: event.time_stamp() - touch.at,
                velocity,
                cancelled,
                multi_touch: touch.multi_touch || event.touches().length() > 0,
                scrolled: (self.scroll_top() - touch.top).abs() > 1.0,
                offset: touch.offset,
                page_width: touch.page_width,
                reduced: self.reduced_motion(),
            };
            let settle = settle(&release, touch.index, touch.count);
            drop(state);
            self.run_settle(touch.index, settle);
        }

        /// Selects the target tab of a completed swipe and moves the marked panels from where
        /// they are to the settle position (or back after a snap back).
        fn run_settle(self: &Rc<Self>, index: usize, settle: Settle) {
            let mut state = self.state.borrow_mut();
            state.settling = true;
            state.animating = true;
            state.rendered = false;
            state.completing = settle.target.is_some();
            let generation = state.generation;
            let width = self
                .area()
                .map(|area| {
                    let _ = area.set_attribute("data-swipe-drag", "settling");
                    f64::from(area.client_width())
                })
                .unwrap_or_default();
            let transition = format!("transform {}ms {SETTLE_EASING}", settle.ms);
            for mark in &state.marks {
                // Flush the released position so the transition starts from it.
                let _ = self
                    .window
                    .get_computed_style(&mark.element)
                    .ok()
                    .flatten()
                    .map(|style| style.get_property_value("transform"));
                let style = mark.element.style();
                if settle.ms > 0.0 {
                    let _ = style.set_property("transition", &transition);
                }
                let neighbour = mark
                    .element
                    .get_attribute("data-swipe-page")
                    .and_then(|page| page.parse::<usize>().ok())
                    .filter(|page| *page != index);
                let x = match neighbour {
                    Some(page) => neighbour_offset(settle.offset, index, page, width),
                    None => settle.offset,
                };
                let _ = style.set_property("transform", &translate(x));
            }
            drop(state);
            let shared = self.clone();
            let done = Closure::once_into_js(move || shared.settled(generation, settle.target));
            if self
                .window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    done.unchecked_ref(),
                    settle.ms as i32,
                )
                .is_err()
            {
                self.reset(true);
                return;
            }
            if let Some(target) = settle.target {
                (self.reports.borrow_mut().complete)(target);
            }
        }

        fn settled(self: &Rc<Self>, generation: u64, target: Option<usize>) {
            let rendered = {
                let mut state = self.state.borrow_mut();
                if state.generation != generation {
                    return;
                }
                state.animating = false;
                state.rendered
            };
            if target.is_none() || rendered {
                self.reset(true);
                return;
            }
            if let Some(area) = self.area() {
                let _ = area.set_attribute("data-swipe-drag", "settled");
            }
            let shared = self.clone();
            let fallback = Closure::once_into_js(move || {
                if shared.state.borrow().generation == generation {
                    shared.reset(true);
                }
            });
            let _ = self
                .window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    fallback.unchecked_ref(),
                    SETTLED_FALLBACK_MS,
                );
        }

        /// Drops the marks and the running drag; reports the rest if `report` and a drag
        /// was reported.
        fn reset(&self, report: bool) {
            let Ok(mut state) = self.state.try_borrow_mut() else {
                return;
            };
            if state.marks.is_empty() && !state.settling && state.touch.is_none() && !state.reported
            {
                return;
            }
            state.generation += 1;
            state.settling = false;
            state.animating = false;
            state.rendered = false;
            state.completing = false;
            state.touch = None;
            let reported = std::mem::take(&mut state.reported);
            for mark in state.marks.drain(..) {
                unmark(&mark);
            }
            drop(state);
            if let Some(area) = self.area() {
                let _ = area.remove_attribute("data-swipe-drag");
            }
            if report
                && reported
                && let Ok(mut reports) = self.reports.try_borrow_mut()
            {
                (reports.drag)(false);
            }
        }

        fn scroll_top(&self) -> f64 {
            self.window.scroll_y().unwrap_or(0.0)
        }

        fn reduced_motion(&self) -> bool {
            self.window
                .match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
                .is_some_and(|media| media.matches())
        }
    }

    fn page(area: &Element, index: usize) -> Option<HtmlElement> {
        area.query_selector(&format!(":scope > [data-swipe-page=\"{index}\"]"))
            .ok()??
            .dyn_into()
            .ok()
    }

    fn translate(x: f64) -> String {
        format!("translate3d({x:.2}px,0,0)")
    }

    /// Marks and moves one panel. A neighbour is only a preview: inert and hidden from
    /// assistive technology unless it already was (the settings previews are).
    fn place(marks: &mut Vec<Mark>, element: &HtmlElement, role: &str, x: f64) {
        if !marks.iter().any(|mark| mark.element == *element) {
            let preview = role == "peek";
            let mark = Mark {
                element: element.clone(),
                added_inert: preview && !element.has_attribute("inert"),
                added_hidden: preview && !element.has_attribute("aria-hidden"),
            };
            if mark.added_inert {
                let _ = element.set_attribute("inert", "");
            }
            if mark.added_hidden {
                let _ = element.set_attribute("aria-hidden", "true");
            }
            marks.push(mark);
        }
        let _ = element.set_attribute("data-swipe-role", role);
        let _ = element.style().set_property("transform", &translate(x));
    }

    fn unmark(mark: &Mark) {
        let element = &mark.element;
        let _ = element.remove_attribute("data-swipe-role");
        let style = element.style();
        let _ = style.remove_property("transform");
        let _ = style.remove_property("transition");
        if mark.added_inert {
            let _ = element.remove_attribute("inert");
        }
        if mark.added_hidden {
            let _ = element.remove_attribute("aria-hidden");
        }
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
}
