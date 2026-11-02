//! "Versão do app" in General: the installed build, the updater's state
//! ("Verificando…", "Atualizado", "Atualização disponível: v0.3.0", a failed
//! check) and its two actions, "Verificar atualizações" and "Instalar
//! atualização" (`app::updater`).

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::Settings;
use super::general::{card, heading};
use crate::app::updater::{Stage, updater};
use crate::i18n::{Args, Translate};
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant};

impl Settings {
    pub(super) fn version_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let version = crate::data::feedback::version();
        let shown: SharedString = if version.is_empty() {
            cx.t("settings.general.appVersionUnknown")
        } else {
            version.into()
        };
        let state = updater(cx).map(|updater| {
            let state = updater.read(cx);
            (updater.clone(), state.stage.clone(), state.error.clone())
        });
        let actions = state.map(|(updater, stage, error)| {
            let busy = matches!(stage, Stage::Checking | Stage::Installing(_));
            let line: Option<SharedString> = match &stage {
                Stage::Checking => Some(cx.t("settings.general.checkingUpdates")),
                Stage::UpToDate => Some(cx.t("settings.general.upToDate")),
                Stage::Available(update) => Some(cx.t_with(
                    "settings.general.updateAvailable",
                    &Args::default().with("version", update.version.clone()),
                )),
                Stage::Installing(_) => Some(cx.t("updater.installing")),
                Stage::Idle => None,
            };
            let failed = error.map(|message| {
                cx.t_with(
                    "settings.general.updateCheckFailed",
                    &Args::default().with("message", message),
                )
            });
            let check = {
                let updater = updater.clone();
                Button::new("settings-check-updates")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Sm)
                    .text_size(12.0)
                    .disabled(busy)
                    .on_click(move |_, _, cx| {
                        updater.update(cx, |updater, cx| updater.check(true, cx))
                    })
                    .child(cx.t("settings.general.checkForUpdates"))
            };
            let install = matches!(stage, Stage::Available(_)).then(|| {
                Button::new("settings-install-update")
                    .size(ButtonSize::Sm)
                    .text_size(12.0)
                    .on_click(move |_, _, cx| updater.update(cx, |updater, cx| updater.install(cx)))
                    .child(cx.t("settings.general.installUpdate"))
            });
            div()
                .mt(px(12.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(line.map(|line| {
                    div()
                        .debug_selector(|| "settings-update-state".into())
                        .text_size(px(12.0))
                        .child(line)
                }))
                .children(failed.map(|failed| {
                    div()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(failed)
                }))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child(check)
                        .children(install),
                )
        });
        card(cx)
            .child(heading(
                cx.t("settings.general.appVersionTitle"),
                cx.t("settings.general.appVersionBody"),
                cx,
            ))
            .child(
                div()
                    .mt(px(12.0))
                    .font_family(fonts::MONO)
                    .text_size(px(13.0))
                    .child(shown),
            )
            .children(actions)
            .into_any_element()
    }
}
