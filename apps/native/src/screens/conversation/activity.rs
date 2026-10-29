//! `AssistantActivity`: everything the agent did in a turn besides its
//! answer. While the turn runs its narration shows as it comes and each burst
//! of work is one line wearing the latest step; once it settles all of it
//! folds behind "Trabalhou por 1m 12s · editou 2 arquivos, rodou 3 comandos".

use dcc_core::domain::timeline::Annotation;
use dcc_core::domain::timeline::activity::{
    SUMMARY_KIND_ORDER, ToolKind, TurnSegment, format_elapsed, segment_turn, summarize_turn_work,
    turn_duration_ms,
};
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::ConversationView;
use super::tool_row::{surface_edge, tool_row};
use crate::data::time;
use crate::i18n::{Args, Translate};
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::theme::ActiveTheme;
use crate::ui::{Icon, thinking_indicator};

/// Long runs show their most recent steps; older ones are one click away.
const RUN_PAGE_SIZE: usize = 30;

pub(super) struct TurnActivity<'a> {
    pub key: &'a str,
    /// All the turn's annotations; only the activity ones are drawn here.
    pub annotations: &'a [Annotation],
    pub streaming: bool,
    pub interrupted: bool,
    pub waiting: bool,
    pub started_at: Option<&'a str>,
    pub ended_at: Option<&'a str>,
    /// The agent's robot for the live status line.
    pub live_mascot: Option<AnyElement>,
}

fn reasoning_item(
    view: &ConversationView,
    id: &str,
    label: Option<&str>,
    content: &str,
    thinking: bool,
    scope: &str,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let key = SharedString::from(format!("{scope}-thought-{id}"));
    let open = view.is_open(&key);
    let label = if thinking {
        cx.t("conversation.activity.turn.thinking")
    } else {
        match label.filter(|label| !label.is_empty() && *label != "Thinking") {
            Some(label) => SharedString::from(label.to_string()),
            None => cx.t("conversation.activity.turn.thought"),
        }
    };
    let colors = cx.theme().colors.clone();
    let (muted, foreground) = (colors.muted_foreground, colors.foreground);
    let toggle = key.clone();
    let body = open.then(|| {
        let text = if content.trim().is_empty() {
            cx.t("conversation.reasoning.empty")
        } else {
            SharedString::from(content.to_string())
        };
        div()
            .id(SharedString::from(format!("{key}-body")))
            .max_h(px(256.0))
            .overflow_y_scroll()
            .pl(px(16.0))
            .pt(px(4.0))
            .text_size(px(12.0))
            .line_height(px(24.0))
            .text_color(muted)
            .child(text)
    });
    div()
        .px(px(6.0))
        .py(px(1.0))
        .flex()
        .flex_col()
        .child(
            div()
                .id(key.clone())
                .flex()
                .max_w_full()
                .items_center()
                .gap(px(6.0))
                .py(px(2.0))
                .cursor_pointer()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(muted)
                .hover(move |style| style.text_color(foreground))
                .on_click(cx.listener(move |view, _, _, cx| view.toggle(toggle.clone(), cx)))
                .child(Icon::new("brain").size(12.0))
                .child(div().min_w_0().truncate().child(label))
                .child(
                    Icon::new("chevron-right")
                        .size(12.0)
                        .rotate(if open { 90.0 } else { 0.0 }),
                ),
        )
        .children(body)
        .into_any_element()
}

fn work_item(
    view: &ConversationView,
    item: &Annotation,
    live: bool,
    scope: &str,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    match item {
        Annotation::ToolCall(call) => tool_row(view, call, live, scope, cx),
        Annotation::Reasoning {
            id,
            label,
            content,
            streaming,
            ..
        } => reasoning_item(
            view,
            id,
            label.as_deref(),
            content,
            live && *streaming,
            scope,
            cx,
        ),
        _ => div().into_any_element(),
    }
}

/// The small text buttons of a run (`.dcc-turn-run-more`, `.dcc-turn-earlier`).
fn run_button(
    id: SharedString,
    label: SharedString,
    open: Option<bool>,
    cx: &gpui::App,
) -> gpui::Stateful<gpui::Div> {
    let colors = &cx.theme().colors;
    let (muted, foreground, accent) = (colors.muted_foreground, colors.foreground, colors.accent);
    div()
        .id(id)
        .flex()
        .flex_none()
        .self_start()
        .items_center()
        .gap(px(5.0))
        .mt(px(2.0))
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(6.0))
        .text_size(px(11.0))
        .line_height(px(16.0))
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.bg(accent).text_color(foreground))
        .children(open.map(|open| {
            Icon::new("chevron-right")
                .size(12.0)
                .rotate(if open { 90.0 } else { 0.0 })
        }))
        .child(label)
}

fn run_list(
    view: &ConversationView,
    items: &[&Annotation],
    live: bool,
    scope: &str,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let limit_key = SharedString::from(format!("{scope}-limit"));
    let limit = view.run_limit(&limit_key, RUN_PAGE_SIZE);
    let hidden = items.len().saturating_sub(limit);
    let visible = &items[hidden..];
    let earlier = (hidden > 0).then(|| {
        let key = limit_key.clone();
        run_button(
            SharedString::from(format!("{scope}-earlier")),
            cx.t_with(
                "conversation.activity.timeline.earlier",
                &Args::count(hidden.min(RUN_PAGE_SIZE) as i64),
            ),
            None,
            cx,
        )
        .on_click(
            cx.listener(move |view, _, _, cx| view.show_earlier(key.clone(), RUN_PAGE_SIZE, cx)),
        )
    });
    let mut rows = Vec::with_capacity(visible.len());
    for item in visible {
        rows.push(work_item(view, item, live, scope, cx));
    }
    div()
        .flex()
        .flex_col()
        .gap(px(1.0))
        .min_w_0()
        .children(earlier)
        .children(rows)
        .into_any_element()
}

/// `liveRunHeadline`: a thought that already ended says little, so the last
/// action shows instead; a thought still going stays on top.
fn live_headline<'a>(items: &[&'a Annotation]) -> Option<&'a Annotation> {
    let last = *items.last()?;
    if let Annotation::Reasoning {
        streaming: false, ..
    } = last
        && let Some(tool) = items
            .iter()
            .rev()
            .find(|item| matches!(item, Annotation::ToolCall(_)))
    {
        return Some(tool);
    }
    Some(last)
}

fn live_run(
    view: &ConversationView,
    items: &[&Annotation],
    active: bool,
    scope: &str,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let Some(latest) = live_headline(items) else {
        return div().into_any_element();
    };
    let key = SharedString::from(format!("{scope}-run"));
    let open = view.is_open(&key);
    let edge = surface_edge(cx);
    let run = div().min_w_0().border_l_1().border_color(edge).pl(px(10.0));
    let toggle = key.clone();
    if open {
        let header = run_button(
            SharedString::from(format!("{key}-toggle")),
            cx.t_with(
                "conversation.activity.turn.steps",
                &Args::count(items.len() as i64),
            ),
            Some(true),
            cx,
        )
        .on_click(cx.listener(move |view, _, _, cx| view.toggle(toggle.clone(), cx)));
        let list = run_list(view, items, active, scope, cx);
        return run
            .flex()
            .flex_col()
            .child(header)
            .child(list)
            .into_any_element();
    }
    let current = work_item(view, latest, active, scope, cx);
    let more = (items.len() > 1).then(|| {
        run_button(
            SharedString::from(format!("{key}-more")),
            cx.t_with(
                "conversation.activity.turn.moreSteps",
                &Args::count(items.len() as i64 - 1),
            ),
            None,
            cx,
        )
        .on_click(cx.listener(move |view, _, _, cx| view.toggle(toggle.clone(), cx)))
    });
    run.flex()
        .flex_col()
        .child(current)
        .children(more)
        .into_any_element()
}

fn prose(
    view: &ConversationView,
    content: &str,
    id: &str,
    folded: bool,
    streaming: bool,
    window: &mut Window,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let blocks = if streaming {
        view.paced_markdown(id, content, window, cx)
    } else {
        view.markdown(content)
    };
    let style = if folded {
        MarkdownStyle {
            size: 12.5,
            ..MarkdownStyle::reply()
        }
    } else {
        MarkdownStyle::reply()
    };
    let foreground = cx.theme().colors.foreground;
    div()
        .min_w_0()
        .text_color(foreground)
        .text_size(px(style.size))
        .when(folded, |text| text.opacity(0.86))
        .child(render_markdown(&blocks, style, id, window, cx))
        .into_any_element()
}

fn summary_text(annotations: &[Annotation], cx: &gpui::App) -> (String, usize) {
    let summary = summarize_turn_work(annotations);
    let mut parts: Vec<String> = Vec::new();
    for kind in SUMMARY_KIND_ORDER {
        let count = if kind == ToolKind::Edit {
            summary.edited_files
        } else {
            summary.counts.get(&kind).copied().unwrap_or(0)
        };
        if count > 0 {
            parts.push(
                cx.t_with(
                    &format!("conversation.activity.turn.parts.{}", kind.key()),
                    &Args::count(count as i64),
                )
                .to_string(),
            );
        }
    }
    if parts.is_empty() && summary.thoughts > 0 {
        parts.push(
            cx.t_with(
                "conversation.activity.turn.parts.thoughts",
                &Args::count(summary.thoughts as i64),
            )
            .to_string(),
        );
    }
    (parts.join(", "), summary.failures)
}

pub(super) fn turn_activity(
    view: &ConversationView,
    activity: TurnActivity<'_>,
    window: &mut Window,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let segments = segment_turn(activity.annotations);
    let live = !activity.interrupted && activity.streaming;
    let colors = cx.theme().colors.clone();
    let scope = activity.key;
    if live {
        let count = segments.len();
        let mut rows: Vec<AnyElement> = Vec::with_capacity(count + 1);
        for (index, segment) in segments.iter().enumerate() {
            let segment_scope = format!("{scope}-s{index}");
            rows.push(match segment {
                TurnSegment::Prose(annotation) => {
                    let (content, streaming) = match annotation {
                        Annotation::Commentary {
                            content, streaming, ..
                        } => (content.as_str(), *streaming),
                        _ => ("", false),
                    };
                    let streaming = streaming && index + 1 == count;
                    prose(view, content, &segment_scope, false, streaming, window, cx)
                }
                TurnSegment::Work(items) => {
                    live_run(view, items, index + 1 == count, &segment_scope, cx)
                }
            });
        }
        let label = if activity.waiting {
            cx.t("conversation.activity.timeline.waiting")
        } else if matches!(segments.last(), None | Some(TurnSegment::Prose(_))) {
            cx.t("conversation.activity.turn.writing")
        } else {
            cx.t("conversation.activity.turn.working")
        };
        let since = activity.started_at.and_then(time::parse_ms).or_else(|| {
            activity.annotations.iter().find_map(|item| match item {
                Annotation::Commentary { created_at, .. }
                | Annotation::Reasoning { created_at, .. } => {
                    created_at.as_deref().and_then(time::parse_ms)
                }
                Annotation::ToolCall(call) => call.created_at.as_deref().and_then(time::parse_ms),
                _ => None,
            })
        });
        let clock = since.map(|since| format_elapsed(view.now_ms() - since));
        let marker = activity.live_mascot.unwrap_or_else(|| {
            if activity.waiting {
                Icon::new("circle-pause").size(14.0).into_any_element()
            } else {
                thinking_indicator(SharedString::from(format!("{scope}-thinking")), 13.0)
                    .into_any_element()
            }
        });
        rows.push(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .min_w_0()
                .mt(px(2.0))
                .pt(px(8.0))
                .pb(px(6.0))
                .text_size(px(12.0))
                .text_color(if activity.waiting {
                    colors.accent_idea
                } else {
                    colors.muted_foreground
                })
                .child(marker)
                .child(div().min_w_0().truncate().child(label))
                .children(clock.map(|clock| {
                    div()
                        .flex()
                        .flex_shrink_0()
                        .gap(px(6.0))
                        .opacity(0.75)
                        .font_features(crate::ui::tabular_nums())
                        .child("·")
                        .child(clock)
                }))
                .into_any_element(),
        );
        return div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .min_w_0()
            .mb(px(10.0))
            .children(rows)
            .into_any_element();
    }

    let key = SharedString::from(format!("{scope}-fold"));
    let open = view.is_open(&key);
    let (summary, failures) = summary_text(activity.annotations, cx);
    let duration = turn_duration_ms(activity.started_at, activity.ended_at).map(format_elapsed);
    let title = match (activity.interrupted, duration) {
        (true, Some(duration)) => cx.t_with(
            "conversation.activity.turn.interrupted",
            &Args::default().with("duration", duration),
        ),
        (true, None) => cx.t("conversation.activity.turn.interruptedNoDuration"),
        (false, Some(duration)) => cx.t_with(
            "conversation.activity.turn.worked",
            &Args::default().with("duration", duration),
        ),
        (false, None) => cx.t("conversation.activity.turn.workedNoDuration"),
    };
    let (muted, foreground, accent) = (colors.muted_foreground, colors.foreground, colors.accent);
    let toggle = key.clone();
    let icon = if activity.interrupted {
        Some("circle-alert")
    } else if summary.is_empty() {
        Some("brain")
    } else {
        None
    };
    let fold = div()
        .id(key.clone())
        .flex()
        .items_center()
        .gap(px(6.0))
        .max_w_full()
        .min_w_0()
        .self_start()
        .pl(px(4.0))
        .pr(px(8.0))
        .py(px(4.0))
        .ml(px(-4.0))
        .rounded(px(8.0))
        .text_size(px(12.0))
        .line_height(px(18.0))
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.bg(accent).text_color(foreground))
        .on_click(cx.listener(move |view, _, _, cx| view.toggle(toggle.clone(), cx)))
        .child(
            Icon::new("chevron-right")
                .size(14.0)
                .rotate(if open { 90.0 } else { 0.0 }),
        )
        .children(icon.map(|icon| Icon::new(icon).size(14.0)))
        .child(
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::MEDIUM)
                .text_color(foreground.opacity(0.85))
                .child(title),
        )
        .when(!summary.is_empty(), |fold| {
            fold.child(div().min_w_0().truncate().child(format!("· {summary}")))
        })
        .when(failures > 0, |fold| {
            fold.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(6.0))
                    .py(px(1.0))
                    .rounded(px(6.0))
                    .bg(colors.destructive.opacity(0.09))
                    .text_size(px(10.5))
                    .text_color(colors.destructive)
                    .child(Icon::new("circle-alert").size(12.0))
                    .child(cx.t_with(
                        "conversation.activity.failed",
                        &Args::count(failures as i64),
                    )),
            )
        });
    let body = open.then(|| {
        let mut rows: Vec<AnyElement> = Vec::with_capacity(segments.len());
        for (index, segment) in segments.iter().enumerate() {
            let segment_scope = format!("{scope}-s{index}");
            rows.push(match segment {
                TurnSegment::Prose(annotation) => {
                    let content = match annotation {
                        Annotation::Commentary { content, .. } => content.as_str(),
                        _ => "",
                    };
                    prose(view, content, &segment_scope, true, false, window, cx)
                }
                TurnSegment::Work(items) => run_list(view, items, false, &segment_scope, cx),
            });
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .mt(px(6.0))
            .mb(px(4.0))
            .ml(px(6.0))
            .pl(px(10.0))
            .border_l_1()
            .border_color(surface_edge(cx))
            .children(rows)
    });
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .mb(px(10.0))
        .child(fold)
        .children(body)
        .into_any_element()
}
