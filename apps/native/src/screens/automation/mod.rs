//! "Automação do projeto" (`WorkspaceProjectAutomationDialog`): the task's
//! `.dcc.toml` read from the desktop, edited as a draft (setup, delivery
//! policy, checks and fixes with their hooks), saved against the hash it
//! was read with, and its tasks run in the checkout with the report below.
//! Runs wait for a save; a fix asks first.

mod tasks;
#[cfg(test)]
mod tests;
mod view;

use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Window};

use crate::data::automation::{Config, Draft, Kind, RunOutput, Task};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::Translate;
use crate::ui::Toaster;
use crate::ui::text_input::{TextInput, TextInputEvent};

pub enum AutomationEvent {
    Closed,
    /// A run changed files: the checkout's state is read again.
    WorkspaceChanged,
}

/// A task card's fields, kept by a stable number (rows move when one goes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Id,
    Label,
    Cwd,
    Command,
    Timeout,
}

pub(crate) struct TaskFields {
    uid: usize,
    pub(crate) id: Entity<TextInput>,
    pub(crate) label: Entity<TextInput>,
    pub(crate) cwd: Entity<TextInput>,
    pub(crate) command: Entity<TextInput>,
    pub(crate) timeout: Entity<TextInput>,
    _events: Vec<Subscription>,
}

pub enum Loaded {
    Loading,
    Failed(String),
    Ready(Config),
}

pub struct ProjectAutomation {
    root: String,
    api: Option<DesktopApi>,
    pub(crate) loaded: Loaded,
    pub(crate) draft: Draft,
    pub(crate) saving: bool,
    pub(crate) running: Vec<String>,
    pub(crate) result: Option<RunOutput>,
    /// Report steps opened or closed by hand (failed ones start open).
    pub(crate) toggled: std::collections::HashSet<usize>,
    /// "Esta tarefa pode alterar arquivos…?" for these ids.
    pub(crate) confirm_fix: Option<Vec<String>>,
    pub(crate) setup: Entity<TextInput>,
    pub(crate) approvals: Entity<TextInput>,
    pub(crate) fields: Vec<TaskFields>,
    next_uid: usize,
    _inputs: Vec<Subscription>,
}

impl EventEmitter<AutomationEvent> for ProjectAutomation {}

fn input(
    text: &str,
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<ProjectAutomation>,
) -> Entity<TextInput> {
    let placeholder = placeholder.to_string();
    let text = text.to_string();
    cx.new(|cx| {
        let mut input = TextInput::new(window, cx).with_placeholder(placeholder);
        input.set_text(text, cx);
        input
    })
}

impl ProjectAutomation {
    pub fn new(
        root: String,
        api: Option<DesktopApi>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let setup_placeholder = cx.t("automation.setupPlaceholder").to_string();
        let setup = input("", &setup_placeholder, window, cx);
        let approvals = input("0", "", window, cx);
        let inputs = vec![
            cx.subscribe(&setup, |this, input, event, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    this.draft.setup_command = input.read(cx).text().to_string();
                    cx.notify();
                }
            }),
            cx.subscribe(&approvals, |this, input, event, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    // `Math.min(20, Math.max(0, Number(value) || 0))`
                    let typed: f64 = input.read(cx).text().trim().parse().unwrap_or(0.0);
                    this.draft.policy.minimum_approvals = typed.clamp(0.0, 20.0) as u32;
                    cx.notify();
                }
            }),
        ];
        let mut automation = Self {
            root,
            api,
            loaded: Loaded::Loading,
            draft: Draft::default(),
            saving: false,
            running: Vec::new(),
            result: None,
            toggled: Default::default(),
            confirm_fix: None,
            setup,
            approvals,
            fields: Vec::new(),
            next_uid: 0,
            _inputs: inputs,
        };
        automation.load(window, cx);
        automation
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, root) = (self.api.clone(), self.root.clone());
        cx.spawn_in(window, async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.ok_or(ApiError::DesktopClosed)?.automation(&root) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match read {
                Ok(config) => this.apply(config, window, cx),
                Err(error) => {
                    this.loaded = Loaded::Failed(error.to_string());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The config read or saved becomes the draft and the fields.
    fn apply(&mut self, config: Config, window: &mut Window, cx: &mut Context<Self>) {
        self.draft = Draft::from(&config);
        let setup = self.draft.setup_command.clone();
        self.setup.update(cx, |input, cx| input.set_text(setup, cx));
        let approvals = self.draft.policy.minimum_approvals.to_string();
        self.approvals
            .update(cx, |input, cx| input.set_text(approvals, cx));
        self.loaded = Loaded::Ready(config);
        self.rebuild_fields(window, cx);
        cx.notify();
    }

    fn rebuild_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tasks = self.draft.tasks.clone();
        self.fields = tasks
            .iter()
            .map(|task| self.task_fields(task, window, cx))
            .collect();
    }

    fn task_fields(
        &mut self,
        task: &Task,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TaskFields {
        let uid = self.next_uid;
        self.next_uid += 1;
        let id = input(&task.id, "", window, cx);
        let label = input(task.label.as_deref().unwrap_or_default(), "", window, cx);
        let cwd = input(task.cwd.as_deref().unwrap_or_default(), ".", window, cx);
        let command = input(&task.command, "yarn lint", window, cx);
        let timeout = input(&task.timeout_seconds.to_string(), "", window, cx);
        let events = [
            (&id, Part::Id),
            (&label, Part::Label),
            (&cwd, Part::Cwd),
            (&command, Part::Command),
            (&timeout, Part::Timeout),
        ]
        .into_iter()
        .map(|(field, part)| {
            cx.subscribe(field, move |this, input, event, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    let text = input.read(cx).text().to_string();
                    this.typed(uid, part, text, cx);
                }
            })
        })
        .collect();
        TaskFields {
            uid,
            id,
            label,
            cwd,
            command,
            timeout,
            _events: events,
        }
    }

    fn typed(&mut self, uid: usize, part: Part, text: String, cx: &mut Context<Self>) {
        let Some(index) = self.fields.iter().position(|fields| fields.uid == uid) else {
            return;
        };
        self.draft.update_task(index, |task| match part {
            Part::Id => task.id = text,
            Part::Label => task.label = Some(text),
            Part::Cwd => task.cwd = (!text.is_empty()).then_some(text),
            Part::Command => task.command = text,
            // `Number(value)`: what is not one reads 0 (the desktop refuses it).
            Part::Timeout => task.timeout_seconds = text.trim().parse().unwrap_or(0),
        });
        cx.notify();
    }

    pub(crate) fn set_kind(&mut self, index: usize, kind: Kind, cx: &mut Context<Self>) {
        self.draft.update_task(index, |task| task.kind = kind);
        cx.notify();
    }

    pub(crate) fn add_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.draft.add_task();
        if let Some(task) = self.draft.tasks.last().cloned() {
            let fields = self.task_fields(&task, window, cx);
            self.fields.push(fields);
        }
        cx.notify();
    }

    pub(crate) fn remove_task(&mut self, index: usize, cx: &mut Context<Self>) {
        self.draft.remove_task(index);
        if index < self.fields.len() {
            self.fields.remove(index);
        }
        cx.notify();
    }

    pub(crate) fn config(&self) -> Option<&Config> {
        match &self.loaded {
            Loaded::Ready(config) => Some(config),
            _ => None,
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.config().is_some_and(|config| self.draft.dirty(config))
    }

    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.config() else {
            return;
        };
        let (api, root, draft, hash) = (
            self.api.clone(),
            self.root.clone(),
            self.draft.clone(),
            config.config_hash.clone(),
        );
        self.saving = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    api.ok_or(ApiError::DesktopClosed)?.save_automation(
                        &root,
                        &draft,
                        hash.as_deref(),
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match saved {
                    Ok(config) => {
                        this.apply(config, window, cx);
                        Toaster::success(cx.t("automation.saved"), cx);
                    }
                    Err(error) => {
                        Toaster::error_with(cx.t("automation.saveFailed"), error.to_string(), cx)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `runTasks`: a fix asks first.
    pub(crate) fn run(&mut self, ids: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        let fix = self
            .draft
            .tasks
            .iter()
            .any(|task| ids.contains(&task.id) && task.kind == Kind::Fix);
        if fix {
            self.confirm_fix = Some(ids);
            cx.notify();
            return;
        }
        self.run_now(ids, window, cx);
    }

    pub(crate) fn run_now(
        &mut self,
        ids: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_fix = None;
        let Some(config) = self.config() else {
            return;
        };
        if ids.is_empty() {
            return;
        }
        let (api, root, hash) = (
            self.api.clone(),
            self.root.clone(),
            config.config_hash.clone(),
        );
        self.running = ids.clone();
        self.result = None;
        self.toggled.clear();
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let ran = cx
                .background_executor()
                .spawn(async move {
                    api.ok_or(ApiError::DesktopClosed)?
                        .run_automation(&root, &ids, hash.as_deref())
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.running.clear();
                match ran {
                    Ok(output) => {
                        if output.passed() {
                            Toaster::success(cx.t("automation.runPassed"), cx);
                        } else {
                            Toaster::error(cx.t("automation.runFailed"), cx);
                        }
                        if output.changed_files {
                            cx.emit(AutomationEvent::WorkspaceChanged);
                            this.load(window, cx);
                        }
                        this.result = Some(output);
                    }
                    Err(error) => {
                        Toaster::error_with(cx.t("automation.runFailed"), error.to_string(), cx)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(AutomationEvent::Closed);
    }
}
