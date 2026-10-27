//! Computer Use's cards: the Mac's support, the providers' matrix, the
//! two permissions and the armed grant.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use super::{card, heading, hint, mark, mcp_label, remaining};
use crate::data::computer_use::{AccessKind, ComputerStatus, Permission};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon, spinner};

impl Settings {
    pub(super) fn computer_support(
        &self,
        status: &ComputerStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let unsupported = match (status.platform.as_str(), status.minimum_macos_version) {
            ("macos", Some(version)) => cx.t_with(
                "settings.computerUse.unsupportedMacHint",
                &Args::default().with("version", version.to_string()),
            ),
            _ => cx.t("settings.computerUse.unsupportedPlatformHint"),
        };
        card(cx)
            .bg(colors.muted.opacity(0.15))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .items_start()
                            .gap(px(12.0))
                            .child(mark("monitor-cog", cx))
                            .child(
                                div()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(heading(
                                                cx.t("settings.computerUse.externalAppsTitle"),
                                                cx,
                                            ))
                                            .child(Badge::new(
                                                cx.t("settings.computerUse.experimental"),
                                                BadgeVariant::Warn,
                                            )),
                                    )
                                    .child(
                                        hint(12.0, cx.t("settings.computerUse.promptHint"), cx)
                                            .mt(px(4.0)),
                                    ),
                            ),
                    )
                    .child(
                        Badge::new(
                            cx.t(if status.supported {
                                "settings.computerUse.supported"
                            } else {
                                "settings.computerUse.unsupported"
                            }),
                            if status.supported {
                                BadgeVariant::Success
                            } else {
                                BadgeVariant::Outline
                            },
                        )
                        .height(32.0)
                        .padding_x(12.0)
                        .text_size(12.0)
                        .weight(FontWeight::NORMAL),
                    ),
            )
            .when(!status.supported, |card| {
                card.child(
                    div()
                        .mt(px(16.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.6))
                        .bg(colors.background.opacity(0.5))
                        .p(px(12.0))
                        .child(hint(12.0, unsupported, cx)),
                )
            })
            .into_any_element()
    }

    pub(super) fn computer_matrix(
        &self,
        status: &ComputerStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let providers = self.providers.catalog.clone().unwrap_or_default();
        let session = self.session.session_id.is_some();
        let current = self.session.provider_id.clone();
        let label = current.as_deref().map(|id| {
            providers
                .iter()
                .find(|provider| provider.id.0 == id)
                .map_or(id.to_string(), |provider| provider.label.clone())
        });
        let cell = |value: SharedString, header: bool| {
            div()
                .flex_1()
                .px(px(8.0))
                .py(px(8.0))
                .when(header, |cell| cell.font_weight(FontWeight::MEDIUM))
                .child(value)
        };
        let head = div()
            .flex()
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .text_color(colors.muted_foreground)
            .child(cell(cx.t("settings.computerUse.matrix.provider"), true))
            .child(cell(cx.t("settings.computerUse.matrix.mcp"), true))
            .child(cell(cx.t("settings.computerUse.matrix.vision"), true))
            .child(cell(cx.t("settings.computerUse.matrix.stop"), true));
        let count = providers.len();
        let rows = providers.iter().enumerate().map(|(index, provider)| {
            let is_current = current.as_deref() == Some(provider.id.0.as_str());
            div()
                .flex()
                .when(index + 1 < count, |row| {
                    row.border_b_1().border_color(colors.border.opacity(0.4))
                })
                .child(
                    div()
                        .flex_1()
                        .px(px(8.0))
                        .py(px(8.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(provider.label.clone())
                        .when(is_current, |cell| {
                            cell.child(
                                div()
                                    .ml(px(4.0))
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(colors.muted_foreground)
                                    .child(format!(
                                        "({})",
                                        cx.t("settings.computerUse.matrix.current")
                                    )),
                            )
                            .flex()
                        }),
                )
                .child(
                    cell(cx.t(mcp_label(&provider.capabilities.mcp_support)), false)
                        .text_color(colors.muted_foreground),
                )
                .child(
                    cell(
                        cx.t(if provider.capabilities.vision {
                            "settings.computerUse.matrix.visionYes"
                        } else {
                            "settings.computerUse.matrix.visionNo"
                        }),
                        false,
                    )
                    .text_color(colors.muted_foreground),
                )
                .child(
                    cell(
                        cx.t(if session {
                            "settings.computerUse.matrix.stopAvailable"
                        } else {
                            "settings.computerUse.matrix.stopWhenActive"
                        }),
                        false,
                    )
                    .text_color(colors.muted_foreground),
                )
        });
        card(cx)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .child(heading(cx.t("settings.computerUse.matrix.title"), cx))
                            .child(
                                hint(12.0, cx.t("settings.computerUse.matrix.hint"), cx)
                                    .mt(px(4.0)),
                            ),
                    )
                    .when(session, |row| {
                        row.child(if status.runtime_attached {
                            Badge::new(
                                cx.t_with(
                                    "settings.computerUse.matrix.sessionAttached",
                                    &Args::default().with(
                                        "provider",
                                        label.unwrap_or_else(|| {
                                            cx.t("settings.computerUse.matrix.unknownProvider")
                                                .to_string()
                                        }),
                                    ),
                                ),
                                BadgeVariant::Success,
                            )
                        } else {
                            Badge::new(
                                cx.t("settings.computerUse.matrix.sessionNotAttached"),
                                BadgeVariant::Outline,
                            )
                        })
                    }),
            )
            .child(
                div()
                    .debug_selector(|| "settings-computer-matrix".into())
                    .mt(px(16.0))
                    .min_w(px(520.0))
                    .text_size(px(12.0))
                    .child(head)
                    .children(rows),
            )
            .into_any_element()
    }

    fn permission_row(
        &self,
        kind: AccessKind,
        permission: Permission,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let key = match kind {
            AccessKind::Accessibility => "accessibility",
            AccessKind::ScreenRecording => "screenRecording",
        };
        let busy = self.computer.busy;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .p(px(12.0))
            .child(
                div()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .when(permission.granted, |row| {
                                row.child(Icon::new("check").size(14.0).color(colors.success))
                            })
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(
                                        cx.t(&format!("settings.computerUse.permissions.{key}")),
                                    ),
                            )
                            .child(Badge::new(
                                cx.t(if permission.granted {
                                    "settings.computerUse.permissionState.granted"
                                } else {
                                    "settings.computerUse.permissionState.notGranted"
                                }),
                                if permission.granted {
                                    BadgeVariant::Success
                                } else {
                                    BadgeVariant::Warn
                                },
                            )),
                    )
                    .child(
                        hint(
                            11.0,
                            cx.t(&format!("settings.computerUse.permissions.{key}Hint")),
                            cx,
                        )
                        .mt(px(4.0)),
                    ),
            )
            .when(!permission.granted && permission.can_request, |row| {
                row.child(
                    Button::new(SharedString::from(format!(
                        "settings-computer-request-{key}"
                    )))
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Sm)
                    .text_size(12.0)
                    .disabled(busy.is_some())
                    .on_click(cx.listener(move |settings, _, _, cx| {
                        settings.request_computer_access(kind, cx)
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(busy == Some(key), |label| {
                                label.child(spinner("settings-computer-requesting", 14.0))
                            })
                            .child(cx.t("settings.computerUse.requestPermission")),
                    ),
                )
            })
            .into_any_element()
    }

    pub(super) fn computer_permissions(
        &self,
        status: &ComputerStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        card(cx)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.0))
                    .child(
                        div().mt(px(2.0)).child(
                            Icon::new("shield-alert")
                                .size(16.0)
                                .color(colors.muted_foreground),
                        ),
                    )
                    .child(
                        div()
                            .child(heading(cx.t("settings.computerUse.permissionsTitle"), cx))
                            .child(
                                hint(12.0, cx.t("settings.computerUse.permissionsHint"), cx)
                                    .mt(px(4.0)),
                            )
                            .when(
                                cfg!(debug_assertions) && status.platform == "macos",
                                |column| {
                                    column.child(
                                        hint(
                                            11.0,
                                            cx.t("settings.computerUse.devPermissionHint"),
                                            cx,
                                        )
                                        .mt(px(8.0)),
                                    )
                                },
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.permission_row(
                        AccessKind::ScreenRecording,
                        status.screen_recording,
                        cx,
                    ))
                    .child(self.permission_row(
                        AccessKind::Accessibility,
                        status.accessibility,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn computer_grant(
        &self,
        status: &ComputerStatus,
        left: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let names = status.granted_app_names();
        let apps = if names.is_empty() {
            cx.t_with(
                "settings.computerUse.activeAppsCount",
                &Args::count(status.grant.allowed_bundle_ids.len() as i64),
            )
            .to_string()
        } else {
            names.join(", ")
        };
        let disarming = self.computer.busy == Some("disarm");
        card(cx)
            .debug_selector(|| "settings-computer-grant".into())
            .bg(colors.muted.opacity(0.15))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.0))
                    .child(
                        div().mt(px(2.0)).child(
                            Icon::new("clock-3")
                                .size(16.0)
                                .color(colors.muted_foreground),
                        ),
                    )
                    .child(
                        div()
                            .child(heading(cx.t("settings.computerUse.activeTitle"), cx))
                            .child(
                                hint(
                                    12.0,
                                    cx.t_with(
                                        "settings.computerUse.activeHint",
                                        &Args::default().with("time", remaining(left)),
                                    ),
                                    cx,
                                )
                                .mt(px(4.0)),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(apps),
                            ),
                    ),
            )
            .child(
                Button::new("settings-computer-disarm")
                    .variant(ButtonVariant::Destructive)
                    .size(ButtonSize::Sm)
                    .text_size(12.0)
                    .disabled(disarming)
                    .on_click(cx.listener(|settings, _, _, cx| settings.disarm_computer(cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(disarming, |label| {
                                label.child(spinner("settings-computer-disarming", 14.0))
                            })
                            .child(cx.t("settings.computerUse.disable")),
                    ),
            )
            .into_any_element()
    }
}
