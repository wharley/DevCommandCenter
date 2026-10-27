//! "Gerenciar"'s actions and rows: a new logo picked, the scopes saved and
//! signed in again, and a tool's row — its name, the server's hints
//! (read-only, destructive…) and its policy (Padrão, Permitir, Negar).

use gpui::{AnyElement, Context, PathPromptOptions, SharedString, Window, div, prelude::*, px};

use super::super::Settings;
use crate::data::connector_custom::read_logo;
use crate::data::connectors::View;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Toaster, spinner};

impl Settings {
    pub(super) fn pick_manage_logo(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .connectors
            .managed
            .as_ref()
            .map(|managed| managed.id.clone())
        else {
            return;
        };
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let logo = cx
                .background_executor()
                .spawn(async move { read_logo(&path) })
                .await;
            let _ = this.update(cx, |settings, cx| match logo {
                Some(logo) => settings.manage_change(
                    "logo".into(),
                    move |api| api.set_connector_logo(&id, Some(&logo)).map(|()| None),
                    |_, _, _| {},
                    cx,
                ),
                None => Toaster::error(cx.t("settings.connectors.custom.invalidLogo"), cx),
            });
        })
        .detach();
    }

    /// "Salvar e reconectar": the scopes saved, then the sign-in again.
    pub(super) fn save_scopes_and_reconnect(
        &mut self,
        view: View,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(api), Some(managed)) = (self.general.api(), self.connectors.managed.as_mut())
        else {
            return;
        };
        let scopes = managed.scopes.read(cx).text().trim().to_string();
        managed.busy = Some("scopes".into());
        cx.notify();
        let id = view.id().to_string();
        cx.spawn_in(window, async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move { api.set_connector_scopes(&id, &scopes) })
                .await;
            let _ = this.update_in(cx, |settings, window, cx| {
                match saved {
                    Ok(()) => {
                        settings.connectors.managed = None;
                        settings.load_connectors(cx);
                        settings.reconnect(&view, window, cx);
                    }
                    Err(error) => {
                        if let Some(managed) = settings.connectors.managed.as_mut() {
                            managed.busy = None;
                        }
                        Toaster::error(error.to_string(), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn tool_row(
        &self,
        id: &str,
        tool: &crate::data::connectors::Tool,
        decision: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let busy = self
            .connectors
            .managed
            .as_ref()
            .and_then(|managed| managed.busy.clone());
        let hints = tool.hints().into_iter().map(|hint| {
            div()
                .h(px(16.0))
                .px(px(4.0))
                .rounded(px(26.0))
                .border_1()
                .border_color(colors.border)
                .text_size(px(9.0))
                .text_color(colors.muted_foreground)
                .child(cx.t(&format!("settings.integrations.toolAnnotation.{hint}")))
        });
        let choices = ["ask", "allow", "deny"].into_iter().map(|value| {
            let selected = decision == value;
            let label = if value == "ask" {
                cx.t("settings.connectors.manage.policyDefault")
            } else {
                cx.t(&format!("settings.integrations.toolPolicy.{value}"))
            };
            let pending = busy.as_deref() == Some(format!("{}:{value}", tool.name).as_str());
            let (connector, name) = (id.to_string(), tool.name.clone());
            div()
                .id(SharedString::from(format!(
                    "manage-policy-{}-{value}",
                    tool.name
                )))
                .debug_selector({
                    let tool = tool.name.clone();
                    move || format!("manage-policy-{tool}-{value}")
                })
                .flex()
                .items_center()
                .justify_center()
                .h(px(24.0))
                .min_w(px(48.0))
                .px(px(6.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(colors.border.opacity(0.5))
                .text_size(px(10.0))
                .when(selected, |choice| choice.bg(colors.accent))
                .when(busy.is_none(), |choice| {
                    choice
                        .cursor_pointer()
                        .hover(|choice| choice.bg(colors.muted))
                        .on_click(cx.listener(move |settings, _, _, cx| {
                            let (connector, name) = (connector.clone(), name.clone());
                            let busy = format!("{name}:{value}");
                            settings.manage_change(
                                busy,
                                move |api| {
                                    api.set_tool_policy(&connector, &name, value).map(|()| None)
                                },
                                |_, _, _| {},
                                cx,
                            )
                        }))
                })
                .child(if pending {
                    spinner(
                        SharedString::from(format!("manage-policy-busy-{}", tool.name)),
                        12.0,
                    )
                    .into_any_element()
                } else {
                    label.into_any_element()
                })
        });
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(colors.border.opacity(0.4))
            .px(px(12.0))
            .py(px(8.0))
            .child(
                div()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_family(cx.theme().fonts.mono.clone())
                            .text_size(px(11.0))
                            .text_color(colors.foreground)
                            .child(tool.name.clone()),
                    )
                    .when(!tool.hints().is_empty(), |column| {
                        column.child(
                            div()
                                .mt(px(4.0))
                                .flex()
                                .flex_wrap()
                                .gap(px(4.0))
                                .children(hints),
                        )
                    }),
            )
            .child(div().flex().flex_none().gap(px(4.0)).children(choices))
            .into_any_element()
    }
}
