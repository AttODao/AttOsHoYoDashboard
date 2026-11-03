//! Startup update check and the prompt/download/install dialog state.
use crate::mobile::{
    log, notify, storage,
    update_install::{
        self, InstallNotice, InstallResult, Prepared, Progress, SKIP_FILE, Stage, Step, UpdateError,
    },
    updates::{self, UpdateCheck},
};
use dioxus::{core::Task, prelude::*};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Flow {
    Hidden,
    Prompt,
    /// Android's notification permission dialog is open; continues after the answer.
    AwaitingNotificationAnswer,
    /// "Install unknown apps" is off; continues by itself once the app is back with it on.
    NeedsInstallPermission,
    Downloading(Progress),
    Verifying,
    Installing,
    Failed(UpdateError),
}
/// Texts of the "updated" notification: title, body, channel name.
type NoticeTexts = (String, String, &'static str);
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Updates {
    pub(crate) check: Signal<Option<UpdateCheck>>,
    pub(crate) flow: Signal<Flow>,
    task: Signal<Option<Task>>,
    /// The verified APK while the installer runs or after it was cancelled (dropping deletes it).
    prepared: Signal<Option<Prepared>>,
    notice: Signal<Option<NoticeTexts>>,
}
impl Updates {
    pub(crate) fn later(mut self) {
        self.prepared.set(None);
        self.flow.set(Flow::Hidden);
    }
    pub(crate) fn skip(mut self) {
        if let Some(check) = self.check.read().as_ref()
            && let Err(error) = storage::write(SKIP_FILE, &update_install::skip_json(&check.latest))
        {
            log::warn(&format!("could not save skipped update: {error}"));
        }
        self.prepared.set(None);
        self.flow.set(Flow::Hidden);
    }
    pub(crate) fn start(mut self, title: String, body: String, channel: &'static str) {
        let from_prompt = match *self.flow.read() {
            Flow::Prompt => true,
            Flow::NeedsInstallPermission | Flow::Failed(_) => false,
            _ => return,
        };
        if self.task.read().is_some() {
            return;
        }
        self.notice.set(Some((title, body, channel)));
        self.proceed(from_prompt);
    }
    /// Asks for what is missing, then installs the kept APK or downloads a new one.
    fn proceed(mut self, from_prompt: bool) {
        if self.task.read().is_some() {
            return;
        }
        let Some(check) = self.check.read().clone() else {
            return;
        };
        let Some(asset) = check.asset else {
            return;
        };
        let Some((title, body, channel)) = self.notice.read().clone() else {
            return;
        };
        let latest = check.latest;
        let ready = self
            .prepared
            .read()
            .as_ref()
            .is_some_and(|prepared| prepared.is_for(&latest));
        let plan = update_install::plan(
            from_prompt,
            notify::permission_granted(),
            update_install::install_allowed(),
            ready,
        );
        if plan.request_notifications && update_install::request_notification_permission() {
            self.flow.set(Flow::AwaitingNotificationAnswer);
            return;
        }
        let kept = match plan.step {
            Step::AskInstallPermission => {
                self.flow.set(Flow::NeedsInstallPermission);
                return;
            }
            Step::Install => {
                self.flow.set(Flow::Installing);
                self.prepared.write().take()
            }
            Step::Download => {
                self.prepared.set(None);
                self.flow.set(Flow::Downloading(Progress {
                    received: 0,
                    total: asset.size,
                }));
                None
            }
        };
        let task = spawn(async move {
            let notice = InstallNotice {
                version: latest.to_string(),
                title: &title,
                body,
                channel,
            };
            let prepared = match kept {
                Some(prepared) => Ok(prepared),
                None => {
                    update_install::download_and_verify(&asset, &latest, |stage| {
                        self.flow.set(match stage {
                            Stage::Progress(p) => Flow::Downloading(p),
                            Stage::Verifying => Flow::Verifying,
                        })
                    })
                    .await
                }
            };
            let result = match prepared {
                Ok(prepared) => {
                    self.flow.set(Flow::Installing);
                    let (kept, result) = update_install::install(prepared, &notice).await;
                    if result.is_ok() {
                        self.prepared.set(kept);
                    }
                    result
                }
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                log::warn(&format!("update failed: {error}"));
                self.flow.set(Flow::Failed(error));
            }
            self.task.set(None);
        });
        self.task.set(Some(task));
    }
    pub(crate) fn open_settings(mut self) {
        if !update_install::open_install_settings() {
            self.flow.set(Flow::Failed(UpdateError::Install));
        }
    }
    pub(crate) fn cancel(mut self) {
        let task = self.task.write().take();
        if let Some(task) = task {
            task.cancel();
        }
        self.prepared.set(None);
        update_install::clear_downloads();
        self.flow.set(Flow::Prompt);
    }
    pub(crate) fn close(mut self) {
        self.prepared.set(None);
        self.flow.set(Flow::Hidden);
    }
}

/// Runs once at launch, without delaying startup, and preserves the result context.
pub(super) fn use_update_check() -> Updates {
    let mut check = use_context_provider(|| Signal::new(None::<UpdateCheck>));
    let mut flow = use_signal(|| Flow::Hidden);
    let task = use_signal(|| None::<Task>);
    let prepared = use_signal(|| None::<Prepared>);
    let notice = use_signal(|| None::<NoticeTexts>);
    use_future(move || async move {
        update_install::clear_downloads();
        match updates::check().await {
            Ok(result) => {
                log::info(&format!(
                    "update check: current {}, latest {}, update available: {}",
                    result.current, result.latest, result.update_available
                ));
                let prompt = update_install::should_prompt(
                    &result,
                    storage::read(SKIP_FILE)
                        .as_deref()
                        .and_then(update_install::skipped_version)
                        .as_ref(),
                );
                check.set(Some(result));
                if prompt {
                    flow.set(Flow::Prompt);
                }
            }
            Err(error) => log::warn(&format!("update check failed: {error}")),
        }
    });
    let updates = Updates {
        check,
        flow,
        task,
        prepared,
        notice,
    };
    // Installer results, and every return to the foreground (no result then).
    update_install::use_install_events(move || {
        let mut updates = updates;
        let result = update_install::take_install_result();
        let state = updates.flow.peek().clone();
        match state {
            Flow::Installing => {
                if let Some(result @ (InstallResult::Aborted | InstallResult::Failed)) = result {
                    if !update_install::keeps_verified_package(result) {
                        updates.prepared.set(None);
                    }
                    let error = if result == InstallResult::Aborted {
                        UpdateError::InstallAborted
                    } else {
                        UpdateError::Install
                    };
                    updates.flow.set(Flow::Failed(error));
                }
            }
            Flow::AwaitingNotificationAnswer => updates.proceed(false),
            // Back from the settings screen: continue as soon as the permission is on.
            Flow::NeedsInstallPermission if update_install::install_allowed() => {
                updates.proceed(false)
            }
            _ => {}
        }
    });
    updates
}
