//! `ToolCallRow`: what a tool call did on one line (verb, target, badges),
//! with the command, changes, output and input one click away.

use dcc_core::domain::timeline::ToolCall;
use dcc_core::domain::timeline::activity::{
    DiffLineKind, ToolKind, classify_diff_line, classify_tool_action, diff_stats, mcp_tool_label,
    strip_ansi, tool_target,
};
use gpui::{AnyElement, App, ClipboardItem, Context, Hsla, SharedString, div, prelude::*, px};

use super::ConversationView;
use crate::data::plan_checklist::{PlanChecklist, PlanStepStatus, parse_plan_checklist};
use crate::i18n::{Args, Translate};
use crate::theme::{ActiveTheme, color::mix_oklab};
use crate::ui::{Icon, thinking_indicator};

pub(super) fn kind_icon(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Command => "terminal",
        ToolKind::Edit => "file-pen-line",
        ToolKind::Read => "file-text",
        ToolKind::Search => "search",
        ToolKind::Web => "globe",
        ToolKind::Plan => "list-todo",
        ToolKind::Agent => "bot",
        ToolKind::Mcp => "plug",
        ToolKind::Other => "wrench",
    }
}

/// `--dcc-surface-edge`: the border at 80%.
pub(super) fn surface_edge(cx: &App) -> Hsla {
    cx.theme().colors.border.opacity(0.8)
}

fn copy_button(id: SharedString, value: String, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    let (muted, foreground, accent) = (colors.muted_foreground, colors.foreground, colors.accent);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.0))
        .rounded(px(5.0))
        .cursor_pointer()
        .text_color(muted)
        .hover(move |style| style.bg(accent).text_color(foreground))
        .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(value.clone())))
        .child(Icon::new("copy").size(12.0))
        .into_any_element()
}

fn section(
    id: &str,
    title: SharedString,
    extra: Option<AnyElement>,
    copy: String,
    body: AnyElement,
    cx: &App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    let edge = surface_edge(cx);
    div()
        .min_w_0()
        .overflow_hidden()
        .border_1()
        .border_color(edge)
        .rounded(px(8.0))
        .bg(colors.muted.opacity(0.22))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .pl(px(10.0))
                .pr(px(6.0))
                .py(px(4.0))
                .border_b_1()
                .border_color(edge)
                .text_size(px(10.5))
                .text_color(colors.muted_foreground)
                .child(div().mr_auto().child(title))
                .children(extra)
                .child(copy_button(
                    SharedString::from(format!("{id}-copy")),
                    copy,
                    cx,
                )),
        )
        .child(body)
        .into_any_element()
}

/// `.dcc-tool-command`, `.dcc-tool-output`: mono 11px at 1.6, scrolling past
/// `max_height`.
fn pre(id: String, max_height: f32, color: Hsla, cx: &App) -> gpui::Stateful<gpui::Div> {
    div()
        .id(SharedString::from(id))
        .max_h(px(max_height))
        .overflow_y_scroll()
        .px(px(10.0))
        .py(px(8.0))
        .font_family(cx.theme().fonts.mono.clone())
        .text_size(px(11.0))
        .line_height(px(17.6))
        .text_color(color)
}

fn diff_block(id: String, diff: &str, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    let (success, destructive, foreground, muted) = (
        colors.success,
        colors.destructive,
        colors.foreground,
        colors.muted_foreground,
    );
    let lines = diff.strip_suffix('\n').unwrap_or(diff).split('\n');
    pre(id, 320.0, foreground, cx)
        .overflow_x_scroll()
        .children(lines.map(|line| {
            let row = div().px(px(4.0)).mx(px(-4.0)).whitespace_nowrap();
            let row = match classify_diff_line(line) {
                DiffLineKind::Add => row
                    .bg(success.opacity(0.13))
                    .text_color(mix_oklab(success, 0.7, foreground)),
                DiffLineKind::Del => row.bg(destructive.opacity(0.11)).text_color(mix_oklab(
                    destructive,
                    0.7,
                    foreground,
                )),
                DiffLineKind::Hunk | DiffLineKind::Meta => row.text_color(muted),
                DiffLineKind::Context => row,
            };
            row.child(SharedString::from(if line.is_empty() {
                " ".to_string()
            } else {
                line.to_string()
            }))
        }))
        .into_any_element()
}

fn checklist_view(checklist: &PlanChecklist, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .text_size(px(12.0))
        .children(checklist.steps.iter().map(|step| {
            let (icon, icon_color, text_color) = match step.status {
                PlanStepStatus::Completed => {
                    ("circle-check", colors.success, colors.muted_foreground)
                }
                PlanStepStatus::InProgress => ("circle-dashed", colors.primary, colors.foreground),
                PlanStepStatus::Pending => (
                    "circle",
                    colors.muted_foreground.opacity(0.6),
                    colors.foreground,
                ),
            };
            div()
                .flex()
                .items_start()
                .gap(px(8.0))
                .line_height(px(16.2))
                .text_color(text_color)
                .when(step.status == PlanStepStatus::InProgress, |row| {
                    row.font_weight(gpui::FontWeight::MEDIUM)
                })
                .child(
                    div()
                        .mt(px(1.0))
                        .child(Icon::new(icon).size(14.0).color(icon_color)),
                )
                .child(div().min_w_0().child(SharedString::from(step.text.clone())))
        }))
        .into_any_element()
}

fn details(
    call: &ToolCall,
    key: &str,
    failed: bool,
    view: &ConversationView,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let mono = theme.fonts.mono.clone();
    let detail = call.detail.as_ref();
    let kind = classify_tool_action(&call.action);
    let command = detail
        .and_then(|detail| detail.command.clone())
        .or_else(|| call.command.clone());
    let file = detail
        .and_then(|detail| detail.file.clone())
        .or_else(|| call.file.clone());
    let raw_output = detail
        .and_then(|detail| detail.output.clone())
        .or_else(|| (!call.content.trim().is_empty()).then(|| call.content.clone()))
        .unwrap_or_default();
    let output = strip_ansi(&raw_output).trim_end().to_string();
    let container = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .mt(px(4.0))
        .mb(px(8.0))
        .ml(px(22.0))
        .min_w_0();
    if kind == ToolKind::Plan
        && let Some(checklist) =
            parse_plan_checklist(detail.and_then(|detail| detail.input.as_deref()))
    {
        return container
            .child(checklist_view(&checklist, cx))
            .into_any_element();
    }
    let mut children: Vec<AnyElement> = Vec::new();
    if let Some(file) = file.as_ref().filter(|_| kind != ToolKind::Command) {
        children.push(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_w_0()
                .font_family(mono.clone())
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(Icon::new("file-text").size(12.0))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(file.clone())),
                )
                .into_any_element(),
        );
    }
    if let Some(command) = command.as_ref().filter(|_| kind != ToolKind::Edit) {
        let body = pre(format!("{key}-command"), 128.0, colors.foreground, cx)
            .child(
                div()
                    .flex()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(colors.muted_foreground)
                            .child("$ "),
                    )
                    .child(div().min_w_0().child(SharedString::from(command.clone()))),
            )
            .into_any_element();
        children.push(section(
            &format!("{key}-cmd"),
            cx.t("conversation.toolCall.command"),
            None,
            command.clone(),
            body,
            cx,
        ));
    }
    let diff = detail.and_then(|detail| detail.diff.clone());
    if let Some(diff) = diff.as_ref() {
        let body = diff_block(format!("{key}-diff"), diff, cx);
        children.push(section(
            &format!("{key}-changes"),
            cx.t("conversation.toolCall.changes"),
            None,
            diff.clone(),
            body,
            cx,
        ));
    }
    let exit_code = detail.and_then(|detail| detail.exit_code);
    if !output.is_empty() {
        let color = if failed {
            mix_oklab(colors.destructive, 0.75, colors.foreground)
        } else {
            colors.foreground
        };
        let extra = exit_code.map(|code| {
            div()
                .font_family(mono.clone())
                .text_size(px(10.5))
                .text_color(if code != 0 {
                    colors.destructive
                } else {
                    colors.muted_foreground
                })
                .child(cx.t_with(
                    "conversation.toolCall.exitCode",
                    &Args::default().with("code", code.to_string()),
                ))
                .into_any_element()
        });
        let body = pre(format!("{key}-output"), 320.0, color, cx)
            .child(SharedString::from(output.clone()))
            .into_any_element();
        children.push(section(
            &format!("{key}-out"),
            cx.t("conversation.toolCall.output"),
            extra,
            output,
            body,
            cx,
        ));
    } else if failed {
        let reason = call
            .failure
            .clone()
            .filter(|reason| !reason.is_empty())
            .map(SharedString::from)
            .unwrap_or_else(|| cx.t("conversation.toolCall.failedFallback"));
        children.push(
            div()
                .text_size(px(12.0))
                .text_color(colors.destructive)
                .child(reason)
                .into_any_element(),
        );
    } else if diff.is_none() && detail.and_then(|detail| detail.input.as_ref()).is_none() {
        children.push(
            div()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("conversation.toolCall.noOutput"))
                .into_any_element(),
        );
    }
    if detail.is_some_and(|detail| detail.truncated) {
        children.push(
            div()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("conversation.toolCall.truncated"))
                .into_any_element(),
        );
    }
    if let Some(input) = detail
        .and_then(|detail| detail.input.clone())
        .filter(|_| diff.is_none())
    {
        let input_key = SharedString::from(format!("{key}-input"));
        let open = view.is_open(&input_key);
        let toggle = input_key.clone();
        let (muted, foreground) = (colors.muted_foreground, colors.foreground);
        let mut block = div().flex().flex_col().child(
            div()
                .id(input_key.clone())
                .flex()
                .items_center()
                .gap(px(4.0))
                .cursor_pointer()
                .text_size(px(11.0))
                .text_color(muted)
                .hover(move |style| style.text_color(foreground))
                .on_click(cx.listener(move |view, _, _, cx| view.toggle(toggle.clone(), cx)))
                .child(
                    Icon::new("chevron-right")
                        .size(10.0)
                        .rotate(if open { 90.0 } else { 0.0 }),
                )
                .child(cx.t("conversation.toolCall.input")),
        );
        if open {
            block = block.child(
                pre(format!("{key}-input-body"), 320.0, colors.foreground, cx)
                    .mt(px(4.0))
                    .border_1()
                    .border_color(surface_edge(cx))
                    .rounded(px(8.0))
                    .bg(colors.muted.opacity(0.22))
                    .child(SharedString::from(input)),
            );
        }
        children.push(block.into_any_element());
    }
    container.children(children).into_any_element()
}

pub(super) fn tool_row(
    view: &ConversationView,
    call: &ToolCall,
    live: bool,
    scope: &str,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let key = SharedString::from(format!("{scope}-tool-{}", call.id));
    let open = view.is_open(&key);
    let failed = call.failure.is_some();
    let running = !failed && live && call.streaming;
    let unfinished = !failed && !live && call.streaming;
    let kind = classify_tool_action(&call.action);
    let checklist = (kind == ToolKind::Plan)
        .then(|| {
            parse_plan_checklist(
                call.detail
                    .as_ref()
                    .and_then(|detail| detail.input.as_deref()),
            )
        })
        .flatten();
    let plan_target = checklist.as_ref().map(|checklist| match checklist.current {
        Some(current) => cx.t_with(
            "conversation.toolCall.planProgress",
            &Args::default()
                .with("done", checklist.completed.to_string())
                .with("total", checklist.steps.len().to_string())
                .with("step", checklist.steps[current].text.clone()),
        ),
        None => cx.t_with(
            "conversation.toolCall.planDone",
            &Args::default().with("total", checklist.steps.len().to_string()),
        ),
    });
    let target: Option<SharedString> = plan_target.or_else(|| {
        tool_target(call)
            .or_else(|| match kind {
                ToolKind::Mcp => Some(mcp_tool_label(&call.action)),
                ToolKind::Other => Some(call.action.clone()),
                _ => None,
            })
            .map(SharedString::from)
    });
    let stats = diff_stats(
        call.detail
            .as_ref()
            .and_then(|detail| detail.diff.as_deref()),
    );
    let exit_code = call.detail.as_ref().and_then(|detail| detail.exit_code);
    let verb = cx.t(&format!(
        "conversation.toolCall.verbs.{}.{}",
        kind.key(),
        if running { "live" } else { "done" }
    ));
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let mono = theme.fonts.mono.clone();
    let (muted, foreground, hover_bg) = (
        colors.muted_foreground,
        colors.foreground,
        colors.muted.opacity(0.45),
    );
    let status: AnyElement = if running {
        thinking_indicator(SharedString::from(format!("{key}-live")), 12.0).into_any_element()
    } else if unfinished {
        div()
            .text_size(px(10.5))
            .text_color(muted)
            .child(cx.t("conversation.activity.timeline.unfinished"))
            .into_any_element()
    } else if failed {
        Icon::new("circle-alert")
            .size(12.0)
            .color(colors.destructive)
            .into_any_element()
    } else {
        Icon::new("circle-check")
            .size(12.0)
            .color(colors.success.opacity(0.8))
            .into_any_element()
    };
    let toggle = key.clone();
    let summary = div()
        .id(key.clone())
        .flex()
        .items_center()
        .gap(px(6.0))
        .w_full()
        .min_w_0()
        .px(px(6.0))
        .py(px(3.0))
        .rounded(px(6.0))
        .text_size(px(12.0))
        .line_height(px(18.0))
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg).text_color(foreground))
        .on_click(cx.listener(move |view, _, _, cx| view.toggle(toggle.clone(), cx)))
        .child(
            div()
                .opacity(0.6)
                .child(
                    Icon::new("chevron-right")
                        .size(12.0)
                        .rotate(if open { 90.0 } else { 0.0 }),
                ),
        )
        .child(
            div()
                .when(failed, |icon| icon.text_color(colors.destructive))
                .when(!failed, |icon| icon.opacity(0.8))
                .child(Icon::new(kind_icon(kind)).size(14.0)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(foreground.opacity(0.85))
                .child(verb),
        )
        .children(target.map(|target| {
            div()
                .min_w_0()
                .truncate()
                .text_color(foreground.opacity(0.7))
                .when(kind == ToolKind::Command, |text| {
                    text.font_family(mono.clone()).text_size(px(11.0))
                })
                .child(target)
        }))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.0))
                .ml_auto()
                .pl(px(6.0))
                .children(stats.map(|stats| {
                    div()
                        .flex()
                        .gap(px(4.0))
                        .font_family(mono.clone())
                        .text_size(px(10.5))
                        .when(stats.additions > 0, |row| {
                            row.child(
                                div()
                                    .text_color(colors.success)
                                    .child(format!("+{}", stats.additions)),
                            )
                        })
                        .when(stats.deletions > 0, |row| {
                            row.child(
                                div()
                                    .text_color(colors.destructive)
                                    .child(format!("−{}", stats.deletions)),
                            )
                        })
                }))
                .children(exit_code.filter(|code| *code != 0).map(|code| {
                    div()
                        .font_family(mono.clone())
                        .text_size(px(10.5))
                        .text_color(colors.destructive)
                        .child(cx.t_with(
                            "conversation.toolCall.exitCode",
                            &Args::default().with("code", code.to_string()),
                        ))
                }))
                .child(status),
        );
    div()
        .min_w_0()
        .child(summary)
        .when(open, |row| row.child(details(call, &key, failed, view, cx)))
        .into_any_element()
}
