//! "Conectores" (`ConnectorsPanel`): the search, the connected ones (status,
//! "Reconectar", and a menu to test, reconnect, sign out or remove), then
//! the catalog by category, "Conectar" with DCC's own sign-in, a custom
//! one, managing one, and "Avançado" (servers added by hand).

mod advanced;
mod connect;
mod connect_view;
mod custom;
mod glyph;
mod import;
mod manage;
mod manage_tools;
mod view;

use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::Settings;
use crate::data::connectors::{Integration, Status, Target, View};
use crate::data::desktop_api::ApiError;
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;
use crate::ui::text_input::{TextInput, TextInputEvent};
#[cfg(test)]
pub(super) use advanced::{Form, Panel};
pub(super) use connect::{ConnectEvent, ConnectorConnect};
#[cfg(test)]
pub(super) use custom::ConnectorCustom;

/// What waits for "Desconectar …?" / "Remover …?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Confirm {
    Disconnect(View),
    Remove(View),
}

pub(super) struct Connectors {
    /// `None` while reading; the error when the desktop could not answer.
    pub(super) integrations: Option<Result<Vec<Integration>, String>>,
    pub(super) search: Entity<TextInput>,
    /// `test:<id>`, `disconnect:<id>`, `remove:<id>` while it runs.
    pub(super) busy: Option<String>,
    pub(super) confirm: Option<Confirm>,
    pub(super) connect: Option<(Entity<ConnectorConnect>, Subscription)>,
    /// "Adicionar conector personalizado".
    pub(super) custom: Option<(Entity<custom::ConnectorCustom>, Subscription)>,
    /// "Gerenciar" for this one.
    pub(super) managed: Option<manage::Managed>,
    pub(super) import: import::Import,
    /// "Avançado": servers added by hand.
    pub(super) advanced: advanced::Advanced,
    started: bool,
    _search_events: Subscription,
}

impl Connectors {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let placeholder = Translate::t(&**cx, "settings.connectors.searchPlaceholder");
        let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let events = cx.subscribe(&search, |_, input, event, cx| match event {
            TextInputEvent::Changed => cx.notify(),
            // Esc with text clears it first; then it closes Settings.
            TextInputEvent::Cancelled => {
                if input.read(cx).text().is_empty() {
                    cx.emit(super::SettingsEvent::Closed);
                } else {
                    input.update(cx, |input, cx| input.set_text("", cx));
                    cx.notify();
                }
            }
            _ => {}
        });
        Self {
            integrations: None,
            search,
            busy: None,
            confirm: None,
            connect: None,
            custom: None,
            managed: None,
            import: import::Import::default(),
            advanced: advanced::Advanced::default(),
            started: false,
            _search_events: events,
        }
    }

    pub(super) fn list(&self) -> &[Integration] {
        match &self.integrations {
            Some(Ok(integrations)) => integrations,
            _ => &[],
        }
    }

    #[cfg(test)]
    pub(super) fn reset(&mut self) {
        self.integrations = None;
        let path = self.import.path.take();
        self.import = import::Import::default();
        self.import.path = path;
        self.advanced = advanced::Advanced::default();
        self.started = false;
    }
}

fn message(error: ApiError, cx: &gpui::App) -> String {
    error.text(cx)
}

impl Settings {
    /// Read once "Conectores" shows.
    pub(super) fn ensure_connectors(&mut self, cx: &mut Context<Self>) {
        if self.connectors.started {
            return;
        }
        self.connectors.started = true;
        self.load_connectors(cx);
    }

    pub(super) fn load_connectors(&mut self, cx: &mut Context<Self>) {
        let api = self.general.api();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.ok_or(ApiError::DesktopClosed)?.connectors() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let loaded = read.is_ok();
                settings.connectors.integrations = Some(read.map_err(|error| message(error, cx)));
                if loaded {
                    settings.ensure_import_offer(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_connect(
        &mut self,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.connectors.connect.is_some() {
            return;
        }
        let api = self.general.api();
        let integrations = self.connectors.list().to_vec();
        let dialog = cx.new(|cx| ConnectorConnect::new(target, integrations, api, window, cx));
        let events = cx.subscribe(&dialog, |settings, _, event, cx| match event {
            ConnectEvent::Closed => {
                settings.connectors.connect = None;
                cx.notify();
            }
            ConnectEvent::Finished => settings.load_connectors(cx),
        });
        self.connectors.connect = Some((dialog, events));
        cx.notify();
    }

    pub(super) fn open_custom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.connectors.custom.is_some() {
            return;
        }
        let dialog = cx.new(|cx| custom::ConnectorCustom::new(window, cx));
        let events = cx.subscribe_in(
            &dialog,
            window,
            |settings, _, event, window, cx| match event {
                custom::CustomEvent::Closed => {
                    settings.connectors.custom = None;
                    cx.notify();
                }
                custom::CustomEvent::Submitted(target) => {
                    settings.connectors.custom = None;
                    settings.open_connect(target.clone(), window, cx);
                }
            },
        );
        self.connectors.custom = Some((dialog, events));
        cx.notify();
    }

    /// `reconnect`: a remote connector's sign-in again.
    pub(super) fn reconnect(&mut self, view: &View, window: &mut Window, cx: &mut Context<Self>) {
        let Some(url) = view.integration.url() else {
            return;
        };
        let target = Target {
            name: view.name.clone(),
            url: url.to_string(),
            preset: view.preset,
            ..Target::default()
        };
        self.open_connect(target, window, cx);
    }

    /// `runTest`: connected (with the tool count), needs reconnecting, or failed.
    pub(super) fn test_connector(&mut self, view: &View, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        let (id, name) = (view.id().to_string(), view.name.clone());
        self.connectors.busy = Some(format!("test:{id}"));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let check = cx
                .background_executor()
                .spawn(async move { api.test_connector(&id) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let name_args = Args::default().with("name", name.clone());
                match check {
                    Ok(check) if check.state == "connected" => Toaster::success(
                        cx.t_with(
                            "settings.connectors.toast.testConnected",
                            &Args::count(check.tools.len() as i64).with("name", name),
                        ),
                        cx,
                    ),
                    Ok(check) if check.state == "needsAuth" => Toaster::info(
                        cx.t_with("settings.connectors.toast.testNeedsAuth", &name_args),
                        cx,
                    ),
                    Ok(check) => Toaster::error(
                        cx.t_with(
                            "settings.connectors.toast.testFailed",
                            &name_args.with("message", check.message.unwrap_or_default()),
                        ),
                        cx,
                    ),
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                settings.connectors.busy = None;
                settings.load_connectors(cx);
            });
        })
        .detach();
    }

    /// After "Desconectar …?" or "Remover …?".
    pub(super) fn confirmed(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(confirm)) = (self.general.api(), self.connectors.confirm.take())
        else {
            return;
        };
        let (view, remove) = match confirm {
            Confirm::Disconnect(view) => (view, false),
            Confirm::Remove(view) => (view, true),
        };
        let (id, name, signed_in) = (view.id().to_string(), view.name.clone(), view.signed_in);
        self.connectors.busy = Some(format!(
            "{}:{id}",
            if remove { "remove" } else { "disconnect" }
        ));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    if !remove || signed_in {
                        api.disconnect_connector(&id)?;
                    }
                    if remove {
                        api.remove_connector(&id)?;
                    }
                    Ok::<_, ApiError>(())
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                match done {
                    Ok(()) => Toaster::success(
                        cx.t_with(
                            if remove {
                                "settings.connectors.toast.removed"
                            } else {
                                "settings.connectors.toast.disconnected"
                            },
                            &Args::default().with("name", name),
                        ),
                        cx,
                    ),
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                settings.connectors.busy = None;
                settings.load_connectors(cx);
            });
        })
        .detach();
    }

    /// The status line under a connected one's name.
    pub(super) fn connector_status(&self, view: &View, cx: &gpui::App) -> gpui::SharedString {
        match view.status {
            Status::NeedsAuth => cx.t("settings.connectors.status.needsAuth"),
            Status::Failed => cx.t("settings.connectors.status.failed"),
            Status::Off => cx.t("settings.connectors.status.off"),
            Status::Unchecked => match view.preset {
                Some(preset) => cx.t(&format!("settings.connectors.catalog.{}", preset.id)),
                None => cx.t("settings.connectors.status.unchecked"),
            },
            Status::Connected if view.tool_count > 0 => cx.t_with(
                "settings.connectors.status.connectedTools",
                &Args::count(view.tool_count as i64),
            ),
            Status::Connected => cx.t("settings.connectors.status.connected"),
        }
    }
}
