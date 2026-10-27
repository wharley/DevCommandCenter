//! "Configuração avançada" (`ProviderRuntimeFields`): the executable, the
//! homes and the subagent limit the adapter honors, saved as typed, and
//! "Redefinir" when something is set.

use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{AnyElement, Context, Entity, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use crate::data::provider_runtime::SUBAGENT_OPTIONS;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::text_input::TextInput;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Menu, MenuEntry, MenuItem};

impl Settings {
    pub(super) fn advanced(
        &self,
        provider: &ProviderDescriptor,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let draft = self.providers.runtime.draft(&provider.id.0);
        let customized = draft.customized();
        let open = self.providers.advanced_open;
        let header = div()
            .id("settings-provider-advanced")
            .debug_selector(|| "settings-provider-advanced".into())
            .flex()
            .items_center()
            .gap(px(8.0))
            .w_full()
            .cursor_pointer()
            .on_click(cx.listener(|settings, _, _, cx| {
                settings.providers.advanced_open = !settings.providers.advanced_open;
                cx.notify();
            }))
            .child(
                Icon::new(if open {
                    "chevron-down"
                } else {
                    "chevron-right"
                })
                .size(14.0)
                .color(colors.muted_foreground),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(cx.t("settings.model.advancedTitle")),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t(if customized {
                        "settings.model.advancedCustomized"
                    } else {
                        "settings.model.advancedHint"
                    })),
            );
        let capabilities = &provider.capabilities;
        let fields = open.then(|| {
            let binary = capabilities.supports_runtime_binary.then(|| {
                self.field(
                    "binary",
                    cx.t("settings.model.runtimeBinaryPath"),
                    &self.providers.binary,
                    Some(cx.t("settings.model.runtimeBinaryHint")),
                    cx,
                )
            });
            let home = capabilities.supports_runtime_home.then(|| {
                self.field(
                    "home",
                    cx.t("settings.model.runtimeHomePath"),
                    &self.providers.home,
                    None,
                    cx,
                )
            });
            let shadow = capabilities.supports_shadow_home.then(|| {
                self.field(
                    "shadow-home",
                    cx.t("settings.model.runtimeShadowHomePath"),
                    &self.providers.shadow_home,
                    None,
                    cx,
                )
            });
            let subagents = capabilities
                .supports_subagent_concurrency
                .then(|| self.subagents(&draft.max_concurrent_subagents, cx));
            let reset = customized.then(|| {
                Button::new("settings-provider-runtime-reset")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .height(28.0)
                    .padding_x(8.0)
                    .text_size(12.0)
                    .text_color(colors.muted_foreground)
                    .on_click(cx.listener(|settings, _, _, cx| settings.clear_runtime(cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(Icon::new("refresh-ccw").size(14.0))
                            .child(cx.t("settings.model.runtimeReset")),
                    )
            });
            div()
                .mt(px(16.0))
                .max_w(px(512.0))
                .pl(px(20.0))
                .flex()
                .flex_col()
                .items_start()
                .gap(px(16.0))
                .children(binary)
                .children(home)
                .children(shadow)
                .children(subagents)
                .children(reset)
        });
        div()
            .border_t_1()
            .border_color(colors.border.opacity(0.5))
            .px(px(24.0))
            .py(px(16.0))
            .child(header)
            .children(fields)
            .into_any_element()
    }

    fn field(
        &self,
        id: &'static str,
        label: SharedString,
        input: &Entity<TextInput>,
        hint: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(label),
            )
            .child(
                div()
                    .debug_selector(move || format!("settings-provider-{id}"))
                    .flex()
                    .items_center()
                    .h(px(32.0))
                    .px(px(12.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(colors.input)
                    .text_size(px(12.0))
                    .child(input.clone()),
            )
            .children(hint.map(|hint| {
                div()
                    .text_size(px(11.0))
                    .line_height(px(17.9))
                    .text_color(colors.muted_foreground)
                    .child(hint)
            }))
            .into_any_element()
    }

    /// The `<select>` of subagents: Automático, then the options.
    fn subagents(&self, value: &str, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let auto = cx.t("settings.model.runtimeSubagentConcurrencyAuto");
        let label: SharedString = if value.is_empty() {
            auto.clone()
        } else {
            value.to_string().into()
        };
        let this = cx.entity().downgrade();
        let trigger_colors = colors.clone();
        let menu = Menu::dropdown(
            "settings-provider-subagents",
            move |open| {
                div()
                    .debug_selector(|| "settings-provider-subagents".into())
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .h(px(32.0))
                    .px(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(if open {
                        trigger_colors.ring
                    } else {
                        trigger_colors.input
                    })
                    .bg(trigger_colors.background)
                    .text_size(px(12.0))
                    .text_color(trigger_colors.foreground)
                    .child(label)
                    .child(
                        Icon::new("chevron-down")
                            .size(14.0)
                            .color(trigger_colors.muted_foreground),
                    )
                    .into_any_element()
            },
            move |_, _| {
                std::iter::once((String::new(), auto.clone()))
                    .chain(
                        SUBAGENT_OPTIONS
                            .iter()
                            .map(|option| (option.to_string(), option.to_string().into())),
                    )
                    .map(|(value, label)| {
                        let this = this.clone();
                        MenuEntry::Item(MenuItem::new(label, move |_, cx| {
                            let value = value.clone();
                            let _ =
                                this.update(cx, |settings, cx| settings.set_subagents(value, cx));
                        }))
                    })
                    .collect()
            },
        )
        .min_width(160.0);
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(cx.t("settings.model.runtimeSubagentConcurrency")),
            )
            .child(div().w_full().child(menu))
            .child(
                div()
                    .text_size(px(11.0))
                    .line_height(px(17.9))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("settings.model.runtimeSubagentConcurrencyHint")),
            )
            .into_any_element()
    }
}
