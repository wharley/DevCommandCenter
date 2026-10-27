//! "Atalhos de teclado" (`ShortcutCheatsheetDialog`): the app's shortcuts,
//! one bordered row each with its keys. Opens from the help's footer.

use gpui::{App, FontWeight, IntoElement, RenderOnce, Window, div, prelude::*, px};

use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Dialog, command_key, shortcut_keys};

/// The rows in the desktop's order: what each does and its keys.
pub fn rows() -> Vec<(&'static str, Vec<&'static str>)> {
    let command = command_key();
    vec![
        ("shortcutsSheet.commandPalette", vec![command, "K"]),
        (
            "shortcutsSheet.commandPaletteLegacy",
            vec![command, "Shift", "P"],
        ),
        ("shortcutsSheet.focusComposer", vec![command, "Shift", "L"]),
        ("shortcutsSheet.toggleTerminal", vec![command, "J"]),
        ("shortcutsSheet.quickOpen", vec![command, "P"]),
        (
            "shortcutsSheet.workspaceSearch",
            vec![command, "Shift", "F"],
        ),
        (
            "shortcutsSheet.inspectorChanges",
            vec![command, "Shift", "G"],
        ),
        ("shortcutsSheet.inspectorFiles", vec![command, "Shift", "E"]),
        ("shortcutsSheet.sendPrompt", vec![command, "Enter"]),
        (
            "shortcutsSheet.steerSession",
            vec![command, "Shift", "Enter"],
        ),
        ("shortcutsSheet.abortSession", vec!["Esc"]),
        ("shortcutsSheet.openPreferredEditor", vec![command, "O"]),
    ]
}

type OnClose = Box<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ShortcutSheet {
    on_close: OnClose,
}

impl ShortcutSheet {
    pub fn new(on_close: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self {
            on_close: Box::new(on_close),
        }
    }
}

impl RenderOnce for ShortcutSheet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let viewport = window.viewport_size();
        let (width, height) = (f32::from(viewport.width), f32::from(viewport.height));
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let radius = f32::from(theme.radius.xxl);
        let rows = rows().into_iter().map(|(label, keys)| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .px(px(16.0))
                .py(px(12.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(cx.t(label)),
                )
                .child(shortcut_keys(&keys, cx))
        });
        let on_close = self.on_close;
        Dialog::new("shortcuts-dialog", move |window, cx| on_close(window, cx))
            .max_width((width * 0.92).min(560.0))
            .max_height((height * 0.88).min(760.0))
            .padding(0.0)
            .gap(0.0)
            .radius(radius)
            .surface(colors.background)
            .shadow_2xl()
            .child(
                div()
                    .debug_selector(|| "shortcuts-dialog".into())
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .p(px(20.0))
                    // `DialogHeader` (`gap-2`) with `space-y-2`: 16px from
                    // the badge to the title, 8 more under the title.
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(8.0))
                            .child(
                                div().mb(px(8.0)).child(
                                    Badge::new(cx.t("shortcutsSheet.badge"), BadgeVariant::Outline)
                                        .height(28.0)
                                        .padding_x(10.0)
                                        .text_size(11.0)
                                        .weight(FontWeight::NORMAL),
                                ),
                            )
                            .child(
                                div()
                                    .mb(px(8.0))
                                    .text_size(px(15.0))
                                    .line_height(px(15.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .child(cx.t("shortcutsSheet.title")),
                            ),
                    )
                    .child(
                        div()
                            .id("shortcuts-rows")
                            .mt(px(20.0))
                            .min_h_0()
                            .overflow_y_scroll()
                            .pr(px(4.0))
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .children(rows),
                    ),
            )
    }
}
