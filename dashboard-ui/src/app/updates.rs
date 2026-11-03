//! Startup update check and the prompt/download/install dialog state.
use crate::mobile::{
    log, storage,
    update_install::{self, InstallNotice, InstallResult, Progress, SKIP_FILE, Stage, UpdateError},
    updates::{self, UpdateCheck},
};
use dioxus::{core::Task, prelude::*};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Flow {
    Hidden,
    Prompt,
    Downloading(Progress),
    Verifying,
    Installing,
    Failed(UpdateError),
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Updates {
    pub(crate) check: Signal<Option<UpdateCheck>>,
    pub(crate) flow: Signal<Flow>,
    task: Signal<Option<Task>>,
}
impl Updates {
    pub(crate) fn later(mut self) {
        self.flow.set(Flow::Hidden);
    }
    pub(crate) fn skip(mut self) {
        if let Some(check) = self.check.read().as_ref()
            && let Err(error) = storage::write(SKIP_FILE, &update_install::skip_json(&check.latest))
        {
            log::warn(&format!("could not save skipped update: {error}"));
        }
        self.flow.set(Flow::Hidden);
    }
    pub(crate) fn start(mut self, title: String, body: String, channel: &'static str) {
        if self.task.read().is_some()
            || !matches!(*self.flow.read(), Flow::Prompt | Flow::Failed(_))
        {
            return;
        }
        let Some(check) = self.check.read().clone() else {
            return;
        };
        let Some(asset) = check.asset else {
            return;
        };
        let latest = check.latest;
        self.flow.set(Flow::Downloading(Progress {
            received: 0,
            total: asset.size,
        }));
        let task = spawn(async move {
            let notice = InstallNotice {
                version: latest.to_string(),
                title: &title,
                body,
                channel,
            };
            let result = match update_install::download_and_verify(&asset, &latest, |stage| {
                self.flow.set(match stage {
                    Stage::Progress(p) => Flow::Downloading(p),
                    Stage::Verifying => Flow::Verifying,
                })
            })
            .await
            {
                Ok(prepared) => {
                    self.flow.set(Flow::Installing);
                    update_install::install(prepared, &notice).await
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
    pub(crate) fn cancel(mut self) {
        let task = self.task.write().take();
        if let Some(task) = task {
            task.cancel();
        }
        update_install::clear_downloads();
        self.flow.set(Flow::Prompt);
    }
    pub(crate) fn close(mut self) {
        self.flow.set(Flow::Hidden);
    }
}

/// Runs once at launch, without delaying startup, and preserves the result context.
pub(super) fn use_update_check() -> Updates {
    let mut check = use_context_provider(|| Signal::new(None::<UpdateCheck>));
    let mut flow = use_signal(|| Flow::Hidden);
    let task = use_signal(|| None::<Task>);
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
    update_install::use_install_events(move || {
        let result = update_install::take_install_result();
        if *flow.read() != Flow::Installing {
            return;
        }
        match result {
            Some(InstallResult::Aborted) => flow.set(Flow::Failed(UpdateError::InstallAborted)),
            Some(InstallResult::Failed) => flow.set(Flow::Failed(UpdateError::Install)),
            _ => {}
        }
    });
    Updates { check, flow, task }
}
