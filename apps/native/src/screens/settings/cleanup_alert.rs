//! "Geral" › the completed tasks' space: whether the team room shows how
//! much they take and from which size (`data::cleanup_alert`). What can be
//! deleted, and only on a click, stays DCC's rule.

use std::path::PathBuf;

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::Settings;
use super::general::{card, heading};
use crate::data::cleanup_alert::{CleanupAlert, THRESHOLD_CHOICES};
use crate::i18n::Translate;
use crate::ui::{Switch, Toaster, ToggleGroup};

/// The notice's settings as `cleanup-alert.json` keeps them (none in tests).
pub(super) struct CleanupSettings {
    path: Option<PathBuf>,
    alert: CleanupAlert,
}

impl CleanupSettings {
    pub(super) fn load() -> Self {
        let path = (!cfg!(test)).then(|| crate::prefs::native_file("cleanup-alert.json"));
        let alert = path.as_deref().map(CleanupAlert::load).unwrap_or_default();
        Self { path, alert }
    }
}

impl Settings {
    fn save_cleanup(&mut self, enabled: bool, threshold_gb: u32, cx: &mut Context<Self>) {
        self.cleanup.alert.enabled = enabled;
        self.cleanup.alert.threshold_gb = threshold_gb;
        if let Some(path) = &self.cleanup.path
            && let Err(error) = CleanupAlert::save_settings(path, enabled, threshold_gb)
        {
            Toaster::error(error.to_string(), cx);
        }
        cx.notify();
    }

    pub(super) fn cleanup_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let alert = self.cleanup.alert.clone();
        let entity = cx.entity().downgrade();
        let limits = {
            let entity = entity.clone();
            let picked = THRESHOLD_CHOICES
                .iter()
                .position(|gb| *gb == alert.threshold_gb);
            ToggleGroup::new(
                "settings-cleanup-limit",
                THRESHOLD_CHOICES
                    .iter()
                    .map(|gb| format!("{gb} GB").into())
                    .collect(),
                picked,
            )
            .on_select(move |index, _, cx| {
                let _ = entity.update(cx, |settings, cx| {
                    settings.save_cleanup(true, THRESHOLD_CHOICES[index], cx)
                });
            })
        };
        let threshold = alert.threshold_gb;
        card(cx)
            .debug_selector(|| "settings-cleanup".into())
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(24.0))
                    .child(heading(
                        cx.t("settings.general.cleanup.title"),
                        cx.t("settings.general.cleanup.body"),
                        cx,
                    ))
                    .child(
                        Switch::new("settings-cleanup-switch", alert.enabled).on_toggle(
                            move |on, _, cx| {
                                let _ = entity.update(cx, |settings, cx| {
                                    settings.save_cleanup(on, threshold, cx)
                                });
                            },
                        ),
                    ),
            )
            .when(alert.enabled, |card| {
                card.child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .text_size(px(12.0))
                        .child(cx.t("settings.general.cleanup.limit"))
                        .child(limits),
                )
            })
            .into_any_element()
    }
}
