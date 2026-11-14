//! A task's origin on screen (`data::task_origin`): one quiet line —
//! "Origem: Dev na sala Checkout ← fonte Slack · menções a mim" — that a
//! click opens into the chain, oldest first, from the message that started
//! it to the agent that opened the task, with whatever the host adds after
//! (the pull request, its review). A row that happened in a room goes there
//! on a click; a message's link opens in the browser. The task's
//! conversation and the pull request hub draw the same one.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use crate::data::task_origin::{ChainRow, OriginRead, rows, start_label};
use crate::data::time::medium_date_time;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

fn text(key: &'static str, args: &[(&'static str, String)], cx: &gpui::App) -> SharedString {
    let args = args.iter().fold(Args::default(), |args, (name, value)| {
        args.with(name, value.clone())
    });
    cx.t_with(key, &args)
}

fn room_name(read: &OriginRead, room: &Option<String>, cx: &gpui::App) -> SharedString {
    match room {
        Some(id) => read
            .rooms
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.clone())
            .into(),
        None => cx.t("origin.roomGeneral"),
    }
}

/// The line, and under it the chain while `open`. `prefix` names the
/// elements for the host's tests.
pub(crate) fn origin_strip<T: 'static>(
    prefix: &'static str,
    read: &OriginRead,
    open: bool,
    tail: Vec<ChainRow>,
    toggle: fn(&mut T, &mut Context<T>),
    go: fn(&mut T, String, Option<String>, &mut Context<T>),
    cx: &mut Context<T>,
) -> Option<AnyElement> {
    let last = read.origin.steps.last()?;
    let colors = cx.theme().colors.clone();
    let english = cx.global::<I18n>().locale() == Locale::En;
    let who = cx.t_with(
        "origin.who",
        &Args::default()
            .with("agent", last.agent.clone())
            .with("room", room_name(read, &last.room, cx).to_string()),
    );
    let from = start_label(&read.origin).map(|(key, args)| text(key, &args, cx));
    let line = div()
        .id(SharedString::from(format!("{prefix}-line")))
        .debug_selector(move || format!("{prefix}-line"))
        .flex()
        .items_center()
        .gap(px(6.0))
        .min_w_0()
        .text_size(px(11.5))
        .text_color(colors.muted_foreground)
        .cursor_pointer()
        .on_click(cx.listener(move |host, _, _, cx| toggle(host, cx)))
        .child(
            Icon::new("corner-down-right")
                .size(13.0)
                .color(colors.muted_foreground),
        )
        .child(div().flex_none().child(cx.t("origin.label")))
        .child(div().flex_none().text_color(colors.foreground).child(who))
        .children(from.map(|from| {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_w_0()
                .child(
                    Icon::new("arrow-left")
                        .size(11.0)
                        .color(colors.muted_foreground),
                )
                .child(div().truncate().child(from))
        }))
        .child(
            Icon::new(if open { "chevron-up" } else { "chevron-down" })
                .size(12.0)
                .color(colors.muted_foreground),
        );
    let chain = open.then(|| {
        let mut all = rows(&read.origin);
        all.extend(tail);
        div()
            .debug_selector(move || format!("{prefix}-chain"))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .mt(px(8.0))
            .max_w(px(520.0))
            .px(px(14.0))
            .py(px(12.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.card)
            .children(all.into_iter().enumerate().map(|(index, row)| {
                let title = text(row.title.0, &row.title.1, cx);
                let place = row.place.clone();
                let mut under: Vec<String> = Vec::new();
                if !row.detail.is_empty() {
                    under.push(format!("\"{}\"", row.detail));
                }
                if let Some((_, room)) = &row.place {
                    under.push(
                        cx.t_with(
                            "origin.inRoom",
                            &Args::default().with("room", room_name(read, room, cx).to_string()),
                        )
                        .to_string(),
                    );
                }
                if !row.at.is_empty() {
                    under.push(medium_date_time(&row.at, english));
                }
                let link = row.link.clone().map(|url| {
                    div()
                        .id(SharedString::from(format!("{prefix}-link-{index}")))
                        .debug_selector(move || format!("{prefix}-link-{index}"))
                        .flex_none()
                        .text_color(colors.primary)
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            cx.open_url(&url);
                        })
                        .child(cx.t("origin.open"))
                });
                div()
                    .id(SharedString::from(format!("{prefix}-row-{index}")))
                    .debug_selector(move || format!("{prefix}-row-{index}"))
                    .flex()
                    .items_start()
                    .gap(px(10.0))
                    .when(place.is_some(), |element| {
                        element
                            .cursor_pointer()
                            .on_click(cx.listener(move |host, _, _, cx| {
                                if let Some((agent, room)) = place.clone() {
                                    go(host, agent, room, cx);
                                }
                            }))
                    })
                    .child(
                        div().mt(px(2.0)).child(
                            Icon::new(row.icon)
                                .size(14.0)
                                .color(colors.muted_foreground),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(div().min_w_0().child(title))
                                    .children(link),
                            )
                            .when(!under.is_empty(), |column| {
                                column.child(
                                    div()
                                        .text_size(px(11.5))
                                        .line_height(px(17.0))
                                        .text_color(colors.muted_foreground)
                                        .child(under.join(" · ")),
                                )
                            }),
                    )
            }))
    });
    Some(
        div()
            .debug_selector(move || prefix.to_string())
            .flex()
            .flex_col()
            .child(line)
            .children(chain)
            .into_any_element(),
    )
}
