//! The configuration tab: the status card ("Status do serviço" with the
//! mode and what it means), the settings card (mode, server, token, the
//! advanced namespaces and folder, "Salvar configuração" and "Reiniciar DCC
//! agora") and the sidecar's folders, log, version and endpoint to copy.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use super::super::decision::field;
use super::StatusState;
use crate::data::ai_memory_settings::MODES;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster, Tracked, spinner};

fn small(text: impl Into<SharedString>, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(11.0))
        .line_height(px(17.9))
        .text_color(cx.theme().colors.muted_foreground)
        .child(text.into())
        .into_any_element()
}

fn label(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.foreground)
        .child(text)
        .into_any_element()
}

/// `text-[11px] font-medium uppercase tracking-[0.08em]`
fn eyebrow(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.muted_foreground)
        .child(Tracked::new(text.to_uppercase(), 11.0, 0.08))
        .into_any_element()
}

fn labelled(title: SharedString, body: AnyElement, cx: &gpui::App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .min_w_0()
        .child(label(title, cx))
        .child(body)
        .into_any_element()
}

impl Settings {
    fn memory_status_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let memory = &self.memory;
        let (mode, healthy, body, problem) = match &memory.status {
            StatusState::Loading => (
                "unknown".to_string(),
                false,
                cx.t("settings.aiMemory.loading"),
                None,
            ),
            StatusState::Failed => (
                "unknown".to_string(),
                false,
                cx.t("settings.aiMemory.error"),
                None,
            ),
            StatusState::Ready(status) => (
                status.mode.clone(),
                status.healthy(),
                cx.t(&format!("settings.aiMemory.statusBody.{}", status.mode)),
                status.message.clone(),
            ),
        };
        let loading = memory.status_loading;
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
                                    .child(Icon::new("database").size(16.0).color(colors.primary)),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(colors.foreground)
                                            .child(cx.t("settings.aiMemory.title")),
                                    )
                                    .child(
                                        div()
                                            .mt(px(4.0))
                                            .text_size(px(12.0))
                                            .line_height(px(19.5))
                                            .text_color(colors.muted_foreground)
                                            .child(cx.t("settings.aiMemory.hint")),
                                    )
                                    .child(
                                        div()
                                            .mt(px(8.0))
                                            .text_size(px(11.0))
                                            .line_height(px(17.9))
                                            .text_color(colors.muted_foreground.opacity(0.9))
                                            .child(cx.t("settings.aiMemory.operationsHint")),
                                    ),
                            ),
                    )
                    .child(
                        Button::new("settings-memory-refresh-status")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .disabled(loading)
                            .on_click(
                                cx.listener(|settings, _, _, cx| settings.load_memory_status(cx)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(if loading {
                                        spinner("settings-memory-status-loading", 14.0)
                                            .into_any_element()
                                    } else {
                                        Icon::new("refresh-cw").size(14.0).into_any_element()
                                    })
                                    .child(cx.t("settings.aiMemory.refresh")),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "settings-memory-status".into())
                    .mt(px(16.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.5))
                    .bg(colors.background)
                    .p(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(if healthy {
                                        Icon::new("circle-check")
                                            .size(16.0)
                                            .color(tw::EMERALD_500.hsla())
                                    } else {
                                        Icon::new("circle-alert")
                                            .size(16.0)
                                            .color(tw::AMBER_500.hsla())
                                    })
                                    .child(cx.t("settings.aiMemory.status")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .h(px(28.0))
                                    .px(px(10.0))
                                    .rounded(px(26.0))
                                    .border_1()
                                    .border_color(colors.border)
                                    .text_size(px(11.0))
                                    .text_color(colors.foreground)
                                    .child(cx.t(&format!("settings.aiMemory.modes.{mode}"))),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(colors.muted_foreground)
                            .child(body),
                    )
                    .children(problem.map(|problem| {
                        div()
                            .mt(px(8.0))
                            .text_size(px(11.0))
                            .line_height(px(17.9))
                            .text_color(colors.destructive)
                            .child(problem)
                    })),
            )
            .into_any_element()
    }

    fn memory_settings_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let memory = &self.memory;
        let modes = MODES.iter().map(|mode| {
            let selected = memory.draft.mode == *mode;
            let mode = *mode;
            div()
                .id(SharedString::from(format!("settings-memory-mode-{mode}")))
                .debug_selector(move || format!("settings-memory-mode-{mode}"))
                .flex()
                .items_center()
                .h(px(32.0))
                .px(px(12.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .text_size(px(12.0))
                .text_color(colors.foreground)
                .cursor_pointer()
                .when(selected, |pill| pill.bg(colors.accent))
                .when(!selected, |pill| pill.hover(|pill| pill.bg(colors.muted)))
                .on_click(cx.listener(move |settings, _, _, cx| settings.set_memory_mode(mode, cx)))
                .child(cx.t(&format!("settings.aiMemory.modes.{mode}")))
        });
        // `disabled` unless remote: the value shows, faded, and cannot be typed.
        let remote = memory.draft.mode == "remote";
        let url = if remote {
            field("settings-memory-url", &memory.base_url, cx)
        } else {
            let text = memory.draft.base_url.clone().unwrap_or_default();
            div()
                .debug_selector(|| "settings-memory-url".into())
                .flex()
                .items_center()
                .w_full()
                .h(px(32.0))
                .px(px(12.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(colors.input)
                .opacity(0.5)
                .cursor_not_allowed()
                .text_size(px(14.0))
                .text_color(if text.is_empty() {
                    colors.muted_foreground
                } else {
                    colors.foreground
                })
                .child(if text.is_empty() {
                    "http://127.0.0.1:49374".to_string()
                } else {
                    text
                })
                .into_any_element()
        };
        let open = memory.advanced_open;
        let advanced = div()
            .rounded(px(10.0))
            .border_1()
            .border_color(colors.border.opacity(0.5))
            .bg(colors.muted.opacity(0.1))
            .px(px(12.0))
            .py(px(8.0))
            .child(
                div()
                    .id("settings-memory-advanced")
                    .debug_selector(|| "settings-memory-advanced".into())
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .on_click(cx.listener(|settings, _, _, cx| {
                        settings.memory.advanced_open = !settings.memory.advanced_open;
                        cx.notify();
                    }))
                    .child(
                        Icon::new(if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        })
                        .size(12.0),
                    )
                    .child(label(cx.t("settings.aiMemory.advancedTitle"), cx)),
            )
            .when(open, |details| {
                details
                    .child(
                        div()
                            .mt(px(8.0))
                            .child(small(cx.t("settings.aiMemory.advancedHint"), cx)),
                    )
                    .child(
                        div().mt(px(8.0)).child(
                            div()
                                .text_size(px(11.0))
                                .line_height(px(17.9))
                                .text_color(colors.muted_foreground.opacity(0.9))
                                .child(cx.t("settings.aiMemory.technicalHint")),
                        ),
                    )
                    .child(
                        div()
                            .mt(px(12.0))
                            .grid()
                            .grid_cols(2)
                            .gap(px(16.0))
                            .child(labelled(
                                cx.t("settings.aiMemory.workspaceLabel"),
                                field("settings-memory-workspace", &memory.workspace, cx),
                                cx,
                            ))
                            .child(labelled(
                                cx.t("settings.aiMemory.projectLabel"),
                                field("settings-memory-project", &memory.project, cx),
                                cx,
                            ))
                            .child(div().col_span(2).child(labelled(
                                cx.t("settings.aiMemory.dataDir"),
                                field("settings-memory-data-dir", &memory.data_dir, cx),
                                cx,
                            ))),
                    )
            });
        let saving = memory.saving;
        div()
            .debug_selector(|| "settings-memory-configuration".into())
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .p(px(16.0))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(16.0))
                    .child(
                        div()
                            .col_span(2)
                            .child(eyebrow(cx.t("settings.aiMemory.modeLabel"), cx))
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(4.0))
                                    .children(modes),
                            ),
                    )
                    .child(labelled(cx.t("settings.aiMemory.url"), url, cx))
                    .child(labelled(
                        cx.t("settings.aiMemory.tokenLabel"),
                        field("settings-memory-token", &memory.token, cx),
                        cx,
                    ))
                    .child(
                        div()
                            .col_span(2)
                            .child(small(cx.t("settings.aiMemory.connectionHint"), cx)),
                    )
                    .child(div().col_span(2).child(advanced)),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .border_t_1()
                    .border_color(colors.border.opacity(0.4))
                    .pt(px(12.0))
                    .child(small(cx.t("settings.aiMemory.settingsHint"), cx))
                    .child(
                        Button::new("settings-memory-save")
                            .size(ButtonSize::Sm)
                            .disabled(saving)
                            .on_click(cx.listener(|settings, _, _, cx| settings.save_memory(cx)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .children(
                                        saving.then(|| spinner("settings-memory-saving", 14.0)),
                                    )
                                    .child(cx.t("settings.aiMemory.save")),
                            ),
                    )
                    .when(memory.restart_required, |row| {
                        row.child(
                            Button::new("settings-memory-restart")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .on_click(
                                    cx.listener(|settings, _, _, cx| settings.restart_desktop(cx)),
                                )
                                .child(cx.t("settings.aiMemory.restartNow")),
                        )
                    }),
            )
            .into_any_element()
    }

    fn memory_facts(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let StatusState::Ready(status) = &self.memory.status else {
            return None;
        };
        let colors = cx.theme().colors.clone();
        let facts = [
            (
                "settings.aiMemory.dataDir",
                status.data_dir.clone(),
                "data-dir",
            ),
            ("settings.aiMemory.logPath", status.log_path.clone(), "log"),
            (
                "settings.aiMemory.version",
                status.version.clone(),
                "version",
            ),
            ("settings.aiMemory.url", status.url.clone(), "url"),
        ]
        .into_iter()
        .map(|(key, value, id)| {
            let copied = value.clone();
            div()
                .rounded(px(14.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .p(px(12.0))
                .child(eyebrow(cx.t(key), cx))
                .child(
                    div()
                        .mt(px(8.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_family(cx.theme().fonts.mono.clone())
                                .text_size(px(11.0))
                                .text_color(colors.foreground)
                                .child(value.unwrap_or_else(|| {
                                    cx.t("settings.aiMemory.notAvailable").to_string()
                                })),
                        )
                        .children(copied.map(|value| {
                            Button::new(SharedString::from(format!("settings-memory-copy-{id}")))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::IconSm)
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        value.clone(),
                                    ));
                                    Toaster::success(cx.t("settings.aiMemory.copied"), cx);
                                }))
                                .child(Icon::new("copy").size(14.0))
                        })),
                )
        });
        Some(
            div()
                .grid()
                .grid_cols(2)
                .gap(px(12.0))
                .children(facts)
                .into_any_element(),
        )
    }

    pub(super) fn memory_configuration(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.memory_status_card(cx))
            .child(self.memory_settings_card(cx))
            .children(self.memory_facts(cx))
            .into_any_element()
    }
}
