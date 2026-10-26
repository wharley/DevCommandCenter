//! The dialog as `CommandDialog` draws it: 760px at 18% from the top, a
//! muted heading band, the field, then the groups ("Workspace atual",
//! "Outros workspaces" or "Todas as sessões") with each session's title,
//! "Arquivada", the task and branch, the excerpt and when it changed.

use gpui::{AnyElement, Context, FontWeight, KeyDownEvent, Render, Window, div, prelude::*, px};

use super::{SessionSearch, SessionSearchEvent};
use crate::data::session_search::{updated_at_label, workspace_label};
use crate::i18n::{Locale, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Dialog, Icon};

impl SessionSearch {
    fn item(&self, index: usize, english: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let result = &self.results[index];
        let highlighted = index == self.highlighted;
        let archived = result.archived_at.is_some().then(|| {
            div()
                .flex_shrink_0()
                .rounded(px(6.0))
                .border_1()
                .border_color(colors.border)
                .px(px(6.0))
                .text_size(px(10.0))
                .line_height(px(16.0))
                .child(cx.t("workbench.sessionSearch.archived"))
        });
        let snippet = result.snippet.trim().to_string();
        div()
            .id(("session-result", index))
            .debug_selector(move || format!("session-result-{index}"))
            .flex()
            .items_start()
            .gap(px(12.0))
            .rounded(px(2.0))
            .px(px(8.0))
            .py(px(8.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .cursor_pointer()
            .when(highlighted, |item| item.bg(colors.muted))
            .on_mouse_move(cx.listener(move |search, _, _, cx| {
                if search.highlighted != index {
                    search.highlighted = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |search, _, _, cx| search.pick(index, cx)))
            .child(
                div().mt(px(2.0)).child(
                    Icon::new("history")
                        .size(16.0)
                        .stroke(1.8)
                        .color(colors.muted_foreground),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(result.thread_title.clone()),
                            )
                            .children(archived),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(colors.muted_foreground)
                            .child(workspace_label(result)),
                    )
                    .when(!snippet.is_empty(), |column| {
                        column.child(
                            div()
                                .mt(px(4.0))
                                .max_h(px(39.0))
                                .overflow_hidden()
                                .text_size(px(12.0))
                                .line_height(px(19.5))
                                .text_color(colors.foreground.opacity(0.8))
                                .child(snippet),
                        )
                    }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(updated_at_label(&result.updated_at, english)),
            )
            .into_any_element()
    }

    fn group(
        &self,
        heading: gpui::SharedString,
        range: std::ops::Range<usize>,
        english: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if range.is_empty() {
            return None;
        }
        let muted = cx.theme().colors.muted_foreground;
        let items: Vec<AnyElement> = range.map(|index| self.item(index, english, cx)).collect();
        Some(
            div()
                .p(px(4.0))
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(6.0))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(muted)
                        .child(heading),
                )
                .children(items)
                .into_any_element(),
        )
    }
}

impl Render for SessionSearch {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<crate::i18n::I18n>().locale() == Locale::En;
        let viewport = window.viewport_size();
        let has_query = !self.input.read(cx).text().trim().is_empty();
        let current = self.current_count();
        let total = self.results.len();
        let other_heading = if self.current_task.is_some() {
            cx.t("workbench.sessionSearch.otherWorkspacesGroup")
        } else {
            cx.t("workbench.sessionSearch.allSessionsGroup")
        };
        let groups = [
            self.group(
                cx.t("workbench.sessionSearch.currentWorkspaceGroup"),
                0..current,
                english,
                cx,
            ),
            self.group(other_heading, current..total, english, cx),
        ];
        let loading = self.loading.then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(16.0))
                .py(px(12.0))
                .text_size(px(14.0))
                .text_color(colors.muted_foreground)
                .child(Icon::new("search").size(16.0).stroke(1.8))
                .child(cx.t("workbench.sessionSearch.loading"))
        });
        let empty = (!self.loading && total == 0).then(|| {
            div()
                .py(px(24.0))
                .text_center()
                .text_size(px(14.0))
                .child(cx.t("workbench.sessionSearch.empty"))
        });
        let list_height = (f32::from(viewport.height) * 0.58).min(512.0);
        let close = cx.entity().downgrade();
        let input_focus = gpui::Focusable::focus_handle(self.input.read(cx), cx);
        Dialog::new("session-search-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |_, cx| cx.emit(SessionSearchEvent::Closed));
        })
        .top(0.18)
        .radius(16.0)
        .max_width(760.0)
        .max_height((f32::from(viewport.height) * 0.84).min(620.0))
        .padding(0.0)
        .gap(0.0)
        .close_button(false)
        .initial_focus(input_focus)
        .child(
            div()
                .border_b_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.muted.opacity(0.2))
                .px(px(16.0))
                .py(px(12.0))
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.muted_foreground)
                        .child(cx.t("workbench.sessionSearch.title").to_uppercase()),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(12.0))
                        .line_height(px(19.5))
                        .text_color(colors.muted_foreground)
                        .child(if has_query {
                            cx.t("workbench.sessionSearch.searchingDescription")
                        } else {
                            cx.t("workbench.sessionSearch.recentDescription")
                        }),
                ),
        )
        .child(
            div().p(px(4.0)).child(
                div()
                    .id("session-search-field")
                    .debug_selector(|| "session-search-field".into())
                    .flex()
                    .h(px(32.0))
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(colors.input.opacity(0.3))
                    .bg(colors.input.opacity(0.3))
                    .px(px(10.0))
                    .text_size(px(14.0))
                    .on_key_down(cx.listener(|search, event: &KeyDownEvent, _, cx| {
                        match event.keystroke.key.as_str() {
                            "down" => search.move_highlight(true, cx),
                            "up" => search.move_highlight(false, cx),
                            _ => return,
                        }
                        cx.stop_propagation();
                    }))
                    .child(div().opacity(0.5).child(Icon::new("search").size(16.0)))
                    .child(div().flex_1().min_w_0().child(self.input.clone())),
            ),
        )
        .child(
            div()
                .id("session-search-list")
                .max_h(px(list_height))
                .overflow_y_scroll()
                .children(loading)
                .children(empty)
                .children(groups.into_iter().flatten()),
        )
    }
}
