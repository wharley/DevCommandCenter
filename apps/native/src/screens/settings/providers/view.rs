//! The list and the chosen provider's detail (`providers-settings.tsx`).

use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use super::super::Settings;
use crate::data::provider_text::{self, Tone};
use crate::i18n::{Args, Translate};
use crate::project::ProviderIcon;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Icon, Switch, card_shadow};

/// `max-w-[60ch]` at 12px.
const MEASURE: f32 = 432.0;

fn tone_color(tone: Tone, muted: Hsla) -> Hsla {
    match tone {
        Tone::Ready => tw::EMERALD_500.hsla(),
        Tone::Attention => tw::AMBER_500.hsla(),
        Tone::Off => muted.opacity(0.35),
    }
}

/// The catalog's text in the person's language when it is a known one.
fn said(cx: &gpui::App, key: Option<String>, fallback: &str) -> SharedString {
    match key {
        Some(key) if cx.global::<crate::i18n::I18n>().has(&key, &Args::default()) => cx.t(&key),
        _ => fallback.to_string().into(),
    }
}

impl Settings {
    pub(in super::super) fn providers_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let Some(current) = self.providers.current().cloned() else {
            return div()
                .rounded(px(14.0))
                .border_1()
                .border_dashed()
                .border_color(colors.border.opacity(0.6))
                .p(px(24.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t(if self.providers.catalog.is_some() {
                    "settings.model.runtimeNoSupportedProviders"
                } else {
                    "settings.model.usageLoading"
                }))
                .into_any_element();
        };
        let catalog = self.providers.catalog.clone().unwrap_or_default();
        let rail = catalog
            .iter()
            .map(|provider| self.rail_item(provider, provider.id == current.id, cx))
            .collect::<Vec<_>>();
        div()
            .debug_selector(|| "settings-providers".into())
            .flex()
            .min_h(px(520.0))
            .overflow_hidden()
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background)
            .child(
                div()
                    .flex_none()
                    .w(px(216.0))
                    .border_r_1()
                    .border_color(colors.border.opacity(0.6))
                    .bg(colors.muted.opacity(0.25))
                    .p(px(8.0))
                    .child(div().flex().flex_col().gap(px(4.0)).children(rail)),
            )
            .child(self.provider_detail(&current, cx))
            .into_any_element()
    }

    fn rail_item(
        &self,
        provider: &ProviderDescriptor,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let status = provider_text::rail_status(provider, 0);
        let line = match status.count {
            Some(count) => cx.t_with(status.key, &Args::count(count)),
            None => cx.t(status.key),
        };
        let id = provider.id.0.clone();
        let selector = format!("settings-provider-{id}");
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .flex()
            .items_center()
            .gap(px(10.0))
            .rounded(px(10.0))
            .px(px(10.0))
            .py(px(8.0))
            .cursor_pointer()
            .map(|item| {
                if active {
                    item.bg(colors.background)
                        .shadow(card_shadow())
                        .border_1()
                        .border_color(colors.border.opacity(0.6))
                } else {
                    item.border_1()
                        .border_color(gpui::transparent_black())
                        .hover(|item| item.bg(colors.background.opacity(0.6)))
                }
            })
            .on_click(
                cx.listener(move |settings, _, _, cx| settings.choose_provider(id.clone(), cx)),
            )
            .child(
                ProviderIcon::new(&provider.id.0)
                    .size(16.0)
                    .opacity(if provider.enabled { 1.0 } else { 0.45 }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .when(active, |label| {
                                label
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                            })
                            .when(!active, |label| {
                                label.text_color(colors.foreground.opacity(0.85))
                            })
                            .child(provider.label.clone()),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(
                                div()
                                    .flex_none()
                                    .size(px(6.0))
                                    .rounded_full()
                                    .bg(tone_color(status.tone, colors.muted_foreground)),
                            )
                            .child(div().truncate().child(line)),
                    ),
            )
            .into_any_element()
    }

    fn provider_detail(&self, provider: &ProviderDescriptor, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let amber = match cx.theme().appearance {
            Appearance::Light => tw::AMBER_600.hsla(),
            Appearance::Dark => tw::AMBER_400.hsla(),
        };
        let id = provider.id.0.clone();
        let enabled = provider.enabled;
        let pending = self.providers.pending.contains(&id);
        let checking = self.providers.checking;
        let description = said(
            cx,
            provider_text::provider_description_key(&id),
            &provider.description,
        );
        let problem = provider_text::health_reason(provider)
            .filter(|_| enabled)
            .map(|reason| match provider_text::setup_message(reason) {
                Some(message) => {
                    let mut args = Args::default();
                    if let Some(command) = message.command {
                        args = args.with("command", command);
                    }
                    if let Some(binary) = message.binary {
                        args = args.with("binary", binary);
                    }
                    cx.t_with(message.key, &args)
                }
                None => reason.to_string().into(),
            });
        let toggle_id = id.clone();
        let header = div()
            .flex()
            .items_start()
            .gap(px(12.0))
            .px(px(24.0))
            .pt(px(24.0))
            .pb(px(20.0))
            .child(div().mt(px(2.0)).child(ProviderIcon::new(&id).size(20.0)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .child(provider.label.clone()),
                            )
                            .when(!provider.stable, |row| {
                                row.child(
                                    div()
                                        .flex_none()
                                        .text_size(px(11.0))
                                        .text_color(colors.muted_foreground)
                                        .child(cx.t("settings.model.experimental")),
                                )
                            }),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .max_w(px(MEASURE))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(colors.muted_foreground)
                            .child(description),
                    )
                    .children(problem.map(|problem| {
                        div()
                            .debug_selector(|| "settings-provider-problem".into())
                            .mt(px(8.0))
                            .max_w(px(MEASURE))
                            .flex()
                            .flex_wrap()
                            .gap_x(px(4.0))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(amber)
                            .child(problem)
                            .child(
                                div()
                                    .id("settings-provider-recheck")
                                    .debug_selector(|| "settings-provider-recheck".into())
                                    .font_weight(FontWeight::MEDIUM)
                                    .map(|link| {
                                        if checking {
                                            link.text_color(colors.muted_foreground)
                                        } else {
                                            link.text_color(colors.foreground)
                                                .cursor_pointer()
                                                .hover(|link| link.underline())
                                                .on_click(cx.listener(|settings, _, _, cx| {
                                                    settings.recheck_providers(cx)
                                                }))
                                        }
                                    })
                                    .child(cx.t(if checking {
                                        "settings.model.rechecking"
                                    } else {
                                        "settings.model.recheck"
                                    })),
                            )
                    }))
                    .when(!enabled, |column| {
                        column.child(
                            div()
                                .mt(px(8.0))
                                .max_w(px(MEASURE))
                                .text_size(px(12.0))
                                .line_height(px(19.5))
                                .text_color(colors.muted_foreground)
                                .child(cx.t("settings.model.availabilityHint")),
                        )
                    })
                    .children(self.providers.errors.get(&id).map(|error| {
                        div()
                            .mt(px(8.0))
                            .text_size(px(12.0))
                            .text_color(colors.destructive)
                            .child(error.clone())
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pt(px(2.0))
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t(if pending {
                        "settings.model.pending"
                    } else if enabled {
                        "settings.model.enabled"
                    } else {
                        "settings.model.disabled"
                    }))
                    .child(
                        Switch::new("settings-provider-enabled", enabled)
                            .disabled(pending)
                            .on_toggle({
                                let this = cx.entity().downgrade();
                                move |on, _, cx| {
                                    let id = toggle_id.clone();
                                    let _ = this.update(cx, |settings, cx| {
                                        settings.set_availability(id, on, cx)
                                    });
                                }
                            }),
                    ),
            );
        div()
            .flex_1()
            .min_w_0()
            .child(header)
            .child(detail_section(
                cx.t("settings.model.modelsTitle"),
                Some(cx.t("settings.model.modelsHint")),
                self.model_list(provider, cx),
                cx,
            ))
            .children(
                crate::data::provider_runtime::supports_runtime(&provider.capabilities)
                    .then(|| self.advanced(provider, cx)),
            )
            .into_any_element()
    }

    fn model_list(&self, provider: &ProviderDescriptor, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        if provider.models.is_empty() {
            return div()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("settings.model.modelsEmpty"))
                .into_any_element();
        }
        let enabled = provider.enabled;
        let rows = provider.models.iter().enumerate().map(|(index, model)| {
            let selected =
                self.providers.pick.as_ref().is_some_and(|pick| {
                    pick.provider_id == provider.id.0 && pick.model_id == model.id
                });
            let (provider_id, model_id) = (provider.id.0.clone(), model.id.clone());
            let selector = format!("settings-model-{}", model.id);
            let description = (!model.description.trim().is_empty()).then(|| {
                said(
                    cx,
                    provider_text::model_description_key(&model.description),
                    &model.description,
                )
            });
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .flex()
                .items_center()
                .gap(px(12.0))
                .px(px(12.0))
                .py(px(8.0))
                .when(index > 0, |row| {
                    row.border_t_1().border_color(colors.border.opacity(0.5))
                })
                .when(selected, |row| row.bg(colors.accent.opacity(0.5)))
                .map(|row| {
                    if enabled {
                        row.cursor_pointer()
                            .hover(|row| row.bg(colors.accent.opacity(0.4)))
                            .on_click(cx.listener(move |settings, _, _, cx| {
                                settings.pick_model(provider_id.clone(), model_id.clone(), cx)
                            }))
                    } else {
                        row.opacity(0.6).cursor_not_allowed()
                    }
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(13.0))
                                        .text_color(colors.foreground)
                                        .child(model.label.clone()),
                                )
                                .when(model.recommended, |line| {
                                    line.child(
                                        div()
                                            .flex_none()
                                            .text_size(px(11.0))
                                            .text_color(colors.muted_foreground)
                                            .child(cx.t("settings.model.modelRecommended")),
                                    )
                                }),
                        )
                        .children(description.map(|description| {
                            div()
                                .mt(px(2.0))
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(colors.muted_foreground)
                                .child(description)
                        })),
                )
                .when(selected, |row| {
                    row.child(Icon::new("check").size(16.0).color(colors.foreground))
                })
        });
        div()
            .overflow_hidden()
            .rounded(px(10.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background)
            .children(rows)
            .into_any_element()
    }
}

/// `DetailSection`
pub(super) fn detail_section(
    title: SharedString,
    hint: Option<SharedString>,
    content: AnyElement,
    cx: &gpui::App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .border_t_1()
        .border_color(colors.border.opacity(0.5))
        .px(px(24.0))
        .py(px(20.0))
        .child(
            div()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(title),
                )
                .children(hint.map(|hint| {
                    div()
                        .mt(px(2.0))
                        .max_w(px(MEASURE))
                        .text_size(px(12.0))
                        .line_height(px(19.5))
                        .text_color(colors.muted_foreground)
                        .child(hint)
                })),
        )
        .child(content)
        .into_any_element()
}
