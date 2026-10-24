//! `DelegationCard`: work handed to another agent, centered under
//! `max-w-[42rem]` — who runs it and its state, then its report and the
//! files it touched. Its buttons (review, open the child, close the task)
//! come with screen 2c.

use dcc_core::domain::delegation::DelegationStatus;
use dcc_core::domain::timeline::{DelegationPhase, TimelineDelegation, TimelineMessage};
use gpui::{AnyElement, App, Context, FontWeight, Hsla, SharedString, Window, div, prelude::*, px};

use super::ConversationView;
use super::system_rows::centered;
use crate::agent::{AgentMascot, RobotState};
use crate::data::conversation::Conversation;
use crate::data::prompt::basename;
use crate::i18n::{Args, Translate};
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::project::{model_label, provider_label};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::Icon;

/// Reports longer than this open collapsed, showing their first lines.
const COLLAPSED_REPORT_CHARS: usize = 320;
const MAX_FILE_CHIPS: usize = 6;

fn phase_status(phase: DelegationPhase) -> DelegationStatus {
    match phase {
        DelegationPhase::Requested => DelegationStatus::Queued,
        DelegationPhase::Running => DelegationStatus::Running,
        DelegationPhase::Completed => DelegationStatus::Completed,
        DelegationPhase::Failed => DelegationStatus::Failed,
        DelegationPhase::Cancelled => DelegationStatus::Cancelled,
    }
}

fn status_key(status: &DelegationStatus) -> &'static str {
    match status {
        DelegationStatus::Draft => "draft",
        DelegationStatus::Queued => "queued",
        DelegationStatus::Running => "running",
        DelegationStatus::ReviewPending => "review_pending",
        DelegationStatus::Completed => "completed",
        DelegationStatus::Failed => "failed",
        DelegationStatus::Cancelled => "cancelled",
    }
}

/// `delegationStatusClass`: border, background and text of the badge.
fn status_colors(status: &DelegationStatus, waiting: bool, cx: &App) -> (Hsla, Hsla, Hsla) {
    let dark = cx.theme().appearance == Appearance::Dark;
    let colors = &cx.theme().colors;
    let tinted = |base: tw::Oklch, light: tw::Oklch, dark_text: tw::Oklch, border: f32| {
        (
            base.alpha(border),
            base.alpha(0.1),
            if dark { dark_text.hsla() } else { light.hsla() },
        )
    };
    if waiting {
        return tinted(tw::AMBER_500, tw::AMBER_800, tw::AMBER_200, 0.4);
    }
    match status {
        DelegationStatus::Completed => {
            tinted(tw::EMERALD_500, tw::EMERALD_700, tw::EMERALD_300, 0.3)
        }
        DelegationStatus::Failed => (
            colors.destructive.opacity(0.3),
            colors.destructive.opacity(0.1),
            colors.destructive,
        ),
        DelegationStatus::Cancelled => (
            colors.muted_foreground.opacity(0.3),
            colors.muted.opacity(0.3),
            colors.muted_foreground,
        ),
        DelegationStatus::Running => tinted(tw::SKY_500, tw::SKY_700, tw::SKY_300, 0.3),
        DelegationStatus::ReviewPending => tinted(tw::AMBER_500, tw::AMBER_700, tw::AMBER_300, 0.3),
        _ => (
            colors.border,
            colors.muted.opacity(0.3),
            colors.muted_foreground,
        ),
    }
}

pub(super) fn delegation_card(
    view: &ConversationView,
    data: &Conversation,
    message: &TimelineMessage,
    delegation: &TimelineDelegation,
    window: &mut Window,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let record = data.delegations.get(&delegation.id);
    let status = record.map_or_else(
        || phase_status(delegation.phase),
        |record| record.status.clone(),
    );
    let is_task = record.is_some_and(|record| record.is_task);
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let mono = theme.fonts.mono.clone();
    let radius = theme.radius;
    if is_task
        && status == DelegationStatus::Cancelled
        && record.is_some_and(|record| record.child_session_id.is_none())
    {
        let title = record
            .and_then(|record| record.title.clone())
            .map(|title| format!(" · {title}"));
        return centered(
            div()
                .w_full()
                .max_w(px(672.0))
                .truncate()
                .text_size(px(11.5))
                .line_height(px(20.0))
                .text_color(colors.muted_foreground)
                .child(format!(
                    "{}{}",
                    cx.t("delegation.proposal.dismissed"),
                    title.unwrap_or_default()
                )),
        )
        .into_any_element();
    }
    let child_session = record
        .and_then(|record| record.child_session_id.clone())
        .or_else(|| delegation.child_session_id.clone());
    let child = child_session.as_ref().and_then(|id| data.children.get(id));
    let child_busy = child.map(|child| child.busy);
    let waiting = child.is_some_and(|child| child.waiting)
        && matches!(status, DelegationStatus::Running | DelegationStatus::Queued);
    // An open task between turns waits for the person, it is not working.
    let task_idle = is_task && status == DelegationStatus::Running && child_busy == Some(false);
    let status_label = if waiting {
        cx.t("delegation.card.waitingBadge")
    } else if task_idle {
        cx.t("delegation.task.open")
    } else {
        cx.t(&format!(
            "inspector.delegations.status.{}",
            status_key(&status)
        ))
    };
    // `decisions.modelLabel ?? decisions.providerLabel`.
    let agent_label = record.map(|record| match &record.model_id {
        Some(model) => model_label(model),
        None => provider_label(&record.provider_id)
            .map_or_else(|| record.provider_id.clone(), str::to_string),
    });
    let mode_label = if is_task {
        Some(cx.t("delegation.proposal.kind"))
    } else {
        None
    };
    let title = record
        .and_then(|record| {
            record
                .title
                .clone()
                .filter(|title| is_task && !title.trim().is_empty())
        })
        .or_else(|| {
            child
                .map(|child| child.title.trim().to_string())
                .filter(|title| !title.is_empty() && !title.starts_with("Delegated "))
        })
        .or_else(|| {
            record.and_then(|record| {
                record
                    .instruction
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim().to_string())
            })
        })
        .map(SharedString::from)
        .unwrap_or_else(|| cx.t("delegation.card.title"));
    let subtitle = [
        mode_label.map(|label| label.to_string()),
        agent_label.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let subtitle = if subtitle.is_empty() {
        cx.t("delegation.card.title")
    } else {
        subtitle.into()
    };
    let running =
        matches!(status, DelegationStatus::Running | DelegationStatus::Queued) && !task_idle;
    let summary = record
        .and_then(|record| record.result_summary.clone())
        .or_else(|| delegation.summary.clone())
        .or_else(|| delegation.reason.clone())
        .or_else(|| (!message.content.is_empty()).then(|| message.content.clone()))
        .or_else(|| record.map(|record| record.prompt.clone()))
        .unwrap_or_default();
    let (badge_border, badge_bg, badge_text) = status_colors(&status, waiting, cx);
    let dark = cx.theme().appearance == Appearance::Dark;
    let agent = agent_label
        .clone()
        .unwrap_or_else(|| cx.t("delegation.card.agentFallback").to_string());
    let note = if waiting {
        Some((
            cx.t_with(
                "delegation.card.waitingOnYou",
                &Args::default().with("agent", agent.clone()),
            ),
            if dark {
                tw::AMBER_200.hsla()
            } else {
                tw::AMBER_800.hsla()
            },
        ))
    } else if running {
        Some((
            cx.t_with(
                "delegation.card.working",
                &Args::default().with("agent", agent.clone()),
            ),
            colors.muted_foreground,
        ))
    } else {
        None
    };
    let report_key = SharedString::from(format!("report-{}", message.id));
    let report_open = view.is_open(&report_key);
    let long = summary.chars().count() > COLLAPSED_REPORT_CHARS;
    let report = (!summary.trim().is_empty() && !running).then(|| {
        let blocks = view.markdown(&summary);
        let style = MarkdownStyle {
            size: 12.5,
            ..MarkdownStyle::reply()
        };
        let body = div()
            .text_size(px(12.5))
            .text_color(colors.foreground)
            .when(long && !report_open, |body| {
                body.max_h(px(112.0)).overflow_hidden()
            })
            .child(render_markdown(
                &blocks,
                style,
                &format!("{}-report", message.id),
                window,
                cx,
            ));
        let toggle = report_key.clone();
        let (muted, foreground) = (colors.muted_foreground, colors.foreground);
        let button = long.then(|| {
            div()
                .id(report_key.clone())
                .mt(px(4.0))
                .flex()
                .self_start()
                .items_center()
                .gap(px(4.0))
                .rounded(radius.sm)
                .cursor_pointer()
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .hover(move |style| style.text_color(foreground))
                .on_click(cx.listener(move |view, _, _, cx| view.toggle(toggle.clone(), cx)))
                .child(Icon::new("chevron-down").size(14.0).rotate(if report_open {
                    180.0
                } else {
                    0.0
                }))
                .child(if report_open {
                    cx.t("delegation.card.hideReport")
                } else {
                    cx.t("delegation.card.showReport")
                })
        });
        div()
            .mt(px(8.0))
            .flex()
            .flex_col()
            .child(body)
            .children(button)
    });
    let touched = record
        .map(|record| record.touched_files.clone())
        .unwrap_or_default();
    let files = (!touched.is_empty()).then(|| {
        div()
            .mt(px(10.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t_with(
                        "delegation.card.changedFiles",
                        &Args::count(touched.len() as i64),
                    )),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(4.0))
                    .children(touched.iter().take(MAX_FILE_CHIPS).map(|path| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .max_w(px(224.0))
                            .rounded(radius.md)
                            .border_1()
                            .border_color(colors.border.opacity(0.6))
                            .bg(colors.muted.opacity(0.3))
                            .px(px(6.0))
                            .py(px(2.0))
                            .font_family(mono.clone())
                            .text_size(px(10.5))
                            .text_color(colors.foreground.opacity(0.8))
                            .child(
                                Icon::new("file-code-corner")
                                    .size(12.0)
                                    .color(colors.muted_foreground),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .child(SharedString::from(basename(path).to_string())),
                            )
                    }))
                    .when(touched.len() > MAX_FILE_CHIPS, |chips| {
                        chips.child(
                            div()
                                .px(px(4.0))
                                .py(px(2.0))
                                .text_size(px(10.5))
                                .text_color(colors.muted_foreground)
                                .child(format!("+{}", touched.len() - MAX_FILE_CHIPS)),
                        )
                    }),
            )
    });
    let pulse = (status == DelegationStatus::Running && !task_idle && !waiting).then(|| {
        div()
            .mr(px(4.0))
            .size(px(6.0))
            .rounded_full()
            .bg(badge_text)
    });
    let robot_state = if running && !waiting {
        RobotState::Working
    } else {
        RobotState::Idle
    };
    let provider = record.map(|record| record.provider_id.clone());
    let model = record.and_then(|record| record.model_id.clone());
    centered(
        div()
            .w_full()
            .max_w(px(672.0))
            .rounded(radius.xl)
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.card.opacity(0.7))
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.0))
                    .child(
                        div().mt(px(2.0)).child(
                            AgentMascot::new(provider.as_deref(), model.as_deref(), robot_state)
                                .moving(view.mascot_frame()),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .line_height(px(20.0))
                                    .text_color(colors.foreground)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .line_height(px(16.0))
                                    .text_color(colors.muted_foreground)
                                    .child(subtitle),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .h(px(20.0))
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .px(px(6.0))
                            .rounded(radius.md)
                            .border_1()
                            .border_color(badge_border)
                            .bg(badge_bg)
                            .text_size(px(10.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(badge_text)
                            .children(pulse)
                            .child(status_label),
                    ),
            )
            .children(note.map(|(text, color)| {
                div()
                    .mt(px(8.0))
                    .text_size(px(12.0))
                    .line_height(px(20.0))
                    .text_color(color)
                    .child(text)
            }))
            .children(report)
            .children(files),
    )
    .into_any_element()
}
