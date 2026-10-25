//! The workbench stage (`stageContent` in `WorkspacePanel`): content opened
//! over the conversation, in a tab strip whose first tab is the Chat. The
//! conversation stays mounted underneath; the Chat tab shrinks the stage to
//! its strip, with the agent's work and an unread reply marked on it. Esc
//! goes back to the Chat. The content is a change's diff, opened from
//! "Mudanças" (`WorkspaceEditorSurface`, side by side for a modified file),
//! or files in tabs (`FileTabsSurface`).

mod dialogs;
mod files;
mod image;
mod save;
mod view;

#[cfg(test)]
mod tests;

use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
use gpui::{
    AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, Task, Window, actions,
};

use crate::screens::diff_view::DiffView;
use crate::ui::tab_strip::TabStripState;

actions!(stage, [ShowChat, Save]);

const CONTEXT: &str = "Stage";

pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("escape", ShowChat, Some(CONTEXT)),
        gpui::KeyBinding::new("cmd-s", Save, Some(CONTEXT)),
    ]);
}

/// What an unsaved-changes question is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Discard {
    Tab(usize),
    All,
    /// A diff opening in place of the files.
    ForDiff(DiffRequest),
}

/// The disk moved on under an edit (`ReconcileDialog`).
pub(crate) struct Reconcile {
    pub path: String,
    pub name: String,
    pub disk: String,
    pub mine: String,
    pub view: Entity<crate::screens::file_view::FileView>,
    pub diff: Entity<DiffView>,
}

/// A change to show (`WorkspaceGitPreviewSelection`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffRequest {
    pub root: String,
    pub path: String,
    pub name: String,
    pub status: String,
    pub scope: WorkspaceGitPreviewScope,
    /// The branch's base, for a committed change.
    pub base: Option<String>,
    /// Opened from "Mudanças" (`gitDiffExpanded`): closing goes back there.
    pub from_changes: bool,
}

pub(crate) enum DiffLoad {
    Loading(#[allow(dead_code)] Task<()>),
    Ready(Entity<DiffView>),
    Failed(String),
}

pub(crate) struct StageDiff {
    pub request: DiffRequest,
    pub load: DiffLoad,
    /// The theme it was colored for.
    dark: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageEvent {
    /// The content closed; back to "Mudanças" when it was opened from there.
    Closed { to_changes: bool },
}

pub struct Stage {
    pub(crate) diff: Option<StageDiff>,
    /// Or the open files (one or the other: opening one replaces the other).
    pub(crate) files: Option<files::FileTabs>,
    /// The Chat tab is in front (the stage shows only its strip).
    pub(crate) chat_active: bool,
    /// The agent finished while the content was in front.
    pub(crate) chat_unread: bool,
    pub(crate) agent_working: bool,
    pub(crate) strip: TabStripState,
    focus: FocusHandle,
    /// "Descartar alterações não salvas?" waiting for an answer.
    pub(crate) discard: Option<Discard>,
    pub(crate) reconcile: Option<Reconcile>,
    /// The editor takes the focus at the next frame (a file just read).
    pub(crate) focus_editor: bool,
    /// The desktop to save through (tests point it elsewhere).
    pub(crate) endpoint: Option<crate::data::desktop_api::DesktopApi>,
    _save: Option<Task<()>>,
}

impl EventEmitter<StageEvent> for Stage {}

impl Focusable for Stage {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Stage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            diff: None,
            files: None,
            chat_active: false,
            chat_unread: false,
            agent_working: false,
            strip: TabStripState::default(),
            focus: cx.focus_handle(),
            discard: None,
            reconcile: None,
            focus_editor: false,
            endpoint: None,
            _save: None,
        }
    }

    /// Something is on the stage.
    pub fn has_content(&self) -> bool {
        self.diff.is_some() || self.files.is_some()
    }

    pub fn chat_active(&self) -> bool {
        self.chat_active
    }

    /// The change on the stage (its row is marked in "Mudanças").
    pub fn open_diff_path(&self) -> Option<(&WorkspaceGitPreviewScope, &str)> {
        self.diff
            .as_ref()
            .map(|diff| (&diff.request.scope, diff.request.path.as_str()))
    }

    /// A new request brings the content forward (`stageRequestKey`).
    pub fn open_diff(&mut self, request: DiffRequest, window: &mut Window, cx: &mut Context<Self>) {
        // Unsaved files ask before the diff takes their place.
        let dirty = self
            .files
            .as_ref()
            .is_some_and(|tabs| tabs.files.iter().any(|file| file.is_dirty(cx)));
        if dirty {
            self.discard = Some(Discard::ForDiff(request));
            cx.notify();
            return;
        }
        self.replace_with_diff(request, window, cx);
    }

    pub(crate) fn replace_with_diff(
        &mut self,
        request: DiffRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files = None;
        self.chat_active = false;
        let dark = cx.theme_dark();
        self.load(request, dark, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn load(&mut self, request: DiffRequest, dark: bool, cx: &mut Context<Self>) {
        let read = request.clone();
        let task = cx.spawn(async move |this, cx| {
            let path = read.path.clone();
            let content = cx
                .background_executor()
                .spawn(async move {
                    dcc_infra::workspace_file_preview::file_preview_content(
                        &read.root,
                        &read.path,
                        &read.status,
                        &read.scope,
                        read.base.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |stage, cx| {
                let Some(diff) = stage.diff.as_mut().filter(|diff| diff.request.path == path)
                else {
                    return;
                };
                diff.load = match content {
                    // `diffStyle: inline ? "unified" : "split"`: added, removed and
                    // new files read as one column, a modified one side by side.
                    Ok((original, modified, inline)) => DiffLoad::Ready(cx.new(|cx| {
                        if inline {
                            DiffView::new(path.clone(), original, modified, dark, cx)
                        } else {
                            DiffView::side_by_side(path.clone(), original, modified, dark, cx)
                        }
                    })),
                    Err(error) => DiffLoad::Failed(error),
                };
                cx.notify();
            });
        });
        self.diff = Some(StageDiff {
            request,
            load: DiffLoad::Loading(task),
            dark,
        });
    }

    /// A diff colored for the other theme is read again.
    pub(crate) fn follow_theme(&mut self, cx: &mut Context<Self>) {
        let dark = cx.theme_dark();
        if let Some(diff) = self.diff.as_ref().filter(|diff| diff.dark != dark) {
            let request = diff.request.clone();
            self.load(request, dark, cx);
        }
    }

    /// The tab's X (`onCloseSurface`).
    pub fn close(&mut self, cx: &mut Context<Self>) {
        let to_changes = match (self.diff.take(), self.files.take()) {
            (Some(diff), _) => diff.request.from_changes,
            (None, Some(_)) => false,
            (None, None) => return,
        };
        self.chat_active = false;
        self.chat_unread = false;
        cx.emit(StageEvent::Closed { to_changes });
        cx.notify();
    }

    /// Another task: the stage starts empty.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.diff.take().is_some() | self.files.take().is_some() {
            self.chat_active = false;
            self.chat_unread = false;
            cx.notify();
        }
    }

    pub fn show_chat(&mut self, cx: &mut Context<Self>) {
        if self.has_content() && !self.chat_active {
            self.chat_active = true;
            self.chat_unread = false;
            cx.notify();
        }
    }

    pub fn show_content(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.chat_active {
            self.chat_active = false;
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    /// The agent's turn: one that ends behind the content marks the Chat tab.
    pub fn set_agent_working(&mut self, working: bool, cx: &mut Context<Self>) {
        if self.agent_working == working {
            return;
        }
        if self.agent_working && !working && !self.chat_active && self.has_content() {
            self.chat_unread = true;
        }
        self.agent_working = working;
        cx.notify();
    }
}

trait ThemeDark {
    fn theme_dark(&self) -> bool;
}

impl<T> ThemeDark for Context<'_, T> {
    fn theme_dark(&self) -> bool {
        use crate::theme::{ActiveTheme, Appearance};
        self.theme().appearance == Appearance::Dark
    }
}
