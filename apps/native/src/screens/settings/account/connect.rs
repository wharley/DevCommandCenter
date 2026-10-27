//! "Conectar GitHub/GitLab" (`ForgeConnectDialog`): `gh auth login` or
//! `glab auth login` in a terminal of the desktop's, drawn here. The dialog
//! closes when the command ends well (or on Esc / ✕); then the new login is
//! looked for for up to 8s, the repositories without one get it, and it is
//! said.

use std::collections::HashSet;
use std::time::Duration;

use futures::StreamExt;
use gpui::{
    AppContext, Context, Entity, EventEmitter, FontWeight, Render, Task, Window, div, prelude::*,
    px,
};

use crate::data::desktop_api::DesktopApi;
use crate::data::forge_cli::Forge;
use crate::data::terminal_api::{ShellSpec, TerminalEvent};
use crate::screens::terminal::{ShellContext, TerminalSession};
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, shortcut_keys};

/// `NEW_LOGIN_POLL_TIMEOUT_MS` / `NEW_LOGIN_POLL_INTERVAL_MS`
const PROBE_FOR: Duration = Duration::from_secs(8);
const PROBE_EVERY: Duration = Duration::from_secs(1);

pub enum ConnectEvent {
    /// The dialog is gone; the login is still being looked for.
    Hidden,
    /// Done: the login found, if any.
    Finished(Option<String>),
}

pub struct ForgeConnect {
    pub forge: Forge,
    pub host: String,
    pub session: Entity<TerminalSession>,
    api: Option<DesktopApi>,
    /// The logins before signing in.
    baseline: HashSet<String>,
    pub closing: bool,
    _stream: Option<Task<()>>,
    _probe: Option<Task<()>>,
}

impl EventEmitter<ConnectEvent> for ForgeConnect {}

impl ForgeConnect {
    pub fn new(
        forge: Forge,
        host: String,
        api: Option<DesktopApi>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let host = forge.host(&host);
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|shell| !shell.trim().is_empty())
            .unwrap_or_else(|| "/bin/zsh".into());
        let spec = ShellSpec {
            cwd: "/".into(),
            command: shell,
            args: vec!["-lc".into(), forge.shell_command(&host)],
            cols: 96,
            rows: 24,
        };
        let owner = format!("forge-connect:{}:{host}", forge.key());
        let session_api = api.clone();
        let session = cx.new(|cx| {
            TerminalSession::new(owner, spec, ShellContext::default(), session_api, cx)
                .without_banner()
        });
        window.focus(&session.read(cx).focus_handle(), cx);
        let mut connect = Self {
            forge,
            host,
            session,
            api,
            baseline: HashSet::new(),
            closing: false,
            _stream: None,
            _probe: None,
        };
        connect.read_baseline(cx);
        connect.follow(cx);
        connect
    }

    fn read_baseline(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        let (forge, host) = (self.forge, self.host.clone());
        cx.spawn(async move |this, cx| {
            let logins = cx
                .background_executor()
                .spawn(async move { api.forge_cli_logins(forge, &host).unwrap_or_default() })
                .await;
            let _ = this.update(cx, |connect, _| {
                connect.baseline = logins.into_iter().collect()
            });
        })
        .detach();
    }

    /// What the shell prints, from the desktop's stream (ours alone).
    fn follow(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<TerminalEvent>();
        std::thread::spawn(move || {
            let mut first = true;
            while !sender.is_closed() {
                if let Ok(events) = api.terminal_events() {
                    if !first && sender.unbounded_send(TerminalEvent::Lagged).is_err() {
                        return;
                    }
                    first = false;
                    for event in events {
                        if sender.unbounded_send(event).is_err() {
                            return;
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        self._stream = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = receiver.next().await {
                if this
                    .update(cx, |connect, cx| connect.dispatch(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn dispatch(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        let ours = |pty: &str, cx: &Context<Self>| self.session.read(cx).pty_id() == Some(pty);
        match event {
            TerminalEvent::Output { pty_id, data } if ours(&pty_id, cx) => {
                self.session
                    .update(cx, |session, cx| session.output(&data, cx));
            }
            TerminalEvent::Exit { pty_id, code } if ours(&pty_id, cx) => {
                self.session
                    .update(cx, |session, cx| session.exited(code, cx));
                if code == Some(0) {
                    self.close(cx);
                }
            }
            TerminalEvent::Lagged => self.session.update(cx, |session, cx| session.resync(cx)),
            _ => {}
        }
    }

    /// `handleClose`: the shell ends, the dialog goes, the new login is
    /// looked for and the repositories get it.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self._stream = None;
        self.session.update(cx, |session, _| session.kill());
        cx.emit(ConnectEvent::Hidden);
        cx.notify();
        let Some(api) = self.api.clone() else {
            cx.emit(ConnectEvent::Finished(None));
            return;
        };
        let (forge, host, baseline) = (self.forge, self.host.clone(), self.baseline.clone());
        self._probe = Some(cx.spawn(async move |this, cx| {
            let started = std::time::Instant::now();
            let mut last: Vec<String> = Vec::new();
            let mut found = None;
            while started.elapsed() < PROBE_FOR {
                let probe = api.clone();
                let probe_host = host.clone();
                if let Ok(logins) = cx
                    .background_executor()
                    .spawn(async move { probe.forge_cli_logins(forge, &probe_host) })
                    .await
                {
                    found = logins
                        .iter()
                        .find(|login| !baseline.contains(*login))
                        .cloned();
                    last = logins;
                    if found.is_some() || cfg!(test) {
                        break;
                    }
                }
                cx.background_executor().timer(PROBE_EVERY).await;
            }
            let login = found.or_else(|| last.first().cloned());
            if login.is_some() {
                let backfill = api.clone();
                let _ = cx
                    .background_executor()
                    .spawn(async move { backfill.backfill_forge_bindings() })
                    .await;
            }
            let _ = this.update(cx, |_, cx| cx.emit(ConnectEvent::Finished(login)));
        }));
    }
}

impl Render for ForgeConnect {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.closing {
            return div().into_any_element();
        }
        let colors = cx.theme().colors.clone();
        let dot = match self.forge {
            Forge::Gitlab => hex(0xFC6D26),
            Forge::Github => colors.foreground,
        };
        let close = cx.entity().downgrade();
        let focus = self.session.read(cx).focus_handle();
        Dialog::new("forge-connect", move |_, cx| {
            let _ = close.update(cx, |connect, cx| connect.close(cx));
        })
        .max_width(640.0)
        .padding(0.0)
        .gap(0.0)
        .close_button(false)
        .initial_focus(focus)
        .child(
            div()
                .debug_selector(|| "forge-connect".into())
                .w_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .h(px(40.0))
                        .px(px(12.0))
                        .border_b_1()
                        .border_color(colors.border.opacity(0.55))
                        .child(
                            div()
                                .flex()
                                .min_w_0()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(div().flex_none().size(px(8.0)).rounded_full().bg(dot))
                                .child(format!("Conectar {}", self.forge.label()))
                                .child(
                                    div()
                                        .truncate()
                                        .text_color(colors.muted_foreground.opacity(0.8))
                                        .child(format!("· {}", self.host)),
                                ),
                        )
                        .child(
                            div().ml_auto().child(
                                Button::new("forge-connect-close")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Sm)
                                    .padding_x(8.0)
                                    .muted()
                                    .on_click(cx.listener(|connect, _, _, cx| connect.close(cx)))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(shortcut_keys(&["Esc"], cx))
                                            .child(Icon::new("x").size(14.0).stroke(1.8)),
                                    ),
                            ),
                        ),
                )
                .child(
                    div()
                        .h(px(360.0))
                        .bg(colors.card)
                        .child(self.session.clone()),
                ),
        )
        .into_any_element()
    }
}

/// `connectedToastMessage` (Portuguese on the desktop too).
pub fn connected_message(forge: Forge, login: &str) -> String {
    if login.is_empty() {
        format!("{} conectado", forge.label())
    } else {
        format!("{} conectado como @{login}", forge.label())
    }
}
