//! The picker's models as each provider's CLI lists them for the account it
//! runs as (`dcc_providers::model_discovery`, kept a few minutes per
//! account): read again when the catalog arrives and when the account
//! changes. A CLI that does not answer leaves the list as it was.

use std::sync::Arc;

use dcc_core::domain::provider::{HealthStatus, ProviderDescriptor};
use dcc_core::ports::ProviderRuntimeConfig;
use dcc_providers::model_discovery;
use gpui::Context;

use super::Composer;
use crate::data::account_selection;

impl Composer {
    /// The runtime a turn on `provider` runs with, its account's folder
    /// included (the desktop resolves it for turns; the list is read here).
    fn listing_runtime(&self, provider: &ProviderDescriptor) -> ProviderRuntimeConfig {
        let mut runtime = self.runtime_with_account(provider);
        runtime.config_dir = runtime.account_id.as_ref().and_then(|id| {
            account_selection::known()?
                .into_iter()
                .find(|account| &account.id == id)
                .map(|account| account.config_dir)
        });
        runtime
    }

    pub(crate) fn refresh_cli_models(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        let Some(catalog) = self.catalog.clone() else {
            return;
        };
        let asks: Vec<(String, ProviderRuntimeConfig)> = catalog
            .iter()
            .filter(|provider| {
                model_discovery::reads_models(&provider.id.0)
                    && provider.enabled
                    && matches!(provider.health, HealthStatus::Healthy)
            })
            .map(|provider| (provider.id.0.clone(), self.listing_runtime(provider)))
            .collect();
        if asks.is_empty() {
            return;
        }
        // A newer read replaces one still running (an account just changed).
        self.cli_models_read = Some(cx.spawn(async move |this, cx| {
            let answers = cx
                .background_executor()
                .spawn(async move {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .ok()?;
                    Some(
                        runtime.block_on(futures::future::join_all(asks.into_iter().map(
                            |(id, listing)| async move {
                                let models = model_discovery::discover(&id, Some(&listing)).await;
                                (id, models)
                            },
                        ))),
                    )
                })
                .await;
            let Some(answers) = answers else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                let Some(catalog) = this.catalog.as_ref() else {
                    return;
                };
                let mut providers = (**catalog).clone();
                for (id, models) in answers {
                    if let Some(provider) = providers.iter_mut().find(|p| p.id.0 == id) {
                        model_discovery::apply(&mut provider.models, models);
                    }
                }
                this.catalog = Some(Arc::new(providers));
                cx.notify();
            });
        }));
    }
}
