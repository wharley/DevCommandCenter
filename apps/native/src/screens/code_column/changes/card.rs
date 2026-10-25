//! One file's card (`ReviewChangeCard`): the header (chevron, icon, name
//! and folder, +/−, status letter and the file's git actions) over its
//! diff at the desktop's height.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, img, prelude::*, px};

use super::super::CodeColumn;
use super::view::{card_shadow, dirname, row_button, stats, status_color};
use super::{CardDiff, ChangeCard, ChangeGroup, card_diff_height};
use crate::data::desktop_api::GitAction;
use crate::i18n::Translate;
use crate::project::file_icon::file_icon;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, spinner};

fn body(card: &ChangeCard, at: usize, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let centered = || {
        div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_size(px(11.0))
    };
    let content = match &card.diff {
        CardDiff::Ready(diff) => diff.clone().into_any_element(),
        CardDiff::Failed(error) => centered()
            .px(px(16.0))
            .text_color(colors.destructive)
            .child(Icon::new("circle-alert").size(14.0))
            .child(div().text_center().child(SharedString::from(error.clone())))
            .into_any_element(),
        CardDiff::Idle | CardDiff::Loading(_) => centered()
            .text_color(colors.muted_foreground)
            .child(spinner(("change-card-loading", at), 14.0))
            .child(cx.t("inspector.changes.loadingDiff"))
            .into_any_element(),
    };
    div()
        .debug_selector(move || format!("change-card-body-{at}"))
        .h(px(card_diff_height(
            card.entry.insertions,
            card.entry.deletions,
        )))
        .overflow_hidden()
        .bg(colors.background)
        .child(content)
        .into_any_element()
}

/// The staged file's "unstage", the unstaged one's "discard" and "stage"
/// (none on a conflict or on the branch).
fn actions(column: &CodeColumn, at: usize, cx: &mut Context<CodeColumn>) -> Vec<AnyElement> {
    let card = &column.changes.cards[at];
    let busy = column.changes.busy;
    let path = card.entry.path.clone();
    match card.group {
        ChangeGroup::Staged => vec![row_button(
            ("change-card-unstage", at),
            "minus",
            busy,
            false,
            cx.listener(move |column, _, _, cx| column.git_file(GitAction::Unstage, &path, cx)),
            cx,
        )],
        ChangeGroup::Unstaged if !card.unmerged() => {
            let stage = path.clone();
            vec![
                row_button(
                    ("change-card-discard", at),
                    "undo-2",
                    busy,
                    false,
                    cx.listener(move |column, _, _, cx| {
                        column.git_file(GitAction::Discard, &path, cx)
                    }),
                    cx,
                ),
                row_button(
                    ("change-card-stage", at),
                    "plus",
                    busy,
                    false,
                    cx.listener(move |column, _, _, cx| {
                        column.git_file(GitAction::Stage, &stage, cx)
                    }),
                    cx,
                ),
            ]
        }
        _ => Vec::new(),
    }
}

pub(super) fn card(column: &CodeColumn, at: usize, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let card = &column.changes.cards[at];
    let entry = &card.entry;
    let folder = dirname(&entry.path).to_string();
    let open = card.open;
    let mut buttons = actions(column, at, cx);
    // "Abrir diff ampliado": the diff on the stage.
    buttons.push(row_button(
        ("change-card-expand", at),
        "expand",
        false,
        false,
        cx.listener(move |column, _, _, cx| column.open_change(at, cx)),
        cx,
    ));
    let header = div()
        .flex()
        .min_h(px(44.0))
        .items_center()
        .gap(px(8.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.45))
        .px(px(10.0))
        .py(px(6.0))
        .child(
            div()
                .id(("change-card-toggle", at))
                .debug_selector(move || format!("change-card-{at}"))
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .on_click(cx.listener(move |column, _, _, cx| column.toggle_card(at, cx)))
                .child(
                    Icon::new(if open {
                        "chevron-down"
                    } else {
                        "chevron-right"
                    })
                    .size(14.0)
                    .color(colors.muted_foreground),
                )
                .child(img(file_icon(&entry.name)).size(px(14.0)).flex_shrink_0())
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_baseline()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(
                            div()
                                .flex_shrink_0()
                                .max_w_full()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(entry.name.clone()),
                        )
                        .when(!folder.is_empty(), |line| {
                            line.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .ml(px(6.0))
                                    .truncate()
                                    .text_size(px(10.0))
                                    .text_color(colors.muted_foreground)
                                    .child(folder),
                            )
                        }),
                ),
        )
        .child(
            stats(entry.insertions, entry.deletions, false, cx)
                .children(crate::screens::code_column::review::review_badge(
                    column,
                    &entry.path,
                    cx,
                ))
                .child(
                    div()
                        .min_w(px(16.0))
                        .text_center()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(status_color(&entry.status, cx))
                        .child(entry.status.clone()),
                )
                .children(buttons),
        );
    div()
        .px(px(8.0))
        .pb(px(8.0))
        .child(
            div()
                .overflow_hidden()
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.background)
                .shadow(card_shadow())
                .child(header)
                .when(open, |article| article.child(body(card, at, cx))),
        )
        .into_any_element()
}
