//! Android Back key. `MainActivity.kt` hands every Back press to the page (`window.__ahdBack`)
//! once this hook has installed it: the topmost open dialog closes through its own `onclose`
//! (as Escape would), a dialog that must stay open (an update that is downloading or
//! installing) keeps the key, and with no dialog open the activity goes back in the WebView
//! history, or to the background on the first entry (`AhdBack.navigate`). Every app dialog
//! opens through `dom::Modal` (`use_modal`), which registers it here, so this registry is the
//! one record of what is open; the stack logic is platform-free and tested natively.

/// What one Back press does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackAction<H> {
    /// Call the topmost dialog's close handler.
    Close(H),
    /// The topmost dialog must stay open: swallow the key.
    Stay,
    /// Nothing is open: previous screen, or background on the first one.
    Navigate,
}

struct Entry<H> {
    key: u64,
    closes: bool,
    onclose: H,
}

/// Open dialogs in the order they were shown (the last one is on top of the top layer).
pub(crate) struct OpenDialogs<H> {
    next: u64,
    entries: Vec<Entry<H>>,
}

impl<H: Copy> OpenDialogs<H> {
    pub(crate) const fn new() -> Self {
        Self {
            next: 0,
            entries: Vec::new(),
        }
    }

    /// Records a newly shown dialog; `closes` says whether Back may dismiss it.
    pub(crate) fn open(&mut self, closes: bool, onclose: H) -> u64 {
        let key = self.next;
        self.next += 1;
        self.entries.push(Entry {
            key,
            closes,
            onclose,
        });
        key
    }

    /// A dialog whose state changes while it stays open (the update dialog) updates this.
    pub(crate) fn set_closes(&mut self, key: u64, closes: bool) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            entry.closes = closes;
        }
    }

    pub(crate) fn remove(&mut self, key: u64) {
        self.entries.retain(|entry| entry.key != key);
    }

    pub(crate) fn back(&self) -> BackAction<H> {
        match self.entries.last() {
            None => BackAction::Navigate,
            Some(entry) if entry.closes => BackAction::Close(entry.onclose),
            Some(_) => BackAction::Stay,
        }
    }
}

#[cfg(feature = "mobile")]
mod runtime {
    use super::{BackAction, OpenDialogs};
    use dioxus::prelude::*;
    use std::cell::RefCell;

    thread_local! {
        // The VirtualDom (every dialog and the Back listener) runs on this one thread.
        static DIALOGS: RefCell<OpenDialogs<EventHandler<()>>> = const { RefCell::new(OpenDialogs::new()) };
    }

    pub(crate) fn register(closes: bool, onclose: EventHandler<()>) -> u64 {
        DIALOGS.with_borrow_mut(|dialogs| dialogs.open(closes, onclose))
    }

    pub(crate) fn set_closes(key: u64, closes: bool) {
        DIALOGS.with_borrow_mut(|dialogs| dialogs.set_closes(key, closes));
    }

    pub(crate) fn unregister(key: u64) {
        // `try_with`: a dialog dropped while the thread shuts down has nothing left to remove.
        let _ = DIALOGS.try_with(|dialogs| dialogs.borrow_mut().remove(key));
    }

    /// Installs the page side of the Back key; call once from the root component.
    pub(crate) fn use_back_key() {
        use_hook(|| {
            let mut eval = document::eval("window.__ahdBack = () => dioxus.send(true);");
            spawn(async move {
                while eval.recv::<bool>().await.is_ok() {
                    // Copied out first: closing drops the dialog, which unregisters it.
                    let action = DIALOGS.with_borrow(OpenDialogs::back);
                    match action {
                        BackAction::Close(onclose) => onclose.call(()),
                        BackAction::Stay => {}
                        BackAction::Navigate => navigate(),
                    }
                }
            });
        });
    }

    fn navigate() {
        if crate::mobile::notify::call_on(
            "dev.dioxus.main.AhdBack",
            "navigate",
            "(Landroid/app/Activity;)Z",
            &[],
            true,
        )
        .as_deref()
            != Some("true")
        {
            crate::mobile::log::warn("could not hand the Back key to the activity");
        }
    }
}
#[cfg(feature = "mobile")]
pub(crate) use runtime::{register, set_closes, unregister, use_back_key};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_navigates_only_when_nothing_is_open() {
        let mut dialogs = OpenDialogs::<&str>::new();
        assert_eq!(dialogs.back(), BackAction::Navigate);
        let menu = dialogs.open(true, "menu");
        assert_eq!(dialogs.back(), BackAction::Close("menu"));
        dialogs.remove(menu);
        assert_eq!(dialogs.back(), BackAction::Navigate);
    }

    #[test]
    fn back_closes_the_topmost_dialog_first() {
        let mut dialogs = OpenDialogs::<&str>::new();
        let update = dialogs.open(true, "update");
        let confirm = dialogs.open(true, "confirm");
        assert_eq!(dialogs.back(), BackAction::Close("confirm"));
        dialogs.remove(confirm);
        assert_eq!(dialogs.back(), BackAction::Close("update"));
        dialogs.remove(update);
        assert_eq!(dialogs.back(), BackAction::Navigate);
    }

    #[test]
    fn a_dialog_that_must_stay_keeps_the_key_without_exposing_the_one_below() {
        let mut dialogs = OpenDialogs::<&str>::new();
        dialogs.open(true, "menu");
        let update = dialogs.open(false, "update");
        assert_eq!(dialogs.back(), BackAction::Stay);
        dialogs.set_closes(update, true);
        assert_eq!(dialogs.back(), BackAction::Close("update"));
        dialogs.set_closes(update, false);
        assert_eq!(dialogs.back(), BackAction::Stay);
        dialogs.remove(update);
        assert_eq!(dialogs.back(), BackAction::Close("menu"));
    }

    #[test]
    fn removing_out_of_order_or_twice_keeps_the_others() {
        let mut dialogs = OpenDialogs::<&str>::new();
        let first = dialogs.open(true, "first");
        let second = dialogs.open(true, "second");
        dialogs.remove(first);
        dialogs.remove(first);
        dialogs.set_closes(first, false);
        assert_eq!(dialogs.back(), BackAction::Close("second"));
        let third = dialogs.open(true, "third");
        assert_ne!(third, first);
        dialogs.remove(second);
        assert_eq!(dialogs.back(), BackAction::Close("third"));
    }
}
