//! "Antigravity para contas pessoais do Google" (`AntigravitySetup`): the
//! DCC-managed runtime, installed from the verified official release, then
//! the Google sign-in; the badges say what is in place (or the executable
//! set in Advanced, green only when it runs).

use dcc_core::domain::provider::{HealthStatus, ProviderDescriptor};
use gpui::{AnyElement, Context, Hsla, SharedString, div, prelude::*, px};

use super::super::{Settings, SettingsEvent};
use crate::data::provider_usage::AntigravityStatus;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster, spinner};

#[derive(Default)]
pub(in super::super) struct Antigravity {
    pub(in super::super) status: Option<AntigravityStatus>,
    loading: bool,
    pub(in super::super) installing: bool,
    pub(in super::super) connecting: bool,
}

fn message(
    error: crate::data::desktop_api::ApiError,
    fallback: SharedString,
    cx: &gpui::App,
) -> SharedString {
    match error {
        crate::data::desktop_api::ApiError::DesktopClosed => cx.t("native.desktopClosed"),
        crate::data::desktop_api::ApiError::Failed(text) if !text.is_empty() => text.into(),
        _ => fallback,
    }
}

impl Settings {
    fn antigravity_runtime(&self) -> Option<dcc_core::ports::provider::ProviderRuntimeConfig> {
        let provider = self
            .providers
            .catalog
            .as_ref()?
            .iter()
            .find(|provider| provider.id.0 == "antigravity")?;
        crate::data::provider_runtime::to_config(
            &self.providers.runtime.draft("antigravity"),
            &provider.capabilities,
        )
    }

    /// Asked when Antigravity is on screen and not known yet.
    pub(super) fn ensure_antigravity(&mut self, cx: &mut Context<Self>) {
        let on_screen = self
            .providers
            .current()
            .is_some_and(|provider| provider.id.0 == "antigravity");
        let state = &self.providers.antigravity;
        if on_screen && state.status.is_none() && !state.loading {
            self.load_antigravity(cx);
        }
    }

    fn load_antigravity(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        let runtime = self.antigravity_runtime();
        self.providers.antigravity.loading = true;
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_executor()
                .spawn(async move { api.antigravity_status(runtime.as_ref()) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let state = &mut settings.providers.antigravity;
                state.loading = false;
                state.status = status.ok().or(state.status.take());
                cx.notify();
            });
        })
        .detach();
    }

    /// After installing or signing in: the catalog and the status again.
    fn antigravity_changed(&mut self, cx: &mut Context<Self>) {
        self.load_providers(cx);
        self.load_antigravity(cx);
        cx.emit(SettingsEvent::ProvidersChanged);
    }

    pub(super) fn install_antigravity(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        self.providers.antigravity.installing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let installed = cx
                .background_executor()
                .spawn(async move { api.install_antigravity() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.providers.antigravity.installing = false;
                match installed {
                    Ok(version) => {
                        Toaster::success(
                            cx.t_with(
                                "settings.model.antigravityInstallSucceeded",
                                &Args::default().with("version", version),
                            ),
                            cx,
                        );
                        settings.antigravity_changed(cx);
                    }
                    Err(error) => {
                        let fallback = cx.t("settings.model.antigravityInstallFailed");
                        Toaster::error(message(error, fallback, cx), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn connect_antigravity(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        let runtime = self.antigravity_runtime();
        self.providers.antigravity.connecting = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let connected = cx
                .background_executor()
                .spawn(async move { api.connect_antigravity(runtime.as_ref()) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.providers.antigravity.connecting = false;
                match connected {
                    Ok(models) => {
                        Toaster::success(
                            cx.t_with(
                                "settings.model.antigravityConnectSucceeded",
                                &Args::count(models.len() as i64),
                            ),
                            cx,
                        );
                        settings.antigravity_changed(cx);
                    }
                    Err(error) => {
                        let fallback = cx.t("settings.model.antigravityConnectFailed");
                        Toaster::error(message(error, fallback, cx), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn antigravity_setup(
        &self,
        provider: &ProviderDescriptor,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let state = &self.providers.antigravity;
        let status = state.status.clone();
        let custom = self
            .providers
            .runtime
            .draft("antigravity")
            .binary_path
            .trim()
            .to_string();
        let custom_ready = matches!(provider.health, HealthStatus::Healthy);
        let pill = |icon: &'static str,
                    label: SharedString,
                    tone: Hsla,
                    title: Option<String>|
         -> AnyElement {
            let pill = div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .h(px(20.0))
                .px(px(8.0))
                .rounded(px(26.0))
                .bg(mix_oklab(tone, 0.2, colors.secondary))
                .text_size(px(10.0))
                .text_color(colors.foreground)
                .child(Icon::new(icon).size(12.0))
                .child(label);
            match title {
                Some(title) => crate::ui::Tooltip::text("settings-antigravity-custom", pill, title)
                    .into_any_element(),
                None => pill.into_any_element(),
            }
        };
        let mut badges: Vec<AnyElement> = Vec::new();
        if !custom.is_empty() {
            badges.push(pill(
                if custom_ready {
                    "circle-check"
                } else {
                    "triangle-alert"
                },
                cx.t(if custom_ready {
                    "settings.model.antigravityCustomRuntime"
                } else {
                    "settings.model.antigravityCustomRuntimeBroken"
                }),
                if custom_ready {
                    colors.success
                } else {
                    colors.warning
                },
                Some(custom.clone()),
            ));
        } else if let Some(status) = status
            .as_ref()
            .filter(|status| status.managed_runtime_installed)
        {
            badges.push(pill(
                "circle-check",
                cx.t_with(
                    "settings.model.antigravityRuntimeInstalled",
                    &Args::default().with(
                        "version",
                        status.runtime_version.clone().unwrap_or_default(),
                    ),
                ),
                colors.success,
                None,
            ));
        }
        if let Some(status) = status.as_ref().filter(|status| status.signed_in) {
            badges.push(pill(
                "circle-check",
                cx.t_with(
                    "settings.model.antigravitySignedIn",
                    &Args::count(status.cached_model_count as i64),
                ),
                colors.success,
                None,
            ));
        }
        let installed = status
            .as_ref()
            .is_some_and(|status| status.managed_runtime_installed);
        let signed_in = status.as_ref().is_some_and(|status| status.signed_in);
        let (installing, connecting) = (state.installing, state.connecting);
        let install = Button::new("settings-antigravity-install")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Sm)
            .text_size(12.0)
            .disabled(installing || connecting || status.is_none() || installed)
            .on_click(cx.listener(|settings, _, _, cx| settings.install_antigravity(cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(if installing {
                        spinner("settings-antigravity-installing", 16.0).into_any_element()
                    } else {
                        Icon::new(if installed {
                            "circle-check"
                        } else {
                            "download"
                        })
                        .size(16.0)
                        .into_any_element()
                    })
                    .child(cx.t(if installing {
                        "settings.model.antigravityInstalling"
                    } else if installed {
                        "settings.model.antigravityInstalled"
                    } else {
                        "settings.model.antigravityInstall"
                    })),
            );
        let connect = Button::new("settings-antigravity-connect")
            .size(ButtonSize::Sm)
            .text_size(12.0)
            .disabled(installing || connecting)
            .on_click(cx.listener(|settings, _, _, cx| settings.connect_antigravity(cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(if connecting {
                        spinner("settings-antigravity-connecting", 16.0).into_any_element()
                    } else {
                        Icon::new(if signed_in { "refresh-ccw" } else { "log-in" })
                            .size(16.0)
                            .into_any_element()
                    })
                    .child(cx.t(if connecting {
                        "settings.model.antigravityConnecting"
                    } else if signed_in {
                        "settings.model.antigravityRefreshModels"
                    } else {
                        "settings.model.antigravityConnect"
                    })),
            );
        div()
            .debug_selector(|| "settings-antigravity".into())
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(19.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("settings.model.antigravitySetupHint")),
            )
            .when(!badges.is_empty(), |column| {
                column.child(div().flex().flex_wrap().gap(px(6.0)).children(badges))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(install)
                    .child(connect),
            )
            .into_any_element()
    }
}
