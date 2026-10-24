//! `CodeFilesTree`: 24px rows, folders with a chevron and their Material
//! icon, files with the git letter in its color, the ignored ones dimmed;
//! the arrow keys move through it, a file drags into the composer as a
//! mention, and its context menu mentions it or copies its path.

use gpui::{
    AnyElement, ClipboardItem, Context, Hsla, KeyDownEvent, Render, SharedString, Window, div, img,
    list, prelude::*, px,
};

use super::{CodeColumn, CodeColumnEvent, Loaded};
use crate::data::code_tree::{CodeTreeRow, RowKind, Tone};
use crate::i18n::Translate;
use crate::project::file_icon::{file_icon, folder_icon};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Icon, Menu, MenuEntry, MenuItem, Toaster, Tooltip, spinner};

/// A file dragged out of the tree: the composer takes it as `@path`.
#[derive(Clone, Debug)]
pub struct DraggedFile(pub String);

impl Render for DraggedFile {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = &cx.theme().colors;
        div()
            .rounded(px(6.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.popover)
            .px(px(8.0))
            .py(px(2.0))
            .text_size(px(12.0))
            .child(format!("@{}", self.0))
    }
}

fn tone_color(tone: Tone) -> Hsla {
    match tone {
        Tone::Modified => tw::AMBER_500.hsla(),
        Tone::Added => tw::EMERALD_500.hsla(),
        Tone::Deleted | Tone::Conflict => tw::RED_500.hsla(),
        Tone::Renamed => tw::SKY_500.hsla(),
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn row(column: &CodeColumn, index: usize, cx: &mut Context<CodeColumn>) -> AnyElement {
    let Some(row) = column.rows.get(index).cloned() else {
        return div().into_any_element();
    };
    let colors = cx.theme().colors.clone();
    let folder = row.is_folder();
    let selected = !folder && column.selected.as_deref() == Some(row.path.as_str());
    let focused = column.focused == Some(index);
    let ignored = !folder
        && matches!(&column.files, Loaded::Ready(files) if files.ignored.contains(&row.path));
    let status = (!folder)
        .then(|| column.by_path.get(&row.path).copied())
        .flatten();
    let folder_changed = folder && column.changed_folders.contains(&row.path);
    let expanded = matches!(row.kind, RowKind::Folder { expanded: true });
    let hover = colors.accent.opacity(0.6);
    let indent = row.depth as f32 * 12.0 + if folder { 6.0 } else { 20.0 };
    let path = row.path.clone();
    let title: SharedString = if ignored {
        format!("{} · {}", row.path, cx.t("codeColumn.ignoredFile")).into()
    } else {
        row.path.clone().into()
    };
    let icon = if folder {
        folder_icon(basename(&row.path), expanded)
    } else {
        file_icon(&row.label)
    };
    let button = div()
        .id(("code-tree-row", index))
        .flex()
        .h(px(24.0))
        .w_full()
        .items_center()
        .gap(px(6.0))
        .rounded(px(2.0))
        .pr(px(8.0))
        .pl(px(indent))
        .cursor_pointer()
        .map(|button| {
            if selected {
                button
                    .bg(colors.muted.opacity(0.7))
                    .text_color(colors.foreground)
            } else if focused {
                button.bg(hover).text_color(colors.foreground)
            } else {
                button
                    .text_color(colors.muted_foreground)
                    .hover(move |style| style.bg(hover).text_color(colors.foreground))
            }
        })
        .when(ignored, |button| button.italic().opacity(0.55))
        .on_click(cx.listener(move |column, _, window, cx| {
            column.focused = Some(index);
            window.focus(&column.focus, cx);
            if folder {
                column.toggle_folder(&path, cx);
            } else {
                column.open_file(&path, cx);
            }
        }))
        .when(folder, |button| {
            button.child(
                Icon::new(if expanded {
                    "chevron-down"
                } else {
                    "chevron-right"
                })
                .size(12.0)
                .stroke(2.0),
            )
        })
        .child(img(icon).size(px(14.0)).flex_shrink_0())
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(12.0))
                .line_height(px(24.0))
                .when_some(status, |label, status| {
                    label.text_color(tone_color(status.tone))
                })
                .child(SharedString::from(row.label.clone())),
        )
        .child(
            div()
                .ml_auto()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.0))
                .children(status.map(|status| {
                    div()
                        .w(px(10.0))
                        .text_center()
                        .font_family(cx.theme().fonts.mono.clone())
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(tone_color(status.tone))
                        .child(status.letter.to_string())
                }))
                .when(folder_changed, |marks| {
                    marks.child(
                        div()
                            .size(px(6.0))
                            .rounded_full()
                            .bg(tw::AMBER_500.alpha(0.8)),
                    )
                }),
        );
    // The path on hover, as the desktop's `title`.
    let tip = |button: gpui::Stateful<gpui::Div>| {
        Tooltip::text(("code-tree-tip", index), button, title.clone())
            .delay(std::time::Duration::from_millis(700))
    };
    if folder {
        return tip(button).into_any_element();
    }
    let dragged = row.path.clone();
    let button = tip(button.on_drag(DraggedFile(dragged), |file, _, _, cx| {
        cx.new(|_| file.clone())
    }));
    let (mention, copy) = (row.path.clone(), row.path.clone());
    let entity = cx.entity().downgrade();
    let mention_label = cx.t("codeColumn.mentionInChat");
    let copy_label = cx.t("codeColumn.copyPath");
    Menu::context(("code-tree-menu", index), button, move |_, _| {
        let (entity, mention, copy) = (entity.clone(), mention.clone(), copy.clone());
        vec![
            MenuEntry::from(
                MenuItem::new(mention_label.clone(), move |_, cx| {
                    let _ = entity.update(cx, |_, cx| {
                        cx.emit(CodeColumnEvent::Mention(mention.clone()))
                    });
                })
                .icon(Icon::new("at-sign").size(14.0).stroke(1.8)),
            ),
            MenuEntry::from(
                MenuItem::new(copy_label.clone(), move |_, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                    Toaster::success(cx.t("codeColumn.pathCopied"), cx);
                })
                .icon(Icon::new("copy").size(14.0).stroke(1.8)),
            ),
        ]
    })
    .into_any_element()
}

fn note(text: SharedString, color: Hsla) -> AnyElement {
    div()
        .px(px(8.0))
        .py(px(12.0))
        .text_size(px(12.0))
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// Arrows, Home/End, →/← through folders, Enter opens.
fn key_down(column: &mut CodeColumn, event: &KeyDownEvent, cx: &mut Context<CodeColumn>) {
    let count = column.rows.len();
    if count == 0 {
        return;
    }
    let index = column.focused.unwrap_or(0).min(count - 1);
    let row: CodeTreeRow = column.rows[index].clone();
    let target = match event.keystroke.key.as_str() {
        "down" => Some((index + 1).min(count - 1)),
        "up" => Some(index.saturating_sub(1)),
        "home" => Some(0),
        "end" => Some(count - 1),
        "right" => match row.kind {
            RowKind::Folder { expanded: false } => {
                column.set_folder_open(&row.path, true, cx);
                None
            }
            RowKind::Folder { expanded: true } => Some((index + 1).min(count - 1)),
            RowKind::File => return,
        },
        "left" => match (&row.kind, &row.parent_path) {
            (RowKind::Folder { expanded: true }, _) => {
                column.set_folder_open(&row.path, false, cx);
                None
            }
            (_, Some(parent)) => column
                .rows
                .iter()
                .position(|candidate| candidate.path == *parent),
            _ => None,
        },
        "enter" => {
            if row.is_folder() {
                column.toggle_folder(&row.path, cx);
            } else {
                column.open_file(&row.path, cx);
            }
            None
        }
        _ => return,
    };
    cx.stop_propagation();
    if let Some(target) = target {
        column.focused = Some(target);
        column.list.scroll_to_reveal_item(target);
        cx.notify();
    }
}

pub(super) fn files_tree(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let body = if column.root.is_none() {
        note(
            cx.t("inspector.codeSection.unavailable"),
            colors.muted_foreground,
        )
    } else {
        match &column.files {
            Loaded::Pending => div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(8.0))
                .py(px(12.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(spinner("code-tree-loading", 14.0))
                .child(cx.t("inspector.codeSection.loading"))
                .into_any_element(),
            Loaded::Failed(error) => note(error.clone().into(), colors.destructive),
            Loaded::Ready(files) if files.paths.is_empty() => {
                note(cx.t("inspector.codeSection.empty"), colors.muted_foreground)
            }
            Loaded::Ready(_) => {
                let entity = cx.entity();
                list(column.list.clone(), move |index, _, cx| {
                    entity.update(cx, |column, cx| row(column, index, cx))
                })
                .size_full()
                .into_any_element()
            }
        }
    };
    div()
        .id("code-tree")
        .debug_selector(|| "code-tree".into())
        .track_focus(&column.focus)
        .on_key_down(cx.listener(|column, event: &KeyDownEvent, _, cx| key_down(column, event, cx)))
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .px(px(8.0))
        .py(px(8.0))
        .child(body)
        .into_any_element()
}
