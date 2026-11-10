//! An agent's approval asked in the team room itself: "Suporte quer usar
//! slack_send_message" with Permitir, Sempre permitir (a connector's read
//! tool), Negar and Recusar com motivo — the conversation's own answers
//! (`conversation::approval_reason`), so the person does not leave the room
//! to answer. A provider that cannot read a denial's reason gets it as the
//! next message, as in the conversation.

use gpui::{
    AnyElement, AppContext, Context, FontWeight, SharedString, Window, div, prelude::*, px,
};

use super::room::TeamRoom;
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::data::desktop_api::TurnOptions;
use crate::data::room_approvals::PendingApproval;
use crate::i18n::{Args, Translate};
use crate::screens::conversation::approval_reason::{may_always_allow, reason_message, short_tool};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, TextInput, Toaster};

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
enum Answer {
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

    fn answer(
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

    fn open_reason(&mut self, request_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder(cx.t("conversation.permission.reasonPlaceholder"))
        });
        let handle = gpui::Focusable::focus_handle(input.read(cx), cx);
        window.focus(&handle, cx);
        self.approval_reason = Some((request_id, input));
        cx.notify();
    }

    pub(super) fn approval_rows(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        self.approvals(agents)
            .into_iter()
            .enumerate()
            .map(|(index, approval)| {
                let tool = short_tool(&approval.request.tool_name).to_string();
                let busy = self.answering.contains(&approval.request.request_id);
                let reason_open = self
                    .approval_reason
                    .as_ref()
                    .filter(|(id, _)| *id == approval.request.request_id)
                    .map(|(_, input)| input.clone());
                let detail = approval
                    .request
                    .command
                    .clone()
                    .or_else(|| approval.request.file.clone())
                    .or_else(|| approval.request.description.clone());
                let button = |id: &str, label: SharedString, variant: ButtonVariant| {
                    Button::new(SharedString::from(format!("room-approval-{id}-{index}")))
                        .variant(variant)
                        .size(ButtonSize::Sm)
                        .disabled(busy)
                        .child(label)
                };
                let (allow, always, deny, why) = (
                    approval.clone(),
                    approval.clone(),
                    approval.clone(),
                    approval.request.request_id.clone(),
                );
                let mut actions = div().flex().flex_wrap().justify_end().gap(px(6.0));
                if reason_open.is_none() {
                    actions = actions
                        .child(
                            button(
                                "why",
                                cx.t("conversation.permission.denyWithReason"),
                                ButtonVariant::Ghost,
                            )
                            .on_click(cx.listener(
                                move |room, _, window, cx| {
                                    room.open_reason(why.clone(), window, cx)
                                },
                            )),
                        )
                        .child(
                            button(
                                "deny",
                                cx.t("conversation.permission.deny"),
                                ButtonVariant::Outline,
                            )
                            .on_click(cx.listener(
                                move |room, _, _, cx| room.answer(&deny, Answer::Deny, None, cx),
                            )),
                        )
                        .when(may_always_allow(&approval.request.tool_name), |row| {
                            row.child(
                                button(
                                    "always",
                                    cx.t("conversation.permission.always"),
                                    ButtonVariant::Outline,
                                )
                                .on_click(cx.listener(
                                    move |room, _, _, cx| {
                                        room.answer(&always, Answer::Always, None, cx)
                                    },
                                )),
                            )
                        })
                        .child(
                            button(
                                "allow",
                                cx.t("conversation.permission.allow"),
                                ButtonVariant::Default,
                            )
                            .on_click(cx.listener(
                                move |room, _, _, cx| room.answer(&allow, Answer::Allow, None, cx),
                            )),
                        );
                }
                let reason_row = reason_open.map(|input| {
                    let sent = approval.clone();
                    let field = input.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .px(px(10.0))
                                .py(px(6.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(colors.input)
                                .child(input),
                        )
                        .child(
                            button(
                                "send-deny",
                                cx.t("conversation.permission.denySend"),
                                ButtonVariant::Default,
                            )
                            .on_click(cx.listener(
                                move |room, _, _, cx| {
                                    let reason = field.read(cx).text().trim().to_string();
                                    if !reason.is_empty() {
                                        room.answer(&sent, Answer::Deny, Some(reason), cx);
                                    }
                                },
                            )),
                        )
                });
                div()
                    .debug_selector(move || format!("room-approval-{index}"))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .px(px(14.0))
                    .py(px(12.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(crate::theme::tailwind::AMBER_500.hsla().opacity(0.6))
                    .bg(crate::theme::tailwind::AMBER_500.hsla().opacity(0.06))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(AgentAvatarView::new(
                                SharedString::from(format!("room-approval-face-{index}")),
                                &approval.agent.avatar,
                                None,
                                24.0,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(
                                        cx.t_with(
                                            "agents.room.approval",
                                            &Args::default()
                                                .with("agent", approval.agent.name.clone())
                                                .with("tool", tool),
                                        ),
                                    ),
                            ),
                    )
                    .children(detail.map(|detail| {
                        div()
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .line_clamp(3)
                            .child(detail)
                    }))
                    .child(actions)
                    .children(reason_row)
                    .into_any_element()
            })
            .collect()
    }
}
