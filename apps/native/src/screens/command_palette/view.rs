//! The palette as the desktop draws it: the band with "Paleta de comandos"
//! and its description, the field, then the rows. Items are `items-start
//! gap-3 py-2`: an action with "Abre a superfície selecionada do DCC" under
//! it and its keys on the right, a task with its branch (and "Atual"), a
//! conversation with its task, a file with its folder.

use gpui::{AnyElement, Context, FontWeight, KeyDownEvent, Render, Window, div, prelude::*, px};

use super::items::{ItemKind, Row};
use super::{CommandPalette, PaletteEvent};
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::Icon;
use crate::ui::command::{
    command_banner, command_dialog, command_empty, command_field, command_heading, command_item,
    command_list_height, command_separator, command_shortcut,
};

impl CommandPalette {
    fn item(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let item = &self.items[index];
        let line = |text: String| {
            div()
                .truncate()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let icon = |name: &'static str| {
            div().mt(px(2.0)).flex_none().child(
                Icon::new(name)
                    .size(16.0)
                    .stroke(1.8)
                    .color(colors.muted_foreground),
            )
        };
        let title = div()
            .truncate()
            .font_weight(FontWeight::MEDIUM)
            .child(item.label.clone());
        let (leading, body, trailing): (Option<AnyElement>, AnyElement, Option<AnyElement>) =
            match &item.kind {
                ItemKind::Action { shortcut } => (
                    None,
                    div()
                        .child(title)
                        .child(line(cx.t("commandPalette.actionDestination").to_string()))
                        .into_any_element(),
                    shortcut
                        .clone()
                        .map(|keys| command_shortcut(keys, cx).into_any_element()),
                ),
                ItemKind::Task { branch, selected } => (
                    Some(icon("folder-git-2").into_any_element()),
                    div()
                        .child(title)
                        .child(line(branch.clone()))
                        .into_any_element(),
                    selected.then(|| {
                        command_shortcut(cx.t("commandPalette.selected"), cx).into_any_element()
                    }),
                ),
                ItemKind::Session {
                    workspace,
                    snippet,
                    archived,
                } => (
                    Some(icon("history").into_any_element()),
                    div()
                        .child(title)
                        .child(line(workspace.clone()))
                        .children(snippet.clone().map(|snippet| {
                            div()
                                .mt(px(4.0))
                                .max_h(px(39.0))
                                .overflow_hidden()
                                .text_size(px(12.0))
                                .line_height(px(19.5))
                                .text_color(colors.foreground.opacity(0.8))
                                .child(snippet)
                        }))
                        .when(*archived, |body| {
                            body.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("workbench.sessionSearch.archived")),
                            )
                        })
                        .into_any_element(),
                    None,
                ),
                ItemKind::File { dir } => {
                    let dir = if dir.is_empty() {
                        cx.t("commandPalette.workspaceRoot").to_string()
                    } else {
                        dir.clone()
                    };
                    (
                        Some(icon("file-code-corner").into_any_element()),
                        div()
                            .child(title)
                            .child(
                                div()
                                    .truncate()
                                    .font_family(fonts::MONO)
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(dir),
                            )
                            .into_any_element(),
                        None,
                    )
                }
            };
        command_item(("palette-item", index), index == self.highlighted, cx)
            .debug_selector(move || format!("palette-item-{index}"))
            .items_start()
            .gap(px(12.0))
            .py(px(8.0))
            .on_mouse_move(cx.listener(move |palette, _, _, cx| palette.hover(index, cx)))
            .on_click(cx.listener(move |palette, _, _, cx| palette.run(index, cx)))
            .children(leading)
            .child(div().flex_1().min_w_0().child(body))
            .children(trailing)
            .into_any_element()
    }

    fn row(&self, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        match row {
            Row::Empty => command_empty(cx.t("commandPalette.empty")).into_any_element(),
            Row::Heading(heading) => div()
                .px(px(4.0))
                .pt(px(4.0))
                .child(command_heading(heading.clone(), cx))
                .into_any_element(),
            Row::Item(index) => div()
                .px(px(4.0))
                .child(self.item(*index, cx))
                .into_any_element(),
            Row::GroupEnd => div().h(px(4.0)).into_any_element(),
            Row::Separator => command_separator(cx).into_any_element(),
            Row::LoadingSessions => div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(16.0))
                .py(px(8.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(Icon::new("search").size(14.0).stroke(1.8))
                .child(cx.t("commandPalette.loadingSessions"))
                .into_any_element(),
        }
    }
}

impl Render for CommandPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<AnyElement> = self
            .rows
            .clone()
            .iter()
            .map(|row| self.row(row, cx))
            .collect();
        let close = cx.entity().downgrade();
        let focus = gpui::Focusable::focus_handle(self.input.read(cx), cx);
        command_dialog(
            "command-palette-dialog",
            focus,
            move |_, cx| {
                let _ = close.update(cx, |_, cx| cx.emit(PaletteEvent::Closed));
            },
            window,
            cx,
        )
        .child(command_banner(
            cx.t("commandPalette.title"),
            cx.t("commandPalette.description"),
            cx,
        ))
        .child(
            command_field("command-palette-field", self.input.clone(), cx).on_key_down(
                cx.listener(|palette, event: &KeyDownEvent, _, cx| {
                    match event.keystroke.key.as_str() {
                        "down" => palette.move_highlight(true, cx),
                        "up" => palette.move_highlight(false, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }),
            ),
        )
        .child(
            div()
                .id("command-palette-list")
                .max_h(px(command_list_height(window)))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .children(rows),
        )
    }
}
