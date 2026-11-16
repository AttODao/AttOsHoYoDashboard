//! The tab pager in the app's WebView, through `document::eval`; the browser build does the
//! same with web-sys (`pages/swipe_tabs.rs`, which documents the marks and the flow). The
//! script measures touches and moves the panels per frame without a round trip; it
//! mirrors `app::swipe::{starts_inside, lock, drag_offset, neighbour_offset}` with the
//! constants of `app::swipe::POLICY`, and asks Rust (`app::swipe::settle`) once per release
//! where the panels settle.
use super::dom::quoted;
use crate::app::swipe::{IGNORE_SELECTOR, POLICY, Release, settle};
use dioxus::{core::Task, prelude::*};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Moves the panels of the element with `id` and reports drags and completed swipes.
/// Dropping it removes the listeners and the marks.
pub(crate) struct SwipeListener {
    id: String,
    token: u64,
    task: Task,
    eval: document::Eval,
}

static SWIPE_TOKENS: AtomicU64 = AtomicU64::new(1);

/// What the script reports.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Report {
    /// The touch locked as a horizontal pager drag (`true`), or the panels are at rest
    /// again (`false`).
    Drag { active: bool },
    /// The finger lifted (or the system cancelled the touch).
    Release {
        #[serde(flatten)]
        release: Release,
        index: usize,
        count: usize,
    },
    /// The swipe completed (the panels are on their way to the neighbour): select it.
    Settled { target: usize },
}

/// What Rust tells the script.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Command {
    Settle {
        index: usize,
        #[serde(flatten)]
        settle: crate::app::swipe::Settle,
    },
    Reset,
}

const SWIPE_SCRIPT: &str = r#"
const id = __ID__, token = __TOKEN__, ignore = __IGNORE__, P = __POLICY__;
const swipes = window.__ahdSwipes = window.__ahdSwipes || {};
if (swipes[id]) swipes[id].detach();
const scrollsX = (e, area) => {
  for (; e && e !== area; e = e.parentElement) {
    if (e.scrollWidth > e.clientWidth + 1) {
      const o = getComputedStyle(e).overflowX;
      if (o === "auto" || o === "scroll") return true;
    }
  }
  return false;
};
// Mirrors of app::swipe (constants from POLICY).
const startsInside = (x, w) => w > 2 * P.edge && x >= P.edge && x <= w - P.edge;
const lockFor = (dx, dy, scrolled) => {
  if (scrolled || !isFinite(dx) || !isFinite(dy)) return "scroll";
  if (Math.hypot(dx, dy) < P.slop) return "undecided";
  return Math.abs(dx) > Math.abs(dy) * P.dominance ? "pager" : "scroll";
};
const peekFor = (dx, i, n) => dx < 0 ? (i + 1 < n ? i + 1 : null) : dx > 0 ? (i > 0 && i < n ? i - 1 : null) : null;
const offsetFor = (dx, i, n) => {
  if (!isFinite(dx) || dx === 0) return 0;
  if (peekFor(dx, i, n) !== null) return dx;
  return Math.sign(dx) * P.overscroll * (1 - 1 / (Math.abs(dx) * P.resistance / P.overscroll + 1));
};
const beside = (offset, i, next, width) => next > i ? offset + width + P.gap : offset - width - P.gap;
const area = () => document.getElementById(id);
const pageOf = (a, i) => a && a.querySelector(':scope > [data-swipe-page="' + i + '"]');
const reduced = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
let touch = null, marks = new Map(), settling = false, animating = false, rendered = false, completing = false, reported = false, frame = 0, generation = 0, released = -1;
const unmark = (el) => {
  const m = marks.get(el);
  marks.delete(el);
  delete el.dataset.swipeRole;
  el.style.removeProperty("transform");
  el.style.removeProperty("transition");
  if (m && m.inert) el.removeAttribute("inert");
  if (m && m.hidden) el.removeAttribute("aria-hidden");
};
const place = (el, role, x) => {
  if (!marks.has(el)) {
    const preview = role === "peek";
    const m = { inert: preview && !el.hasAttribute("inert"), hidden: preview && !el.hasAttribute("aria-hidden") };
    if (m.inert) el.setAttribute("inert", "");
    if (m.hidden) el.setAttribute("aria-hidden", "true");
    marks.set(el, m);
  }
  el.dataset.swipeRole = role;
  el.style.setProperty("transform", "translate3d(" + x.toFixed(2) + "px,0,0)");
};
const reset = () => {
  generation++;
  if (frame) cancelAnimationFrame(frame);
  frame = 0; touch = null; settling = false; animating = false; rendered = false; completing = false;
  for (const el of [...marks.keys()]) unmark(el);
  const a = area();
  if (a) delete a.dataset.swipeDrag;
  if (reported) { reported = false; dioxus.send({ kind: "drag", active: false }); }
};
const draw = () => {
  frame = 0;
  const t = touch, a = area();
  if (!t || t.lock !== "pager" || !a) return;
  t.offset = offsetFor(t.dx, t.index, t.count);
  const lead = pageOf(a, t.index);
  if (!lead) return;
  const next = peekFor(t.dx, t.index, t.count), peek = next === null ? null : pageOf(a, next);
  for (const el of [...marks.keys()]) if (el !== lead && el !== peek) unmark(el);
  place(lead, "lead", t.offset);
  if (peek) place(peek, "peek", beside(t.offset, t.index, next, t.width));
  // A neighbour rendered only after the drag started (settings previews) is picked up by
  // a later frame, even while the finger rests.
  else if (next !== null && touch === t && !frame) frame = requestAnimationFrame(draw);
};
const release = (t, ev, extra) => {
  settling = true;
  // Rust answers with "settle"; without an answer the panels return instead of sticking.
  const mine = generation;
  released = mine;
  setTimeout(() => { const a = area(); if (mine === generation && a && a.dataset.swipeDrag === "") reset(); }, 1500);
  const s = t.samples, first = s[0], last = s[s.length - 1];
  dioxus.send(Object.assign({
    kind: "release", index: t.index, count: t.count, dx: t.dx, ms: ev.timeStamp - t.at,
    velocity: last[0] > first[0] ? (last[1] - first[1]) / (last[0] - first[0]) : 0,
    cancelled: false, multi_touch: t.multi, scrolled: Math.abs(window.scrollY - t.top) > 1,
    offset: t.offset, page_width: t.width, reduced: reduced(),
  }, extra));
};
const start = (ev) => {
  if (ev.touches.length !== 1) { if (touch) touch.multi = true; return; }
  touch = null;
  if (settling) {
    // A panel still snapping back can be caught; one settling on another tab cannot.
    const a = area();
    if (completing || !animating || !a) return;
    reset();
  }
  const a = area(), t = ev.target;
  if (!a || a.dataset.swipeEnabled !== "true" || !(t instanceof Element) || !a.contains(t)) return;
  if (document.querySelector("dialog[open]") || t.closest(ignore) || scrollsX(t, a)) return;
  const p = ev.touches[0], index = parseInt(a.dataset.swipeIndex, 10), count = parseInt(a.dataset.swipeCount, 10);
  if (!startsInside(p.clientX, window.innerWidth) || !(index >= 0) || !(count > 0)) return;
  touch = { x: p.clientX, y: p.clientY, at: ev.timeStamp, top: window.scrollY, multi: false, lock: "undecided",
    dx: 0, offset: 0, index, count, width: 0, samples: [[ev.timeStamp, p.clientX]] };
};
const move = (ev) => {
  const t = touch;
  if (!t) return;
  const multi = t.multi || ev.touches.length !== 1, p = ev.touches[0];
  if (!p) return;
  const dx = p.clientX - t.x, dy = p.clientY - t.y, scrolled = Math.abs(window.scrollY - t.top) > 1;
  t.samples.push([ev.timeStamp, p.clientX]);
  while (t.samples.length > 2 && ev.timeStamp - t.samples[1][0] > P.window) t.samples.shift();
  if (t.lock === "undecided") {
    if (multi) { touch = null; return; }
    const lock = lockFor(dx, dy, scrolled);
    if (lock === "undecided") return;
    const a = area();
    if (lock === "scroll" || !a) { touch = null; return; }
    t.lock = "pager"; t.width = a.clientWidth;
    a.dataset.swipeDrag = "";
    reported = true;
    dioxus.send({ kind: "drag", active: true });
  } else if (multi || scrolled) {
    // A pinch or a page scroll takes over: the panels return.
    touch = null; t.multi = t.multi || multi;
    release(t, ev, { multi_touch: t.multi, scrolled });
    return;
  }
  // The pager owns this touch (see pages/swipe_tabs.rs: a dropped scroll's fling would
  // swallow the next tap).
  if (ev.cancelable) ev.preventDefault();
  t.dx = dx;
  if (!frame) frame = requestAnimationFrame(draw);
};
const end = (ev) => {
  const t = touch;
  touch = null;
  if (!t || t.lock !== "pager") return;
  const cancelled = ev.type !== "touchend", p = ev.changedTouches[0];
  if (!cancelled && p) { t.dx = p.clientX - t.x; t.samples.push([ev.timeStamp, p.clientX]); }
  if (frame) cancelAnimationFrame(frame);
  touch = t; draw(); touch = null;
  release(t, ev, { cancelled, multi_touch: t.multi || ev.touches.length > 0 });
};
const settle = (msg) => {
  const a = area(), mine = generation;
  // An answer for a release that a reset already dropped.
  if (released !== mine || !settling) return;
  if (!a) { reset(); return; }
  a.dataset.swipeDrag = "settling";
  completing = msg.target !== null; animating = true; rendered = false;
  const width = a.clientWidth;
  for (const el of marks.keys()) {
    getComputedStyle(el).transform; // start the transition from the released position
    if (msg.ms > 0) el.style.setProperty("transition", "transform " + msg.ms + "ms " + P.easing);
    const page = parseInt(el.dataset.swipePage, 10);
    const x = page === msg.index ? msg.offset : beside(msg.offset, msg.index, page, width);
    el.style.setProperty("transform", "translate3d(" + x.toFixed(2) + "px,0,0)");
  }
  // The swipe completed when the finger lifted: select the tab now, like a tap.
  if (completing) dioxus.send({ kind: "settled", target: msg.target });
  setTimeout(() => {
    if (mine !== generation) return;
    animating = false;
    if (!completing || rendered) { reset(); return; }
    a.dataset.swipeDrag = "settled";
    // Rust resets after the re-render; this only covers a refused change.
    setTimeout(() => { if (mine === generation) reset(); }, 1000);
  }, msg.ms);
};
// Rust's reset after the re-render with the new tab waits for a running animation.
const resetAfterRender = () => { if (animating) rendered = true; else reset(); };
const listeners = [["touchstart", start], ["touchmove", move], ["touchend", end], ["touchcancel", end]];
for (const [type, fn] of listeners) window.addEventListener(type, fn, { passive: type !== "touchmove" });
const detach = () => { for (const [type, fn] of listeners) window.removeEventListener(type, fn); reported = false; reset(); };
swipes[id] = { token, detach };
for (;;) {
  const msg = await dioxus.recv();
  if (!swipes[id] || swipes[id].token !== token) break;
  if (msg.kind === "settle") settle(msg); else resetAfterRender();
}
"#;

impl SwipeListener {
    pub(crate) fn attach(
        id: String,
        mut on_drag: impl FnMut(bool) + 'static,
        mut on_complete: impl FnMut(usize) + 'static,
    ) -> Self {
        let token = SWIPE_TOKENS.fetch_add(1, Ordering::Relaxed);
        let script = SWIPE_SCRIPT
            .replace("__ID__", &quoted(&id))
            .replace("__TOKEN__", &token.to_string())
            .replace("__IGNORE__", &quoted(IGNORE_SELECTOR))
            .replace(
                "__POLICY__",
                &serde_json::to_string(&POLICY).unwrap_or_else(|_| "{}".to_owned()),
            );
        let eval = document::eval(&script);
        let task = spawn(async move {
            let mut reports = eval;
            while let Ok(report) = reports.recv::<Report>().await {
                match report {
                    Report::Drag { active } => on_drag(active),
                    Report::Release {
                        release,
                        index,
                        count,
                    } => {
                        let settle = settle(&release, index, count);
                        let _ = reports.send(Command::Settle { index, settle });
                    }
                    Report::Settled { target } => on_complete(target),
                }
            }
        });
        Self {
            id,
            token,
            task,
            eval,
        }
    }

    /// Drops every drag mark (after the re-render that selected the new panel).
    pub(crate) fn reset(&self) {
        let _ = self.eval.send(Command::Reset);
    }
}

impl Drop for SwipeListener {
    fn drop(&mut self) {
        self.task.cancel();
        // A remounted area with the same id has already replaced this listener.
        let _ = document::eval(&format!(
            "const id = {}; const s = window.__ahdSwipes && window.__ahdSwipes[id]; \
             if (s && s.token === {}) {{ s.detach(); delete window.__ahdSwipes[id]; }}",
            quoted(&self.id),
            self.token
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_and_commands_match_the_script() {
        let report: Report = serde_json::from_str(
            r#"{"kind":"release","index":0,"count":2,"dx":-80,"ms":200,"velocity":-0.5,"cancelled":false,"multi_touch":false,"scrolled":false,"offset":-80,"page_width":320,"reduced":true}"#,
        )
        .expect("release report");
        let Report::Release {
            release,
            index,
            count,
        } = report
        else {
            panic!("not a release");
        };
        let command = serde_json::to_value(Command::Settle {
            index,
            settle: settle(&release, index, count),
        })
        .expect("settle command");
        assert_eq!(command["kind"], "settle");
        assert_eq!(command["target"], 1);
        assert_eq!(command["index"], 0);
        assert_eq!(command["ms"], 0.0);
        assert!(matches!(
            serde_json::from_str::<Report>(r#"{"kind":"drag","active":true}"#),
            Ok(Report::Drag { active: true })
        ));
        assert!(matches!(
            serde_json::from_str::<Report>(r#"{"kind":"settled","target":1}"#),
            Ok(Report::Settled { target: 1 })
        ));
        assert_eq!(
            serde_json::to_value(Command::Reset).expect("reset")["kind"],
            "reset"
        );
        for name in ["__ID__", "__TOKEN__", "__IGNORE__", "__POLICY__"] {
            assert!(SWIPE_SCRIPT.contains(name), "{name}");
        }
    }
}
