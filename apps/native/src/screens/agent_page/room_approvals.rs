//! An agent's approval asked in the team room itself (drawn docked to the
//! field, `room_dock`): "Suporte quer usar
//! slack_send_message" with Permitir, Sempre permitir (a connector's read
//! tool), Negar and Recusar com motivo — the conversation's own answers
//! (`conversation::approval_reason`), so the person does not leave the room
//! to answer. A provider that cannot read a denial's reason gets it as the
//! next message, as in the conversation.

use gpui::{AppContext, Context, Window};

use super::room::TeamRoom;
use crate::data::agents::AgentRecord;
use crate::data::desktop_api::TurnOptions;
use crate::data::room_approvals::PendingApproval;
use crate::i18n::Translate;
use crate::screens::conversation::approval_reason::{reason_message, short_tool};
use crate::ui::{TextInput, Toaster};

/// The providers that read a denial's reason themselves.
const READS_DENIAL_REASON: [&str; 1] = ["claude_code"];

/// One approval waiting in the room.
#[derive(Clone)]
pub(crate) struct RoomApproval {
    pub agent: AgentRecord,
    pub session_id: String,
    pub request: PendingApproval,
}

/// The person's answer.
#[derive(Clone, Copy)]
pub(super) enum Answer {
    Allow,
    Always,
    Deny,
}

impl TeamRoom {
    /// The approvals the room's conversations wait on.
    pub(crate) fn approvals(&self, agents: &[AgentRecord]) -> Vec<RoomApproval> {
        self.feeds
            .iter()
            .filter(|feed| self.shows(&feed.agent_id))
            .filter_map(|feed| {
                let request = feed.pending_approval()?;
                let agent = agents.iter().find(|agent| agent.id == feed.agent_id)?;
                Some(RoomApproval {
                    agent: agent.clone(),
                    session_id: feed.session_id.clone(),
                    request,
                })
            })
            .collect()
    }

    pub(super) fn answer(
        &mut self,
        approval: &RoomApproval,
        answer: Answer,
        reason: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let request = approval.request.request_id.clone();
        if !self.answering.insert(request.clone()) {
            return;
        }
        self.approval_reason = None;
        cx.notify();
        let session = approval.session_id.clone();
        let tool = short_tool(&approval.request.tool_name).to_string();
        let queue = reason.is_some()
            && !READS_DENIAL_REASON
                .contains(&approval.agent.provider_id.as_deref().unwrap_or_default());
        cx.spawn(async move |this, cx| {
            let request_id = request.clone();
            let sent = cx
                .background_executor()
                .spawn(async move {
                    let allow = !matches!(answer, Answer::Deny);
                    let always = matches!(answer, Answer::Always);
                    api.respond_permission_with(
                        &session,
                        &request_id,
                        allow,
                        reason.as_deref(),
                        always,
                    )?;
                    if let (true, Some(reason)) = (queue, reason.as_deref()) {
                        api.queue_turn(
                            &session,
                            &reason_message(&tool, reason),
                            &TurnOptions::default(),
                        )?;
                    }
                    Ok::<_, crate::data::desktop_api::ApiError>(())
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                room.answering.remove(&request);
                if let Err(error) = sent {
                    Toaster::error(error.to_string(), cx);
                }
                if !cfg!(test) {
                    room.sidebar
                        .update(cx, |sidebar, cx| sidebar.reload_now(cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_reason(
        &mut self,
        request_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder(cx.t("conversation.permission.reasonPlaceholder"))
        });
        let handle = gpui::Focusable::focus_handle(input.read(cx), cx);
        window.focus(&handle, cx);
        self.approval_reason = Some((request_id, input));
        cx.notify();
    }
}
