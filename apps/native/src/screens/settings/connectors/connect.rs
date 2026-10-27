//! "Conectar …" (`ConnectDialog`): what the connector gets, then DCC's own
//! sign-in in the browser (or a personal token, or the person's own app
//! when the server registers no clients — Slack, guided). A connector this
//! attempt added is removed again if the sign-in does not finish.

use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Window};

use crate::data::connectors::{Auth, Finish, Integration, Start, Target, integration_for_url};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;
use crate::ui::text_input::{TextInput, TextInputEvent};

pub enum ConnectEvent {
    Closed,
    /// Something changed: the list is read again.
    Finished,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Intro,
    Waiting,
    Failed(String),
    /// The server registers no clients: the person's app or a token.
    NeedsApp,
}

#[derive(Clone, Debug)]
enum Attempt {
    OAuth {
        client_id: Option<String>,
        client_secret: Option<String>,
    },
    Token(String),
}

pub struct ConnectorConnect {
    pub(super) target: Target,
    integrations: Vec<Integration>,
    api: Option<DesktopApi>,
    pub(super) phase: Phase,
    pub(in super::super) token: Entity<TextInput>,
    pub(super) client_id: Entity<TextInput>,
    pub(super) client_secret: Entity<TextInput>,
    /// The sign-in waiting on the browser, to cancel it.
    login_id: Option<String>,
    /// Shown when the manifest or redirect was just copied.
    pub(super) copied: Option<&'static str>,
    closed: bool,
    _inputs: Vec<Subscription>,
}

impl EventEmitter<ConnectEvent> for ConnectorConnect {}

impl ConnectorConnect {
    pub fn new(
        target: Target,
        integrations: Vec<Integration>,
        api: Option<DesktopApi>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let token = cx.new(|cx| TextInput::new(window, cx).masked());
        let client_label = cx.t("settings.connectors.connect.clientIdLabel");
        let client_id = cx.new(|cx| TextInput::new(window, cx).with_placeholder(client_label));
        let secret_label = cx.t("settings.connectors.connect.ownApp.secretLabel");
        let client_secret = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder(secret_label)
                .masked()
        });
        let inputs = [&token, &client_id, &client_secret]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |_, _, event, cx| {
                    if matches!(event, TextInputEvent::Changed) {
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            target,
            integrations,
            api,
            phase: Phase::Intro,
            token,
            client_id,
            client_secret,
            login_id: None,
            copied: None,
            closed: false,
            _inputs: inputs,
        }
    }

    pub(super) fn text(&self, input: &Entity<TextInput>, cx: &gpui::App) -> String {
        input.read(cx).text().trim().to_string()
    }

    /// The logo already saved for this connector, outside the catalog.
    pub(super) fn logo_uri(&self) -> Option<String> {
        if self.target.preset.is_some() {
            return None;
        }
        integration_for_url(&self.integrations, &self.target.url)
            .and_then(|found| found.logo.clone())
    }

    pub(in super::super) fn primary_disabled(&self, cx: &gpui::App) -> bool {
        match self.target.auth() {
            Auth::Token => self.text(&self.token, cx).is_empty(),
            Auth::SlackApp => self.text(&self.client_id, cx).is_empty(),
            Auth::OAuth => false,
        }
    }

    pub(super) fn primary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let attempt = match self.target.auth() {
            Auth::Token => Attempt::Token(self.text(&self.token, cx)),
            Auth::SlackApp => Attempt::OAuth {
                client_id: Some(self.text(&self.client_id, cx)),
                client_secret: None,
            },
            Auth::OAuth => Attempt::OAuth {
                client_id: None,
                client_secret: None,
            },
        };
        self.run(attempt, window, cx);
    }

    pub(super) fn connect_own_app(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let secret = self.text(&self.client_secret, cx);
        let attempt = Attempt::OAuth {
            client_id: Some(self.text(&self.client_id, cx)),
            client_secret: (!secret.is_empty()).then_some(secret),
        };
        self.run(attempt, window, cx);
    }

    pub(super) fn connect_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let token = self.text(&self.token, cx);
        self.run(Attempt::Token(token), window, cx);
    }

    fn failed(&mut self, message: String, cx: &mut Context<Self>) {
        self.phase = Phase::Failed(message);
        cx.notify();
    }

    fn finish(&mut self, tools: Option<usize>, cx: &mut Context<Self>) {
        let name = Args::default().with("name", self.target.name.clone());
        let text = match tools {
            Some(count) => cx.t_with(
                "settings.connectors.toast.connectedTools",
                &Args::count(count as i64).with("name", self.target.name.clone()),
            ),
            None => cx.t_with("settings.connectors.toast.connected", &name),
        };
        Toaster::success(text, cx);
        cx.emit(ConnectEvent::Finished);
        self.close(cx);
    }

    fn failure_text(&self, reason: &str, message: &str, cx: &gpui::App) -> String {
        cx.t_with(
            &format!("settings.connectors.failure.{reason}"),
            &Args::default()
                .with("name", self.target.name.clone())
                .with("message", message.to_string()),
        )
        .to_string()
    }

    fn run(&mut self, attempt: Attempt, window: &mut Window, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            self.failed(cx.t("native.desktopClosed").to_string(), cx);
            return;
        };
        self.phase = Phase::Waiting;
        cx.notify();
        let (target, integrations) = (self.target.clone(), self.integrations.clone());
        match attempt {
            Attempt::Token(token) => {
                let failed_text = cx.t("settings.connectors.status.failed").to_string();
                cx.spawn(async move |this, cx| {
                    // A new token replaces the old one, not a second connector.
                    let done = cx
                        .background_executor()
                        .spawn(async move {
                            let previous = integration_for_url(&integrations, &target.url).cloned();
                            let integration =
                                api.ensure_connector(&target, &integrations, Some(&token))?;
                            let check = api.test_connector(&integration.definition.id)?;
                            if check.state != "connected" {
                                let _ = api.remove_connector(&integration.definition.id);
                                return Ok(Err(check.message.unwrap_or(failed_text)));
                            }
                            if let Some(previous) = previous {
                                let _ = api.remove_connector(&previous.definition.id);
                            }
                            Ok::<_, ApiError>(Ok(check.tools.len()))
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        match done {
                            Ok(Ok(tools)) => this.finish(Some(tools), cx),
                            Ok(Err(message)) => this.failed(message, cx),
                            Err(error) => this.failed(error.to_string(), cx),
                        }
                        cx.emit(ConnectEvent::Finished);
                    });
                })
                .detach();
            }
            Attempt::OAuth {
                client_id,
                client_secret,
            } => {
                cx.spawn_in(window, async move |this, cx| {
                    let started = cx
                        .background_executor()
                        .spawn(async move {
                            let new = integration_for_url(&integrations, &target.url).is_none();
                            let integration = api.ensure_connector(&target, &integrations, None)?;
                            let id = integration.definition.id.clone();
                            let start = api.start_connect(
                                &id,
                                client_id.as_deref(),
                                client_secret.as_deref(),
                            );
                            Ok::<_, ApiError>((api, id, new, start))
                        })
                        .await;
                    let (api, id, new, start) = match started {
                        Ok((api, id, new, Ok(start))) => (api, id, new, start),
                        Ok((api, id, new, Err(error))) => {
                            if new {
                                let _ = api.remove_connector(&id);
                            }
                            let _ = this.update(cx, |this, cx| {
                                this.failed(error.to_string(), cx);
                                cx.emit(ConnectEvent::Finished);
                            });
                            return;
                        }
                        Err(error) => {
                            let _ = this.update(cx, |this, cx| {
                                this.failed(error.to_string(), cx);
                                cx.emit(ConnectEvent::Finished);
                            });
                            return;
                        }
                    };
                    let finish = match start {
                        Start::NoAuthRequired => {
                            let tester = api.clone();
                            let test_id = id.clone();
                            match cx
                                .background_executor()
                                .spawn(async move { tester.test_connector(&test_id) })
                                .await
                            {
                                Ok(check) => Finish::Connected { check: Some(check) },
                                Err(error) => Finish::Failed {
                                    reason: "other".into(),
                                    message: error.to_string(),
                                },
                            }
                        }
                        Start::Failed { reason, message } => Finish::Failed { reason, message },
                        Start::Authorize {
                            login_id,
                            authorization_url,
                        } => {
                            let _ = this.update(cx, |this, cx| {
                                this.login_id = Some(login_id.clone());
                                cx.open_url(&authorization_url);
                            });
                            let waiter = api.clone();
                            cx.background_executor()
                                .spawn(async move { waiter.wait_connect(&login_id) })
                                .await
                                .unwrap_or_else(|error| Finish::Failed {
                                    reason: "other".into(),
                                    message: error.to_string(),
                                })
                        }
                    };
                    // A connector this attempt added goes when the sign-in did not finish.
                    if new && matches!(finish, Finish::Failed { .. }) {
                        let remover = api.clone();
                        let _ = cx
                            .background_executor()
                            .spawn(async move { remover.remove_connector(&id) })
                            .await;
                    }
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.login_id = None;
                        cx.emit(ConnectEvent::Finished);
                        if this.closed {
                            return;
                        }
                        // Back in front after the browser.
                        window.activate_window();
                        match finish {
                            Finish::Connected { check } => {
                                this.finish(check.map(|check| check.tools.len()), cx)
                            }
                            Finish::Failed { reason, .. } if reason == "cancelled" => {
                                this.close(cx)
                            }
                            Finish::Failed { reason, .. }
                                if reason == "clientRequired"
                                    && this.target.auth() != Auth::SlackApp =>
                            {
                                this.phase = Phase::NeedsApp;
                                cx.notify();
                            }
                            Finish::Failed { reason, message } => {
                                let text = this.failure_text(&reason, &message, cx);
                                this.failed(text, cx);
                            }
                        }
                    });
                })
                .detach();
            }
        }
    }

    /// `close`: a sign-in waiting on the browser is cancelled.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        if let (Some(login_id), Some(api)) = (self.login_id.take(), self.api.clone()) {
            cx.background_executor()
                .spawn(async move {
                    let _ = api.cancel_connect(&login_id);
                })
                .detach();
        }
        cx.emit(ConnectEvent::Closed);
    }

    pub(super) fn copy(&mut self, what: &'static str, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.copied = Some(what);
        cx.notify();
    }
}
