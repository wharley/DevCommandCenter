//! The session's live events (`SessionEventFeed compact`): Resumo or
//! Diagnóstico, this session's or all, each event's badge and line, the
//! empty states, and "Ir para o fim" when scrolled up. New events are
//! followed to the bottom.

use gpui::{
    AnyElement, Context, FontWeight, Hsla, SharedString, div, linear_color_stop, linear_gradient,
    prelude::*, px,
};

use super::SessionPanel;
use crate::data::session_activity::{Text, Tone, is_milestone, label, payload, presentation, tone};
use crate::i18n::{Args, Translate};
use crate::theme::color::mix_oklab;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Badge, BadgeVariant, Icon};

fn text(value: &Text, cx: &gpui::App) -> SharedString {
    match value {
        Text::Owned(value) => value.clone().into(),
        Text::Key(key, args) => {
            let mut with = Args::default();
            for (name, value) in args {
                with = with.with(name, value.clone());
            }
            cx.t_with(key, &with)
        }
        Text::Prefixed(session, key) => format!("{session} · {}", cx.t(key)).into(),
    }
}

/// The left edge's color by tone (`.dcc-session-event[data-tone]`).
fn edge(tone: Tone, cx: &gpui::App) -> Hsla {
    let theme = cx.theme();
    let colors = &theme.colors;
    let strong = mix_oklab(colors.border, 0.75, colors.foreground);
    match tone {
        Tone::Success => colors.success.opacity(0.8),
        Tone::Warn => colors.warning.opacity(0.8),
        Tone::Secondary => if theme.appearance == Appearance::Dark {
            tw::EMERALD_500.hsla()
        } else {
            // `--chart-2` in the light theme.
            tw::Oklch(0.6, 0.118, 184.704).hsla()
        }
        .opacity(0.7),
        Tone::Outline => strong.opacity(0.7),
    }
}

fn badge(tone: Tone) -> BadgeVariant {
    match tone {
        Tone::Success => BadgeVariant::Success,
        Tone::Warn => BadgeVariant::Warn,
        Tone::Secondary => BadgeVariant::Secondary,
        Tone::Outline => BadgeVariant::Outline,
    }
}

fn toggle(
    id: &'static str,
    labels: [SharedString; 2],
    first: bool,
    on: impl Fn(&mut SessionPanel, bool) + Clone + 'static,
    cx: &mut Context<SessionPanel>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let items = labels.into_iter().enumerate().map(|(index, label)| {
        let chosen = (index == 0) == first;
        let on = on.clone();
        div()
            .id(SharedString::from(format!("{id}-{index}")))
            .debug_selector(move || format!("{id}-{index}"))
            .flex()
            .items_center()
            .h(px(24.0))
            .px(px(8.0))
            .rounded(px(6.0))
            .text_size(px(11.0))
            .cursor_pointer()
            .when(chosen, |item| item.bg(colors.muted))
            .hover(|item| item.bg(colors.muted))
            .on_click(cx.listener(move |panel, _, _, cx| {
                on(panel, index == 0);
                cx.notify();
            }))
            .child(label)
    });
    div()
        .flex()
        .gap(px(2.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(colors.border.opacity(0.5))
        .bg(colors.muted.opacity(0.2))
        .p(px(2.0))
        .children(items)
        .into_any_element()
}

impl SessionPanel {
    pub(super) fn feed(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (events, session) = self
            .data(cx)
            .map(|data| (data.activity.as_ref().clone(), data.session_id.clone()))
            .unwrap_or_default();
        let can_filter = session.is_some();
        let current_only = can_filter && self.current_only;
        // The live events are this session's already (the desktop's stream
        // keeps the selected one's too).
        let scoped: Vec<_> = events
            .iter()
            .filter(|event| {
                !current_only || Some(event.session_id.0.as_str()) == session.as_deref()
            })
            .collect();
        let shown: Vec<_> = scoped
            .iter()
            .copied()
            .filter(|event| !self.summary || is_milestone(event))
            .collect();
        let latest = shown.last().map_or_else(
            || format!("{}-empty", self.summary),
            |last| format!("{}-{}-{}", self.summary, label(last), shown.len()),
        );
        if self.last_event.as_deref() != Some(latest.as_str()) {
            self.last_event = Some(latest);
            self.feed_scroll.scroll_to_bottom();
        }
        let empty = if self.summary && !scoped.is_empty() {
            "sessionEventFeed.emptySummary"
        } else if current_only && !events.is_empty() {
            "sessionEventFeed.emptyForSession"
        } else {
            "sessionEventFeed.emptyTimeline"
        };
        let rows: Vec<AnyElement> = shown
            .iter()
            .map(|event| {
                let tone = tone(event);
                let (title, line) = if self.summary {
                    let (title, line) = presentation(event);
                    (cx.t(title), text(&line, cx))
                } else {
                    (label(event).into(), text(&payload(event), cx))
                };
                let mine = Some(event.session_id.0.as_str()) == session.as_deref();
                div()
                    .px(px(20.0))
                    .pb(px(6.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(7.2))
                            .p(px(14.4))
                            .rounded(px(8.0))
                            .border_l(px(3.0))
                            .border_color(edge(tone, cx))
                            .bg(linear_gradient(
                                180.0,
                                linear_color_stop(colors.muted.opacity(0.78), 0.0),
                                linear_color_stop(colors.card.opacity(0.94), 1.0),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        Badge::new(title, badge(tone)).weight(FontWeight::NORMAL),
                                    )
                                    .when(!self.summary, |header| {
                                        header.child(
                                            Badge::new(
                                                if mine {
                                                    cx.t("sessionEventFeed.thisSessionBadge")
                                                } else {
                                                    event
                                                        .session_id
                                                        .0
                                                        .chars()
                                                        .take(8)
                                                        .collect::<String>()
                                                        .into()
                                                },
                                                if mine {
                                                    BadgeVariant::Secondary
                                                } else {
                                                    BadgeVariant::Outline
                                                },
                                            )
                                            .weight(FontWeight::NORMAL),
                                        )
                                    }),
                            )
                            .child(div().text_size(px(13.0)).line_height(px(17.9)).child(line)),
                    )
                    .into_any_element()
            })
            .collect();
        let scrolled_up = {
            let offset = self.feed_scroll.offset().y;
            let max = self.feed_scroll.max_offset().y;
            f32::from(max + offset) > 24.0
        };
        let body = if rows.is_empty() {
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .px(px(32.0))
                .child(
                    div()
                        .max_w(px(448.0))
                        .text_center()
                        .text_size(px(13.0))
                        .line_height(px(21.1))
                        .text_color(colors.muted_foreground)
                        .child(cx.t(empty)),
                )
                .into_any_element()
        } else {
            div().flex().flex_col().children(rows).into_any_element()
        };
        div()
            .debug_selector(|| "session-feed".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .pb(px(4.0))
                    .pt(px(8.0))
                    .child(toggle(
                        "session-feed-view",
                        [
                            cx.t("sessionEventFeed.viewSummary"),
                            cx.t("sessionEventFeed.viewDiagnostics"),
                        ],
                        self.summary,
                        |panel, first| panel.summary = first,
                        cx,
                    ))
                    .when(can_filter, |bar| {
                        bar.child(toggle(
                            "session-feed-scope",
                            [
                                cx.t("sessionEventFeed.scopeCurrent"),
                                cx.t("sessionEventFeed.scopeAll"),
                            ],
                            self.current_only,
                            |panel, first| panel.current_only = first,
                            cx,
                        ))
                    }),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("session-feed-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.feed_scroll)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_h_full()
                                    .child(div().h(px(24.0)).flex_none())
                                    .child(body),
                            ),
                    )
                    .when(scrolled_up && !shown.is_empty(), |area| {
                        area.child(
                            div()
                                .absolute()
                                .bottom(px(4.0))
                                .left_0()
                                .right_0()
                                .flex()
                                .justify_center()
                                .py(px(6.0))
                                .child(
                                    div()
                                        .id("session-feed-bottom")
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(colors.border.opacity(0.6))
                                        .bg(colors.card)
                                        .px(px(12.0))
                                        .py(px(4.0))
                                        .text_size(px(12.0))
                                        .text_color(colors.muted_foreground)
                                        .shadow_sm()
                                        .cursor_pointer()
                                        .hover(|pill| pill.text_color(colors.foreground))
                                        .on_click(cx.listener(|panel, _, _, cx| {
                                            panel.feed_scroll.scroll_to_bottom();
                                            cx.notify();
                                        }))
                                        .child(Icon::new("chevron-down").size(14.0).stroke(2.0))
                                        .child(cx.t("conversation.scrollToBottom")),
                                ),
                        )
                    }),
            )
            .into_any_element()
    }
}
