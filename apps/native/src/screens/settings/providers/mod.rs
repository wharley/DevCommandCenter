//! "Provedores" (`ProvidersSettings`): the providers on the left with one
//! line of status, and the chosen one on the right — whether it is on, the
//! model new tasks start with, and how DCC launches it. Accounts and plan
//! usage come with the next part.

mod account_rows;
mod accounts;
mod antigravity;
mod runtime;
mod usage;
mod usage_resets;
mod view;

#[cfg(test)]
pub(super) use accounts::Status as AccountStatusView;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::{Settings, SettingsEvent};
use crate::data::provider_runtime::{Draft, Store};
use crate::data::provider_selection::Pick;
use crate::ui::text_input::{TextInput, TextInputEvent};

/// The advanced fields typed into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Field {
    Binary,
    Home,
    ShadowHome,
}

pub(super) struct Providers {
    pub(super) catalog: Option<Vec<ProviderDescriptor>>,
    /// The provider clicked in the list (else the default's, else the first).
    pub(super) chosen: Option<String>,
    /// New tasks' provider and model (`dcc.selectedProviderId/ModelId`).
    pub(super) pick: Option<Pick>,
    pub(super) pending: HashSet<String>,
    pub(super) errors: HashMap<String, String>,
    pub(super) checking: bool,
    pub(super) runtime: Store,
    runtime_path: PathBuf,
    /// "Configuração avançada" open; starts open when something is set.
    pub(super) advanced_open: bool,
    /// The provider the fields were filled for.
    filled_for: Option<String>,
    pub(super) binary: Entity<TextInput>,
    pub(super) home: Entity<TextInput>,
    pub(super) shadow_home: Entity<TextInput>,
    pub(super) accounts: accounts::Accounts,
    pub(super) usage: usage::Usage,
    pub(super) antigravity: antigravity::Antigravity,
    _inputs: Vec<Subscription>,
}

impl Providers {
    pub(super) fn new(pick: Option<Pick>, window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let t = |key: &str| crate::i18n::Translate::t(&**cx, key);
        let placeholders = [
            t("settings.model.runtimeBinaryPlaceholder"),
            t("settings.model.runtimeHomePlaceholder"),
            t("settings.model.runtimeShadowHomePlaceholder"),
        ];
        let [binary, home, shadow_home] = placeholders.map(|placeholder| {
            cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder))
        });
        let inputs = [
            (binary.clone(), Field::Binary),
            (home.clone(), Field::Home),
            (shadow_home.clone(), Field::ShadowHome),
        ]
        .into_iter()
        .map(|(input, field)| {
            cx.subscribe(&input, move |settings, _, event, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    settings.runtime_typed(field, cx);
                }
            })
        })
        .collect();
        let runtime_path = crate::data::provider_runtime::path();
        Self {
            catalog: None,
            chosen: None,
            pick,
            pending: HashSet::new(),
            errors: HashMap::new(),
            checking: false,
            runtime: if cfg!(test) {
                Store::default()
            } else {
                Store::load(&runtime_path)
            },
            runtime_path,
            advanced_open: false,
            filled_for: None,
            binary,
            home,
            shadow_home,
            accounts: accounts::Accounts::new(window, cx),
            usage: usage::Usage::default(),
            antigravity: antigravity::Antigravity::default(),
            _inputs: inputs,
        }
    }

    #[cfg(test)]
    pub(super) fn set_runtime_path(&mut self, path: PathBuf) {
        self.runtime = Store::load(&path);
        self.runtime_path = path;
    }

    /// The provider on the right.
    pub(super) fn current(&self) -> Option<&ProviderDescriptor> {
        let catalog = self.catalog.as_ref()?;
        let find = |id: Option<&str>| {
            id.and_then(|id| catalog.iter().find(|provider| provider.id.0 == id))
        };
        find(self.chosen.as_deref())
            .or_else(|| find(self.pick.as_ref().map(|pick| pick.provider_id.as_str())))
            .or_else(|| catalog.first())
    }
}

impl Settings {
    /// The catalog from the desktop (else the built-in one checked here),
    /// with the executables set in Advanced checked.
    pub(super) fn load_providers(&mut self, cx: &mut Context<Self>) {
        let api = self.general.api();
        let db_path = crate::data::loader::database_path();
        cx.spawn(async move |this, cx| {
            let (providers, accounts) = cx
                .background_executor()
                .spawn(async move {
                    let accounts = api.as_ref().and_then(|api| api.provider_accounts().ok());
                    if let Some(Ok(providers)) = api.map(|api| api.providers()) {
                        return (providers, accounts);
                    }
                    if cfg!(test) {
                        return (dcc_providers::offline_catalog().providers, accounts);
                    }
                    let providers = crate::data::provider_catalog::checked(&db_path)
                        .unwrap_or_else(|| crate::data::provider_catalog::offline(&db_path));
                    (providers, accounts)
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                if let Some(accounts) = accounts {
                    // The desktop answers: what it could not say is asked again.
                    settings
                        .providers
                        .accounts
                        .statuses
                        .retain(|_, status| !matches!(status, accounts::Status::Failed(_)));
                    settings.providers.accounts.list = accounts;
                }
                settings.load_account_statuses(cx);
                let mut providers = providers;
                settings
                    .providers
                    .runtime
                    .apply_executable_health(&mut providers);
                settings.providers.catalog = Some(providers);
                settings.providers.checking = false;
                settings.fill_runtime_fields(cx);
                settings.ensure_usage(cx);
                settings.ensure_antigravity(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn choose_provider(&mut self, provider_id: String, cx: &mut Context<Self>) {
        self.providers.chosen = Some(provider_id);
        self.fill_runtime_fields(cx);
        self.load_account_statuses(cx);
        self.ensure_usage(cx);
        self.ensure_antigravity(cx);
        cx.notify();
    }

    /// The fields show the current provider's draft (`key={current.id}`).
    fn fill_runtime_fields(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .providers
            .current()
            .map(|provider| provider.id.0.clone())
        else {
            return;
        };
        if self.providers.filled_for.as_deref() == Some(id.as_str()) {
            return;
        }
        let draft = self.providers.runtime.draft(&id);
        self.providers.advanced_open = draft.customized();
        self.providers.filled_for = Some(id);
        for (input, text) in [
            (&self.providers.binary, draft.binary_path),
            (&self.providers.home, draft.home_path),
            (&self.providers.shadow_home, draft.shadow_home_path),
        ] {
            input.update(cx, |input, cx| {
                if input.text() != text {
                    input.set_text(text, cx);
                }
            });
        }
    }

    pub(super) fn runtime_typed(&mut self, field: Field, cx: &mut Context<Self>) {
        let Some(id) = self.providers.filled_for.clone() else {
            return;
        };
        let input = match field {
            Field::Binary => &self.providers.binary,
            Field::Home => &self.providers.home,
            Field::ShadowHome => &self.providers.shadow_home,
        };
        let text = input.read(cx).text().to_string();
        let mut draft = self.providers.runtime.draft(&id);
        let slot = match field {
            Field::Binary => &mut draft.binary_path,
            Field::Home => &mut draft.home_path,
            Field::ShadowHome => &mut draft.shadow_home_path,
        };
        if *slot == text {
            return;
        }
        *slot = text;
        self.save_runtime(&id, draft, cx);
    }

    pub(super) fn set_subagents(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(id) = self.providers.filled_for.clone() else {
            return;
        };
        let draft = Draft {
            max_concurrent_subagents: value,
            ..self.providers.runtime.draft(&id)
        };
        self.save_runtime(&id, draft, cx);
    }

    /// "Redefinir": back to how the provider runs by default.
    pub(super) fn clear_runtime(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.providers.filled_for.clone() else {
            return;
        };
        self.save_runtime(&id, Draft::default(), cx);
        self.providers.filled_for = None;
        let open = self.providers.advanced_open;
        self.fill_runtime_fields(cx);
        self.providers.advanced_open = open;
    }

    fn save_runtime(&mut self, provider_id: &str, draft: Draft, cx: &mut Context<Self>) {
        let providers = &mut self.providers;
        providers.runtime.set(provider_id, draft);
        if let Err(error) = providers.runtime.save(&providers.runtime_path) {
            eprintln!("[dcc-native] could not save the provider runtime: {error}");
        }
        // A configured executable that cannot run means setup, at once.
        if let Some(catalog) = providers.catalog.as_mut()
            && let Some(provider) = catalog.iter_mut().find(|p| p.id.0 == provider_id)
        {
            if let dcc_core::domain::provider::HealthStatus::Degraded { reason } = &provider.health
                && reason.contains("executable path is invalid")
            {
                provider.health = dcc_core::domain::provider::HealthStatus::Healthy;
            }
            providers
                .runtime
                .apply_executable_health(std::slice::from_mut(provider));
        }
        cx.emit(SettingsEvent::ProvidersChanged);
        cx.notify();
    }

    pub(super) fn set_availability(
        &mut self,
        provider_id: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let api = self.general.api();
        self.providers.pending.insert(provider_id.clone());
        self.providers.errors.remove(&provider_id);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let id = provider_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api.ok_or(crate::data::desktop_api::ApiError::DesktopClosed)?
                        .set_provider_availability(&id, enabled)
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.providers.pending.remove(&provider_id);
                match result {
                    Ok(()) => {
                        if let Some(provider) =
                            settings.providers.catalog.as_mut().and_then(|catalog| {
                                catalog.iter_mut().find(|p| p.id.0 == provider_id)
                            })
                        {
                            provider.enabled = enabled;
                        }
                        cx.emit(SettingsEvent::ProvidersChanged);
                    }
                    Err(error) => {
                        let message = match error {
                            crate::data::desktop_api::ApiError::DesktopClosed => {
                                crate::i18n::Translate::t(&**cx, "native.desktopClosed").to_string()
                            }
                            error => error.to_string(),
                        };
                        settings.providers.errors.insert(provider_id, message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Verificar de novo": signing in from a terminal does not tell DCC.
    pub(super) fn recheck_providers(&mut self, cx: &mut Context<Self>) {
        self.providers.checking = true;
        cx.notify();
        self.load_providers(cx);
        cx.emit(SettingsEvent::ProvidersChanged);
    }

    /// A model of the list becomes the default for new tasks.
    pub(super) fn pick_model(
        &mut self,
        provider_id: String,
        model_id: String,
        cx: &mut Context<Self>,
    ) {
        let pick = Pick {
            provider_id,
            model_id,
        };
        self.providers.pick = Some(pick.clone());
        cx.emit(SettingsEvent::DefaultPick(pick));
        cx.notify();
    }
}
