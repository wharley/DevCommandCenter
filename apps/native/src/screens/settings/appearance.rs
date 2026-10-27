//! "Aparência": the language (Português, English), the theme as three
//! little windows (light, dark, the system's), the density, and the note
//! that it all applies at once.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::Settings;
use super::preview::{Look, preview};
use crate::i18n::{Locale, Translate};
use crate::theme::{ActiveTheme, Density, ThemePreference};
use crate::ui::{Icon, ToggleGroup};

/// `.dcc-settings-preference`: 18px in, the surface edge, `rounded-[14px]`,
/// half the sidebar's color.
pub(super) fn preference(cx: &Context<Settings>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .p(px(18.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(crate::ui::surface_edge(theme))
        .bg(theme.colors.sidebar.opacity(0.5))
}

/// The title (13px medium, the unlayered CSS beats `text-[14px]`) and hint.
fn heading(title: SharedString, hint: SharedString, cx: &Context<Settings>) -> gpui::Div {
    div()
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().colors.foreground)
                .child(title),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .text_color(cx.theme().colors.muted_foreground)
                .child(hint),
        )
}

impl Settings {
    fn theme_tiles(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = crate::ui::surface_edge(cx.theme());
        let tiles = [
            (ThemePreference::Light, "sun-medium", "light", Look::Light),
            (ThemePreference::Dark, "moon", "dark", Look::Dark),
            (ThemePreference::System, "monitor", "system", Look::System),
        ]
        .into_iter()
        .map(|(value, icon, key, look)| {
            let pressed = self.theme == value;
            let hover = colors.muted_foreground;
            div()
                .id(SharedString::from(format!("settings-theme-{key}")))
                .debug_selector(move || format!("settings-theme-{key}"))
                .flex_1()
                .flex_basis(px(0.0))
                .min_w_0()
                .p(px(6.0))
                .rounded(px(12.0))
                .border_1()
                .cursor_pointer()
                .map(|tile| {
                    if pressed {
                        tile.border_color(colors.accent_calm)
                            .bg(colors.accent_calm.opacity(0.05))
                    } else {
                        tile.border_color(edge)
                            .hover(move |style| style.border_color(hover))
                    }
                })
                .on_click(cx.listener(move |settings, _, _, cx| settings.set_theme(value, cx)))
                .child(
                    div()
                        .h(px(93.0))
                        .overflow_hidden()
                        .rounded(px(7.0))
                        .border_1()
                        .border_color(gpui::Hsla::from(gpui::rgba(0x8080802e)))
                        .child(preview(look)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(5.0))
                        .pt(px(10.0))
                        .pb(px(4.0))
                        .text_size(px(12.0))
                        .child(Icon::new(icon).size(15.0))
                        .child(cx.t(&format!("settings.appearance.{key}")))
                        .child(
                            div()
                                .ml_auto()
                                .flex()
                                .flex_none()
                                .size(px(17.0))
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .border_1()
                                .map(|check| {
                                    if pressed {
                                        check
                                            .border_color(colors.accent_calm)
                                            .bg(colors.accent_calm)
                                            .text_color(colors.background)
                                            .child(Icon::new("check").size(12.0))
                                    } else {
                                        check.border_color(edge)
                                    }
                                }),
                        ),
                )
                .into_any_element()
        });
        preference(cx)
            .child(heading(
                cx.t("settings.appearance.theme"),
                cx.t("settings.appearance.themeHint"),
                cx,
            ))
            .child(div().mt(px(18.0)).flex().gap(px(14.0)).children(tiles))
            .into_any_element()
    }

    pub(super) fn appearance(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let entity = cx.entity().downgrade();
        let language = ToggleGroup::new(
            "settings-locale",
            vec![cx.t("settings.localePtBr"), cx.t("settings.localeEn")],
            Some(usize::from(self.locale == Locale::En)),
        )
        .accent()
        .on_select(move |index, _, cx| {
            let locale = if index == 0 { Locale::PtBr } else { Locale::En };
            let _ = entity.update(cx, |settings, cx| settings.set_locale(locale, cx));
        });
        let entity = cx.entity().downgrade();
        let density = ToggleGroup::new(
            "settings-density",
            vec![
                cx.t("settings.appearance.comfortable"),
                cx.t("settings.appearance.compact"),
            ],
            Some(usize::from(self.density == Density::Compact)),
        )
        .on_select(move |index, _, cx| {
            let density = if index == 0 {
                Density::Comfortable
            } else {
                Density::Compact
            };
            let _ = entity.update(cx, |settings, cx| settings.set_density(density, cx));
        });
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                preference(cx).child(
                    div()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap(px(24.0))
                        .child(heading(
                            cx.t("settings.languageTitle"),
                            cx.t("settings.languageHint"),
                            cx,
                        ))
                        .child(language),
                ),
            )
            .child(self.theme_tiles(cx))
            .child(
                preference(cx).child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_start()
                        .justify_between()
                        .gap(px(20.0))
                        .child(div().flex_1().min_w(px(180.0)).child(heading(
                            cx.t("settings.appearance.density"),
                            cx.t("settings.appearance.densityHint"),
                            cx,
                        )))
                        .child(density),
                ),
            )
            .child(
                div()
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .p(px(16.0))
                    .text_size(px(12.0))
                    .line_height(px(19.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("settings.appearance.colorSystemHint")),
            )
            .into_any_element()
    }
}
