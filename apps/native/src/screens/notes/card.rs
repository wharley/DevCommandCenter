//! A note in the library (`note-preview`): its project and mark, title,
//! three lines of text, where it came from, and its tags; and the library
//! with nothing to show (`notes-empty`, the bulb and its two dots).

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px, relative};

use super::palette::note_colors;
use super::{NotesWorkspace, StatusFilter};
use crate::data::notes::ProjectNote;
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Icon, Tracked};

impl NotesWorkspace {
    pub(super) fn card(
        &self,
        note: &ProjectNote,
        index: usize,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let palette = note_colors(&note.color, theme.appearance == Appearance::Dark);
        let colors = &theme.colors;
        let icon = if note.status == "completed" {
            Icon::new("check-check").size(14.0)
        } else if note.pinned {
            Icon::new("pin").size(13.0)
        } else {
            Icon::new("arrow-up-right").size(14.0)
        };
        let title = if note.title.is_empty() {
            cx.t("notes.untitled").to_string()
        } else {
            note.title.clone()
        };
        let text = if note.content.is_empty() {
            cx.t("notes.contentPlaceholder").to_string()
        } else {
            note.content.clone()
        };
        let origin = if note.source_task_title.is_empty() {
            cx.t("notes.projectNote").to_string()
        } else {
            note.source_task_title.clone()
        };
        let tag = |text: SharedString| {
            div()
                .mt(px(7.0))
                .text_size(px(9.0))
                .text_color(palette.accent)
                .child(text)
        };
        let id = note.id.clone();
        div()
            .id(SharedString::from(format!("note-card-{index}")))
            .debug_selector(move || format!("note-card-{index}"))
            .flex()
            .flex_col()
            .w(px(width))
            .min_h(px(208.0))
            .p(px(17.0))
            .overflow_hidden()
            .bg(palette.fill)
            .border_1()
            .border_color(palette.line)
            .rounded_tl(px(19.0))
            .rounded_tr(px(19.0))
            .rounded_br(px(19.0))
            .rounded_bl(px(5.0))
            .cursor_pointer()
            .hover(|card| {
                card.rounded_bl(px(12.0)).shadow(vec![gpui::BoxShadow {
                    color: gpui::black().opacity(0x44 as f32 / 255.0),
                    offset: gpui::point(px(0.0), px(10.0)),
                    blur_radius: px(25.0),
                    spread_radius: px(-12.0),
                    inset: false,
                }])
            })
            .on_click(cx.listener(move |workspace, _, _, cx| workspace.show(&id, cx)))
            .child(
                div()
                    .flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .text_size(px(10.0))
                    .text_color(palette.accent)
                    .child(div().min_w_0().truncate().child(note.project_name.clone()))
                    .child(icon.color(palette.accent)),
            )
            .child(
                div()
                    .mt(px(15.0))
                    .mb(px(8.0))
                    .text_size(px(15.0))
                    .line_height(relative(1.4))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.foreground)
                    .child(Tracked::new(title, 15.0, -0.025)),
            )
            .child(
                div()
                    .mb(px(16.0))
                    .text_size(px(12.0))
                    .line_height(relative(1.75))
                    .text_color(colors.muted_foreground)
                    .line_clamp(3)
                    .child(text),
            )
            .child(
                div()
                    .mt_auto()
                    .pt(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .max_w_full()
                    .text_size(px(9.0))
                    .text_color(colors.muted_foreground)
                    .child(Icon::new("message-circle").size(12.0))
                    .child(div().min_w_0().truncate().child(origin)),
            )
            .when(!note.context_snapshot.is_empty(), |card| {
                card.child(tag(cx.t("notes.contextIncluded")))
            })
            .when(note.implementation_task_id.is_some(), |card| {
                card.child(tag(cx.t("notes.taskLinked")))
            })
            .into_any_element()
    }

    pub(super) fn empty(&self, search: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let title = if self.status == StatusFilter::Completed {
            "notes.emptyCompleted"
        } else if search {
            "notes.noResults"
        } else {
            "notes.emptyTitle"
        };
        div()
            .debug_selector(|| "notes-empty".into())
            .flex()
            .flex_col()
            .min_h(px(235.0))
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .p(px(25.0))
            .text_color(colors.muted_foreground)
            .child(
                div()
                    .relative()
                    .mb(px(7.0))
                    .flex()
                    .size(px(78.0))
                    .items_center()
                    .justify_center()
                    .rounded_tl(px(27.0))
                    .rounded_tr(px(27.0))
                    .rounded_br(px(27.0))
                    .rounded_bl(px(9.0))
                    .bg(colors.foreground.opacity(0.04))
                    .child(Icon::new("lightbulb").size(28.0))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right(px(-12.0))
                            .size(px(9.0))
                            .rounded_full()
                            .bg(crate::theme::color::hex(0xd4b279).opacity(0x55 as f32 / 255.0)),
                    )
                    .child(
                        div()
                            .absolute()
                            .bottom(px(9.0))
                            .left(px(-15.0))
                            .size(px(6.0))
                            .rounded_full()
                            .bg(crate::theme::color::hex(0xa69ac1).opacity(0x66 as f32 / 255.0)),
                    ),
            )
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(Tracked::new(cx.t(title), 16.0, -0.025)),
            )
            .child(
                div()
                    .max_w(px(350.0))
                    .text_size(px(12.0))
                    .line_height(relative(1.7))
                    .text_center()
                    .child(cx.t("notes.emptyHint")),
            )
            .into_any_element()
    }
}
