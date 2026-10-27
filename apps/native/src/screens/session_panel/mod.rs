//! The stage's "Sessão" tab (`WorkspaceInspectorSidebar variant="session"`):
//! a header with the milestones' count, its tabs, and "Atividade" — the
//! workspace's delegations (open the child, review its diff, apply, discard,
//! mark reviewed) over the session's live events (`SessionEventFeed`).
//! It reads the open conversation as it refreshes.

mod activity;
mod delegation;
mod feed;

#[cfg(test)]
mod tests;

use gpui::{
    Context, Entity, EventEmitter, IntoElement, Render, ScrollHandle, Subscription, Window,
};

use crate::data::conversation::{Conversation, DelegationRecord};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::{Args, Translate};
use crate::screens::conversation::ConversationView;
use crate::screens::stage::DiffRequest;
use crate::ui::Toaster;
use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionTab {
    #[default]
    Activity,
    Context,
    Spec,
}

impl SessionTab {
    pub fn key(self) -> &'static str {
        match self {
            Self::Activity => "activity",
            Self::Context => "context",
            Self::Spec => "spec",
        }
    }
}

pub enum SessionPanelEvent {
    /// "Abrir filha".
    OpenSession(String),
    /// A delegation's file, on the stage in the Session tab's place.
    OpenDiff(Box<DiffRequest>),
}

pub struct SessionPanel {
    pub(crate) conversation: Entity<ConversationView>,
    pub(crate) tab: SessionTab,
    /// `scope`: this session's events only.
    pub(crate) current_only: bool,
    /// `view`: the milestones (or every event, for diagnostics).
    pub(crate) summary: bool,
    /// `apply:<id>`, `discard:<id>`, `approve:<id>` while it runs.
    pub(crate) busy: Option<String>,
    pub(crate) feed_scroll: ScrollHandle,
    /// The last event shown, to follow new ones to the bottom.
    pub(crate) last_event: Option<String>,
    session: Option<String>,
    pub(crate) api: Option<DesktopApi>,
    _conversation: Subscription,
}

impl EventEmitter<SessionPanelEvent> for SessionPanel {}

impl SessionPanel {
    pub fn new(
        conversation: Entity<ConversationView>,
        tab: SessionTab,
        cx: &mut Context<Self>,
    ) -> Self {
        let session = conversation
            .read(cx)
            .data()
            .and_then(|data| data.session_id.clone());
        let observe = cx.observe(&conversation, |panel, conversation, cx| {
            let session = conversation
                .read(cx)
                .data()
                .and_then(|data| data.session_id.clone());
            // Another conversation opens on its activity.
            if session != panel.session {
                panel.session = session;
                panel.tab = SessionTab::Activity;
            }
            cx.notify();
        });
        Self {
            conversation,
            tab,
            current_only: true,
            summary: true,
            busy: None,
            feed_scroll: ScrollHandle::new(),
            last_event: None,
            session,
            api: if cfg!(test) { None } else { DesktopApi::load() },
            _conversation: observe,
        }
    }

    pub fn set_tab(&mut self, tab: SessionTab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    pub(crate) fn data<'a>(&self, cx: &'a gpui::App) -> Option<&'a Conversation> {
        self.conversation.read(cx).data()
    }

    /// `selectDelegationPreview`: the worktree's change for an
    /// implementation waiting for review, else the task's.
    pub(crate) fn review_file(
        &mut self,
        delegation: &DelegationRecord,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        let pending = delegation.awaits_review();
        let root = if pending {
            match &delegation.worktree {
                Some(worktree) => worktree.clone(),
                None => {
                    let key = if delegation.child_session_id.is_none() {
                        "inspector.delegations.applyMissingChild"
                    } else {
                        "inspector.delegations.applyMissingWorktree"
                    };
                    Toaster::error(cx.t(key), cx);
                    return;
                }
            }
        } else {
            let Some(root) = self.data(cx).and_then(|data| data.workspace_path.clone()) else {
                return;
            };
            root
        };
        let unstaged = delegation.implement
            && matches!(
                delegation.status,
                dcc_core::domain::delegation::DelegationStatus::ReviewPending
                    | dcc_core::domain::delegation::DelegationStatus::Completed
            );
        cx.emit(SessionPanelEvent::OpenDiff(Box::new(DiffRequest {
            root,
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            status: "M".into(),
            scope: if unstaged {
                WorkspaceGitPreviewScope::Unstaged
            } else {
                WorkspaceGitPreviewScope::Committed
            },
            base: None,
            from_changes: false,
            review: None,
        })));
    }

    fn run<T: Send + 'static>(
        &mut self,
        busy: String,
        action: impl FnOnce(DesktopApi) -> Result<T, ApiError> + Send + 'static,
        done: impl FnOnce(T, &gpui::App) -> gpui::SharedString + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        self.busy = Some(busy);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { action(api) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.busy = None;
                match result {
                    Ok(value) => {
                        let text = done(value, cx);
                        Toaster::success(text, cx)
                    }
                    Err(error) => Toaster::error(error.to_string(), cx),
                }
                let conversation = panel.conversation.clone();
                conversation.update(cx, |conversation, cx| conversation.refresh_now(cx));
                cx.notify();
            });
        })
        .detach();
    }

    /// `handleApplyDelegation`: onto the task's checkout, then approved.
    pub(crate) fn apply(&mut self, delegation: &DelegationRecord, cx: &mut Context<Self>) {
        let Some(root) = self.data(cx).and_then(|data| data.workspace_path.clone()) else {
            Toaster::error(cx.t("inspector.delegations.applyMissingWorkspace"), cx);
            return;
        };
        let id = delegation.id.clone();
        self.run(
            format!("apply:{id}"),
            move |api| api.apply_delegation(&id, &root),
            |count, cx| cx.t_with("inspector.delegations.applied", &Args::count(count as i64)),
            cx,
        );
    }

    /// `handleDiscardDelegation`: the worktree thrown away, cancelled.
    pub(crate) fn discard(&mut self, delegation: &DelegationRecord, cx: &mut Context<Self>) {
        let Some(root) = self.data(cx).and_then(|data| data.workspace_path.clone()) else {
            Toaster::error(cx.t("inspector.delegations.applyMissingWorkspace"), cx);
            return;
        };
        let id = delegation.id.clone();
        self.run(
            format!("discard:{id}"),
            move |api| api.discard_delegation(&id, &root),
            |(), cx| cx.t("inspector.delegations.discarded"),
            cx,
        );
    }

    /// `handleApproveDelegation`: "Marcar revisada".
    pub(crate) fn approve(&mut self, delegation: &DelegationRecord, cx: &mut Context<Self>) {
        let id = delegation.id.clone();
        self.run(
            format!("approve:{id}"),
            move |api| api.approve_delegation(&id),
            |(), cx| cx.t("inspector.delegations.reviewed"),
            cx,
        );
    }
}

impl DesktopApi {
    /// `approveDelegation`: accepted with its own report as the summary.
    pub fn approve_delegation(&self, id: &str) -> Result<(), ApiError> {
        self.post(
            &format!(
                "/api/v1/delegations/{}/approve",
                crate::data::desktop_api::encode(id)
            ),
            &serde_json::json!({}),
        )
        .map(|_| ())
    }
}

impl Render for SessionPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_panel(cx)
    }
}
