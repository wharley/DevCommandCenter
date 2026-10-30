//! "Conectar GitHub/GitLab" (`ForgeConnectDialog`): `gh auth login` or
//! `glab auth login` in a `CommandDialog` (a terminal of the desktop's). The dialog
//! closes when the command ends well (or on Esc / ✕); then the new login is
//! looked for for up to 8s, the repositories without one get it, and it is
//! said.

use std::collections::HashSet;
use std::time::Duration;

use gpui::{
    AppContext, Context, Entity, EventEmitter, Render, Subscription, Task, Window, prelude::*,
};

use crate::data::desktop_api::DesktopApi;
use crate::data::forge_cli::Forge;
use crate::screens::terminal::command_dialog::{CommandDialog, CommandDialogEvent, Leading};
use crate::theme::color::hex;

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
    terminal: Entity<CommandDialog>,
    api: Option<DesktopApi>,
    /// The logins before signing in.
    baseline: HashSet<String>,
    pub closing: bool,
    _terminal: Subscription,
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
        let owner = format!("forge-connect:{}:{host}", forge.key());
        let leading = Leading::Dot(match forge {
            Forge::Gitlab => Some(hex(0xFC6D26)),
            Forge::Github => None,
        });
        let (command, detail) = (forge.shell_command(&host), format!("· {host}"));
        let terminal_api = api.clone();
        let terminal = cx.new(|cx| {
            CommandDialog::new(
                "forge-connect",
                owner,
                command,
                format!("Conectar {}", forge.label()),
                Some(detail.into()),
                leading,
                terminal_api,
                window,
                cx,
            )
        });
        // Ending well or closed: the login is looked for.
        let events = cx.subscribe(&terminal, |connect, _, event, cx| match event {
            CommandDialogEvent::Exited(Some(0)) | CommandDialogEvent::Closed => connect.close(cx),
            CommandDialogEvent::Exited(_) => {}
        });
        let mut connect = Self {
            forge,
            host,
            terminal,
            api,
            baseline: HashSet::new(),
            closing: false,
            _terminal: events,
            _probe: None,
        };
        connect.read_baseline(cx);
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

    /// `handleClose`: the shell ends, the dialog goes, the new login is
    /// looked for and the repositories get it.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.terminal.update(cx, |terminal, cx| terminal.close(cx));
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if self.closing {
            return gpui::div().into_any_element();
        }
        self.terminal.clone().into_any_element()
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
