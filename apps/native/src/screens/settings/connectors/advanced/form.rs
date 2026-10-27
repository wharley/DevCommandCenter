//! "Nova integração MCP": the name, HTTP (its URL) or a local command (the
//! executable, one argument per line, the folder), the scope, and the
//! credentials (a header or an environment variable, the secret masked).
//! "Revisar" checks it like the desktop and creates it turned off.

use gpui::{AnyElement, Context, Entity, FontWeight, SharedString, div, prelude::*, px};

use super::super::super::Settings;
use super::parts::{labelled, text, title};
use crate::data::mcp_advanced::{ScopeDraft, TransportDraft};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_input::TextInput;
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon, TextArea, spinner};

fn label(value: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.foreground)
        .child(value)
        .into_any_element()
}

/// shadcn's `Input`: h-8 rounded-lg border-input px-2.5.
fn input(id: String, input: &Entity<TextInput>, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .debug_selector(move || id.clone())
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .h(px(32.0))
        .px(px(10.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(colors.input)
        .text_size(px(14.0))
        .child(input.clone())
        .into_any_element()
}

fn area(input: &Entity<TextArea>, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .debug_selector(|| "settings-mcp-args".into())
        .w_full()
        .min_h(px(80.0))
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(colors.input)
        .font_family(cx.theme().fonts.mono.clone())
        .text_size(px(12.0))
        .child(input.clone())
        .into_any_element()
}

fn grid(children: Vec<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .children(children)
        .into_any_element()
}

impl Settings {
    fn transport_choice(
        &self,
        transport: TransportDraft,
        chosen: TransportDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (key, icon) = match transport {
            TransportDraft::Http => ("http", "earth"),
            TransportDraft::Stdio => ("stdio", "command"),
        };
        div()
            .id(SharedString::from(format!("settings-mcp-transport-{key}")))
            .debug_selector(move || format!("settings-mcp-transport-{key}"))
            .flex()
            .flex_1()
            .items_center()
            .gap(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(12.0))
            .py(px(8.0))
            .cursor_pointer()
            .when(transport == chosen, |choice| choice.bg(colors.muted))
            .hover(|choice| choice.bg(colors.muted))
            .on_click(
                cx.listener(move |settings, _, _, cx| {
                    settings.set_advanced_transport(transport, cx)
                }),
            )
            .child(Icon::new(icon).size(16.0))
            .child(
                div()
                    .child(title(
                        12.0,
                        cx.t(&format!("settings.integrations.{key}")),
                        cx,
                    ))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t(&format!("settings.integrations.{key}Hint"))),
                    ),
            )
            .into_any_element()
    }

    fn scope_choice(
        &self,
        scope: ScopeDraft,
        chosen: ScopeDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (key, enabled) = match scope {
            ScopeDraft::Session => ("Session", self.session.session_id.is_some()),
            ScopeDraft::Project => ("Project", self.session.project_id.is_some()),
            ScopeDraft::Global => ("Global", true),
        };
        div()
            .id(SharedString::from(format!("settings-mcp-scope-{key}")))
            .debug_selector(move || format!("settings-mcp-scope-{key}"))
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .h(px(32.0))
            .px(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .text_size(px(11.0))
            .when(scope == chosen, |choice| choice.bg(colors.muted))
            .when(!enabled, |choice| choice.opacity(0.5))
            .when(enabled, |choice| {
                choice
                    .cursor_pointer()
                    .hover(|choice| choice.bg(colors.muted))
                    .on_click(
                        cx.listener(move |settings, _, _, cx| {
                            settings.set_advanced_scope(scope, cx)
                        }),
                    )
            })
            .child(cx.t(&format!("settings.integrations.scope{key}")))
            .into_any_element()
    }

    fn credentials(&self, http: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let Some(form) = self.connectors.advanced.form.as_ref() else {
            return div().into_any_element();
        };
        let rows: Vec<AnyElement> = form
            .credentials
            .iter()
            .map(|credential| {
                let id = credential.id;
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.5))
                    .bg(colors.background)
                    .p(px(12.0))
                    .child(
                        div()
                            .flex_grow(0.8)
                            .flex_basis(px(0.0))
                            .min_w_0()
                            .child(input(
                                format!("settings-mcp-credential-name-{id}"),
                                &credential.name,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_grow(1.2)
                            .flex_basis(px(0.0))
                            .min_w_0()
                            .child(input(
                                format!("settings-mcp-credential-secret-{id}"),
                                &credential.secret,
                                cx,
                            )),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-mcp-credential-remove-{id}"
                        )))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Icon)
                        .on_click(cx.listener(move |settings, _, _, cx| {
                            settings.remove_advanced_credential(id, cx)
                        }))
                        .child(Icon::new("x").size(14.0)),
                    )
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.5))
            .pt(px(16.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .child(label(cx.t("settings.integrations.credentialsLabel"), cx))
                            .child(
                                text(
                                    11.0,
                                    true,
                                    cx.t(if http {
                                        "settings.integrations.httpCredentialsHint"
                                    } else {
                                        "settings.integrations.stdioCredentialsHint"
                                    }),
                                    cx,
                                )
                                .mt(px(4.0)),
                            ),
                    )
                    .child(
                        Button::new("settings-mcp-credential-add")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .text_size(12.0)
                            .on_click(cx.listener(|settings, _, window, cx| {
                                settings.add_advanced_credential(window, cx)
                            }))
                            .child(labelled(
                                Icon::new("plus").size(14.0).into_any_element(),
                                cx.t("settings.integrations.addCredential"),
                            )),
                    ),
            )
            .child(if rows.is_empty() {
                div()
                    .rounded(px(8.0))
                    .border_1()
                    .border_dashed()
                    .border_color(colors.border.opacity(0.6))
                    .px(px(12.0))
                    .py(px(12.0))
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("settings.integrations.noCredentials"))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(rows)
                    .into_any_element()
            })
            .into_any_element()
    }

    pub(super) fn advanced_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let Some(form) = self.connectors.advanced.form.as_ref() else {
            return div().into_any_element();
        };
        let (transport, scope) = (form.transport, form.scope);
        let http = transport == TransportDraft::Http;
        let creating = self.connectors.advanced.busy.as_deref() == Some("create");
        let hint = |key: &str| text(11.0, true, cx.t(key), cx).into_any_element();
        let target = if http {
            grid(vec![
                label(cx.t("settings.integrations.urlLabel"), cx),
                input("settings-mcp-url".into(), &form.url, cx),
                hint("settings.integrations.urlHint"),
            ])
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(grid(vec![
                    label(cx.t("settings.integrations.executableLabel"), cx),
                    input("settings-mcp-executable".into(), &form.executable, cx),
                    hint("settings.integrations.executableHint"),
                ]))
                .child(grid(vec![
                    label(cx.t("settings.integrations.argumentsLabel"), cx),
                    area(&form.args, cx),
                    hint("settings.integrations.argumentsHint"),
                ]))
                .child(grid(vec![
                    label(cx.t("settings.integrations.cwdLabel"), cx),
                    input("settings-mcp-cwd".into(), &form.cwd, cx),
                ]))
                .into_any_element()
        };
        let scope_hint = match scope {
            ScopeDraft::Session => cx.t("settings.integrations.scopeSessionHint"),
            ScopeDraft::Project => cx.t_with(
                "settings.integrations.scopeProjectHint",
                &Args::default().with(
                    "workspace",
                    self.workspace
                        .0
                        .clone()
                        .unwrap_or_else(|| cx.t("settings.currentWorkspace").to_string()),
                ),
            ),
            ScopeDraft::Global => cx.t("settings.integrations.scopeGlobalHint"),
        };
        div()
            .debug_selector(|| "settings-mcp-create".into())
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .child(
                                div().ml(px(-8.0)).mb(px(8.0)).child(
                                    Button::new("settings-mcp-back")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Sm)
                                        .text_size(12.0)
                                        .on_click(cx.listener(|settings, _, _, cx| {
                                            settings.advanced_back(cx)
                                        }))
                                        .child(labelled(
                                            Icon::new("arrow-left").size(14.0).into_any_element(),
                                            cx.t("settings.integrations.back"),
                                        )),
                                ),
                            )
                            .child(title(14.0, cx.t("settings.integrations.createTitle"), cx))
                            .child(
                                text(12.0, true, cx.t("settings.integrations.createHint"), cx)
                                    .mt(px(4.0)),
                            ),
                    )
                    .child(Badge::new(
                        cx.t("settings.integrations.disabledUntilReview"),
                        BadgeVariant::Outline,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .bg(colors.muted.opacity(0.1))
                    .p(px(16.0))
                    .child(grid(vec![
                        label(cx.t("settings.integrations.nameLabel"), cx),
                        input("settings-mcp-name".into(), &form.name, cx),
                    ]))
                    .child(grid(vec![
                        label(cx.t("settings.integrations.transportLabel"), cx),
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(self.transport_choice(TransportDraft::Http, transport, cx))
                            .child(self.transport_choice(TransportDraft::Stdio, transport, cx))
                            .into_any_element(),
                    ]))
                    .child(target)
                    .child(grid(vec![
                        label(cx.t("settings.integrations.scopeLabel"), cx),
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(self.scope_choice(ScopeDraft::Session, scope, cx))
                            .child(self.scope_choice(ScopeDraft::Project, scope, cx))
                            .child(self.scope_choice(ScopeDraft::Global, scope, cx))
                            .into_any_element(),
                        text(11.0, true, scope_hint, cx).into_any_element(),
                    ]))
                    .child(self.credentials(http, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("settings-mcp-cancel")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Default)
                            .text_size(14.0)
                            .disabled(creating)
                            .on_click(cx.listener(|settings, _, _, cx| settings.advanced_back(cx)))
                            .child(cx.t("settings.integrations.cancel")),
                    )
                    .child(
                        Button::new("settings-mcp-review")
                            .size(ButtonSize::Default)
                            .text_size(14.0)
                            .disabled(creating)
                            .on_click(
                                cx.listener(|settings, _, _, cx| settings.create_advanced(cx)),
                            )
                            .child(labelled(
                                if creating {
                                    spinner("settings-mcp-creating", 14.0).into_any_element()
                                } else {
                                    Icon::new("shield-check").size(14.0).into_any_element()
                                },
                                cx.t("settings.integrations.review"),
                            )),
                    ),
            )
            .into_any_element()
    }
}
