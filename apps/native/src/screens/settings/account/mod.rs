//! "Conta" (`ForgeCliIntegrationCard`): GitHub or GitLab, the host, who the
//! CLI is signed in as there, the login DCC uses, "Reverificar", and
//! "Conectar no app" / "Trocar conta" with the embedded sign-in. CodeRabbit
//! is not part of the native app.

mod connect;
mod view;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::{AppContext, Context, Entity, Subscription, Task, Window};

use super::Settings;
use crate::data::forge_cli::{CliAccounts, Forge};
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;
use crate::ui::text_input::{TextInput, TextInputEvent};
use connect::{ConnectEvent, ForgeConnect, connected_message};

/// A host typed is read after the typing stops.
const HOST_SETTLE: Duration = Duration::from_millis(400);

type Key = (Forge, String);

pub(super) struct Account {
    pub(super) forge: Forge,
    /// What the host field holds per forge (empty: the default host).
    hosts: HashMap<Forge, String>,
    pub(super) host_input: Entity<TextInput>,
    pub(super) accounts: HashMap<Key, CliAccounts>,
    pub(super) loading: HashSet<Key>,
    pub(super) known_hosts: HashMap<Forge, Vec<String>>,
    /// The logins seen last per host (`useForgeCliLoginsHealth`).
    seen: HashMap<Key, HashSet<String>>,
    pub(super) connect: Option<(Entity<ForgeConnect>, Subscription)>,
    started: bool,
    _settle: Option<Task<()>>,
    _host_events: Subscription,
}

impl Account {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let host_input = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).with_placeholder("github.com");
            input.set_text(Forge::Github.default_host(), cx);
            input
        });
        let events = cx.subscribe(&host_input, |settings, input, event, cx| {
            if matches!(event, TextInputEvent::Changed) {
                let text = input.read(cx).text().to_string();
                settings.account_host_typed(text, cx);
            }
        });
        Self {
            forge: Forge::Github,
            hosts: Forge::ALL
                .into_iter()
                .map(|forge| (forge, forge.default_host().to_string()))
                .collect(),
            host_input,
            accounts: HashMap::new(),
            loading: HashSet::new(),
            known_hosts: HashMap::new(),
            seen: HashMap::new(),
            connect: None,
            started: false,
            _settle: None,
            _host_events: events,
        }
    }

    /// As if Settings just opened (a desktop set by a test).
    #[cfg(test)]
    pub(super) fn reset(&mut self) {
        self.accounts.clear();
        self.loading.clear();
        self.known_hosts.clear();
        self.seen.clear();
        self.connect = None;
        self.started = false;
    }

    /// The host DCC asks about (`normalizeForgeHost`).
    pub(super) fn host(&self) -> String {
        self.forge
            .host(self.hosts.get(&self.forge).map_or("", String::as_str))
    }

    pub(super) fn current(&self) -> Option<&CliAccounts> {
        self.accounts.get(&(self.forge, self.host()))
    }
}

impl Settings {
    /// The CLIs run only once "Conta" shows.
    pub(super) fn ensure_account(&mut self, cx: &mut Context<Self>) {
        if self.account.started {
            return;
        }
        self.account.started = true;
        self.load_account(true, cx);
    }

    /// Who is signed in on the host, and the hosts the CLI knows.
    pub(super) fn load_account(&mut self, force: bool, cx: &mut Context<Self>) {
        let api = self.general.api();
        let forge = self.account.forge;
        let host = self.account.host();
        let key = (forge, host.clone());
        self.account.loading.insert(key.clone());
        cx.notify();
        let failure = cx
            .t_with(
                "settings.account.loadingError",
                &Args::default().with("provider", forge.label()),
            )
            .to_string();
        cx.spawn(async move |this, cx| {
            let ask = host.clone();
            let (accounts, hosts) = cx
                .background_executor()
                .spawn(async move {
                    let Some(api) = api else {
                        return (Err(failure), Vec::new());
                    };
                    let accounts = api
                        .forge_cli_accounts(forge, &ask, force)
                        .map_err(|error| error.to_string());
                    (accounts, api.forge_cli_hosts(forge).unwrap_or_default())
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let account = &mut settings.account;
                account.loading.remove(&key);
                account.known_hosts.insert(forge, hosts);
                let accounts = accounts
                    .unwrap_or_else(|message| CliAccounts::unreadable(forge, &host, message));
                let logins: HashSet<String> = accounts.logins().into_iter().collect();
                let changed = account
                    .seen
                    .insert(key.clone(), logins.clone())
                    .is_some_and(|before| before != logins);
                let pick = accounts
                    .ready()
                    .then(|| accounts.effective_login())
                    .flatten()
                    .filter(|login| accounts.selected_login.as_deref() != Some(login.as_str()));
                account.accounts.insert(key, accounts);
                if changed {
                    settings.backfill_forge(cx);
                }
                // The login DCC uses follows the CLI's when none is chosen.
                if let Some(login) = pick {
                    settings.choose_forge_login(login, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn backfill_forge(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                let _ = api.backfill_forge_bindings();
            })
            .detach();
    }

    pub(super) fn select_forge(&mut self, forge: Forge, cx: &mut Context<Self>) {
        let account = &mut self.account;
        account.forge = forge;
        let text = account.hosts.get(&forge).cloned().unwrap_or_default();
        account.host_input.update(cx, |input, cx| {
            input.set_placeholder(forge.default_host(), cx);
            input.set_text(text, cx);
        });
        if !self
            .account
            .accounts
            .contains_key(&(forge, self.account.host()))
        {
            self.load_account(false, cx);
        }
        cx.notify();
    }

    pub(super) fn set_forge_host(&mut self, host: String, cx: &mut Context<Self>) {
        self.account
            .host_input
            .update(cx, |input, cx| input.set_text(host.clone(), cx));
        self.account_host_typed(host, cx);
        cx.notify();
    }

    fn account_host_typed(&mut self, text: String, cx: &mut Context<Self>) {
        let forge = self.account.forge;
        self.account.hosts.insert(forge, text);
        cx.notify();
        self.account._settle = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOST_SETTLE).await;
            let _ = this.update(cx, |settings, cx| {
                if !settings
                    .account
                    .accounts
                    .contains_key(&(forge, settings.account.host()))
                {
                    settings.load_account(false, cx);
                }
            });
        }));
    }

    pub(super) fn choose_forge_login(&mut self, login: String, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        let (forge, host) = (self.account.forge, self.account.host());
        cx.spawn(async move |this, cx| {
            let chosen = cx
                .background_executor()
                .spawn(async move { api.select_forge_login(forge, &host, &login) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                if chosen.is_ok() {
                    settings.load_account(false, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn open_forge_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.account.connect.is_some() {
            return;
        }
        let (forge, host, api) = (self.account.forge, self.account.host(), self.general.api());
        let connect = cx.new(|cx| ForgeConnect::new(forge, host, api, window, cx));
        let events = cx.subscribe(&connect, move |settings, _, event, cx| match event {
            ConnectEvent::Hidden => cx.notify(),
            ConnectEvent::Finished(login) => {
                settings.account.connect = None;
                settings.load_account(true, cx);
                if let Some(login) = login {
                    Toaster::success(connected_message(forge, login), cx);
                }
                cx.notify();
            }
        });
        self.account.connect = Some((connect, events));
        cx.notify();
    }

    /// The sign-in over Settings, while it shows.
    pub(super) fn forge_connect_dialog(&self, cx: &gpui::App) -> Option<gpui::AnyElement> {
        use gpui::IntoElement;
        let (connect, _) = self.account.connect.as_ref()?;
        (!connect.read(cx).closing).then(|| connect.clone().into_any_element())
    }
}
