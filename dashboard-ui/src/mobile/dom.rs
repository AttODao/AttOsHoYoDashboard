//! DOM work the browser build does through web-sys, done in the app's WebView through
//! `document::eval`. Scripts only receive internal IDs and fixed values, JSON-encoded.
use dioxus::{core::Task, prelude::*};
use std::sync::atomic::{AtomicBool, Ordering};

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

/// Sets `lang`/`data-theme` and the theme-color meta. "system" follows the WebView's
/// color scheme, also when it changes later.
pub(crate) fn apply_preferences(language: &str, theme: &str) {
    let script = format!(
        r##"
        const root = document.documentElement;
        root.setAttribute("lang", {language});
        root.dataset.themeChoice = {theme};
        const media = matchMedia("(prefers-color-scheme: light)");
        const apply = () => {{
          const choice = root.dataset.themeChoice;
          const resolved = choice === "system" ? (media.matches ? "light" : "dark") : choice;
          root.setAttribute("data-theme", resolved);
          const meta = document.getElementById("theme-color");
          if (meta) meta.setAttribute("content", resolved === "light" ? "#F5F4F8" : "#0C0B0E");
        }};
        if (!window.__ahdThemeListener) {{
          window.__ahdThemeListener = apply;
          media.addEventListener("change", apply);
        }}
        apply();
        "##,
        language = quoted(language),
        theme = quoted(theme),
    );
    let _ = document::eval(&script);
}

pub(crate) fn focus(id: &str) {
    let _ = document::eval(&format!(
        "const e = document.getElementById({}); if (e && e.focus) e.focus();",
        quoted(id)
    ));
}

pub(crate) fn set_checked(id: &str, checked: bool) {
    let _ = document::eval(&format!(
        "const e = document.getElementById({}); if (e) e.checked = {checked};",
        quoted(id)
    ));
}

/// An open native `<dialog>`: Escape, backdrop clicks and `close` report through
/// `onclose`; dropping it closes the dialog and restores scroll and focus.
pub(crate) struct Modal {
    id: String,
    task: Task,
}

impl Modal {
    pub(crate) fn open(id: &str, onclose: EventHandler<()>) -> Self {
        let script = format!(
            r#"
            const id = {id};
            const d = document.getElementById(id);
            if (!d) return;
            const previous = document.activeElement;
            try {{ if (!d.open) d.showModal(); }} catch (_) {{ return; }}
            const root = document.documentElement, body = document.body;
            const saved = [root.style.overflow, body.style.overflow];
            root.style.overflow = "hidden";
            body.style.overflow = "hidden";
            const onCancel = (e) => {{ e.preventDefault(); dioxus.send(true); }};
            const onClose = () => dioxus.send(true);
            const onClick = (e) => {{
              if (e.target !== d) return;
              const r = d.getBoundingClientRect();
              if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) dioxus.send(true);
            }};
            d.addEventListener("cancel", onCancel);
            d.addEventListener("close", onClose);
            d.addEventListener("click", onClick);
            window.__ahdModals = window.__ahdModals || {{}};
            window.__ahdModals[id] = () => {{
              d.removeEventListener("cancel", onCancel);
              d.removeEventListener("close", onClose);
              d.removeEventListener("click", onClick);
              if (d.open) d.close();
              root.style.overflow = saved[0];
              body.style.overflow = saved[1];
              if (previous && previous.isConnected && previous.focus) previous.focus();
            }};
            "#,
            id = quoted(id)
        );
        let mut eval = document::eval(&script);
        let task = spawn(async move {
            while eval.recv::<bool>().await.is_ok() {
                onclose.call(());
            }
        });
        Self {
            id: id.to_owned(),
            task,
        }
    }
}

impl Drop for Modal {
    fn drop(&mut self) {
        self.task.cancel();
        let _ = document::eval(&format!(
            "const id = {}; const close = window.__ahdModals && window.__ahdModals[id]; \
             if (close) {{ delete window.__ahdModals[id]; close(); }}",
            quoted(&self.id)
        ));
    }
}

/// Pull-to-refresh on the page's top edge: a single-finger drag down while the page
/// is scrolled to the top moves `#pull-refresh` (CSS variables `--pull`/`--pull-turn`,
/// `data-pulling`/`data-armed`), and releasing past the threshold calls `on_pull`.
/// Open dialogs, touches on form controls and mostly horizontal swipes never pull.
/// Dropping it removes the listeners.
pub(crate) struct PullToRefresh {
    task: Task,
}

const PULL_SCRIPT: &str = r#"
if (window.__ahdPull) window.__ahdPull();
const THRESHOLD = 72, MAX = 112;
const root = () => document.scrollingElement || document.documentElement;
const indicator = () => document.getElementById("pull-refresh");
let startX = 0, startY = null, pull = 0, active = false;
const reset = () => {
  const e = indicator();
  if (!e) return;
  e.style.removeProperty("--pull");
  e.style.removeProperty("--pull-turn");
  delete e.dataset.pulling;
  delete e.dataset.armed;
};
const start = (ev) => {
  startY = null; active = false; pull = 0;
  if (ev.touches.length !== 1 || root().scrollTop > 0 || document.querySelector("dialog[open]")) return;
  const t = ev.target;
  if (t && t.closest && t.closest("input,select,textarea")) return;
  startX = ev.touches[0].clientX; startY = ev.touches[0].clientY;
};
const move = (ev) => {
  if (startY === null || ev.touches.length !== 1) return;
  const dx = ev.touches[0].clientX - startX, dy = ev.touches[0].clientY - startY;
  if (!active) {
    if (dy < -8 || Math.abs(dx) > Math.abs(dy) + 4) { startY = null; return; }
    if (dy < 12 || root().scrollTop > 0) return;
    active = true;
  }
  pull = Math.min(MAX, Math.max(0, (dy - 12) * 0.5));
  const e = indicator();
  if (!e) return;
  e.dataset.pulling = "";
  e.style.setProperty("--pull", pull + "px");
  e.style.setProperty("--pull-turn", (pull * 3) + "deg");
  if (pull >= THRESHOLD) e.dataset.armed = ""; else delete e.dataset.armed;
};
const end = (ev) => {
  const fire = active && pull >= THRESHOLD && ev.type === "touchend";
  startY = null; active = false; pull = 0;
  reset();
  if (fire) dioxus.send(true);
};
const listeners = [["touchstart", start], ["touchmove", move], ["touchend", end], ["touchcancel", end]];
for (const [type, fn] of listeners) window.addEventListener(type, fn, { passive: true });
window.__ahdPull = () => {
  for (const [type, fn] of listeners) window.removeEventListener(type, fn);
  reset();
};
"#;

impl PullToRefresh {
    pub(crate) fn attach(mut on_pull: impl FnMut() + 'static) -> Self {
        let mut eval = document::eval(PULL_SCRIPT);
        let task = spawn(async move {
            while eval.recv::<bool>().await.is_ok() {
                on_pull();
            }
        });
        Self { task }
    }
}

impl Drop for PullToRefresh {
    fn drop(&mut self) {
        self.task.cancel();
        let _ = document::eval(
            "if (window.__ahdPull) { window.__ahdPull(); delete window.__ahdPull; }",
        );
    }
}

/// Whether a media query matches, kept current while the component is mounted.
pub(crate) fn use_media_query(query: &'static str) -> Signal<bool> {
    let mut matches = use_signal(|| false);
    use_hook(move || {
        let mut eval = document::eval(&format!(
            "const m = matchMedia({}); dioxus.send(m.matches); \
             m.addEventListener('change', (e) => dioxus.send(e.matches));",
            quoted(query)
        ));
        spawn(async move {
            while let Ok(value) = eval.recv::<bool>().await {
                matches.set(value);
            }
        });
    });
    matches
}

static HIDDEN: AtomicBool = AtomicBool::new(false);

/// Whether the WebView reports the page hidden (app in the background).
pub(crate) fn hidden() -> bool {
    HIDDEN.load(Ordering::Relaxed)
}

/// Tracks `document.hidden` and calls `on_visible` when the app returns to the foreground.
pub(crate) fn use_visibility(mut on_visible: impl FnMut() + 'static) {
    use_hook(move || {
        let mut eval = document::eval(
            "document.addEventListener('visibilitychange', () => dioxus.send(document.hidden)); \
             dioxus.send(document.hidden);",
        );
        spawn(async move {
            while let Ok(hidden) = eval.recv::<bool>().await {
                let was_hidden = HIDDEN.swap(hidden, Ordering::Relaxed);
                if was_hidden && !hidden {
                    on_visible();
                }
            }
        });
    });
}

/// In-WebView history so the Android back button returns to the previous page.
pub(crate) fn write_path(path: &str, replace: bool) {
    let method = if replace { "replaceState" } else { "pushState" };
    let _ = document::eval(&format!(
        "const p = {}; if (location.pathname !== p || location.hash) history.{method}(null, '', p);",
        quoted(path)
    ));
}

/// Calls `on_pop` with the new path after back/forward.
pub(crate) fn use_popstate(mut on_pop: impl FnMut(String) + 'static) {
    use_hook(move || {
        let mut eval = document::eval(
            "window.addEventListener('popstate', () => dioxus.send(location.pathname));",
        );
        spawn(async move {
            while let Ok(path) = eval.recv::<String>().await {
                on_pop(path);
            }
        });
    });
}
