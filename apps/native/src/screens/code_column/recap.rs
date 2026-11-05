//! The recap strip over the git section (`WorkspaceRecapStrip`): when the
//! task needs the person or is blocked, its state, one sentence on why, the
//! delivery signals behind it and the action that moves it. The rules are
//! dcc-core's `build_workspace_recap`. Not here: the git step (the header
//! below already offers it), "Ver atividade" (the session's tab), the
//! project's automation dialog (not ported yet), and CodeRabbit.

use dcc_core::domain::workspace_recap::{
    DeliveryState, RecapAction, RecapDeliveryFailure, RecapParam, RecapParams, RecapPipeline,
    RecapReviewState, RecapTone, SignalState, SourceStatus, WorkspaceRecap, WorkspaceRecapInput,
    action_in_changes, build_workspace_recap,
};
use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use super::CodeColumn;
use super::review::{PipelineLoad, ReviewLoad};
use crate::data::recap::diff_totals;
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

impl CodeColumn {
    /// The open conversation's turn and the task's delegations waiting on
    /// the person.
    pub fn set_conversation_state(
        &mut self,
        turn_running: bool,
        pending_delegations: usize,
        cx: &mut Context<Self>,
    ) {
        if (turn_running, pending_delegations) != (self.turn_running, self.pending_delegations) {
            // A finished turn has a new review.
            let finished = self.turn_running && !turn_running;
            self.turn_running = turn_running;
            if finished {
                self.read_last_turn(cx);
            }
            self.pending_delegations = pending_delegations;
            cx.notify();
        }
    }

    /// `buildWorkspaceRecap`'s input, as the inspector feeds it.
    fn recap_input(&self) -> WorkspaceRecapInput {
        let (files, additions, deletions) = diff_totals(self.status.as_ref());
        let pr = self.delivery.pr.as_ref();
        let pr_number = pr.and_then(|pr| pr.number);
        let review = &self.review;
        let connected = review.forge.as_ref().map(|forge| forge.connected());
        let review_status = match (pr_number, connected, &review.state) {
            (None, _, _) => SourceStatus::Idle,
            // The forge not read yet is a read in flight, not a failure.
            (_, None, _) | (_, Some(true), None | Some(ReviewLoad::Pending)) => {
                SourceStatus::Loading
            }
            (_, Some(false), _) | (_, _, Some(ReviewLoad::Failed)) => SourceStatus::Error,
            (_, _, Some(ReviewLoad::Ready(_))) => SourceStatus::Available,
        };
        let review_state = match &review.state {
            Some(ReviewLoad::Ready(state)) => Some(RecapReviewState {
                review_state: state.review_state.clone(),
                approvals_available: state.approvals_available,
                approvals_received: i64::from(state.approvals_received),
                approvals_left: state.approvals_left.map(i64::from),
                has_conflicts: state.has_conflicts,
                behind_by: state.behind_by.map(i64::from),
                discussions_resolved: state.discussions_resolved,
                draft: state.draft,
            }),
            _ => None,
        };
        let pipeline_status = if !review.pipeline_enabled() {
            SourceStatus::Idle
        } else {
            match &review.pipeline {
                None | Some(PipelineLoad::Pending) => SourceStatus::Loading,
                Some(PipelineLoad::Failed) => SourceStatus::Error,
                Some(PipelineLoad::Ready(_)) => SourceStatus::Available,
            }
        };
        let pipeline = match &review.pipeline {
            Some(PipelineLoad::Ready(Some(pipeline))) => Some(RecapPipeline {
                status: pipeline.status.clone(),
                failed_jobs: pipeline
                    .jobs
                    .iter()
                    .filter(|job| job.status == "failed" && !job.allow_failure)
                    .count() as i64,
            }),
            _ => None,
        };
        let status = self.status.as_ref();
        WorkspaceRecapInput {
            turn_running: self.turn_running,
            changed_files_count: files as i64,
            additions,
            deletions,
            ahead_of_remote_count: status.map_or(0, |git| i64::from(git.ahead_of_remote_count)),
            conflict_count: status.map_or(0, |git| i64::from(git.conflict_count)),
            committed_vs_base_count: self.changes.branch_count().unwrap_or(0) as i64,
            pr_number,
            pr_state: pr.and_then(|pr| pr.state.clone()),
            pr_merge_state: pr.and_then(|pr| pr.merge_state_status.clone()),
            request_label: self.delivery.request_label(),
            pending_delegation_results_count: self.pending_delegations as i64,
            delivery_failure: review.failure.as_ref().map(|failure| RecapDeliveryFailure {
                operation: failure.operation.clone(),
                classification: failure.classification.clone(),
            }),
            review_state,
            review_status: Some(review_status),
            pipeline,
            pipeline_status: Some(pipeline_status),
            delivery_policy: self.delivery.policy.map(|(policy, _)| policy),
            before_merge_checks_count: self.delivery.policy.map_or(0, |(_, checks)| checks),
            ..WorkspaceRecapInput::new(self.commit_mode())
        }
    }

    /// The task's recap, once the status answered (none under a delegation's
    /// review: the list shows the child's worktree).
    pub(crate) fn workspace_recap(&self) -> Option<WorkspaceRecap> {
        if self.root.is_none() || self.status.is_none() || self.delegation_review.is_some() {
            return None;
        }
        Some(build_workspace_recap(&self.recap_input()))
    }

    /// The strip's button: what the native column can do of it.
    fn recap_action(recap: &WorkspaceRecap) -> Option<RecapAction> {
        action_in_changes(recap.action).filter(|action| {
            matches!(
                action,
                RecapAction::ReviewState
                    | RecapAction::Pipeline
                    | RecapAction::Delivery
                    | RecapAction::Sync
            )
        })
    }

    /// `handleRecapAction`: the section opens where the desktop scrolls to it.
    pub(crate) fn run_recap_action(&mut self, action: RecapAction, cx: &mut Context<Self>) {
        match action {
            RecapAction::ReviewState => self.review.state_open = true,
            RecapAction::Pipeline => self.review.pipeline_open = true,
            RecapAction::Delivery => self.review.failure_open = true,
            RecapAction::Sync => self.ask_sync_base(cx),
            _ => {}
        }
        cx.notify();
    }
}

fn args(params: &RecapParams) -> Args {
    let mut args = match params.get("count") {
        Some(RecapParam::Number(count)) => Args::count(*count),
        _ => Args::default(),
    };
    for (key, value) in params.iter().filter(|(key, _)| **key != "count") {
        args = args.with(key, text(value));
    }
    args
}

fn text(value: &RecapParam) -> String {
    match value {
        RecapParam::Number(number) => number.to_string(),
        RecapParam::Text(text) => text.clone(),
    }
}

/// `STATE_CLASS`: border, background and text of the state badge.
fn state_colors(state: DeliveryState, cx: &Context<CodeColumn>) -> (Hsla, Hsla, Hsla) {
    let colors = &cx.theme().colors;
    let dark = cx.theme().appearance == Appearance::Dark;
    match state {
        DeliveryState::NeedsAttention => (
            tw::AMBER_500.alpha(0.25),
            tw::AMBER_500.alpha(0.1),
            if dark { tw::AMBER_300 } else { tw::AMBER_700 }.hsla(),
        ),
        DeliveryState::Blocked => (
            colors.destructive.opacity(0.25),
            colors.destructive.opacity(0.1),
            colors.destructive,
        ),
        // The strip only shows the two above.
        _ => (
            colors.border.opacity(0.7),
            colors.muted.opacity(0.5),
            colors.muted_foreground,
        ),
    }
}

/// `TONE_DOT_CLASS` for the tones a strip can show.
fn tone_dot(tone: RecapTone, cx: &Context<CodeColumn>) -> Hsla {
    match tone {
        RecapTone::Attention => tw::AMBER_500.hsla(),
        _ => cx.theme().colors.muted_foreground.opacity(0.5),
    }
}

/// `SignalIcon` (`SIGNAL_CLASS`)
fn signal_icon(state: SignalState, cx: &Context<CodeColumn>) -> Icon {
    let dark = cx.theme().appearance == Appearance::Dark;
    let shade = |light: tw::Oklch, night: tw::Oklch| if dark { night } else { light }.hsla();
    let (name, color) = match state {
        SignalState::Passed => ("circle-check", shade(tw::EMERALD_600, tw::EMERALD_400)),
        SignalState::Pending => ("clock-3", shade(tw::SKY_600, tw::SKY_400)),
        SignalState::Attention => ("circle-alert", shade(tw::AMBER_600, tw::AMBER_400)),
        SignalState::Blocked => ("circle-x", cx.theme().colors.destructive),
        SignalState::Unavailable => ("circle-question-mark", cx.theme().colors.muted_foreground),
    };
    Icon::new(name).size(14.0).color(color)
}

fn details(recap: &WorkspaceRecap, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let cells: Vec<AnyElement> = recap
        .signals
        .iter()
        .map(|signal| {
            div()
                .flex()
                .min_w_0()
                .items_start()
                .gap(px(6.0))
                .rounded(px(6.0))
                .bg(colors.background.opacity(0.45))
                .px(px(8.0))
                .py(px(6.0))
                .child(signal_icon(signal.state, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(9.5))
                        .line_height(px(16.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t_with(
                            &format!("inspector.recap.signals.{}", signal.message_key),
                            &args(&signal.params),
                        )),
                )
                .when(signal.required, |cell| {
                    cell.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(8.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground.opacity(0.55))
                            .child(cx.t("inspector.recap.required").to_uppercase()),
                    )
                })
                .into_any_element()
        })
        .collect();
    div()
        .debug_selector(|| "recap-signals".into())
        .mt(px(8.0))
        .grid()
        .grid_cols(2)
        .gap(px(6.0))
        .border_t_1()
        .border_color(colors.border.opacity(0.4))
        .pt(px(8.0))
        .children(cells)
        .into_any_element()
}

/// The strip, when the task needs attention or is blocked.
pub(crate) fn recap_strip(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> Option<AnyElement> {
    let recap = column.workspace_recap().filter(|recap| {
        matches!(
            recap.state,
            DeliveryState::NeedsAttention | DeliveryState::Blocked
        )
    })?;
    let colors = cx.theme().colors.clone();
    let (border, background, foreground) = state_colors(recap.state, cx);
    let message: SharedString = cx.t_with(
        &format!("inspector.recap.messages.{}", recap.message_key),
        &args(&recap.params),
    );
    let open = column.recap_details;
    let action = CodeColumn::recap_action(&recap);
    let badge = div()
        .flex()
        .h(px(20.0))
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .rounded_full()
        .border_1()
        .border_color(border)
        .bg(background)
        .px(px(6.0))
        .text_size(px(9.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(foreground)
        .child(
            div()
                .size(px(6.0))
                .rounded_full()
                .bg(tone_dot(recap.tone, cx)),
        )
        .child(cx.t(&format!("inspector.recap.state.{}", recap.state.id())));
    let toggle = Button::new("recap-details")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .muted()
        .on_click(cx.listener(|column, _, _, cx| {
            column.recap_details = !column.recap_details;
            cx.notify();
        }))
        .child(
            Icon::new(if open {
                "chevron-down"
            } else {
                "chevron-right"
            })
            .size(14.0)
            .color(colors.muted_foreground),
        );
    let button = action.map(|action| {
        Button::new("recap-action")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Xs)
            .on_click(cx.listener(move |column, _, _, cx| column.run_recap_action(action, cx)))
            .child(div().text_size(px(11.0)).child(cx.t_with(
                &action.label_key(),
                &Args::default().with("requestLabel", column.delivery.request_label()),
            )))
    });
    Some(
        div()
            .debug_selector(|| "recap-strip".into())
            .flex_shrink_0()
            .border_t_1()
            .border_color(colors.border.opacity(0.5))
            .bg(colors.sidebar.opacity(0.85))
            .px(px(12.0))
            .py(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(badge)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .line_clamp(2)
                            .text_size(px(11.5))
                            .line_height(px(16.0))
                            .text_color(colors.muted_foreground)
                            .child(message),
                    )
                    .child(
                        div()
                            .debug_selector(|| "recap-details".into())
                            .child(toggle),
                    )
                    .children(button.map(|button| {
                        div()
                            .flex_shrink_0()
                            .debug_selector(|| "recap-action".into())
                            .child(button)
                    })),
            )
            .when(open, |strip| strip.child(details(&recap, cx)))
            .into_any_element(),
    )
}
