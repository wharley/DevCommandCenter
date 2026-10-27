//! The list: the "Avançado" toggle, the header with "Adicionar", what the
//! session's provider supports, reports for servers no longer here, and
//! each server's card (its switch, removal and status in the session).

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::super::super::Settings;
use super::Panel;
use super::parts::{badges, card, credential_badge, labelled, text, title, warning_card};
use crate::data::connectors::Integration;
use crate::data::mcp_advanced::{orphans, scope_of, support_kind, transport_preview};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon, Switch, spinner};

impl Settings {
    /// The toggle at the panel's foot and, open, the advanced panel.
    pub(in super::super::super) fn advanced_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let open = self.connectors.advanced.open;
        div()
            .border_t_1()
            .border_color(colors.border.opacity(0.5))
            .pt(px(16.0))
            .child(
                div()
                    .id("settings-connectors-advanced")
                    .debug_selector(|| "settings-connectors-advanced".into())
                    .flex()
                    .w_full()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .on_click(cx.listener(|settings, _, _, cx| settings.toggle_advanced(cx)))
                    .child(
                        Icon::new(if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        })
                        .size(16.0)
                        .color(colors.muted_foreground),
                    )
                    .child(title(12.0, cx.t("settings.connectors.advanced.title"), cx))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("settings.connectors.advanced.hint")),
                    ),
            )
            .when(open, |section| {
                section.child(
                    div()
                        .debug_selector(|| "settings-mcp-advanced".into())
                        .mt(px(16.0))
                        .child(match &self.connectors.advanced.panel {
                            Panel::List => self.advanced_list(cx),
                            Panel::Create => self.advanced_form(cx),
                            Panel::Review(integration) => self.advanced_review(integration, cx),
                        }),
                )
            })
            .into_any_element()
    }

    fn add_button(&self, id: &'static str, cx: &mut Context<Self>) -> AnyElement {
        Button::new(id)
            .size(ButtonSize::Sm)
            .text_size(12.0)
            .on_click(
                cx.listener(|settings, _, window, cx| settings.open_advanced_create(window, cx)),
            )
            .child(labelled(
                Icon::new("plus").size(14.0).into_any_element(),
                cx.t("settings.integrations.add"),
            ))
            .into_any_element()
    }

    fn compatibility(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let session = &self.session;
        let provider = session.provider_id.as_deref().and_then(|id| {
            self.providers
                .catalog
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find(|provider| provider.id.0 == id)
        });
        let support = provider.map(|provider| &provider.capabilities.mcp_support);
        let (kind, version) = support_kind(support);
        let label = provider
            .map(|provider| provider.label.clone())
            .or(session.provider_id.clone())
            .unwrap_or_default();
        let known = session.session_id.is_some() && session.provider_id.is_some();
        let line = if known {
            cx.t_with(
                &format!("settings.integrations.compatibility.{kind}"),
                &Args::default()
                    .with("provider", label.clone())
                    .with("version", version.unwrap_or_default()),
            )
        } else {
            cx.t("settings.integrations.compatibility.noSession")
        };
        let variant = match kind {
            "verified" => BadgeVariant::Success,
            "runtime" => BadgeVariant::Warn,
            "unsupported" => BadgeVariant::Destructive,
            _ => BadgeVariant::Outline,
        };
        let snapshot = match &self.connectors.advanced.statuses {
            None => cx.t("settings.integrations.runtimeLoading"),
            Some(Err(_)) => cx.t("settings.integrations.runtimeLoadError"),
            Some(Ok(statuses)) => cx.t_with(
                "settings.integrations.runtimeSnapshot",
                &Args::count(statuses.len() as i64),
            ),
        };
        card(Some(colors.background), cx)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
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
                                    .size(px(32.0))
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(8.0))
                                    .bg(colors.muted)
                                    .child(
                                        Icon::new("activity")
                                            .size(16.0)
                                            .color(colors.muted_foreground),
                                    ),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .child(title(
                                        13.0,
                                        cx.t("settings.integrations.compatibilityTitle"),
                                        cx,
                                    ))
                                    .child(text(11.0, true, line, cx).mt(px(4.0))),
                            ),
                    )
                    .when(known, |row| {
                        row.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(6.0))
                                .child(Badge::new(label, BadgeVariant::Outline))
                                .child(Badge::new(
                                    cx.t(&format!(
                                        "settings.integrations.compatibilityBadge.{kind}"
                                    )),
                                    variant,
                                )),
                        )
                    }),
            )
            .when(session.session_id.is_some(), |card| {
                card.child(
                    div()
                        .mt(px(12.0))
                        .border_t_1()
                        .border_color(colors.border.opacity(0.5))
                        .pt(px(12.0))
                        .child(text(11.0, true, snapshot, cx))
                        .child(
                            text(
                                10.0,
                                true,
                                cx.t("settings.integrations.processLifecycleHint"),
                                cx,
                            )
                            .mt(px(6.0)),
                        ),
                )
            })
            .into_any_element()
    }

    fn advanced_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let integrations = self.connectors.list().to_vec();
        let statuses = match &self.connectors.advanced.statuses {
            Some(Ok(statuses)) => statuses.clone(),
            _ => Vec::new(),
        };
        let orphan_count = orphans(&integrations, &statuses).len();
        let header = card(Some(colors.muted.opacity(0.15)), cx)
            .flex()
            .items_start()
            .justify_between()
            .gap(px(12.0))
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
                            .size(px(36.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .bg(colors.primary.opacity(0.1))
                            .child(Icon::new("braces").size(16.0).color(colors.primary)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .child(title(14.0, cx.t("settings.integrations.title"), cx))
                            .child(
                                text(12.0, true, cx.t("settings.integrations.hint"), cx)
                                    .mt(px(4.0)),
                            )
                            .child(
                                text(11.0, true, cx.t("settings.integrations.providerCopy"), cx)
                                    .mt(px(8.0)),
                            ),
                    ),
            )
            .child(self.add_button("settings-mcp-add", cx));
        let orphan = (orphan_count > 0).then(|| {
            warning_card(cx)
                .flex()
                .items_start()
                .gap(px(12.0))
                .child(
                    div()
                        .mt(px(2.0))
                        .child(Icon::new("triangle-alert").size(16.0).color(colors.warning)),
                )
                .child(
                    div()
                        .child(title(
                            12.0,
                            cx.t_with(
                                "settings.integrations.orphanRuntimeTitle",
                                &Args::count(orphan_count as i64),
                            ),
                            cx,
                        ))
                        .child(
                            text(
                                11.0,
                                true,
                                cx.t("settings.integrations.orphanRuntimeHint"),
                                cx,
                            )
                            .mt(px(4.0)),
                        ),
                )
        });
        let body = match &self.connectors.integrations {
            None => card(None, cx)
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .py(px(40.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(spinner("settings-mcp-loading", 16.0))
                .child(cx.t("settings.integrations.loading"))
                .into_any_element(),
            Some(Err(_)) => div()
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.destructive.opacity(0.3))
                .bg(colors.destructive.opacity(0.05))
                .p(px(16.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(colors.destructive)
                        .child(cx.t("settings.integrations.errors.loading")),
                )
                .child(
                    div().mt(px(12.0)).child(
                        Button::new("settings-mcp-retry")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .text_size(12.0)
                            .on_click(
                                cx.listener(|settings, _, _, cx| settings.load_connectors(cx)),
                            )
                            .child(cx.t("settings.integrations.retry")),
                    ),
                )
                .into_any_element(),
            Some(Ok(list)) if list.is_empty() => div()
                .flex()
                .flex_col()
                .items_center()
                .rounded(px(12.0))
                .border_1()
                .border_dashed()
                .border_color(colors.border.opacity(0.6))
                .px(px(20.0))
                .py(px(40.0))
                .child(
                    Icon::new("server")
                        .size(24.0)
                        .color(colors.muted_foreground),
                )
                .child(title(13.0, cx.t("settings.integrations.emptyTitle"), cx).mt(px(12.0)))
                .child(
                    text(12.0, true, cx.t("settings.integrations.emptyHint"), cx)
                        .mt(px(4.0))
                        .max_w(px(448.0))
                        .text_center(),
                )
                .child(
                    div()
                        .mt(px(16.0))
                        .child(self.add_button("settings-mcp-add-empty", cx)),
                )
                .into_any_element(),
            Some(Ok(list)) => div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .children(
                    list.iter()
                        .map(|integration| self.integration_card(integration, &statuses, cx)),
                )
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(header)
            .child(self.compatibility(cx))
            .children(orphan)
            .child(body)
            .into_any_element()
    }

    fn integration_card(
        &self,
        integration: &Integration,
        statuses: &[crate::data::mcp_advanced::RuntimeStatus],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let definition = &integration.definition;
        let id = definition.id.clone();
        let (_, target) = scope_of(integration);
        let needs_trust = integration.needs_trust();
        let advanced = &self.connectors.advanced;
        let idle = advanced.busy.is_none();
        let (state, variant) = if definition.enabled {
            ("settings.integrations.enabled", BadgeVariant::Success)
        } else if needs_trust {
            ("settings.integrations.needsTrust", BadgeVariant::Warn)
        } else {
            ("settings.integrations.disabled", BadgeVariant::Outline)
        };
        let preview = transport_preview(integration);
        let switch_target = integration.clone();
        let enabled = definition.enabled;
        let remove_id = id.clone();
        div()
            .debug_selector({
                let id = id.clone();
                move || format!("settings-mcp-{id}")
            })
            .child(
                card(Some(colors.background), cx)
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                Icon::new(if definition.transport.kind == "http" {
                                                    "earth"
                                                } else {
                                                    "command"
                                                })
                                                .size(16.0)
                                                .color(colors.muted_foreground),
                                            )
                                            .child(title(13.0, definition.display_name.clone(), cx))
                                            .child(Badge::new(cx.t(state), variant)),
                                    )
                                    .child(
                                        div()
                                            .mt(px(8.0))
                                            .max_w(px(672.0))
                                            .truncate()
                                            .font_family(cx.theme().fonts.mono.clone())
                                            .text_size(px(11.0))
                                            .text_color(colors.muted_foreground)
                                            .child(preview),
                                    )
                                    .child(
                                        div()
                                            .mt(px(8.0))
                                            .flex()
                                            .flex_wrap()
                                            .gap(px(6.0))
                                            .children(badges(integration, cx))
                                            .children(target.map(|target| {
                                                div()
                                                    .max_w(px(208.0))
                                                    .overflow_hidden()
                                                    .font_family(cx.theme().fonts.mono.clone())
                                                    .child(Badge::new(
                                                        target.to_string(),
                                                        BadgeVariant::Ghost,
                                                    ))
                                            }))
                                            .children(credential_badge(integration, cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_none()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        Switch::new(
                                            SharedString::from(format!("settings-mcp-switch-{id}")),
                                            enabled,
                                        )
                                        .disabled(!idle)
                                        .on_toggle({
                                            let this = cx.entity().downgrade();
                                            move |on, _, cx| {
                                                let integration = switch_target.clone();
                                                let _ = this.update(cx, |settings, cx| {
                                                    if on {
                                                        settings.connectors.advanced.panel =
                                                            Panel::Review(Box::new(integration));
                                                        cx.notify();
                                                    } else {
                                                        settings.disable_advanced(
                                                            integration.definition.id.clone(),
                                                            cx,
                                                        );
                                                    }
                                                });
                                            }
                                        }),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "settings-mcp-remove-{id}"
                                        )))
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::IconSm)
                                        .disabled(!idle)
                                        .on_click(cx.listener(move |settings, _, _, cx| {
                                            settings.connectors.advanced.remove =
                                                Some(remove_id.clone());
                                            settings.connectors.advanced.delete_credentials = false;
                                            cx.notify();
                                        }))
                                        .child(Icon::new("trash-2").size(14.0)),
                                    ),
                            ),
                    )
                    .child(self.runtime_box(integration, statuses, cx))
                    .children(
                        (advanced.remove.as_deref() == Some(id.as_str()))
                            .then(|| self.remove_confirm(integration, cx)),
                    ),
            )
            .into_any_element()
    }
}
