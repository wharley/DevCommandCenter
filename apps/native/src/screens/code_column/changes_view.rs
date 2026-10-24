//! "Mudanças" drawn: the review header (mark, title, +/−, open or close
//! every card), the "Preparadas" / "Não preparadas" groups and each file's
//! card (`ReviewChangeCard`) with its diff.

use gpui::{
    AnyElement, BoxShadow, Context, FontWeight, Hsla, SharedString, Window, div, img, point,
    prelude::*, px,
};

use super::CodeColumn;
use super::changes::{CardDiff, ChangeCard, ChangeGroup, card_diff_height, summarize};
use crate::i18n::Translate;
use crate::project::file_icon::file_icon;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::tooltip::Side;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, Tracked, spinner, tabular_nums};

/// A child of the scrolling list, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChangesRow {
    Group(ChangeGroup),
    Card(usize),
    Empty,
}

impl CodeColumn {
    /// The list's rows: each group's header, then its cards when open.
    pub(crate) fn changes_rows(&self) -> Vec<ChangesRow> {
        let mut rows = Vec::new();
        for (group, open) in [
            (ChangeGroup::Staged, self.changes.staged_open),
            (ChangeGroup::Unstaged, self.changes.unstaged_open),
        ] {
            let cards: Vec<usize> = self
                .changes
                .cards
                .iter()
                .enumerate()
                .filter(|(_, card)| card.group == group)
                .map(|(at, _)| at)
                .collect();
            if cards.is_empty() {
                continue;
            }
            rows.push(ChangesRow::Group(group));
            if open {
                rows.extend(cards.into_iter().map(ChangesRow::Card));
            }
        }
        if rows.is_empty() {
            rows.push(ChangesRow::Empty);
        }
        rows
    }
}

fn dark(cx: &Context<CodeColumn>) -> bool {
    cx.theme().appearance == Appearance::Dark
}

/// `text-emerald-600 dark:text-emerald-400`
fn added_color(cx: &Context<CodeColumn>) -> Hsla {
    if dark(cx) {
        tw::EMERALD_400.hsla()
    } else {
        tw::EMERALD_600.hsla()
    }
}

/// `STATUS_BADGE_CLASS`
fn status_color(status: &str, cx: &Context<CodeColumn>) -> Hsla {
    let (light, dark_color) = match status {
        "M" => (tw::YELLOW_600, tw::YELLOW_500),
        "A" => (tw::GREEN_600, tw::GREEN_500),
        "D" => (tw::RED_600, tw::RED_500),
        "U" => (tw::ORANGE_600, tw::ORANGE_500),
        "R" | "C" => (tw::VIOLET_600, tw::VIOLET_500),
        "?" => (tw::BLUE_600, tw::BLUE_500),
        _ => return cx.theme().colors.muted_foreground,
    };
    if dark(cx) { dark_color } else { light }.hsla()
}

/// `--dcc-shadow-card`
fn card_shadow() -> Vec<BoxShadow> {
    let shadow = |y: f32, blur: f32, alpha: f32| BoxShadow {
        color: Hsla::black().opacity(alpha),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    };
    vec![shadow(2.0, 3.0, 0.025), shadow(8.0, 24.0, 0.04)]
}

fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(at) if at > 0 => &path[..at],
        _ => "",
    }
}

/// `+N −N` in 10px tabular numbers.
fn stats(insertions: u32, deletions: u32, always: bool, cx: &Context<CodeColumn>) -> gpui::Div {
    let destructive = cx.theme().colors.destructive;
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0))
        .text_size(px(10.0))
        .font_features(tabular_nums())
        .when(always || insertions > 0, |row| {
            row.child(
                div()
                    .text_color(added_color(cx))
                    .child(format!("+{insertions}")),
            )
        })
        .when(always || deletions > 0, |row| {
            row.child(div().text_color(destructive).child(format!("−{deletions}")))
        })
}

fn header(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> impl IntoElement {
    let colors = cx.theme().colors.clone();
    let summary = summarize(column.changes.cards.iter().map(|card| &card.entry));
    let all_open = column.changes.cards_open();
    let toggle_label = cx.t(if all_open {
        "inspector.changes.collapseAll"
    } else {
        "inspector.changes.expandAll"
    });
    div()
        .flex_shrink_0()
        .border_b_1()
        .border_color(colors.border.opacity(0.5))
        .bg(colors.background)
        .child(
            div()
                .flex()
                .min_h(px(40.0))
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .py(px(6.0))
                .child(
                    div()
                        .flex()
                        .size(px(27.0))
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .rounded(px(9.0))
                        .bg(colors.accent_calm.opacity(0.12))
                        .text_color(colors.accent_calm)
                        .child(Icon::new("git-compare-arrows").size(14.0)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(cx.t("review.changes")),
                )
                .child(stats(summary.insertions, summary.deletions, true, cx))
                .when(summary.files > 1, |row| {
                    row.child(
                        Tooltip::new(
                            "changes-toggle-all-tip",
                            Button::new("changes-toggle-all")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::IconSm)
                                .muted()
                                .on_click(
                                    cx.listener(|column, _, _, cx| column.toggle_all_cards(cx)),
                                )
                                .child(Icon::new(if all_open {
                                    "chevrons-down-up"
                                } else {
                                    "chevrons-up-down"
                                })),
                            move |_, _| div().child(toggle_label.clone()).into_any_element(),
                        )
                        .side(Side::Bottom),
                    )
                }),
        )
}

fn group_header(
    column: &CodeColumn,
    group: ChangeGroup,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let (open, key) = match group {
        ChangeGroup::Staged => (
            column.changes.staged_open,
            "inspector.changes.groups.staged",
        ),
        ChangeGroup::Unstaged => (
            column.changes.unstaged_open,
            "inspector.changes.groups.unstaged",
        ),
    };
    let count = column
        .changes
        .cards
        .iter()
        .filter(|card| card.group == group)
        .count();
    let label = cx.t(key).to_uppercase();
    let has_staged = column
        .changes
        .cards
        .iter()
        .any(|card| card.group == ChangeGroup::Staged);
    let foreground = colors.foreground;
    let id = match group {
        ChangeGroup::Staged => "changes-group-staged",
        ChangeGroup::Unstaged => "changes-group-unstaged",
    };
    div()
        // The groups' `border-b … last:border-b-0`, drawn above the second.
        .when(group == ChangeGroup::Unstaged && has_staged, |row| {
            row.border_t_1().border_color(colors.border.opacity(0.35))
        })
        .flex()
        .min_h(px(36.0))
        .w_full()
        .items_center()
        .gap(px(4.0))
        .px(px(8.0))
        .text_color(colors.muted_foreground)
        .font_weight(FontWeight::SEMIBOLD)
        .child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(6.0))
                .cursor_pointer()
                .when(open, |button| button.text_color(foreground))
                .hover(move |style| style.text_color(foreground))
                .on_click(cx.listener(move |column, _, _, cx| column.toggle_group(group, cx)))
                .child(
                    Icon::new("chevron-right")
                        .size(12.0)
                        .rotate(if open { 90.0 } else { 0.0 }),
                )
                .when(group == ChangeGroup::Unstaged, |button| {
                    button.child(
                        Icon::new("laptop")
                            .size(12.0)
                            .color(colors.muted_foreground),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .child(Tracked::new(label, 10.5, 0.055)),
                ),
        )
        .child(
            div()
                .flex()
                .h(px(20.0))
                .min_w(px(20.0))
                .items_center()
                .justify_center()
                .rounded_full()
                .px(px(6.0))
                .bg(colors.secondary)
                .text_color(colors.secondary_foreground)
                .text_size(px(9.5))
                .child(count.to_string()),
        )
        .into_any_element()
}

fn card_body(card: &ChangeCard, at: usize, cx: &mut Context<CodeColumn>) -> AnyElement {
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

fn card(column: &CodeColumn, at: usize, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let card = &column.changes.cards[at];
    let entry = &card.entry;
    let folder = dirname(&entry.path).to_string();
    let open = card.open;
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
            stats(entry.insertions, entry.deletions, false, cx).child(
                div()
                    .min_w(px(16.0))
                    .text_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(status_color(&entry.status, cx))
                    .child(entry.status.clone()),
            ),
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
                .when(open, |article| article.child(card_body(card, at, cx))),
        )
        .into_any_element()
}

fn empty(cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .flex()
        .flex_col()
        .min_h(px(144.0))
        .items_center()
        .justify_center()
        .px(px(20.0))
        .py(px(32.0))
        .text_center()
        .text_size(px(11.0))
        .line_height(px(20.0))
        .text_color(colors.muted_foreground)
        .child(
            div()
                .flex()
                .mb(px(8.0))
                .size(px(32.0))
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(colors.muted.opacity(0.6))
                .child(Icon::new("git-compare-arrows").size(16.0).stroke(1.7)),
        )
        .child(cx.t("inspector.changes.emptyWorking"))
        .into_any_element()
}

/// The tab's content: a message until the status is read, then the review.
pub(crate) fn changes(
    column: &mut CodeColumn,
    window: &mut Window,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    column.changes.follow_theme(dark(cx));
    let message = |text: SharedString, color: Hsla| {
        div()
            .px(px(8.0))
            .py(px(8.0))
            .text_size(px(11.0))
            .line_height(px(20.0))
            .text_color(color)
            .child(text)
            .into_any_element()
    };
    let content = if column.root.is_none() {
        message(
            "No workspace path — cannot read git changes.".into(),
            colors.muted_foreground,
        )
    } else if let Some(error) = column
        .status_error
        .clone()
        .filter(|_| column.status.is_none())
    {
        message(error.into(), colors.destructive)
    } else if column.status.is_none() {
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(24.0))
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .child(spinner("changes-loading", 14.0))
            .child(cx.t("inspector.changes.loadingGit"))
            .into_any_element()
    } else {
        column.load_diffs_in_view(window, cx);
        let rows = column.changes_rows();
        let children: Vec<AnyElement> = rows
            .iter()
            .map(|row| match *row {
                ChangesRow::Group(group) => group_header(column, group, cx),
                ChangesRow::Card(at) => card(column, at, cx),
                ChangesRow::Empty => empty(cx),
            })
            .collect();
        div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .flex_col()
            .child(header(column, cx))
            .child(
                div()
                    .id("changes-scroll")
                    .debug_selector(|| "changes-scroll".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&column.changes.scroll)
                    .bg(colors.muted.opacity(0.08))
                    .text_size(px(11.5))
                    .py(px(4.0))
                    .children(children),
            )
            .into_any_element()
    };
    // The git section around the review (`border-b border-border/60`); its
    // delivery header above arrives with screen 5d.
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .flex_col()
        .overflow_hidden()
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .child(content)
        .into_any_element()
}
