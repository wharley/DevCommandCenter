//! The changes as a tree (`ChangesTreeView`): folders first, open by
//! default, 12px a level; each file a row with its name, folder and
//! counts, swapped on hover for its git actions.

use std::collections::BTreeMap;

use gpui::{
    Animation, AnimationExt, AnyElement, Context, FontWeight, div, img, prelude::*,
    pulsating_between, px,
};

use super::super::CodeColumn;
use super::view::{added_color, row_button, status_color};
use super::{ChangeGroup, Changes, ChangesRow};
use crate::data::code_tree::locale_compare;
use crate::data::desktop_api::GitAction;
use crate::project::file_icon::{file_icon, folder_icon};
use crate::theme::ActiveTheme;
use crate::ui::{Icon, tabular_nums};

#[derive(Default)]
struct Node {
    path: String,
    children: BTreeMap<String, Node>,
    /// The card of a file.
    card: Option<usize>,
}

impl Node {
    fn is_folder(&self) -> bool {
        !self.children.is_empty() && self.card.is_none()
    }
}

/// `buildTree` over one group's files.
fn build(changes: &Changes, group: ChangeGroup) -> Node {
    let mut root = Node::default();
    for (at, card) in changes.cards.iter().enumerate() {
        if card.group != group {
            continue;
        }
        let parts: Vec<&str> = card.entry.path.split('/').collect();
        let mut current = &mut root;
        for (depth, part) in parts.iter().enumerate() {
            let path = parts[..=depth].join("/");
            current = current
                .children
                .entry(part.to_string())
                .or_insert_with(|| Node {
                    path,
                    ..Node::default()
                });
        }
        current.card = Some(at);
    }
    root
}

fn push(
    changes: &Changes,
    group: ChangeGroup,
    node: &Node,
    depth: usize,
    rows: &mut Vec<ChangesRow>,
) {
    let mut children: Vec<(&String, &Node)> = node.children.iter().collect();
    children.sort_by(|(left_name, left), (right_name, right)| {
        right
            .is_folder()
            .cmp(&left.is_folder())
            .then_with(|| locale_compare(left_name, right_name))
    });
    for (name, child) in children {
        if child.is_folder() {
            let open = !changes.closed.contains(&(group, child.path.clone()));
            rows.push(ChangesRow::Folder {
                group,
                path: child.path.clone(),
                name: name.clone(),
                depth,
                open,
            });
            if open {
                push(changes, group, child, depth + 1, rows);
            }
        } else if let Some(card) = child.card {
            rows.push(ChangesRow::File { card, depth });
        }
    }
}

pub(super) fn rows(changes: &Changes, group: ChangeGroup, rows: &mut Vec<ChangesRow>) {
    push(changes, group, &build(changes, group), 0, rows);
}

/// The tree's `pl-1`.
const TREE_INSET: f32 = 4.0;

pub(super) fn folder_row(
    group: ChangeGroup,
    path: &str,
    name: &str,
    depth: usize,
    open: bool,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let hover = colors.accent.opacity(0.6);
    let key = path.to_string();
    div()
        .id(gpui::ElementId::Name(
            format!("changes-folder-{group:?}-{path}").into(),
        ))
        .flex()
        .items_center()
        .gap(px(4.0))
        .ml(px(TREE_INSET))
        .py(px(1.5))
        .pr(px(8.0))
        .pl(px(depth as f32 * 12.0 + 8.0))
        .text_color(colors.muted_foreground)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(
            cx.listener(move |column, _, _, cx| column.toggle_changes_folder(group, &key, cx)),
        )
        .child(
            Icon::new("chevron-right")
                .size(12.0)
                .rotate(if open { 90.0 } else { 0.0 }),
        )
        .child(img(folder_icon(name, open)).size(px(14.0)).flex_shrink_0())
        .child(div().truncate().child(name.to_string()))
        .into_any_element()
}

pub(super) fn file_row(
    column: &CodeColumn,
    at: usize,
    depth: usize,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let card = &column.changes.cards[at];
    let entry = &card.entry;
    let hovered = column.changes.hovered_file == Some(at);
    let flashing = card.group == ChangeGroup::Committed && column.changes.is_flashing(&entry.path);
    let folder = super::view::dirname(&entry.path).to_string();
    let hover_bg = colors.accent.opacity(0.55);
    let actions = hovered.then(|| actions(column, at, cx)).flatten();
    div()
        .id(("changes-file", at))
        .debug_selector(move || format!("changes-file-{at}"))
        .relative()
        .ml(px(TREE_INSET + 4.0))
        .mr(px(4.0))
        .flex()
        .min_h(px(32.0))
        .items_center()
        .gap(px(8.0))
        .rounded(px(6.0))
        .py(px(4.0))
        .pr(px(8.0))
        .pl(px(depth as f32 * 12.0 + 22.0))
        .text_color(colors.muted_foreground)
        .hover(move |style| style.bg(hover_bg))
        .on_hover(cx.listener(move |column, hovered: &bool, _, cx| {
            let next = if *hovered {
                Some(at)
            } else if column.changes.hovered_file == Some(at) {
                None
            } else {
                column.changes.hovered_file
            };
            if next != column.changes.hovered_file {
                column.changes.hovered_file = next;
                cx.notify();
            }
        }))
        .child(img(file_icon(&entry.name)).size(px(14.0)).flex_shrink_0())
        .child(name(entry.name.clone(), flashing, at, cx))
        // On hover the folder and counts give way to the actions.
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_right()
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .when(hovered && actions.is_some(), |folder| folder.invisible())
                .child(folder),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(4.0))
                .text_size(px(10.0))
                .font_features(tabular_nums())
                .when(hovered && actions.is_some(), |stats| stats.invisible())
                .when(entry.insertions > 0, |stats| {
                    stats.child(
                        div()
                            .text_color(added_color(cx))
                            .child(format!("+{}", entry.insertions)),
                    )
                })
                .when(entry.deletions > 0, |stats| {
                    stats.child(
                        div()
                            .text_color(colors.destructive)
                            .child(format!("−{}", entry.deletions)),
                    )
                })
                .child(
                    div()
                        .flex()
                        .h(px(16.0))
                        .min_w(px(16.0))
                        .items_center()
                        .justify_center()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(status_color(&entry.status, cx))
                        .child(entry.status.clone()),
                ),
        )
        .children(actions)
        .into_any_element()
}

/// The name; a file new to the branch shimmers for three seconds
/// (`ShinyFlash`: three 1s sweeps, here a pulse of its color).
fn name(text: String, flashing: bool, at: usize, cx: &Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let label = div()
        .min_w_0()
        .max_w(gpui::relative(0.58))
        .truncate()
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.foreground)
        .child(text);
    if !flashing {
        return label.into_any_element();
    }
    let (from, to) = (colors.foreground, colors.muted_foreground.opacity(0.8));
    label
        .with_animation(
            ("changes-file-flash", at),
            Animation::new(std::time::Duration::from_secs(1))
                .repeat()
                .with_easing(pulsating_between(0.0, 1.0)),
            move |label, delta| {
                let mix = |a: f32, b: f32| a + (b - a) * delta;
                label.text_color(gpui::Hsla {
                    h: from.h,
                    s: mix(from.s, to.s),
                    l: mix(from.l, to.l),
                    a: mix(from.a, to.a),
                })
            },
        )
        .into_any_element()
}

/// `ROW_ACTIONS_CLASS`: a small toolbar over the row's right edge.
fn actions(column: &CodeColumn, at: usize, cx: &mut Context<CodeColumn>) -> Option<AnyElement> {
    let colors = cx.theme().colors.clone();
    let card = &column.changes.cards[at];
    let path = card.entry.path.clone();
    let busy = column.changes.busy;
    let buttons: Vec<AnyElement> = match card.group {
        ChangeGroup::Staged => vec![row_button(
            ("changes-file-unstage", at),
            "minus",
            busy,
            true,
            cx.listener(move |column, _, _, cx| column.git_file(GitAction::Unstage, &path, cx)),
            cx,
        )],
        ChangeGroup::Unstaged if !card.unmerged() => {
            let stage = path.clone();
            vec![
                row_button(
                    ("changes-file-discard", at),
                    "undo-2",
                    busy,
                    true,
                    cx.listener(move |column, _, _, cx| {
                        column.git_file(GitAction::Discard, &path, cx)
                    }),
                    cx,
                ),
                row_button(
                    ("changes-file-stage", at),
                    "plus",
                    busy,
                    true,
                    cx.listener(move |column, _, _, cx| {
                        column.git_file(GitAction::Stage, &stage, cx)
                    }),
                    cx,
                ),
            ]
        }
        _ => return None,
    };
    Some(
        div()
            .absolute()
            .right(px(4.0))
            .top_0()
            .bottom_0()
            .flex()
            .items_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .rounded(px(6.0))
                    .bg(colors.background.opacity(0.95))
                    .px(px(2.0))
                    .py(px(1.0))
                    .shadow_sm()
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .children(buttons),
            )
            .into_any_element(),
    )
}
