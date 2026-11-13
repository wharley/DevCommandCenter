//! "Mudanças" drawn: the review header (mark, title, +/−, open or close
//! every card, list or tree), the scope switch with its hint (and the
//! branches on "Branch"), then the list: "Preparadas" / "Não preparadas"
//! with their cards or tree, or the branch's files under its base.

use gpui::{
    AnyElement, BoxShadow, ClickEvent, Context, ElementId, FontWeight, Hsla, SharedString, Window,
    div, list, point, prelude::*, px,
};

use super::super::{CodeColumn, Loaded};
use super::{ChangeGroup, ChangesRow, Scope, bar, card, summarize, tree};
use crate::i18n::Translate;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::tooltip::Side;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, ListScrollbar, Tooltip, Tracked, spinner, tabular_nums,
};

pub(super) fn dark(cx: &Context<CodeColumn>) -> bool {
    cx.theme().appearance == Appearance::Dark
}

/// `text-emerald-600 dark:text-emerald-400`
pub(super) fn added_color(cx: &Context<CodeColumn>) -> Hsla {
    if dark(cx) {
        tw::EMERALD_400.hsla()
    } else {
        tw::EMERALD_600.hsla()
    }
}

/// `STATUS_BADGE_CLASS`
pub(super) fn status_color(status: &str, cx: &Context<CodeColumn>) -> Hsla {
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
pub(crate) fn card_shadow() -> Vec<BoxShadow> {
    let shadow = |y: f32, blur: f32, alpha: f32| BoxShadow {
        color: Hsla::black().opacity(alpha),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    };
    vec![shadow(2.0, 3.0, 0.025), shadow(8.0, 24.0, 0.04)]
}

pub(super) fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(at) if at > 0 => &path[..at],
        _ => "",
    }
}

/// `+N −N` in 10px tabular numbers.
pub(super) fn stats(
    insertions: u32,
    deletions: u32,
    always: bool,
    cx: &Context<CodeColumn>,
) -> gpui::Div {
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

/// `RowIconButton`: a 16px ghost button with a 14px icon; `muted` is the
/// tree's (muted until hovered).
pub(super) fn row_button(
    id: (&'static str, usize),
    icon: &'static str,
    disabled: bool,
    muted: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    cx: &Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let hover = if muted {
        colors.accent.opacity(0.6)
    } else if dark(cx) {
        colors.accent.opacity(0.5)
    } else {
        colors.accent
    };
    let hover_text = if muted {
        colors.foreground
    } else {
        colors.accent_foreground
    };
    let (name, index) = id;
    div()
        .id(ElementId::from(id))
        .debug_selector(move || format!("{name}-{index}"))
        .flex()
        .size(px(16.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .when(muted, |button| button.text_color(colors.muted_foreground))
        .map(|button| {
            if disabled {
                button.opacity(0.6)
            } else {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover).text_color(hover_text))
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        on_click(event, window, cx)
                    })
            }
        })
        .child(Icon::new(icon).size(14.0))
        .into_any_element()
}

fn icon_button(
    id: &'static str,
    icon: &'static str,
    label: SharedString,
    on_click: impl Fn(&mut CodeColumn, &mut Context<CodeColumn>) + 'static,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    div()
        .debug_selector(move || id.into())
        .child(
            Tooltip::new(
                SharedString::from(format!("{id}-tip")),
                Button::new(id)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .muted()
                    .on_click(cx.listener(move |column, _, _, cx| on_click(column, cx)))
                    .child(Icon::new(icon)),
                move |_, _| div().child(label.clone()).into_any_element(),
            )
            .side(Side::Bottom),
        )
        .into_any_element()
}

/// The scopes as quiet tabs, each with its count; what a scope shows is
/// its tooltip.
fn scopes(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let active = column.review_scope();
    // `availableInspectorReviewScopes`: "Último turno" with a session.
    let last_turn = column
        .last_turn_context()
        .map(|_| (Scope::LastTurn, column.turn.count()));
    let counts = [(
        Scope::Working,
        Some(column.changes.entries(Scope::Working).count()),
    )]
    .into_iter()
    .chain(last_turn)
    .chain([(Scope::Branch, column.changes.branch_count())]);
    let buttons: Vec<AnyElement> = counts
        .into_iter()
        .map(|(scope, count)| {
            let pressed = scope == active;
            let hover = colors.muted.opacity(0.3);
            let tab = div()
                .id(SharedString::from(format!("changes-scope-{}", scope.key())))
                .debug_selector(move || format!("changes-scope-{}", scope.key()))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(8.0))
                .text_size(px(11.0))
                .cursor_pointer()
                .map(|button| {
                    if pressed {
                        button
                            .bg(colors.muted.opacity(0.55))
                            .text_color(colors.foreground)
                            .font_weight(FontWeight::MEDIUM)
                    } else {
                        button
                            .text_color(colors.muted_foreground)
                            .hover(move |style| style.bg(hover))
                    }
                })
                .on_click(cx.listener(move |column, _, _, cx| column.set_scope(scope, cx)))
                .child(
                    div()
                        .truncate()
                        .child(cx.t(&format!("inspector.changes.scopes.{}", scope.key()))),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::NORMAL)
                        .font_features(tabular_nums())
                        .text_color(colors.muted_foreground)
                        .child(count.map_or("—".to_string(), |count| count.to_string())),
                );
            Tooltip::text(
                SharedString::from(format!("changes-scope-tip-{}", scope.key())),
                tab,
                cx.t(&format!("review.scopes.{}", scope.key())),
            )
            .side(Side::Bottom)
            .into_any_element()
        })
        .collect();
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(px(2.0))
        .overflow_hidden()
        .children(buttons)
        .into_any_element()
}

/// `ReviewBranchContext`: the task's branch → its base, names in full.
fn branch_context(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let head = column
        .status
        .as_ref()
        .and_then(|status| status.current_branch.clone())
        .filter(|branch| !branch.is_empty())
        .unwrap_or_else(|| "—".to_string());
    let base = column.changes.base_branch();
    div()
        .px(px(8.0))
        .pb(px(10.0))
        .child(crate::ui::branch_context(head, base, cx))
        .into_any_element()
}

fn header(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let scope = column.review_scope();
    let last_turn = scope == Scope::LastTurn;
    let summary = match column.turn.summary().filter(|_| last_turn) {
        Some(review) => super::Summary {
            files: review.files.len(),
            insertions: review.insertions,
            deletions: review.deletions,
        },
        None if last_turn => super::Summary::default(),
        None => summarize(column.changes.entries(scope)),
    };
    let counted = match scope {
        Scope::Working => true,
        Scope::LastTurn => column.turn.count().is_some(),
        Scope::Branch => column.changes.branch_count().is_some(),
    };
    let all_open = if last_turn {
        column.turn.cards_open()
    } else {
        column.changes.cards_open(scope)
    };
    let layout = column.changes.layout;
    let toggle_cards = (summary.files > 1 && (last_turn || !layout.rows())).then(|| {
        icon_button(
            "changes-toggle-all",
            if all_open {
                "chevrons-down-up"
            } else {
                "chevrons-up-down"
            },
            cx.t(if all_open {
                "inspector.changes.collapseAll"
            } else {
                "inspector.changes.expandAll"
            }),
            |column, cx| column.toggle_all_cards(cx),
            cx,
        )
    });
    let toggle_layout = (!last_turn).then(|| {
        let (icon, tooltip) = layout.button();
        icon_button(
            "changes-toggle-tree",
            icon,
            cx.t(tooltip),
            |column, cx| column.cycle_changes_layout(cx),
            cx,
        )
    });
    // One strip: the scopes, the scope's totals, then how it is laid out.
    div()
        .flex_shrink_0()
        .border_b_1()
        .border_color(colors.border.opacity(0.5))
        .bg(colors.background)
        .child(
            div()
                .flex()
                .min_h(px(36.0))
                .items_center()
                .gap(px(4.0))
                .px(px(6.0))
                .child(scopes(column, cx))
                .when(counted, |row| {
                    row.child(stats(summary.insertions, summary.deletions, true, cx))
                })
                .children(toggle_cards)
                .children(toggle_layout),
        )
        .when(scope == Scope::Branch, |header| {
            header.child(branch_context(column, cx))
        })
        .into_any_element()
}

fn group_header(
    column: &CodeColumn,
    group: ChangeGroup,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let (open, key, id) = match group {
        ChangeGroup::Staged => (
            column.changes.staged_open,
            "inspector.changes.groups.staged",
            "changes-group-staged",
        ),
        _ => (
            column.changes.unstaged_open,
            "inspector.changes.groups.unstaged",
            "changes-group-unstaged",
        ),
    };
    let count = column
        .changes
        .cards
        .iter()
        .filter(|card| card.group == group)
        .count();
    let label = cx.t(key).to_uppercase();
    let foreground = colors.foreground;
    let muted = colors.muted_foreground;
    let conflicts = column
        .status
        .as_ref()
        .is_some_and(|status| status.conflict_count > 0);
    let disabled = column.changes.busy || (group == ChangeGroup::Unstaged && conflicts);
    let header_group = SharedString::from(format!("{id}-header"));
    // The batch button shows only while the header is hovered (it is there,
    // transparent, all along, as on the desktop).
    let batch = div()
        .id(SharedString::from(format!("{id}-batch")))
        .debug_selector(move || format!("{id}-batch"))
        .flex()
        .size(px(16.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .text_color(gpui::transparent_black())
        .group_hover(header_group.clone(), move |style| style.text_color(muted))
        .map(|button| {
            if disabled {
                button.opacity(0.6)
            } else {
                button
                    .cursor_pointer()
                    .hover(move |style| style.text_color(foreground))
                    .on_click(cx.listener(move |column, _, _, cx| match group {
                        ChangeGroup::Staged => column.unstage_all(cx),
                        _ => column.stage_all(cx),
                    }))
            }
        })
        .child(
            Icon::new(if group == ChangeGroup::Staged {
                "minus"
            } else {
                "plus"
            })
            .size(14.0),
        );
    div()
        .group(header_group)
        // The groups' `border-b … last:border-b-0`, drawn above the second.
        .when(
            group == ChangeGroup::Unstaged && column.changes.has(ChangeGroup::Staged),
            |row| row.border_t_1().border_color(colors.border.opacity(0.35)),
        )
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
                        .child(Tracked::new(label, 10.0, 0.055)),
                ),
        )
        .child(batch)
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
                .text_size(px(10.0))
                .child(count.to_string()),
        )
        .into_any_element()
}

/// The branch's base over its files (`origin/` dropped), turning while the
/// first read runs.
fn branch_header(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let loading = matches!(column.changes.branch, Loaded::Pending);
    let base = column
        .changes
        .base_branch()
        .map(|base| base.replace("origin/", ""))
        .unwrap_or_else(|| cx.t("commit.remoteBase").to_string());
    div()
        .flex()
        .min_h(px(32.0))
        .items_center()
        .gap(px(8.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.35))
        .px(px(12.0))
        .text_size(px(11.0))
        .text_color(colors.muted_foreground)
        .child(Icon::new("cloud").size(14.0).stroke(1.8))
        .child(div().flex_1().min_w_0().truncate().child(base))
        .when(loading, |row| {
            row.child(spinner("changes-branch-loading", 12.0))
        })
        .into_any_element()
}

fn empty(icon: &'static str, key: &str, cx: &mut Context<CodeColumn>) -> AnyElement {
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
                .child(Icon::new(icon).size(16.0).stroke(1.7)),
        )
        .child(cx.t(key))
        .into_any_element()
}

/// One row of the list, drawn as it comes near the viewport: a card drawn
/// open starts reading its diff.
fn row(column: &mut CodeColumn, index: usize, cx: &mut Context<CodeColumn>) -> AnyElement {
    let Some(row) = column.changes.shown_row(index) else {
        return div().into_any_element();
    };
    match row {
        ChangesRow::Group(group) => group_header(column, group, cx),
        ChangesRow::Card(at) => {
            if column.changes.waits(at) {
                column.load_diff(at, cx);
            }
            card::card(column, at, cx)
        }
        ChangesRow::Folder {
            group,
            path,
            name,
            depth,
            open,
        } => tree::folder_row(group, &path, &name, depth, open, cx),
        ChangesRow::File { card, depth } => tree::file_row(column, card, depth, cx),
        ChangesRow::BranchHeader => branch_header(column, cx),
        ChangesRow::Space(height) => div().h(px(height as f32)).into_any_element(),
        ChangesRow::Empty => empty("git-compare-arrows", "inspector.changes.emptyWorking", cx),
        ChangesRow::EmptyBranch => empty("cloud", "inspector.changes.emptyBranch", cx),
    }
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
        message(cx.t("commit.noWorkspace"), colors.muted_foreground)
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
    } else if column.review_scope() == Scope::LastTurn {
        div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .flex_col()
            .child(header(column, cx))
            .child(crate::screens::code_column::turn::surface(column, cx))
            .into_any_element()
    } else {
        column.changes.sync_list();
        column.load_eager_diffs(cx);
        let working = column.review_scope() == Scope::Working;
        let has_any = column.changes.entries(Scope::Working).next().is_some();
        let weak = cx.entity().downgrade();
        column.changes.list.set_scroll_handler(move |_, _, cx| {
            let _ = weak.update(cx, |column: &mut CodeColumn, cx| {
                column.changes_scrolled(cx)
            });
        });
        let entity = cx.entity();
        let rows = list(column.changes.list.clone(), move |index, _, cx| {
            entity.update(cx, |column, cx| row(column, index, cx))
        })
        .size_full()
        .pt(px(4.0))
        // Room for the floating "Reverter tudo / Preparar tudo".
        .pb(px(if working && has_any { 64.0 } else { 4.0 }));
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
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .bg(colors.muted.opacity(0.08))
                    .text_size(px(12.0))
                    .child(rows)
                    .child(ListScrollbar::new(
                        "changes-scrollbar",
                        column.changes.list.clone(),
                        column.changes.scrolled_at,
                    )),
            )
            .when(working && has_any, |content| {
                content.child(bar::pill(column, cx))
            })
            .when(column.changes.confirm_revert, |content| {
                content.child(bar::revert_dialog(column, cx))
            })
            .into_any_element()
    };
    // "Atualizando arquivos alterados…" while a step or a sync runs.
    let updating = (column.delivery.busy || column.delivery.syncing).then(|| updating(cx));
    let header = crate::screens::code_column::delivery::header(column, cx);
    let toolbar = crate::screens::code_column::delivery::toolbar(column, cx);
    // The delivery's own dialogs are the `DeliveryOverlay`'s.
    let mut dialogs: Vec<AnyElement> =
        crate::screens::code_column::review::recovery_confirm(column, cx)
            .into_iter()
            .collect();
    dialogs.extend(crate::screens::code_column::review::pipeline_retry_confirm(
        column, cx,
    ));
    dialogs.extend(crate::screens::code_column::turn::dialogs(
        column, window, cx,
    ));
    let delegations = crate::screens::code_column::delegation::delegation_rows(column, cx);
    // The pull request's sections (`grid gap-2 px-2 empty:hidden`).
    let sections: Vec<AnyElement> =
        crate::screens::code_column::review::failure_section(column, cx)
            .into_iter()
            .chain(crate::screens::code_column::review::review_section(
                column, cx,
            ))
            .chain(crate::screens::code_column::review::threads_section(
                column, cx,
            ))
            .chain(crate::screens::code_column::review::pipeline_section(
                column, cx,
            ))
            .collect();
    let recap = crate::screens::code_column::recap::recap_strip(column, cx);
    // The git section (`border-b border-border/60`): the delivery header,
    // the sync bar, then the review under its `border-t`.
    let section = div()
        .flex()
        .flex_1()
        .min_h_0()
        .flex_col()
        .overflow_hidden()
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .child(header)
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .flex_col()
                .gap(px(8.0))
                .overflow_hidden()
                .children(toolbar)
                .children(delegations)
                .when(!sections.is_empty(), |area| {
                    area.child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_shrink_0()
                            .gap(px(8.0))
                            .px(px(8.0))
                            .children(sections),
                    )
                })
                .child(
                    div()
                        .relative()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .flex_col()
                        .border_t_1()
                        .border_color(colors.border.opacity(0.35))
                        .child(content)
                        .children(updating),
                ),
        )
        .children(dialogs);
    // The recap strip sits over it, like under the inspector's mode switch.
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .flex_col()
        .children(recap)
        .child(section)
        .into_any_element()
}

fn updating(cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .absolute()
        .right(px(8.0))
        .top(px(8.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background.opacity(0.95))
        .px(px(8.0))
        .py(px(4.0))
        .text_size(px(10.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.muted_foreground)
        .shadow_sm()
        .child(
            div()
                .text_color(colors.primary)
                .child(spinner("changes-updating", 14.0)),
        )
        .child(cx.t("inspector.changes.updatingGit"))
        .into_any_element()
}
