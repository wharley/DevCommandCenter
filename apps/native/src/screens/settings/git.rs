//! "Git": the open task's project automation (`.dcc.toml`) — its name or
//! checkout, "Configurar automação" (the dialog over Settings) and how the
//! file is shared.

use gpui::{AnyElement, AppContext, Context, FontWeight, Window, div, prelude::*, px};

use super::Settings;
use crate::i18n::Translate;
use crate::screens::automation::{AutomationEvent, ProjectAutomation};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, Icon};

impl Settings {
    pub(super) fn open_automation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self
            .workspace
            .1
            .clone()
            .filter(|root| !root.trim().is_empty())
        else {
            return;
        };
        if self.automation.is_some() {
            return;
        }
        let api = self.general.api();
        let dialog = cx.new(|cx| ProjectAutomation::new(root, api, window, cx));
        let events = cx.subscribe(&dialog, |settings, _, event, cx| match event {
            AutomationEvent::Closed => {
                settings.automation = None;
                cx.notify();
            }
            AutomationEvent::WorkspaceChanged => {}
        });
        self.automation = Some((dialog, events));
        cx.notify();
    }

    pub(super) fn automation_dialog(&self) -> Option<AnyElement> {
        self.automation
            .as_ref()
            .map(|(dialog, _)| dialog.clone().into_any_element())
    }

    pub(super) fn git_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (name, root) = self.workspace.clone();
        let has_root = root.as_deref().is_some_and(|root| !root.trim().is_empty());
        let shown = name
            .or(root)
            .unwrap_or_else(|| cx.t("settings.git.noWorkspace").to_string());
        div()
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.15))
            .p(px(16.0))
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
                            .child(
                                div()
                                    .flex()
                                    .flex_none()
                                    .items_center()
                                    .justify_center()
                                    .size(px(36.0))
                                    .rounded(px(10.0))
                                    .bg(colors.primary.opacity(0.1))
                                    .child(
                                        Icon::new("list-checks").size(16.0).color(colors.primary),
                                    ),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(colors.foreground)
                                            .child(cx.t("settings.git.automationTitle")),
                                    )
                                    .child(
                                        div()
                                            .mt(px(4.0))
                                            .text_size(px(12.0))
                                            .line_height(px(19.5))
                                            .text_color(colors.muted_foreground)
                                            .child(cx.t("settings.git.automationHint")),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(|| "settings-git-workspace".into())
                                            .mt(px(8.0))
                                            .truncate()
                                            .font_family(cx.theme().fonts.mono.clone())
                                            .text_size(px(11.0))
                                            .text_color(colors.muted_foreground)
                                            .child(shown),
                                    ),
                            ),
                    )
                    .child(
                        Button::new("settings-git-automation")
                            .size(ButtonSize::Sm)
                            .disabled(!has_root)
                            .on_click(cx.listener(|settings, _, window, cx| {
                                settings.open_automation(window, cx)
                            }))
                            .child(cx.t("automation.open")),
                    ),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .border_t_1()
                    .border_color(colors.border.opacity(0.5))
                    .pt(px(12.0))
                    .text_size(px(11.0))
                    .line_height(px(17.9))
                    .text_color(colors.muted_foreground)
                    .child(div().child(cx.t("settings.git.sharingHint")))
                    .child(div().mt(px(4.0)).child(cx.t("automation.noPolling"))),
            )
            .into_any_element()
    }
}
