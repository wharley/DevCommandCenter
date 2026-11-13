//! The column's chrome: `bg-sidebar`, a right edge, the 40px header with the
//! "Arquivos" / "Mudanças" tabs (the change count in amber) and closing.

use gpui::{Context, FontWeight, Render, SharedString, Window, div, prelude::*, px};

use super::{CodeColumn, CodeColumnEvent, CodeColumnTab, changes, tree};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, shortcut_keys};

fn tab(
    column: &CodeColumn,
    id: &'static str,
    tab: CodeColumnTab,
    label: SharedString,
    count: usize,
    cx: &mut Context<CodeColumn>,
) -> impl IntoElement {
    let colors = cx.theme().colors.clone();
    let active = column.tab == tab;
    let hover = colors.muted.opacity(0.3);
    let foreground = colors.foreground;
    div()
        .id(id)
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(4.0))
        .rounded(px(6.0))
        .px(px(8.0))
        .py(px(4.0))
        .cursor_pointer()
        .map(|button| {
            if active {
                button
                    .bg(colors.muted.opacity(0.55))
                    .text_color(colors.foreground)
            } else {
                button
                    .text_color(colors.muted_foreground)
                    .hover(move |style| style.bg(hover).text_color(foreground))
            }
        })
        .on_click(cx.listener(move |column, _, _, cx| column.set_tab(tab, cx)))
        .child(
            div()
                .truncate()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .child(label),
        )
        .when(count > 0, |button| {
            button.child(
                div()
                    .text_size(px(10.0))
                    .text_color(tw::AMBER_500.hsla())
                    .child(count.to_string()),
            )
        })
}

impl Render for CodeColumn {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let changes = self.change_count();
        let close_label = cx.t("codeColumn.close");
        let quick_open_label = cx.t("inspector.codeSection.quickOpen");
        let header = div()
            .flex()
            .h(px(40.0))
            .flex_shrink_0()
            .items_center()
            .gap(px(4.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .pl(px(8.0))
            .pr(px(6.0))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(2.0))
                    .child(tab(
                        self,
                        "code-column-files",
                        CodeColumnTab::Files,
                        cx.t("codeColumn.files"),
                        0,
                        cx,
                    ))
                    .child(tab(
                        self,
                        "code-column-changes",
                        CodeColumnTab::Changes,
                        cx.t("codeColumn.changes"),
                        changes,
                        cx,
                    )),
            )
            .child(
                Tooltip::new(
                    "code-column-quick-open-tip",
                    Button::new("code-column-quick-open")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .muted()
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(CodeColumnEvent::OpenQuickOpen)),
                        )
                        .child(Icon::new("search").size(14.0).stroke(1.8)),
                    move |_, cx| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(quick_open_label.clone())
                            .child(shortcut_keys(&[crate::ui::command_key(), "P"], cx))
                            .into_any_element()
                    },
                )
                .side(crate::ui::tooltip::Side::Bottom),
            )
            .child(
                Tooltip::new(
                    "code-column-close-tip",
                    Button::new("code-column-close")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .muted()
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(CodeColumnEvent::Close)))
                        .child(Icon::new("panel-left-close").size(14.0).stroke(1.8)),
                    move |_, cx| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(close_label.clone())
                            .child(shortcut_keys(&[crate::ui::command_key(), "Shift", "E"], cx))
                            .into_any_element()
                    },
                )
                .side(crate::ui::tooltip::Side::Bottom),
            );
        let body = if self.on_machine {
            // Its files are on the machine: its changes are read there.
            super::remote::remote_changes(self, cx)
        } else {
            match self.tab {
                CodeColumnTab::Files => tree::files_tree(self, cx).into_any_element(),
                CodeColumnTab::Changes => changes::changes(self, window, cx),
            }
        };
        div()
            .id("code-column")
            .debug_selector(|| "code-column".into())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h_full()
            .w(px(self.tab.width()))
            .overflow_hidden()
            .border_r_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.sidebar)
            .child(header)
            .child(body)
    }
}
