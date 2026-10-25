//! `DelegationReviewStrip`: where the reading ends, the edits a child agent
//! left in its worktree for this conversation (up to three), each with
//! "Revisar" — the card higher up may be out of sight.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::{ConversationEvent, ConversationView};
use crate::agent::{AgentMascot, RobotState};
use crate::data::conversation::Conversation;
use crate::i18n::{Args, Translate};
use crate::project::{model_label, provider_label};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonSize};

const MAX_STRIPS: usize = 3;

/// The delegations of this conversation waiting for review, in order.
pub(super) fn waiting(data: &Conversation) -> Vec<String> {
    let Some(session) = data.session_id.as_deref() else {
        return Vec::new();
    };
    let mut ids: Vec<String> = data
        .delegations
        .iter()
        .filter(|(_, record)| record.parent_session_id == session && record.awaits_review())
        .map(|(id, _)| id.clone())
        .collect();
    ids.sort();
    ids.truncate(MAX_STRIPS);
    ids
}

pub(super) fn review_strips(
    view: &ConversationView,
    data: &Conversation,
    cx: &mut Context<ConversationView>,
) -> Option<AnyElement> {
    let ids = waiting(data);
    if ids.is_empty() {
        return None;
    }
    let dark = cx.theme().appearance == Appearance::Dark;
    let text = if dark { tw::AMBER_200 } else { tw::AMBER_800 }.hsla();
    let strips: Vec<AnyElement> = ids
        .iter()
        .filter_map(|id| data.delegations.get(id).map(|record| (id.clone(), record)))
        .map(|(id, record)| {
            let agent = record
                .model_id
                .as_deref()
                .map(model_label)
                .unwrap_or_else(|| {
                    provider_label(&record.provider_id)
                        .map_or_else(|| record.provider_id.clone(), str::to_string)
                });
            let message = cx.t_with(
                "delegation.reviewStrip.message",
                &Args::count(record.touched_files.len() as i64).with("agent", agent),
            );
            let review = id.clone();
            div()
                .debug_selector({
                    let id = id.clone();
                    move || format!("review-strip-{id}")
                })
                .w_full()
                .max_w(px(672.0))
                .mx_auto()
                .flex()
                .items_center()
                .gap(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(tw::AMBER_500.alpha(0.3))
                .bg(tw::AMBER_500.alpha(0.1))
                .px(px(12.0))
                .py(px(8.0))
                .child(
                    AgentMascot::new(
                        Some(&record.provider_id),
                        record.model_id.as_deref(),
                        RobotState::Waiting,
                    )
                    .moving(view.mascot_frame()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(text)
                        .child(message),
                )
                .child(
                    Button::new(SharedString::from(format!("review-strip-action-{id}")))
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(ConversationEvent::ReviewDelegation {
                                id: review.clone(),
                                path: None,
                            })
                        }))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .child(cx.t("delegation.reviewStrip.action")),
                        ),
                )
                .into_any_element()
        })
        .collect();
    Some(
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(16.0))
            .pb(px(16.0))
            .children(strips)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use dcc_core::domain::delegation::DelegationStatus;

    use super::waiting;
    use crate::data::conversation::{Conversation, DelegationRecord};

    fn record(parent: &str, status: DelegationStatus, implement: bool) -> DelegationRecord {
        DelegationRecord {
            status,
            is_task: false,
            title: None,
            instruction: String::new(),
            result_summary: None,
            prompt: String::new(),
            touched_files: vec!["a.rs".into()],
            provider_id: "codex".into(),
            model_id: None,
            child_session_id: Some(format!("child-of-{parent}")),
            implement,
            parent_session_id: parent.into(),
            worktree: Some("/tmp/w".into()),
        }
    }

    #[test]
    fn only_this_conversations_implementations_waiting_show_up_to_three() {
        let mut data = Conversation {
            session_id: Some("s-1".into()),
            ..Conversation::default()
        };
        for (id, parent, status, implement) in [
            ("d-1", "s-1", DelegationStatus::ReviewPending, true),
            ("d-2", "s-1", DelegationStatus::Completed, true),
            ("d-3", "s-2", DelegationStatus::ReviewPending, true),
            ("d-4", "s-1", DelegationStatus::ReviewPending, false),
            ("d-5", "s-1", DelegationStatus::ReviewPending, true),
            ("d-6", "s-1", DelegationStatus::ReviewPending, true),
            ("d-7", "s-1", DelegationStatus::ReviewPending, true),
        ] {
            data.delegations
                .insert(id.into(), record(parent, status, implement));
        }
        assert_eq!(waiting(&data), vec!["d-1", "d-5", "d-6"]);
    }
}
