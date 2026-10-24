//! What the next turn runs with: the provider catalog read from the desktop
//! (the phone's `/api/v1/mobile/catalog`, a minute at a time) and the
//! composer's choices (`data::composer_settings`), resolved like the
//! desktop's composer does.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dcc_core::domain::provider::{
    ProviderApprovalPolicy, ProviderDescriptor, ProviderModelDescriptor, TurnControlSupport,
};
use gpui::Context;

use super::Composer;
use crate::data::composer_settings::{self, ComposerSettings, DEFAULT_EFFORT_LEVELS, EffortChoice};
use crate::data::desktop_api::{DesktopApi, TurnOptions};
use crate::prefs;

/// The catalog is read again after this.
const CATALOG_STALE: Duration = Duration::from_secs(60);
const SETTINGS_FILE: &str = "composer-settings.json";

pub(crate) fn load_settings() -> ComposerSettings {
    if cfg!(test) {
        return ComposerSettings::default();
    }
    ComposerSettings::load(&prefs::native_file(SETTINGS_FILE))
}

impl Composer {
    fn task(&self) -> &str {
        self.session.task_id.as_deref().unwrap_or_default()
    }

    /// The conversation's pick in the picker, while it still applies.
    fn selection(&self) -> Option<&composer_settings::Selection> {
        let session = self.session.session_id.as_deref()?;
        let provider = self.session.provider_id.as_deref()?;
        self.settings.selection(session, provider)
    }

    /// The provider the next turn runs on: the pick, else the session's.
    pub(crate) fn selected_provider_id(&self) -> Option<String> {
        self.selection()
            .map(|selection| selection.provider_id.clone())
            .or_else(|| self.session.provider_id.clone())
    }

    pub(crate) fn selected_model_id(&self) -> Option<String> {
        match self.selection() {
            Some(selection) => selection.model_id.clone(),
            None => self.session.model.clone(),
        }
    }

    pub(crate) fn provider(&self) -> Option<&ProviderDescriptor> {
        let id = self.selected_provider_id()?;
        self.catalog
            .as_ref()?
            .iter()
            .find(|provider| provider.id.0 == id)
    }

    pub(crate) fn model(&self) -> Option<&ProviderModelDescriptor> {
        let id = self.selected_model_id()?;
        self.provider()?.models.iter().find(|model| model.id == id)
    }

    /// The picker chose a provider (its first model) or a model.
    pub(crate) fn select_model(
        &mut self,
        provider_id: String,
        model_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let (Some(session), Some(session_provider)) = (
            self.session.session_id.clone(),
            self.session.provider_id.clone(),
        ) else {
            return;
        };
        let model_id = model_id.or_else(|| {
            self.catalog
                .as_ref()?
                .iter()
                .find(|provider| provider.id.0 == provider_id)?
                .models
                .first()
                .map(|model| model.id.clone())
        });
        self.settings.set_selection(
            &session,
            composer_settings::Selection {
                provider_id,
                model_id,
                session_provider_id: session_provider,
            },
        );
        self.save_settings();
        cx.notify();
    }

    pub(crate) fn set_effort(&mut self, choice: EffortChoice, cx: &mut Context<Self>) {
        let task = self.task().to_string();
        self.settings.set_effort(&task, choice);
        self.save_settings();
        cx.notify();
    }

    pub(crate) fn set_fast(&mut self, fast: bool, cx: &mut Context<Self>) {
        self.settings.fast = fast;
        self.save_settings();
        cx.notify();
    }

    pub(crate) fn set_favorites(
        &mut self,
        favorites: Vec<crate::data::model_picker::ModelFavorite>,
        cx: &mut Context<Self>,
    ) {
        self.settings.favorites = crate::data::model_picker::clean_favorites(favorites);
        self.save_settings();
        cx.notify();
    }

    /// `selectedModel?.effortLevels ?? DEFAULT_EFFORT_LEVELS`
    pub(crate) fn effort_levels(&self) -> Vec<String> {
        match self.model() {
            Some(model) => model.effort_levels.clone(),
            None => DEFAULT_EFFORT_LEVELS
                .iter()
                .map(|level| level.to_string())
                .collect(),
        }
    }

    pub(crate) fn effort_choice(&self) -> EffortChoice {
        self.settings.effort(self.task())
    }

    /// The effort the chip shows (`ultrathink` when picked).
    pub(crate) fn effort_label_id(&self) -> String {
        let choice = self.effort_choice();
        if choice.ultrathink {
            "ultrathink".into()
        } else {
            composer_settings::clamp_effort(&choice.effort, &self.effort_levels())
        }
    }

    pub(crate) fn supported_policies(&self) -> Vec<ProviderApprovalPolicy> {
        self.provider()
            .map(|provider| provider.capabilities.approval_policies.clone())
            .unwrap_or_default()
    }

    pub(crate) fn approval(&self) -> Option<ProviderApprovalPolicy> {
        let provider = self.selected_provider_id()?;
        self.settings
            .approval(self.task(), &provider, &self.supported_policies())
    }

    pub(crate) fn set_approval(&mut self, policy: ProviderApprovalPolicy, cx: &mut Context<Self>) {
        let Some(provider) = self.selected_provider_id() else {
            return;
        };
        let task = self.task().to_string();
        self.settings.set_approval(&task, &provider, policy);
        self.save_settings();
        cx.notify();
    }

    pub(crate) fn plan_mode(&self) -> bool {
        self.settings
            .plan(self.task(), self.session.session_id.as_deref())
    }

    pub(crate) fn plan_is_prompt_fallback(&self) -> bool {
        self.provider().is_some_and(|provider| {
            provider.capabilities.plan_mode_support == TurnControlSupport::PromptFallback
        })
    }

    pub(crate) fn toggle_plan(&mut self, cx: &mut Context<Self>) {
        let on = !self.plan_mode();
        let task = self.task().to_string();
        let session = self.session.session_id.clone();
        self.settings.set_plan(&task, session.as_deref(), on);
        self.save_settings();
        let placeholder = if on {
            cx_t(cx, "composer.placeholder.plan")
        } else {
            cx_t(cx, "composer.placeholder.default")
        };
        self.input
            .update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        cx.notify();
    }

    fn save_settings(&self) {
        if cfg!(test) {
            return;
        }
        if let Err(error) = self.settings.save(&prefs::native_file(SETTINGS_FILE)) {
            eprintln!("[dcc-native] could not save the composer settings: {error}");
        }
    }

    /// The choices a send carries (`submitFromComposer`'s envelope).
    pub(crate) fn turn_options(&self, prompt: &str) -> TurnOptions {
        let choice = self.effort_choice();
        let provider_id = self
            .selected_provider_id()
            .filter(|id| Some(id) != self.session.provider_id.as_ref());
        let model = self
            .selected_model_id()
            .filter(|id| provider_id.is_some() || Some(id) != self.session.model.as_ref());
        let delegation = self.catalog.as_ref().and_then(|catalog| {
            dcc_core::domain::turn_instructions::resolve_delegate_task_tool_instructions(
                self.provider(),
                catalog,
            )
        });
        TurnOptions {
            provider_id,
            model,
            tool_instructions: delegation,
            effort: Some(composer_settings::effective_effort(
                &choice.effort,
                &self.effort_levels(),
                choice.ultrathink,
                prompt,
            )),
            approval_policy: self
                .approval()
                .map(|policy| composer_settings::policy_key(policy).to_string()),
            plan_mode: self.plan_mode(),
            fast_mode: self.settings.fast,
        }
    }

    /// A retry runs as a plain turn (`buildAnnotationTurn`): stored effort
    /// and approval, no plan, no fast mode.
    pub(crate) fn retry_options(&self, prompt: &str) -> TurnOptions {
        TurnOptions {
            plan_mode: false,
            fast_mode: false,
            ..self.turn_options(prompt)
        }
    }

    /// Reads the catalog when there is none or it is a minute old.
    pub(crate) fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        if self.catalog_loading
            || self
                .catalog_at
                .is_some_and(|at| at.elapsed() < CATALOG_STALE)
        {
            return;
        }
        self.catalog_loading = true;
        let endpoint = self.endpoint.clone();
        cx.spawn(async move |this, cx| {
            let db_path = crate::data::loader::database_path();
            let offline_path = db_path.clone();
            let live = cx
                .background_executor()
                .spawn(async move {
                    match endpoint
                        .or_else(DesktopApi::load)
                        .map(|api| api.providers())
                    {
                        Some(Ok(providers)) => Ok(providers),
                        // A closed desktop: the built-in list with the
                        // person's on/off switches, at once.
                        _ => Err(crate::data::provider_catalog::offline(&offline_path)),
                    }
                })
                .await;
            let offline = match live {
                Ok(providers) => {
                    let _ = this.update(cx, |this, cx| {
                        this.catalog_loading = false;
                        this.catalog = Some(Arc::new(providers));
                        this.catalog_at = Some(Instant::now());
                        cx.notify();
                    });
                    return;
                }
                Err(offline) => offline,
            };
            if this
                .update(cx, |this, cx| {
                    this.catalog = Some(Arc::new(offline));
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            // Then the same list with each provider's health checked here, so
            // those not installed or signed in move to "precisam de configuração".
            let checked = if cfg!(test) {
                None
            } else {
                cx.background_executor()
                    .spawn(async move { crate::data::provider_catalog::checked(&db_path) })
                    .await
            };
            let _ = this.update(cx, |this, cx| {
                this.catalog_loading = false;
                if let Some(providers) = checked {
                    this.catalog = Some(Arc::new(providers));
                    this.catalog_at = Some(Instant::now());
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn cx_t(cx: &gpui::App, key: &str) -> gpui::SharedString {
    use crate::i18n::Translate;
    cx.t(key)
}
