//! Tab swipe collection in the app's WebView, through `document::eval`; the browser build
//! does the same with web-sys (`pages/swipe_tabs.rs`). Only DOM checks happen in the
//! script; `app::swipe::classify` judges each finished touch.
use super::dom::quoted;
use crate::app::swipe::{Gesture, IGNORE_SELECTOR};
use dioxus::{core::Task, prelude::*};
use std::sync::atomic::{AtomicU64, Ordering};

/// Collects touches on the element with `id` and reports each finished one to
/// `on_gesture`. Dropping it removes the listeners.
pub(crate) struct SwipeListener {
    id: String,
    token: u64,
    task: Task,
}

static SWIPE_TOKENS: AtomicU64 = AtomicU64::new(1);

const SWIPE_SCRIPT: &str = r#"
const id = __ID__, token = __TOKEN__, ignore = __IGNORE__;
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
let touch = null;
const start = (ev) => {
  if (ev.touches.length !== 1) { if (touch) touch.multi = true; return; }
  touch = null;
  const area = document.getElementById(id), t = ev.target;
  if (!area || area.dataset.swipeEnabled !== "true" || !(t instanceof Element) || !area.contains(t)) return;
  if (document.querySelector("dialog[open]") || t.closest(ignore) || scrollsX(t, area)) return;
  const p = ev.touches[0];
  touch = { x: p.clientX, y: p.clientY, at: ev.timeStamp, width: window.innerWidth, top: window.scrollY, multi: false };
};
const end = (ev) => {
  const g = touch;
  touch = null;
  if (!g || ev.type !== "touchend" || !ev.changedTouches.length) return;
  const p = ev.changedTouches[0];
  dioxus.send({
    dx: p.clientX - g.x, dy: p.clientY - g.y, ms: ev.timeStamp - g.at, start_x: g.x, width: g.width,
    scrolled: Math.abs(window.scrollY - g.top) > 1, multi_touch: g.multi || ev.touches.length > 0,
  });
};
const listeners = [["touchstart", start], ["touchend", end], ["touchcancel", end]];
for (const [type, fn] of listeners) window.addEventListener(type, fn, { passive: true });
swipes[id] = { token, detach: () => { for (const [type, fn] of listeners) window.removeEventListener(type, fn); } };
"#;

impl SwipeListener {
    pub(crate) fn attach(id: String, mut on_gesture: impl FnMut(Gesture) + 'static) -> Self {
        let token = SWIPE_TOKENS.fetch_add(1, Ordering::Relaxed);
        let script = SWIPE_SCRIPT
            .replace("__ID__", &quoted(&id))
            .replace("__TOKEN__", &token.to_string())
            .replace("__IGNORE__", &quoted(IGNORE_SELECTOR));
        let mut eval = document::eval(&script);
        let task = spawn(async move {
            while let Ok(gesture) = eval.recv::<Gesture>().await {
                on_gesture(gesture);
            }
        });
        Self { id, token, task }
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
