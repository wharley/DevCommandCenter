//! A server's box under its card: its status in the open session, what to
//! do about it, the tools and their policies (Perguntar, Permitir, Negar),
//! when it was checked; and "Remover integração?" with its credentials.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::super::super::Settings;
use super::parts::{labelled, text, title};
use crate::data::connectors::{Integration, Tool};
use crate::data::mcp_advanced::{
    RuntimeStatus, Tone, policy_summary, runtime_tone, runtime_view, tool_names,
};
use crate::data::time::local_date_time;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon, Switch, spinner};

fn tone(tone: Tone) -> BadgeVariant {
    match tone {
        Tone::Success => BadgeVariant::Success,
        Tone::Destructive => BadgeVariant::Destructive,
        Tone::Warn => BadgeVariant::Warn,
        Tone::Secondary => BadgeVariant::Secondary,
        Tone::Outline => BadgeVariant::Outline,
    }
}

impl Settings {
    pub(super) fn runtime_box(
        &self,
        integration: &Integration,
        statuses: &[RuntimeStatus],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let support = self.session.provider_id.as_deref().and_then(|id| {
            self.providers
                .catalog
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find(|provider| provider.id.0 == id)
                .map(|provider| &provider.capabilities.mcp_support)
        });
        let (kind, status) = runtime_view(integration, statuses, &self.session, support);
        let reported: &[Tool] = status.map_or(&[], |status| status.tools.as_slice());
        let tools = tool_names(integration, reported);
        let hint = match kind.as_str() {
            "restartRequired" => Some("settings.integrations.restartHint"),
            "notReported" => Some("settings.integrations.notReportedHint"),
            _ => None,
        };
        div()
            .mt(px(16.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.border.opacity(0.5))
            .bg(colors.muted.opacity(0.1))
            .p(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(title(11.0, cx.t("settings.integrations.runtimeStatus"), cx))
                            .child(Badge::new(
                                cx.t(&format!("settings.integrations.runtime.{kind}")),
                                tone(runtime_tone(&kind)),
                            )),
                    )
                    .children(status.map(|status| {
                        div()
                            .font_family(cx.theme().fonts.mono.clone())
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child(format!(
                                "{} {}",
                                status.provider_id, status.provider_version
                            ))
                    })),
            )
            .children(hint.map(|hint| text(11.0, true, cx.t(hint), cx).mt(px(8.0))))
            .children(
                status
                    .and_then(|status| status.bounded_error.as_ref())
                    .map(|error| {
                        div()
                            .mt(px(8.0))
                            .text_size(px(11.0))
                            .line_height(px(17.9))
                            .text_color(colors.destructive)
                            .child(error.message.clone())
                    }),
            )
            .when(!tools.is_empty(), |runtime| {
                runtime.child(self.tool_policies(integration, &tools, reported, cx))
            })
            .children(status.map(|status| {
                text(
                    10.0,
                    true,
                    cx.t_with(
                        "settings.integrations.checkedAt",
                        &Args::default().with("date", local_date_time(&status.checked_at)),
                    ),
                    cx,
                )
                .mt(px(8.0))
            }))
            .into_any_element()
    }

    fn tool_policies(
        &self,
        integration: &Integration,
        tools: &[String],
        reported: &[Tool],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let id = integration.definition.id.clone();
        let expanded = self.connectors.advanced.expanded.contains(&id);
        let (total, ask, allow, deny) = policy_summary(integration, tools);
        let count = |key: &str, value: usize| cx.t_with(key, &Args::count(value as i64));
        let toggle_id = id.clone();
        div()
            .mt(px(12.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.5))
            .pt(px(12.0))
            .child(
                div()
                    .id(SharedString::from(format!("settings-mcp-tools-{id}")))
                    .debug_selector({
                        let id = id.clone();
                        move || format!("settings-mcp-tools-{id}")
                    })
                    .flex()
                    .flex_wrap()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |settings, _, _, cx| {
                        let expanded = &mut settings.connectors.advanced.expanded;
                        if !expanded.remove(&toggle_id) {
                            expanded.insert(toggle_id.clone());
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                Icon::new(if expanded {
                                    "chevron-down"
                                } else {
                                    "chevron-right"
                                })
                                .size(16.0)
                                .color(colors.muted_foreground),
                            )
                            .child(title(
                                11.0,
                                cx.t("settings.integrations.toolPoliciesTitle"),
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .justify_end()
                            .gap(px(4.0))
                            .child(Badge::new(
                                count("settings.integrations.toolCount", total),
                                BadgeVariant::Secondary,
                            ))
                            .child(Badge::new(
                                count("settings.integrations.toolAskCount", ask),
                                BadgeVariant::Outline,
                            ))
                            .when(allow > 0, |row| {
                                row.child(Badge::new(
                                    count("settings.integrations.toolAllowCount", allow),
                                    BadgeVariant::Success,
                                ))
                            })
                            .when(deny > 0, |row| {
                                row.child(Badge::new(
                                    count("settings.integrations.toolDenyCount", deny),
                                    BadgeVariant::Destructive,
                                ))
                            }),
                    ),
            )
            .when(expanded, |section| {
                section.child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(text(
                            10.0,
                            true,
                            cx.t("settings.integrations.toolPoliciesHint"),
                            cx,
                        ))
                        .children(tools.iter().map(|tool| {
                            let found = reported.iter().find(|reported| &reported.name == tool);
                            self.advanced_tool_row(integration, tool, found, cx)
                        })),
                )
            })
            .into_any_element()
    }

    fn advanced_tool_row(
        &self,
        integration: &Integration,
        tool: &str,
        reported: Option<&Tool>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let id = integration.definition.id.clone();
        let decision = integration.tool_decision(tool).to_string();
        let busy = self.connectors.advanced.busy.clone();
        let hints = reported.map(Tool::hints).unwrap_or_default();
        let choices = ["ask", "allow", "deny"].into_iter().map(|value| {
            let pending = busy.as_deref() == Some(format!("policy:{id}:{tool}:{value}").as_str());
            let (connector, name) = (id.clone(), tool.to_string());
            div()
                .id(SharedString::from(format!(
                    "settings-mcp-policy-{tool}-{value}"
                )))
                .debug_selector({
                    let tool = tool.to_string();
                    move || format!("settings-mcp-policy-{tool}-{value}")
                })
                .flex()
                .items_center()
                .justify_center()
                .h(px(28.0))
                .min_w(px(56.0))
                .px(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(colors.border.opacity(0.5))
                .text_size(px(10.0))
                .when(decision == value, |choice| choice.bg(colors.muted))
                .when(busy.is_none(), |choice| {
                    choice
                        .cursor_pointer()
                        .hover(|choice| choice.bg(colors.muted))
                        .on_click(cx.listener(move |settings, _, _, cx| {
                            settings.set_advanced_policy(connector.clone(), name.clone(), value, cx)
                        }))
                })
                .child(if pending {
                    spinner(
                        SharedString::from(format!("settings-mcp-policy-busy-{value}")),
                        12.0,
                    )
                    .into_any_element()
                } else {
                    cx.t(&format!("settings.integrations.toolPolicy.{value}"))
                        .into_any_element()
                })
        });
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(colors.border.opacity(0.4))
            .bg(colors.background)
            .px(px(10.0))
            .py(px(8.0))
            .child(
                div()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_family(cx.theme().fonts.mono.clone())
                            .text_size(px(10.0))
                            .text_color(colors.foreground)
                            .child(tool.to_string()),
                    )
                    .when(!hints.is_empty(), |column| {
                        column.child(div().mt(px(6.0)).flex().flex_wrap().gap(px(4.0)).children(
                            hints.into_iter().map(|hint| {
                                Badge::new(
                                    cx.t(&format!("settings.integrations.toolAnnotation.{hint}")),
                                    BadgeVariant::Outline,
                                )
                                .height(20.0)
                                .padding_x(6.0)
                                .text_size(8.0)
                                .weight(gpui::FontWeight::NORMAL)
                            }),
                        ))
                    }),
            )
            .child(div().flex().flex_none().gap(px(4.0)).children(choices))
            .into_any_element()
    }

    pub(super) fn remove_confirm(
        &self,
        integration: &Integration,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let advanced = &self.connectors.advanced;
        let removing = advanced.busy.as_deref()
            == Some(format!("remove:{}", integration.definition.id).as_str());
        let delete = advanced.delete_credentials;
        div()
            .debug_selector(|| "settings-mcp-remove-confirm".into())
            .mt(px(16.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.destructive.opacity(0.25))
            .bg(colors.destructive.opacity(0.05))
            .p(px(12.0))
            .child(title(
                12.0,
                cx.t("settings.integrations.removeConfirmTitle"),
                cx,
            ))
            .child(
                text(
                    11.0,
                    true,
                    cx.t("settings.integrations.removeConfirmHint"),
                    cx,
                )
                .mt(px(4.0)),
            )
            .when(integration.credential_count > 0, |confirm| {
                confirm.child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.5))
                        .bg(colors.background)
                        .px(px(12.0))
                        .py(px(8.0))
                        .child(
                            div()
                                .child(title(
                                    12.0,
                                    cx.t("settings.integrations.deleteCredentials"),
                                    cx,
                                ))
                                .child(
                                    text(
                                        10.0,
                                        true,
                                        cx.t("settings.integrations.deleteCredentialsHint"),
                                        cx,
                                    )
                                    .mt(px(2.0)),
                                ),
                        )
                        .child(
                            Switch::new("settings-mcp-delete-credentials", delete).on_toggle({
                                let this = cx.entity().downgrade();
                                move |on, _, cx| {
                                    let _ = this.update(cx, |settings, cx| {
                                        settings.connectors.advanced.delete_credentials = on;
                                        cx.notify();
                                    });
                                }
                            }),
                        ),
                )
            })
            .child(
                div()
                    .mt(px(12.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("settings-mcp-remove-cancel")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .text_size(12.0)
                            .disabled(removing)
                            .on_click(cx.listener(|settings, _, _, cx| {
                                settings.connectors.advanced.remove = None;
                                settings.connectors.advanced.delete_credentials = false;
                                cx.notify();
                            }))
                            .child(cx.t("settings.integrations.cancel")),
                    )
                    .child(
                        Button::new("settings-mcp-remove-confirmed")
                            .variant(ButtonVariant::Destructive)
                            .size(ButtonSize::Sm)
                            .text_size(12.0)
                            .disabled(removing)
                            .on_click(
                                cx.listener(|settings, _, _, cx| settings.remove_advanced(cx)),
                            )
                            .child(labelled(
                                if removing {
                                    spinner("settings-mcp-removing", 14.0).into_any_element()
                                } else {
                                    Icon::new("trash-2").size(14.0).into_any_element()
                                },
                                cx.t("settings.integrations.remove"),
                            )),
                    ),
            )
            .into_any_element()
    }
}
