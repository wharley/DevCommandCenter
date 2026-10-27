//! The dialog's frame (`.dcc-settings-dialog`: 1140×790 at most, 94vw and
//! 86vh, the surface edge): the navigation, then the section's eyebrow,
//! title and description over its scrolling content.

use gpui::{Context, FontWeight, Render, Window, div, prelude::*, px};

use super::{SectionId, Settings, SettingsEvent};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Dialog, Tracked};

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let edge = crate::ui::surface_edge(theme);
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) * 0.94).min(1140.0);
        let height = (f32::from(viewport.height) * 0.86).min(790.0);
        let key = self.active.key();
        let navigation = self.navigation(window, cx);
        let content = match self.active {
            SectionId::General => self.general(cx),
            SectionId::Appearance => self.appearance(cx),
            SectionId::Model => self.providers_section(cx),
            SectionId::DecisionProvider => self.decision_section(cx),
            SectionId::AiMemory => self.memory_section(cx),
            SectionId::Account => self.account_section(cx),
            SectionId::Git => self.git_section(cx),
            SectionId::Integrations => self.connectors_section(cx),
            SectionId::Shortcuts => self.shortcuts_section(cx),
            SectionId::Experimental => self.experimental(cx),
            _ => div().into_any_element(),
        };
        let confirm = self
            .confirm_dialog(cx)
            .or_else(|| self.remove_account_dialog(cx))
            .or_else(|| self.reset_dialog(cx))
            .or_else(|| self.forge_connect_dialog(cx))
            .or_else(|| self.automation_dialog())
            .or_else(|| self.connector_confirm_dialog(cx))
            .or_else(|| {
                self.connectors
                    .connect
                    .as_ref()
                    .map(|(dialog, _)| dialog.clone().into_any_element())
            });
        let close = cx.entity().downgrade();
        let focus = gpui::Focusable::focus_handle(self.search.read(cx), cx);
        Dialog::new("settings-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |_, cx| cx.emit(SettingsEvent::Closed));
        })
        .max_width(width)
        .max_height(height)
        .padding(0.0)
        .gap(0.0)
        .border(edge)
        .initial_focus(focus)
        .child(
            div()
                .debug_selector(|| "settings-dialog".into())
                .flex()
                .w_full()
                .h(px(height))
                .overflow_hidden()
                .child(navigation)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .overflow_hidden()
                        .child(
                            div()
                                .flex_none()
                                .pt(px(29.0))
                                .pr(px(48.0))
                                .pb(px(23.0))
                                .pl(px(30.0))
                                .border_b_1()
                                .border_color(edge)
                                .child(
                                    div()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(colors.muted_foreground)
                                        .child(Tracked::new(
                                            cx.t(&format!(
                                                "settings.navigation.groups.{}",
                                                self.active.group().key()
                                            )),
                                            10.0,
                                            0.08,
                                        )),
                                )
                                .child(
                                    div()
                                        .debug_selector(|| "settings-title".into())
                                        .my(px(8.0))
                                        .text_size(px(23.0))
                                        .line_height(px(27.6))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(cx.t(&format!("settings.sections.{key}.label"))),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .line_height(px(19.2))
                                        .text_color(colors.muted_foreground)
                                        .child(
                                            cx.t(&format!("settings.sections.{key}.description")),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .id("settings-content")
                                .flex_1()
                                .min_w_0()
                                .min_h_0()
                                .overflow_y_scroll()
                                .track_scroll(&self.scroll)
                                .pt(px(24.0))
                                .px(px(30.0))
                                .pb(px(30.0))
                                .child(content),
                        ),
                )
                .children(confirm),
        )
    }
}
