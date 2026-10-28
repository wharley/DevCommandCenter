//! The quick composer (`QuickComposer`): over any app, a prompt becomes a
//! new task in the project picked, Local or Worktree, on the provider and
//! model picked. Its launch keeps the desktop's journal step by step; an
//! interrupted one shows the prompt kept and waits for the person to check
//! the task before another goes. The composer is the app's own, in quick
//! mode (its send comes here).

#[cfg(test)]
mod tests;
mod view;

use std::path::PathBuf;

use gpui::{AppContext, Context, Entity, FocusHandle, Focusable, Subscription, Window};

use crate::data::conversation::ThreadStart;
use crate::data::desktop_api::DesktopApi;
use crate::data::quick_composer::{
    DesktopSteps, Envelope, LOCAL, Launch, Request, Selection, Turn, WORKTREE, execute, launch_of,
};
use crate::data::tasks::{Project, TaskStatus};
use crate::i18n::Translate;
use crate::screens::composer::{Composer, ComposerEvent, ComposerSession, QuickTurn};

pub const WIDTH: f32 = 720.0;
pub const HEIGHT: f32 = 440.0;

pub enum QuickComposerEvent {
    /// Esc, ✕, or a launch that went: the window goes.
    Close,
    /// "Abrir no DCC": the main window, on the launch's task.
    OpenTask {
        workspace_id: Option<String>,
        session_id: Option<String>,
    },
}

pub struct QuickComposer {
    pub api: Option<DesktopApi>,
    pub db_path: PathBuf,
    pub selection_path: Option<PathBuf>,
    pub selection: Selection,
    pub projects: Vec<Project>,
    pub loading: bool,
    pub error: Option<String>,
    pub launch: Option<Launch>,
    /// The completed launch still worth showing (its task is open).
    pub visible_completed: Option<String>,
    /// The interrupted launch the person checked.
    pub reviewed: Option<String>,
    pub busy: bool,
    /// `loadSequence`: an older read never lands over a newer one.
    load_sequence: u64,
    pub composer: Entity<Composer>,
    focus: FocusHandle,
    _composer: Subscription,
}

impl gpui::EventEmitter<QuickComposerEvent> for QuickComposer {}

impl Focusable for QuickComposer {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl QuickComposer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| {
            let mut composer = Composer::new(window, cx);
            composer.quick = true;
            composer
        });
        let events = cx.subscribe_in(&composer, window, |quick, _, event, window, cx| {
            if let ComposerEvent::QuickSubmit(turn) = event {
                quick.submit((**turn).clone(), window, cx);
            }
        });
        let selection_path =
            (!cfg!(test)).then(|| crate::prefs::native_file("quick-composer.json"));
        let selection = selection_path
            .as_deref()
            .map(Selection::load)
            .unwrap_or_default();
        let mut quick = Self {
            api: if cfg!(test) { None } else { DesktopApi::load() },
            db_path: if cfg!(test) {
                PathBuf::from("/nonexistent/database.sqlite")
            } else {
                crate::data::loader::database_path()
            },
            selection_path,
            selection,
            projects: Vec::new(),
            loading: true,
            error: None,
            launch: None,
            visible_completed: None,
            reviewed: None,
            busy: false,
            load_sequence: 0,
            composer,
            focus: cx.focus_handle(),
            _composer: events,
        };
        quick.refresh(window, cx);
        quick
    }

    pub fn project(&self) -> Option<&Project> {
        self.projects
            .iter()
            .find(|project| project.project_id == self.selection.project_id)
    }

    pub fn mode(&self) -> &'static str {
        self.selection.mode()
    }

    /// An interrupted launch nobody checked yet blocks another.
    pub fn unfinished(&self) -> bool {
        self.launch.as_ref().is_some_and(|launch| {
            launch.phase != "completed" && self.reviewed.as_deref() != Some(launch.id.as_str())
        })
    }

    /// No project, or nothing to run it on.
    pub fn blocked(&self, cx: &gpui::App) -> bool {
        self.project().is_none() || self.composer.read(cx).new_task_pick().is_none()
    }

    fn save_selection(&mut self, cx: &gpui::App) {
        if let Some(path) = &self.selection_path
            && self.selection.save(path).is_err()
        {
            self.error = Some(cx.t("quickComposer.saveFailed").to_string());
        }
    }

    /// The composer on the project picked: its field, draft and pick.
    fn point_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let session = self.project().map(|project| ComposerSession {
            start: Some(ThreadStart {
                workspace_id: format!("quick-composer:{}", project.project_id),
                project_id: project.project_id.clone(),
                additional_workspace_ids: Vec::new(),
                name: None,
                auto_named: true,
            }),
            workspace_path: Some(project.root_path.clone()),
            project_id: Some(project.project_id.clone()),
            ..ComposerSession::default()
        });
        self.composer.update(cx, |composer, cx| {
            composer.set_session(session.unwrap_or_default(), cx);
            composer.focus(window, cx);
        });
    }

    /// `refresh`: the projects, the providers and the last launch.
    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load_sequence += 1;
        let sequence = self.load_sequence;
        self.loading = true;
        cx.notify();
        let (api, db_path) = (self.api.clone(), self.db_path.clone());
        let untitled = cx.t("sidebar.newWorkspace").to_string();
        cx.spawn_in(window, async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let data = crate::data::loader::load(&db_path, &untitled)
                        .map_err(|error| error.to_string())?;
                    let launch = api
                        .as_ref()
                        .and_then(|api| api.quick_composer_raw().ok())
                        .and_then(|status| launch_of(&status));
                    Ok::<_, String>((data, launch))
                })
                .await;
            let _ = this.update_in(cx, |quick, window, cx| {
                if sequence != quick.load_sequence {
                    return;
                }
                quick.loading = false;
                match read {
                    Ok((data, launch)) => {
                        quick.projects = data.projects;
                        if !quick.busy {
                            quick.visible_completed = launch.as_ref().and_then(|launch| {
                                let open = data.tasks.iter().any(|task| {
                                    Some(&task.id) == launch.workspace_id.as_ref()
                                        && !matches!(
                                            task.status,
                                            TaskStatus::Completed | TaskStatus::Archived
                                        )
                                });
                                open.then(|| launch.id.clone())
                            });
                            quick.launch = launch;
                        }
                        // Keep a saved choice even when it is gone; never send
                        // somewhere else silently.
                        if quick.selection.project_id.is_empty()
                            && let Some(first) = quick.projects.first()
                        {
                            quick.selection.project_id = first.project_id.clone();
                            quick.save_selection(cx);
                        }
                        quick.error = None;
                    }
                    Err(error) => quick.error = Some(error),
                }
                quick.point_composer(window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn set_project(&mut self, project_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selection.project_id = project_id;
        self.save_selection(cx);
        self.point_composer(window, cx);
        cx.notify();
    }

    pub fn set_mode(&mut self, local: bool, cx: &mut Context<Self>) {
        let project = self.selection.project_id.clone();
        self.selection
            .modes
            .insert(project, if local { LOCAL } else { WORKTREE }.to_string());
        self.save_selection(cx);
        cx.notify();
    }

    /// `submit`: the launch, step by step, off the UI thread.
    fn submit(&mut self, turn: QuickTurn, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.unfinished() || self.blocked(cx) {
            return;
        }
        let (Some(api), Some(project)) = (self.api.clone(), self.project().cloned()) else {
            return;
        };
        let options = &turn.options;
        let request = Request {
            project_id: project.project_id.clone(),
            root_path: project.root_path.clone(),
            base_branch: project.base_branch.clone(),
            isolation_mode: self.mode().to_string(),
            provider_runtime: if cfg!(test) {
                None
            } else {
                crate::data::provider_runtime::for_provider(&turn.provider_id)
            },
            provider_id: turn.provider_id.clone(),
            model_id: turn.model_id.clone(),
            turn: Turn {
                raw_prompt: turn.prompt.clone(),
                envelope: Envelope {
                    effort: options.effort.clone(),
                    approval_policy: options.approval_policy.clone(),
                    plan_mode: options.plan_mode,
                    fast_mode: options.fast_mode,
                    tool_instructions: options.tool_instructions.clone(),
                },
            },
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        let steps = DesktopSteps {
            api,
            db_path: self.db_path.clone(),
        };
        let (progress, mut seen) = futures::channel::mpsc::unbounded::<Launch>();
        cx.spawn_in(window, async move |this, cx| {
            use futures::StreamExt;
            let work = cx.background_executor().spawn(async move {
                let mut report = |launch: &Launch| {
                    let _ = progress.unbounded_send(launch.clone());
                };
                execute(Launch::new(request), &steps, &mut report)
            });
            // Each saved step shows as it happens.
            let follow = async {
                while let Some(launch) = seen.next().await {
                    let _ = this.update(cx, |quick, cx| {
                        quick.launch = Some(launch);
                        cx.notify();
                    });
                }
            };
            let (done, ()) = futures::join!(work, follow);
            let _ = this.update_in(cx, |quick, _, cx| {
                quick.busy = false;
                let completed = done.phase == "completed";
                if completed {
                    quick.visible_completed = Some(done.id.clone());
                }
                quick.launch = Some(done);
                if completed {
                    quick
                        .composer
                        .update(cx, |composer, cx| composer.clear_quick(cx));
                    cx.emit(QuickComposerEvent::Close);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Já conferi a tarefa · liberar novo pedido": the launch marked failed.
    pub fn recover(&mut self, cx: &mut Context<Self>) {
        let Some(launch) = self.launch.clone() else {
            return;
        };
        if self.busy {
            return;
        }
        if launch.phase == "failed" {
            self.reviewed = Some(launch.id);
            cx.notify();
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        let interrupted = cx.t("quickComposer.interrupted").to_string();
        let next = Launch {
            revision: launch.revision + 1,
            phase: "failed".into(),
            error: Some(interrupted),
            ..launch
        };
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    use crate::data::quick_composer::Steps;
                    DesktopSteps {
                        api,
                        db_path: PathBuf::new(),
                    }
                    .checkpoint(&next)
                })
                .await;
            let _ = this.update(cx, |quick, cx| {
                match saved {
                    Ok(saved) => {
                        quick.reviewed = Some(saved.id.clone());
                        quick.launch = Some(saved);
                    }
                    Err(error) => quick.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn open_task(&mut self, cx: &mut Context<Self>) {
        let launch = self.launch.as_ref();
        cx.emit(QuickComposerEvent::OpenTask {
            workspace_id: launch.and_then(|launch| launch.workspace_id.clone()),
            session_id: launch.and_then(|launch| launch.session_id.clone()),
        });
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(QuickComposerEvent::Close);
    }
}
