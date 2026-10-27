//! "Gerenciar" (`ManageConnectorDialog`): the connector's status and
//! "Testar"; for a custom remote one, its logo and the scopes asked for
//! (saved, then signed in again); the projects it is switched off in; each
//! tool's policy (Padrão / Permitir / Negar); and signing out or removing.

use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, SharedString, Window, div, prelude::*, px,
};

use super::super::Settings;
use super::Confirm;
use super::glyph::glyph;
use crate::data::connector_custom::valid_scopes;
use crate::data::connectors::{Status, View};
use crate::data::desktop_api::ApiError;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_input::TextInput;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Switch, SwitchSize, Toaster, spinner};

pub(in super::super) struct Managed {
    pub(in super::super) id: String,
    pub(super) scopes: Entity<TextInput>,
    /// `scopes`, `project:<id>` or `<tool>:<decision>` while it saves.
    pub(super) busy: Option<String>,
    logo_busy: bool,
    projects: Vec<(String, String)>,
    _scopes_events: gpui::Subscription,
}

impl Settings {
    pub(in super::super) fn open_manage(
        &mut self,
        view: &View,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let saved = view.integration.scopes.clone().unwrap_or_default();
        let scopes = cx.new(|cx| {
            let mut input =
                TextInput::new(window, cx).with_placeholder("support:read reports:read");
            input.set_text(saved, cx);
            input
        });
        let events = cx.subscribe(
            &scopes,
            |_, _, _: &crate::ui::text_input::TextInputEvent, cx| cx.notify(),
        );
        self.connectors.managed = Some(Managed {
            id: view.id().to_string(),
            scopes,
            busy: None,
            logo_busy: false,
            projects: Vec::new(),
            _scopes_events: events,
        });
        cx.notify();
        if cfg!(test) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let projects = cx
                .background_executor()
                .spawn(async move {
                    crate::data::loader::project_names(&crate::data::loader::database_path())
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                if let Some(managed) = settings.connectors.managed.as_mut() {
                    managed.projects = projects;
                }
                cx.notify();
            });
        })
        .detach();
    }

    #[cfg(test)]
    pub(in super::super) fn set_manage_projects(&mut self, projects: Vec<(String, String)>) {
        if let Some(managed) = self.connectors.managed.as_mut() {
            managed.projects = projects;
        }
    }

    fn managed_view(&self) -> Option<View> {
        let managed = self.connectors.managed.as_ref()?;
        self.connectors
            .list()
            .iter()
            .find(|integration| integration.definition.id == managed.id)
            .map(View::of)
    }

    /// One of the dialog's saves: `busy` while it runs, then the list again.
    pub(super) fn manage_change(
        &mut self,
        busy: String,
        change: impl FnOnce(crate::data::desktop_api::DesktopApi) -> Result<Option<String>, ApiError>
        + Send
        + 'static,
        after: impl FnOnce(&mut Settings, Option<String>, &mut Context<Settings>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let (Some(api), Some(managed)) = (self.general.api(), self.connectors.managed.as_mut())
        else {
            return;
        };
        if busy == "logo" {
            managed.logo_busy = true;
        } else {
            managed.busy = Some(busy);
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { change(api) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                if let Some(managed) = settings.connectors.managed.as_mut() {
                    managed.busy = None;
                    managed.logo_busy = false;
                }
                match done {
                    Ok(result) => after(settings, result, cx),
                    Err(error) => Toaster::error(error.to_string(), cx),
                }
                settings.load_connectors(cx);
            });
        })
        .detach();
    }

    pub(in super::super) fn manage_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let managed = self.connectors.managed.as_ref()?;
        let view = self.managed_view()?;
        let colors = cx.theme().colors.clone();
        let id = view.id().to_string();
        let integration = &view.integration;
        let check = integration.connection_check.clone();
        let tools = check
            .as_ref()
            .map(|check| check.tools.clone())
            .unwrap_or_default();
        let busy = self.connectors.busy.is_some();
        let testing = self.connectors.busy.as_deref() == Some(format!("test:{id}").as_str());
        let status: SharedString = match view.status {
            Status::NeedsAuth => cx.t("settings.connectors.status.needsAuth"),
            Status::Failed => check
                .as_ref()
                .and_then(|check| check.message.clone())
                .map(SharedString::from)
                .unwrap_or_else(|| cx.t("settings.connectors.status.failed")),
            Status::Off => cx.t("settings.connectors.status.off"),
            Status::Unchecked => cx.t("settings.connectors.status.unchecked"),
            Status::Connected => cx.t_with(
                "settings.connectors.status.connectedTools",
                &Args::count(view.tool_count as i64),
            ),
        };
        let customizable = view.remote && view.preset.is_none();
        let saved_scopes = integration.scopes.clone().unwrap_or_default();
        let draft = managed.scopes.read(cx).text().trim().to_string();
        let scopes_invalid = !draft.is_empty() && !valid_scopes(&draft);
        let section_title = |text: SharedString| {
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(text)
        };
        let note = |text: SharedString, error: bool| {
            div()
                .text_size(px(11.0))
                .line_height(px(17.9))
                .text_color(if error {
                    colors.destructive
                } else {
                    colors.muted_foreground
                })
                .child(text)
        };
        let small_button = |id: SharedString, label: SharedString, ghost: bool, disabled: bool| {
            Button::new(id)
                .variant(if ghost {
                    ButtonVariant::Ghost
                } else {
                    ButtonVariant::Outline
                })
                .size(ButtonSize::Xs)
                .text_size(12.0)
                .disabled(disabled)
                .child(label)
        };
        let custom = customizable.then(|| {
            let (logo_id, reconnect_view) = (id.clone(), view.clone());
            let remove_id = id.clone();
            let logo_busy = managed.logo_busy;
            let not_found = cx.t("settings.connectors.manage.logoNotFound");
            div()
                .mb(px(20.0))
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(
                    div()
                        .child(section_title(cx.t("settings.connectors.manage.logoTitle")))
                        .child(
                            div()
                                .mt(px(8.0))
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(8.0))
                                .child(glyph(None, view.logo_uri(), &view.name, 36.0, cx))
                                .child(
                                    small_button(
                                        "manage-logo-change".into(),
                                        cx.t("settings.connectors.manage.logoChange"),
                                        false,
                                        logo_busy,
                                    )
                                    .on_click(cx.listener(
                                        |settings, _, _, cx| settings.pick_manage_logo(cx),
                                    )),
                                )
                                .child(
                                    small_button(
                                        "manage-logo-site".into(),
                                        cx.t("settings.connectors.manage.logoFromSite"),
                                        true,
                                        logo_busy,
                                    )
                                    .on_click(cx.listener(
                                        move |settings, _, _, cx| {
                                            let id = logo_id.clone();
                                            let not_found = not_found.clone();
                                            settings.manage_change(
                                                "logo".into(),
                                                move |api| api.fetch_connector_logo(&id),
                                                move |_, logo, cx| {
                                                    if logo.is_none() {
                                                        Toaster::info(not_found, cx);
                                                    }
                                                },
                                                cx,
                                            )
                                        },
                                    )),
                                )
                                .children(logo_busy.then(|| spinner("manage-logo-busy", 12.0)))
                                .when(view.logo_uri().is_some(), |row| {
                                    row.child(
                                        small_button(
                                            "manage-logo-remove".into(),
                                            cx.t("settings.connectors.manage.logoRemove"),
                                            true,
                                            logo_busy,
                                        )
                                        .on_click(
                                            cx.listener(move |settings, _, _, cx| {
                                                let id = remove_id.clone();
                                                settings.manage_change(
                                                    "logo".into(),
                                                    move |api| {
                                                        api.set_connector_logo(&id, None)
                                                            .map(|()| None)
                                                    },
                                                    |_, _, _| {},
                                                    cx,
                                                )
                                            }),
                                        ),
                                    )
                                }),
                        ),
                )
                .child(
                    div()
                        .child(section_title(
                            cx.t("settings.connectors.manage.scopesTitle"),
                        ))
                        .child(
                            div()
                                .mt(px(8.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .debug_selector(|| "manage-scopes".into())
                                        .flex()
                                        .flex_1()
                                        .items_center()
                                        .h(px(32.0))
                                        .px(px(12.0))
                                        .rounded(px(8.0))
                                        .border_1()
                                        .border_color(colors.input)
                                        .font_family(cx.theme().fonts.mono.clone())
                                        .text_size(px(12.0))
                                        .child(managed.scopes.clone()),
                                )
                                .child(
                                    Button::new("manage-scopes-save")
                                        .size(ButtonSize::Sm)
                                        .text_size(12.0)
                                        .disabled(
                                            draft == saved_scopes
                                                || scopes_invalid
                                                || managed.busy.is_some(),
                                        )
                                        .on_click(cx.listener(move |settings, _, window, cx| {
                                            settings.save_scopes_and_reconnect(
                                                reconnect_view.clone(),
                                                window,
                                                cx,
                                            )
                                        }))
                                        .child(cx.t("settings.connectors.manage.saveAndReconnect")),
                                ),
                        )
                        .child(div().mt(px(4.0)).child(note(
                            cx.t(if scopes_invalid {
                                "settings.connectors.custom.invalidScopes"
                            } else {
                                "settings.connectors.manage.scopesHint"
                            }),
                            scopes_invalid,
                        ))),
                )
        });
        let projects = (integration.global() && !managed.projects.is_empty()).then(|| {
            let rows = managed.projects.iter().map(|(project, name)| {
                let on = !integration.excluded_project_ids.contains(project);
                let (connector, project) = (id.clone(), project.clone());
                let this = cx.entity().downgrade();
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(10.0))
                    .px(px(12.0))
                    .py(px(6.0))
                    .hover(|row| row.bg(colors.muted.opacity(0.4)))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .text_color(colors.foreground)
                            .child(name.clone()),
                    )
                    .child(
                        Switch::new(SharedString::from(format!("manage-project-{project}")), on)
                            .size(SwitchSize::Sm)
                            .disabled(managed.busy.is_some())
                            .on_toggle(move |enabled, _, cx| {
                                let (connector, project) = (connector.clone(), project.clone());
                                let _ = this.update(cx, |settings, cx| {
                                    let busy = format!("project:{project}");
                                    settings.manage_change(
                                        busy,
                                        move |api| {
                                            api.set_connector_project(&connector, &project, enabled)
                                                .map(|()| None)
                                        },
                                        |_, _, _| {},
                                        cx,
                                    )
                                });
                            }),
                    )
            });
            div()
                .mb(px(20.0))
                .child(section_title(
                    cx.t("settings.connectors.manage.projectsTitle"),
                ))
                .child(
                    div()
                        .mt(px(2.0))
                        .child(note(cx.t("settings.connectors.manage.projectsHint"), false)),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .children(rows),
                )
        });
        let tool_rows: Vec<AnyElement> = tools
            .iter()
            .map(|tool| self.tool_row(&id, tool, integration.tool_decision(&tool.name), cx))
            .collect();
        let last_checked = check
            .as_ref()
            .and_then(|check| check.checked_at.as_deref())
            .map(|at| {
                cx.t_with(
                    "settings.connectors.manage.lastChecked",
                    &Args::default().with("date", crate::data::time::local_date_time(at)),
                )
            });
        let (test_view, disconnect_view, remove_view) = (view.clone(), view.clone(), view.clone());
        let close = cx.entity().downgrade();
        Some(
            Dialog::new("connector-manage", move |_, cx| {
                let _ = close.update(cx, |settings, cx| {
                    settings.connectors.managed = None;
                    cx.notify();
                });
            })
            .max_width(520.0)
            .max_height(640.0)
            .padding(0.0)
            .gap(0.0)
            .child(
                div()
                    .debug_selector(|| "connector-manage".into())
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .border_b_1()
                            .border_color(colors.border.opacity(0.5))
                            .px(px(20.0))
                            .py(px(16.0))
                            .pr(px(48.0))
                            .child(glyph(
                                view.preset.and_then(|preset| preset.logo),
                                view.logo_uri(),
                                &view.name,
                                36.0,
                                cx,
                            ))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(view.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(12.0))
                                            .text_color(if view.status == Status::NeedsAuth {
                                                colors.warning
                                            } else {
                                                colors.muted_foreground
                                            })
                                            .child(status),
                                    ),
                            )
                            .child(
                                Button::new("manage-test")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .text_size(12.0)
                                    .disabled(busy)
                                    .on_click(cx.listener(move |settings, _, _, cx| {
                                        settings.test_connector(&test_view, cx)
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .children(
                                                testing.then(|| spinner("manage-testing", 14.0)),
                                            )
                                            .child(cx.t(if testing {
                                                "settings.connectors.actions.testing"
                                            } else {
                                                "settings.connectors.actions.test"
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("connector-manage-body")
                            .flex_1()
                            .min_h_0()
                            .max_h(px(480.0))
                            .overflow_y_scroll()
                            .px(px(20.0))
                            .py(px(16.0))
                            .children(custom)
                            .children(projects)
                            .child(section_title(cx.t("settings.connectors.manage.toolsTitle")))
                            .child(div().mt(px(2.0)).child(note(
                                cx.t(if tools.is_empty() {
                                    "settings.connectors.manage.noTools"
                                } else {
                                    "settings.connectors.manage.toolsHint"
                                }),
                                false,
                            )))
                            .child(
                                div()
                                    .mt(px(12.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .children(tool_rows),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .border_t_1()
                            .border_color(colors.border.opacity(0.5))
                            .px(px(20.0))
                            .py(px(12.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .children(last_checked),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_none()
                                    .gap(px(8.0))
                                    .when(view.signed_in, |row| {
                                        row.child(
                                            Button::new("manage-disconnect")
                                                .variant(ButtonVariant::Ghost)
                                                .size(ButtonSize::Sm)
                                                .text_size(12.0)
                                                .disabled(busy)
                                                .on_click(cx.listener(move |settings, _, _, cx| {
                                                    settings.connectors.managed = None;
                                                    settings.connectors.confirm =
                                                        Some(Confirm::Disconnect(
                                                            disconnect_view.clone(),
                                                        ));
                                                    cx.notify();
                                                }))
                                                .child(
                                                    cx.t("settings.connectors.actions.disconnect"),
                                                ),
                                        )
                                    })
                                    .child(
                                        Button::new("manage-remove")
                                            .variant(ButtonVariant::Destructive)
                                            .size(ButtonSize::Sm)
                                            .text_size(12.0)
                                            .disabled(busy)
                                            .on_click(cx.listener(move |settings, _, _, cx| {
                                                settings.connectors.managed = None;
                                                settings.connectors.confirm =
                                                    Some(Confirm::Remove(remove_view.clone()));
                                                cx.notify();
                                            }))
                                            .child(cx.t("settings.connectors.actions.remove")),
                                    ),
                            ),
                    ),
            )
            .into_any_element(),
        )
    }
}
