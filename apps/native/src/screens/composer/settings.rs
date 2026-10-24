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

    pub(crate) fn provider(&self) -> Option<&ProviderDescriptor> {
        let id = self.session.provider_id.as_deref()?;
        self.catalog
            .as_ref()?
            .iter()
            .find(|provider| provider.id.0 == id)
    }

    pub(crate) fn model(&self) -> Option<&ProviderModelDescriptor> {
        let id = self.session.model.as_deref()?;
        self.provider()?.models.iter().find(|model| model.id == id)
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
        let provider = self.session.provider_id.as_deref()?;
        self.settings
            .approval(self.task(), provider, &self.supported_policies())
    }

    pub(crate) fn set_approval(&mut self, policy: ProviderApprovalPolicy, cx: &mut Context<Self>) {
        let Some(provider) = self.session.provider_id.clone() else {
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
        TurnOptions {
            provider_id: None,
            model: None,
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
            let providers = cx
                .background_executor()
                .spawn(async move {
                    endpoint
                        .or_else(DesktopApi::load)
                        .map(|api| api.providers())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.catalog_loading = false;
                match providers {
                    Some(Ok(providers)) => {
                        this.catalog = Some(Arc::new(providers));
                        this.catalog_at = Some(Instant::now());
                    }
                    // A closed or older desktop: try again at the next session switch.
                    _ => this.catalog_at = None,
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
