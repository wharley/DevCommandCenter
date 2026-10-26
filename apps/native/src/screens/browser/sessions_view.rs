//! The "Sessões" button in the Browser's bar and its dialog as the desktop
//! draws them: 520px, the title with `UserRound`, the site box, the hint
//! to sign in by hand, each eligible profile (cyan when picked), the amber
//! confirmation, the error; "Fazer login manualmente" closes, "Importar
//! sessão" imports. It cannot close while importing.

use gpui::{AnyElement, Context, FontWeight, Window, div, prelude::*, px};

use super::sessions::browser_name;
use super::surface::BrowserSurface;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, dialog_footer, spinner};

impl BrowserSurface {
    /// `browser.sessions.open`: on once the page exists.
    pub(super) fn sessions_button(&self, cx: &mut Context<Self>) -> AnyElement {
        Button::new("browser-sessions")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .height(24.0)
            .padding_x(6.0)
            .text_size(12.0)
            .muted()
            .disabled(self.page.is_none())
            .on_click(cx.listener(|surface, _, _, cx| surface.open_sessions(cx)))
            .child(Icon::new("user-round").size(14.0))
            .child(cx.t("browser.sessions.open"))
            .into_any_element()
    }

    pub(super) fn sessions_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.sessions.as_ref()?;
        let colors = cx.theme().colors.clone();
        let site = dialog.site();
        let importing = dialog.importing;
        let header = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .pr(px(28.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(16.0))
                    .line_height(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(Icon::new("user-round").size(16.0))
                    .child(cx.t("browser.sessions.title")),
            )
            .child(
                div()
                    .text_color(colors.muted_foreground)
                    .child(cx.t("browser.sessions.description")),
            );
        let site_box = div()
            .rounded(px(6.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.muted.opacity(0.3))
            .p(px(12.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("browser.sessions.site")),
            )
            .child(
                div().mt(px(4.0)).font_weight(FontWeight::MEDIUM).child(
                    site.clone()
                        .unwrap_or_else(|| cx.t("browser.sessions.noSite").to_string()),
                ),
            );
        let small = |text: gpui::SharedString| {
            div()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let empty = dialog.profiles.is_empty().then(|| {
            div()
                .rounded(px(6.0))
                .border_1()
                .border_dashed()
                .border_color(colors.border)
                .p(px(12.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("browser.sessions.empty"))
        });
        let hover = colors.muted.opacity(0.5);
        let profiles: Vec<AnyElement> = dialog
            .profiles
            .iter()
            .enumerate()
            .map(|(index, profile)| {
                let picked = dialog.selected.as_deref() == Some(profile.id.as_str());
                let id = profile.id.clone();
                div()
                    .id(("browser-session-profile", index))
                    .debug_selector(move || format!("browser-session-profile-{index}"))
                    .w_full()
                    .rounded(px(6.0))
                    .border_1()
                    .p(px(12.0))
                    .when(picked, |row| {
                        row.border_color(tw::CYAN_500.hsla())
                            .bg(tw::CYAN_500.alpha(0.1))
                    })
                    .when(!picked, |row| {
                        row.border_color(colors.border)
                            .hover(move |style| style.bg(hover))
                    })
                    .when(!importing, |row| {
                        row.cursor_pointer()
                            .on_click(cx.listener(move |surface, _, _, cx| {
                                surface.select_session_profile(id.clone(), cx)
                            }))
                    })
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(browser_name(&profile.browser)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child(profile.name.clone()),
                    )
                    .into_any_element()
            })
            .collect();
        let confirm = dialog.selected_profile().map(|profile| {
            let place = site
                .clone()
                .unwrap_or_else(|| cx.t("browser.sessions.thisSite").to_string());
            div()
                .rounded(px(6.0))
                .border_1()
                .border_color(tw::AMBER_500.alpha(0.4))
                .bg(tw::AMBER_500.alpha(0.1))
                .p(px(12.0))
                .text_size(px(12.0))
                .child(
                    cx.t_with(
                        "browser.sessions.confirm",
                        &Args::default()
                            .with("source", browser_name(&profile.browser))
                            .with("site", place),
                    ),
                )
        });
        let error = dialog.error.clone().map(|error| {
            div()
                .text_size(px(12.0))
                .text_color(colors.destructive)
                .child(error)
        });
        let body = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .text_size(px(14.0))
            .child(site_box)
            .child(small(cx.t("browser.sessions.manualHint")))
            .children(empty)
            .children(profiles)
            .children(confirm)
            .children(error);
        let can_import = dialog.selected.is_some() && !importing && site.is_some();
        let footer = dialog_footer(
            [
                Button::new("browser-sessions-manual")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Default)
                    .disabled(importing)
                    .on_click(cx.listener(|surface, _, _, cx| surface.close_sessions(cx)))
                    .child(cx.t("browser.sessions.manual"))
                    .into_any_element(),
                Button::new("browser-sessions-import")
                    .size(ButtonSize::Default)
                    .disabled(!can_import)
                    .on_click(cx.listener(|surface, _, _, cx| surface.import_session(cx)))
                    .when(importing, |button| {
                        button.child(spinner("browser-sessions-importing", 14.0))
                    })
                    .child(cx.t("browser.sessions.import"))
                    .into_any_element(),
            ],
            cx,
        );
        let close = cx.entity().downgrade();
        Some(
            Dialog::new("browser-sessions-dialog", move |_: &mut Window, cx| {
                let _ = close.update(cx, |surface, cx| surface.close_sessions(cx));
            })
            .max_width(520.0)
            .close_button(!importing)
            .child(header)
            .child(body)
            .child(footer)
            .into_any_element(),
        )
    }
}
