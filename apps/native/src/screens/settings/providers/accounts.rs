//! "Contas" (`ProviderAccountsList`): the system login first, then the named
//! accounts oldest first, each with who it is signed in as (or "Entrar",
//! with the terminal command to copy), rename and remove on hover, and a
//! field to add one. Plan usage comes with the next part.

use std::collections::{HashMap, HashSet};

use dcc_core::domain::provider::ProviderAccount;
use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::super::Settings;
use crate::data::desktop_api::ApiError;
use crate::data::provider_accounts::{AccountStatus, SIGN_IN_CANCELLED, of_provider};
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;
use crate::ui::text_input::{TextInput, TextInputEvent};

/// A provider and one of its sign-ins (`None` = the system login).
pub(in super::super) type AccountKey = (String, Option<String>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum Status {
    Checking,
    Failed(String),
    Ready(AccountStatus),
}

pub(in super::super) struct Accounts {
    pub(super) list: Vec<ProviderAccount>,
    pub(in super::super) statuses: HashMap<AccountKey, Status>,
    pub(super) signing_in: HashSet<AccountKey>,
    pub(in super::super) new_label: Entity<TextInput>,
    pub(super) adding: bool,
    /// The account being renamed, its field and the field's events.
    pub(super) renaming: Option<(String, Entity<TextInput>, Subscription)>,
    pub(super) busy: HashSet<String>,
    /// "Remover a conta …?" waiting for an answer.
    pub(super) removing: Option<ProviderAccount>,
    _new_label_events: Subscription,
}

impl Accounts {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let placeholder = Translate::t(&**cx, "settings.model.accountsNamePlaceholder");
        let new_label = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let events = cx.subscribe(&new_label, |settings, _, event, cx| match event {
            TextInputEvent::Submitted => settings.add_account(cx),
            TextInputEvent::Changed => cx.notify(),
            _ => {}
        });
        Self {
            list: Vec::new(),
            statuses: HashMap::new(),
            signing_in: HashSet::new(),
            new_label,
            adding: false,
            renaming: None,
            busy: HashSet::new(),
            removing: None,
            _new_label_events: events,
        }
    }
}

fn message(error: ApiError, cx: &gpui::App) -> String {
    match error {
        ApiError::DesktopClosed => cx.t("native.desktopClosed").to_string(),
        error => error.to_string(),
    }
}

impl Settings {
    /// Who each of the provider's sign-ins is, asked once per opening.
    pub(super) fn load_account_statuses(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self.providers.current().cloned() else {
            return;
        };
        if !provider.capabilities.supports_accounts {
            return;
        }
        let api = self.general.api();
        let closed = cx.t("native.desktopClosed").to_string();
        let accounts = &mut self.providers.accounts;
        let keys: Vec<AccountKey> = std::iter::once(None)
            .chain(
                of_provider(&accounts.list, &provider.id.0)
                    .into_iter()
                    .map(|account| Some(account.id.clone())),
            )
            .map(|account| (provider.id.0.clone(), account))
            .filter(|key| !accounts.statuses.contains_key(key))
            .collect();
        if keys.is_empty() {
            return;
        }
        // No desktop: nothing can say who is signed in.
        let Some(api) = api else {
            for key in keys {
                accounts
                    .statuses
                    .insert(key, Status::Failed(closed.clone()));
            }
            cx.notify();
            return;
        };
        for key in &keys {
            accounts.statuses.insert(key.clone(), Status::Checking);
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            // One after another: the desktop runs each provider's CLI.
            for key in keys {
                let ask = key.clone();
                let api = api.clone();
                let status = cx
                    .background_executor()
                    .spawn(async move { api.provider_account_status(&ask.0, ask.1.as_deref()) })
                    .await;
                let updated = this.update(cx, |settings, cx| {
                    let status = match status {
                        Ok(status) => Status::Ready(status),
                        Err(error) => Status::Failed(message(error, cx)),
                    };
                    settings.providers.accounts.statuses.insert(key, status);
                    cx.notify();
                });
                if updated.is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    /// The accounts again (after adding, renaming or removing one).
    fn reload_accounts(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let accounts = cx
                .background_executor()
                .spawn(async move { api.provider_accounts() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                if let Ok(accounts) = accounts {
                    settings.providers.accounts.list = accounts;
                }
                settings.load_account_statuses(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn sign_in_account(&mut self, key: AccountKey, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        self.providers.accounts.signing_in.insert(key.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let ask = key.clone();
            let signed = cx
                .background_executor()
                .spawn(async move { api.sign_in_provider_account(&ask.0, ask.1.as_deref()) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let accounts = &mut settings.providers.accounts;
                accounts.signing_in.remove(&key);
                match signed {
                    Ok(status) => {
                        let done = match &status.email {
                            Some(email) => cx.t_with(
                                "settings.model.accountsSignInDoneAs",
                                &Args::default().with("email", email.clone()),
                            ),
                            None => cx.t("settings.model.accountsSignInDone"),
                        };
                        accounts.statuses.remove(&key);
                        Toaster::success(done, cx);
                        settings.load_account_statuses(cx);
                    }
                    Err(error) => {
                        let text = message(error, cx);
                        if text != SIGN_IN_CANCELLED {
                            Toaster::error(text, cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn cancel_sign_in(&mut self, key: AccountKey, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                let _ = api.cancel_provider_sign_in(&key.0, key.1.as_deref());
            })
            .detach();
    }

    pub(in super::super) fn add_account(&mut self, cx: &mut Context<Self>) {
        let label = self
            .providers
            .accounts
            .new_label
            .read(cx)
            .text()
            .trim()
            .to_string();
        let (Some(api), Some(provider)) = (self.general.api(), self.providers.current().cloned())
        else {
            return;
        };
        if label.is_empty() || self.providers.accounts.adding {
            return;
        }
        self.providers.accounts.adding = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let created = cx
                .background_executor()
                .spawn(async move { api.create_provider_account(&provider.id.0, &label) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let accounts = &mut settings.providers.accounts;
                accounts.adding = false;
                match created {
                    Ok(_) => {
                        accounts
                            .new_label
                            .update(cx, |input, cx| input.set_text("", cx));
                        settings.reload_accounts(cx);
                    }
                    Err(error) => {
                        let text = match error {
                            ApiError::DesktopClosed => cx.t("native.desktopClosed").to_string(),
                            error => error.to_string(),
                        };
                        Toaster::error(text, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn start_rename(
        &mut self,
        account: &ProviderAccount,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = account.id.clone();
        let input = cx.new(|cx| TextInput::new(window, cx));
        input.update(cx, |input, cx| {
            input.set_text(account.label.clone(), cx);
            input.focus_and_select_all(window, cx);
        });
        let original = account.label.clone();
        let events = cx.subscribe(&input, move |settings, input, event, cx| match event {
            TextInputEvent::Submitted | TextInputEvent::Blurred => {
                let label = input.read(cx).text().trim().to_string();
                settings.save_rename(label, original.clone(), cx);
            }
            TextInputEvent::Cancelled => {
                settings.providers.accounts.renaming = None;
                cx.notify();
            }
            TextInputEvent::Changed => {}
        });
        self.providers.accounts.renaming = Some((id, input, events));
        cx.notify();
    }

    pub(super) fn save_rename(&mut self, label: String, original: String, cx: &mut Context<Self>) {
        let Some((id, _, _)) = self.providers.accounts.renaming.as_ref() else {
            return;
        };
        let id = id.clone();
        if label == original || label.is_empty() {
            self.providers.accounts.renaming = None;
            cx.notify();
            return;
        }
        let Some(api) = self.general.api() else {
            return;
        };
        if !self.providers.accounts.busy.insert(id.clone()) {
            return;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let account = id.clone();
            let renamed = cx
                .background_executor()
                .spawn(async move { api.rename_provider_account(&account, &label) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.providers.accounts.busy.remove(&id);
                match renamed {
                    Ok(()) => {
                        settings.providers.accounts.renaming = None;
                        settings.reload_accounts(cx);
                    }
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn remove_account(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(account)) =
            (self.general.api(), self.providers.accounts.removing.take())
        else {
            return;
        };
        self.providers.accounts.busy.insert(account.id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let id = account.id.clone();
            let removed = cx
                .background_executor()
                .spawn(async move { api.remove_provider_account(&id) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let accounts = &mut settings.providers.accounts;
                accounts.busy.remove(&account.id);
                match removed {
                    Ok(()) => {
                        accounts
                            .statuses
                            .remove(&(account.provider_id.0.clone(), Some(account.id.clone())));
                        settings.reload_accounts(cx);
                    }
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
