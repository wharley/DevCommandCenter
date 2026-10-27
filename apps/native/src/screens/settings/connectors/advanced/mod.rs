//! "Avançado" under Conectores (`McpIntegrationsPanel`): servers added by
//! hand. The list (what the open session's provider supports, each server
//! with its switch, its status in the session and its tools' policies), the
//! form for a new one (HTTP or a local command, its scope and credentials)
//! and the review that turns it on. Both lists are read again every 5s
//! while it is open.

mod form;
mod list;
mod parts;
mod review;
mod runtime;

use std::collections::HashSet;
use std::time::Duration;

use gpui::{AppContext, Context, Entity, Subscription, Task, Window};

use super::super::Settings;
use crate::data::connectors::Integration;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::mcp_advanced::{
    CredentialDraft, Draft, RuntimeStatus, ScopeDraft, TransportDraft, build_input, error_key,
};
use crate::i18n::Translate;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{TextArea, TextAreaEvent, Toaster};

const POLL: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(in super::super) enum Panel {
    #[default]
    List,
    Create,
    Review(Box<Integration>),
}

/// One credential's row: its name and its secret, masked.
pub(in super::super) struct Credential {
    id: usize,
    pub(in super::super) name: Entity<TextInput>,
    pub(in super::super) secret: Entity<TextInput>,
}

/// The new server's form.
pub(in super::super) struct Form {
    pub(in super::super) name: Entity<TextInput>,
    pub(in super::super) url: Entity<TextInput>,
    pub(in super::super) executable: Entity<TextInput>,
    pub(in super::super) args: Entity<TextArea>,
    pub(in super::super) cwd: Entity<TextInput>,
    pub(in super::super) transport: TransportDraft,
    pub(in super::super) scope: ScopeDraft,
    pub(in super::super) credentials: Vec<Credential>,
    next: usize,
    _inputs: Vec<Subscription>,
}

#[derive(Default)]
pub(in super::super) struct Advanced {
    pub(in super::super) open: bool,
    pub(in super::super) panel: Panel,
    /// The open session's reports; `None` while reading.
    pub(in super::super) statuses: Option<Result<Vec<RuntimeStatus>, String>>,
    /// `create`, `activate:<id>`, `disable:<id>`, `remove:<id>`,
    /// `policy:<id>:<tool>:<decision>` while it runs.
    pub(in super::super) busy: Option<String>,
    /// The server waiting for "Remover integração?".
    pub(in super::super) remove: Option<String>,
    pub(in super::super) delete_credentials: bool,
    /// Servers whose tools' policies are open.
    pub(in super::super) expanded: HashSet<String>,
    pub(in super::super) form: Option<Form>,
    poll: Option<Task<()>>,
}

fn notify_on_change(input: &Entity<TextInput>, cx: &mut Context<Settings>) -> Subscription {
    cx.subscribe(input, |_, _, event, cx| {
        if matches!(event, TextInputEvent::Changed) {
            cx.notify();
        }
    })
}

impl Form {
    fn new(scope: ScopeDraft, window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let placeholder = Translate::t(&**cx, "settings.integrations.namePlaceholder");
        let name = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let url =
            cx.new(|cx| TextInput::new(window, cx).with_placeholder("https://mcp.example.com/rpc"));
        let executable =
            cx.new(|cx| TextInput::new(window, cx).with_placeholder("/usr/local/bin/mcp-server"));
        let args = cx.new(|cx| TextArea::new(window, cx).with_placeholder("--transport\nstdio"));
        let optional = Translate::t(&**cx, "settings.integrations.optional");
        let cwd = cx.new(|cx| TextInput::new(window, cx).with_placeholder(optional));
        let mut inputs: Vec<Subscription> = [&name, &url, &executable, &cwd]
            .into_iter()
            .map(|input| notify_on_change(input, cx))
            .collect();
        // One argument per line: Enter is a new line here.
        inputs.push(cx.subscribe(&args, |_, area, event, cx| match event {
            TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
            TextAreaEvent::Changed => cx.notify(),
            _ => {}
        }));
        Self {
            name,
            url,
            executable,
            args,
            cwd,
            transport: TransportDraft::Http,
            scope,
            credentials: Vec::new(),
            next: 0,
            _inputs: inputs,
        }
    }

    pub(in super::super) fn draft(&self, cx: &gpui::App) -> Draft {
        let text = |input: &Entity<TextInput>| input.read(cx).text().to_string();
        Draft {
            display_name: text(&self.name),
            transport: self.transport,
            url: text(&self.url),
            executable: text(&self.executable),
            args_text: self.args.read(cx).text().to_string(),
            cwd: text(&self.cwd),
            scope: self.scope,
            credentials: self
                .credentials
                .iter()
                .map(|credential| CredentialDraft {
                    name: text(&credential.name),
                    secret: text(&credential.secret),
                })
                .collect(),
        }
    }
}

fn failed(error: ApiError, fallback: &str, cx: &gpui::App) -> String {
    match error {
        ApiError::DesktopClosed => cx.t("native.desktopClosed").to_string(),
        ApiError::Failed(message) if !message.is_empty() => message,
        _ => cx.t(fallback).to_string(),
    }
}

impl Settings {
    pub(in super::super) fn toggle_advanced(&mut self, cx: &mut Context<Self>) {
        let advanced = &mut self.connectors.advanced;
        advanced.open = !advanced.open;
        if !advanced.open {
            advanced.poll = None;
            cx.notify();
            return;
        }
        self.load_runtime(cx);
        self.connectors.advanced.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let Ok(()) = this.update(cx, |settings, cx| {
                    settings.load_connectors(cx);
                    settings.load_runtime(cx);
                }) else {
                    return;
                };
            }
        }));
        cx.notify();
    }

    /// `loadMcpRuntimeStatuses` for the open session.
    pub(in super::super) fn load_runtime(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let api = self.general.api();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    api.ok_or(ApiError::DesktopClosed)?
                        .runtime_statuses(&session)
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.connectors.advanced.statuses =
                    Some(read.map_err(|error| error.to_string()));
                cx.notify();
            });
        })
        .detach();
    }

    pub(in super::super) fn open_advanced_create(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = Draft::new(&self.session).scope;
        let form = Form::new(scope, window, cx);
        window.focus(&gpui::Focusable::focus_handle(form.name.read(cx), cx), cx);
        let advanced = &mut self.connectors.advanced;
        advanced.form = Some(form);
        advanced.panel = Panel::Create;
        cx.notify();
    }

    pub(super) fn advanced_back(&mut self, cx: &mut Context<Self>) {
        self.connectors.advanced.panel = Panel::List;
        cx.notify();
    }

    pub(in super::super) fn set_advanced_transport(
        &mut self,
        transport: TransportDraft,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.connectors.advanced.form.as_mut() {
            form.transport = transport;
            form.credentials.clear();
        }
        cx.notify();
    }

    pub(super) fn set_advanced_scope(&mut self, scope: ScopeDraft, cx: &mut Context<Self>) {
        if let Some(form) = self.connectors.advanced.form.as_mut() {
            form.scope = scope;
        }
        cx.notify();
    }

    pub(in super::super) fn add_advanced_credential(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.connectors.advanced.form.is_none() {
            return;
        }
        let value = Translate::t(&**cx, "settings.integrations.credentialValuePlaceholder");
        let name = cx.new(|cx| TextInput::new(window, cx));
        let secret = cx.new(|cx| TextInput::new(window, cx).with_placeholder(value).masked());
        let subscriptions = [notify_on_change(&name, cx), notify_on_change(&secret, cx)];
        let Some(form) = self.connectors.advanced.form.as_mut() else {
            return;
        };
        let header = form.transport == TransportDraft::Http;
        name.update(cx, |name, cx| {
            name.set_placeholder(
                if header {
                    "Authorization"
                } else {
                    "SERVICE_API_KEY"
                },
                cx,
            )
        });
        form._inputs.extend(subscriptions);
        form.credentials.push(Credential {
            id: form.next,
            name,
            secret,
        });
        form.next += 1;
        cx.notify();
    }

    pub(super) fn remove_advanced_credential(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(form) = self.connectors.advanced.form.as_mut() {
            form.credentials.retain(|credential| credential.id != id);
        }
        cx.notify();
    }

    /// Runs one change against the desktop, then reads the list again.
    fn advanced_change<T: Send + 'static>(
        &mut self,
        busy: String,
        fallback: &'static str,
        change: impl FnOnce(DesktopApi) -> Result<T, ApiError> + Send + 'static,
        done: impl FnOnce(&mut Settings, T, &mut Context<Settings>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = self.general.api() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        self.connectors.advanced.busy = Some(busy);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { change(api) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.connectors.advanced.busy = None;
                match result {
                    Ok(value) => done(settings, value, cx),
                    Err(error) => Toaster::error(failed(error, fallback, cx), cx),
                }
                settings.load_connectors(cx);
            });
        })
        .detach();
    }

    /// `handleCreate`: checked like the desktop, then the review.
    pub(in super::super) fn create_advanced(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.connectors.advanced.form.as_ref() else {
            return;
        };
        let body = match build_input(&form.draft(cx), &self.session) {
            Ok(body) => body,
            Err(error) => {
                Toaster::error(cx.t(&error_key(error)), cx);
                return;
            }
        };
        self.advanced_change(
            "create".into(),
            "settings.integrations.errors.create",
            move |api| api.create_integration(&body),
            |settings, integration, cx| {
                let advanced = &mut settings.connectors.advanced;
                advanced.form = None;
                advanced.panel = Panel::Review(Box::new(integration));
                Toaster::success(cx.t("settings.integrations.created"), cx);
            },
            cx,
        );
    }

    /// `handleActivate` with the fingerprint reviewed.
    pub(in super::super) fn activate_advanced(
        &mut self,
        integration: Integration,
        cx: &mut Context<Self>,
    ) {
        self.advanced_change(
            format!("activate:{}", integration.definition.id),
            "settings.integrations.errors.activate",
            move |api| api.activate_connector(&integration),
            |settings, _, cx| {
                settings.connectors.advanced.panel = Panel::List;
                Toaster::success(cx.t("settings.integrations.enabledToast"), cx);
            },
            cx,
        );
    }

    pub(in super::super) fn disable_advanced(&mut self, id: String, cx: &mut Context<Self>) {
        self.advanced_change(
            format!("disable:{id}"),
            "settings.integrations.errors.disable",
            move |api| api.disable_connector(&id),
            |_, (), cx| Toaster::success(cx.t("settings.integrations.disabledToast"), cx),
            cx,
        );
    }

    pub(in super::super) fn remove_advanced(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.connectors.advanced.remove.clone() else {
            return;
        };
        let delete = self.connectors.advanced.delete_credentials;
        self.advanced_change(
            format!("remove:{id}"),
            "settings.integrations.errors.remove",
            move |api| api.remove_integration(&id, delete),
            |settings, (), cx| {
                let advanced = &mut settings.connectors.advanced;
                advanced.remove = None;
                advanced.delete_credentials = false;
                Toaster::success(cx.t("settings.integrations.removedToast"), cx);
            },
            cx,
        );
    }

    pub(in super::super) fn set_advanced_policy(
        &mut self,
        id: String,
        tool: String,
        decision: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.advanced_change(
            format!("policy:{id}:{tool}:{decision}"),
            "settings.integrations.errors.toolPolicy",
            move |api| api.set_tool_policy(&id, &tool, decision),
            |_, (), cx| Toaster::success(cx.t("settings.integrations.toolPolicyUpdated"), cx),
            cx,
        );
    }
}
