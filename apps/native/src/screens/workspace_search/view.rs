//! The list as `WorkspaceSearch` draws it: a mono heading per file (its
//! name, then " · path"), each line with its number on a 40px column and
//! the text with the first hit marked (`bg-primary/20 text-foreground`).

use std::ops::Range;

use gpui::{
    AnyElement, Context, HighlightStyle, KeyDownEvent, Render, SharedString, StyledText, Window,
    div, prelude::*, px,
};

use super::{WorkspaceSearch, WorkspaceSearchEvent};
use crate::data::quick_open::basename;
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::Icon;
use crate::ui::command::{
    command_dialog, command_empty, command_field, command_heading, command_item,
    command_list_height, command_note,
};

/// `highlight`: where the first case-insensitive `query` sits in `text`.
pub(super) fn first_hit(text: &str, query: &str) -> Option<Range<usize>> {
    if query.is_empty() {
        return None;
    }
    let wanted = query.to_lowercase();
    let length = query.chars().count();
    text.char_indices().find_map(|(start, _)| {
        let rest = &text[start..];
        rest.to_lowercase().starts_with(&wanted).then(|| {
            let end = rest
                .char_indices()
                .nth(length)
                .map_or(text.len(), |(offset, _)| start + offset);
            start..end
        })
    })
}

impl WorkspaceSearch {
    fn line_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let Some(found) = self.at(index) else {
            return div().into_any_element();
        };
        let text = found.text.trim().to_string();
        let highlights = first_hit(&text, &self.answered)
            .map(|range| {
                vec![(
                    range,
                    HighlightStyle {
                        color: Some(colors.foreground),
                        background_color: Some(colors.primary.opacity(0.2)),
                        ..Default::default()
                    },
                )]
            })
            .unwrap_or_default();
        command_item(
            ("workspace-search-item", index),
            index == self.highlighted,
            cx,
        )
        .debug_selector(move || format!("workspace-search-item-{index}"))
        .items_baseline()
        .gap(px(10.0))
        .on_mouse_move(cx.listener(move |search, _, _, cx| search.hover(index, cx)))
        .on_click(cx.listener(move |search, _, _, cx| search.pick(index, cx)))
        .child(
            div()
                .flex_none()
                .w(px(40.0))
                .text_right()
                .font_family(fonts::MONO)
                .font_features(crate::ui::tabular_nums())
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(found.line.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(fonts::MONO)
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(StyledText::new(SharedString::from(text)).with_highlights(highlights)),
        )
        .into_any_element()
    }

    /// The list's rows, flat so ↑/↓ can scroll one into view: per file a
    /// heading, then its lines (each group keeps its `p-1`).
    fn rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let mut rows = Vec::new();
        let mut index = 0;
        for group in &self.groups {
            let heading = div()
                .font_family(fonts::MONO)
                .text_size(px(11.0))
                .flex()
                .child(
                    div()
                        .text_color(colors.foreground.opacity(0.8))
                        .child(basename(&group.path).to_string()),
                )
                .child(
                    div()
                        .text_color(colors.muted_foreground)
                        .child(format!(" · {}", group.path)),
                );
            rows.push(
                div()
                    .px(px(4.0))
                    .pt(px(4.0))
                    .child(command_heading(heading, cx))
                    .into_any_element(),
            );
            let last = index + group.matches.len() - 1;
            for _ in &group.matches {
                rows.push(
                    div()
                        .px(px(4.0))
                        .when(index == last, |row| row.pb(px(4.0)))
                        .child(self.line_row(index, cx))
                        .into_any_element(),
                );
                index += 1;
            }
        }
        rows
    }
}

impl Render for WorkspaceSearch {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let has_query = self.has_query(cx);
        let status = if has_query && self.searching && self.groups.is_empty() {
            Some(
                command_note(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(Icon::new("search").size(16.0).stroke(1.8))
                        .child(cx.t("workspaceSearch.searching")),
                    cx,
                )
                .into_any_element(),
            )
        } else if !has_query {
            Some(command_note(cx.t("workspaceSearch.hint"), cx).into_any_element())
        } else if self.groups.is_empty() {
            Some(command_empty(cx.t("workspaceSearch.empty")).into_any_element())
        } else {
            None
        };
        let truncated = self.truncated.then(|| {
            div()
                .px(px(16.0))
                .py(px(8.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("workspaceSearch.truncated"))
        });
        let rows = self.rows(cx);
        let close = cx.entity().downgrade();
        let focus = gpui::Focusable::focus_handle(self.input.read(cx), cx);
        command_dialog(
            "workspace-search-dialog",
            focus,
            move |_, cx| {
                let _ = close.update(cx, |_, cx| cx.emit(WorkspaceSearchEvent::Closed));
            },
            window,
            cx,
        )
        .child(
            command_field("workspace-search-field", self.input.clone(), cx).on_key_down(
                cx.listener(|search, event: &KeyDownEvent, _, cx| {
                    match event.keystroke.key.as_str() {
                        "down" => search.move_highlight(true, cx),
                        "up" => search.move_highlight(false, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }),
            ),
        )
        .child(
            div()
                .id("workspace-search-list")
                .max_h(px(command_list_height(window)))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .children(status)
                .children(rows)
                .children(truncated),
        )
    }
}
