//! The import offer (`ImportOffer`): "Encontramos ClickUp e Linear em
//! Claude Code. Trazer para o DCC?" — once per server; the other apps'
//! files are only read. Bringing them checks each one right away.

use std::path::PathBuf;

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::super::Settings;
use super::view::joined;
use crate::data::connector_custom::{
    NativeServer, app_name, candidates, offered_path, read_offered, remember_offered,
};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster, spinner};

#[derive(Default)]
pub(in super::super) struct Import {
    /// What the providers have; read once.
    found: Option<Vec<NativeServer>>,
    offered: Vec<String>,
    pub(in super::super) path: Option<PathBuf>,
    importing: bool,
}

impl Settings {
    pub(super) fn ensure_import_offer(&mut self, cx: &mut Context<Self>) {
        let import = &mut self.connectors.import;
        if import.found.is_some() {
            return;
        }
        let path = import.path.clone().unwrap_or_else(offered_path);
        import.offered = if cfg!(test) && import.path.is_none() {
            Vec::new()
        } else {
            read_offered(&path)
        };
        import.found = Some(Vec::new());
        let Some(api) = self.general.api() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { api.native_servers().unwrap_or_default() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.connectors.import.found = Some(found);
                cx.notify();
            });
        })
        .detach();
    }

    fn offer(&self) -> Vec<(NativeServer, String)> {
        let import = &self.connectors.import;
        candidates(
            import.found.as_deref().unwrap_or_default(),
            self.connectors.list(),
            &import.offered,
        )
    }

    fn dismiss_offer(&mut self, cx: &mut Context<Self>) {
        let urls: Vec<String> = self
            .offer()
            .into_iter()
            .map(|(server, _)| server.url)
            .collect();
        let import = &mut self.connectors.import;
        if !(cfg!(test) && import.path.is_none()) {
            remember_offered(&import.path.clone().unwrap_or_else(offered_path), &urls);
        }
        import.offered.extend(urls);
        cx.notify();
    }

    fn bring_offer(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        let offer = self.offer();
        self.connectors.import.importing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let imported = cx
                .background_executor()
                .spawn(async move {
                    let imported = api.import_native(&offer)?;
                    // A check right away shows which still need a sign-in.
                    for integration in &imported {
                        let _ = api.test_connector(&integration.definition.id);
                    }
                    Ok::<_, crate::data::desktop_api::ApiError>(imported.len())
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                match imported {
                    Ok(count) => Toaster::success(
                        cx.t_with(
                            "settings.connectors.import.done",
                            &Args::count(count as i64),
                        ),
                        cx,
                    ),
                    Err(error) => Toaster::error(error.to_string(), cx),
                }
                settings.dismiss_offer(cx);
                settings.connectors.import.importing = false;
                settings.load_connectors(cx);
            });
        })
        .detach();
    }

    pub(super) fn import_offer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let offer = self.offer();
        if offer.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<crate::i18n::I18n>().locale();
        let names: Vec<String> = offer.iter().map(|(_, name)| name.clone()).collect();
        let mut apps: Vec<String> = Vec::new();
        for (server, _) in &offer {
            let app = app_name(&server.app).to_string();
            if !apps.contains(&app) {
                apps.push(app);
            }
        }
        let importing = self.connectors.import.importing;
        Some(
            div()
                .debug_selector(|| "settings-connectors-import".into())
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(12.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.muted.opacity(0.3))
                .px(px(12.0))
                .py(px(10.0))
                .child(
                    Icon::new("download")
                        .size(16.0)
                        .color(colors.muted_foreground),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.0))
                        .line_height(px(19.5))
                        .text_color(colors.foreground)
                        .child(
                            cx.t_with(
                                "settings.connectors.import.offer",
                                &Args::default()
                                    .with("names", joined(&names, locale))
                                    .with("apps", joined(&apps, locale)),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            Button::new("settings-connectors-import-later")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .text_size(12.0)
                                .disabled(importing)
                                .on_click(
                                    cx.listener(|settings, _, _, cx| settings.dismiss_offer(cx)),
                                )
                                .child(cx.t("settings.connectors.import.notNow")),
                        )
                        .child(
                            Button::new("settings-connectors-import-bring")
                                .size(ButtonSize::Sm)
                                .text_size(12.0)
                                .disabled(importing)
                                .on_click(
                                    cx.listener(|settings, _, _, cx| settings.bring_offer(cx)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .children(importing.then(|| {
                                            spinner("settings-connectors-importing", 14.0)
                                        }))
                                        .child(cx.t("settings.connectors.import.bring")),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
