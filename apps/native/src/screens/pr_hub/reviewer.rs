//! The built-in reviewer in the hub: who it is and what it can run on (the
//! shell hands over the sidebar's agents and the composer's providers and
//! pick, and the panel's own pick), and a pull request asked to open from
//! the agent's page.

use std::sync::Arc;

use dcc_core::domain::provider::{ProviderDescriptor, ProviderModelDescriptor};
use gpui::Context;

use super::PrHub;
use super::code::Tab;
use crate::data::account_selection;
use crate::data::agents::AgentRecord;
use crate::data::composer_settings::clamp_effort;
use crate::data::pr_review::AgentRun;
use crate::data::pr_review_jobs::PrReviewJobs;
use crate::data::pr_review_pick::ReviewPick;

/// What the reviewer runs on (`reviewProvider`, `reviewModel`), with the
/// panel's effort and account.
#[derive(Clone, Debug)]
pub(crate) struct ReviewTarget {
    pub provider: ProviderDescriptor,
    pub model: Option<ProviderModelDescriptor>,
    /// One of the model's levels; none when it has none.
    pub effort: Option<String>,
    /// `None`: the system login.
    pub account: Option<String>,
}

/// What the reviewer reads from outside the hub.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReviewerSources {
    pub reviewer: Option<AgentRecord>,
    pub providers: Arc<Vec<ProviderDescriptor>>,
    /// The composer's provider and model.
    pub pick: (Option<String>, Option<String>),
}

impl PrHub {
    /// The shell's latest agents, providers and pick.
    pub fn set_sources(
        &mut self,
        reviewer: Option<AgentRecord>,
        providers: Arc<Vec<ProviderDescriptor>>,
        pick: (Option<String>, Option<String>),
        cx: &mut Context<Self>,
    ) {
        self.sources = ReviewerSources {
            reviewer,
            providers,
            pick,
        };
        cx.notify();
    }

    /// Providers that can review read-only.
    pub(crate) fn review_providers(&self) -> Vec<&ProviderDescriptor> {
        self.sources
            .providers
            .iter()
            .filter(|provider| provider.capabilities.supports_read_only_delegation)
            .collect()
    }

    /// The panel's pick, else the reviewer's own provider, else the
    /// composer's, else the first that can review.
    pub(crate) fn review_target(&self) -> Option<ReviewTarget> {
        let providers = self.review_providers();
        let reviewer = self.sources.reviewer.as_ref();
        let (composer_provider, composer_model) = self.sources.pick.clone();
        let find = |id: Option<&str>| {
            providers
                .iter()
                .find(|provider| Some(provider.id.0.as_str()) == id)
                .copied()
        };
        let provider = find(self.review_pick.provider_id.as_deref())
            .or_else(|| find(reviewer.and_then(|r| r.provider_id.as_deref())))
            .or_else(|| find(composer_provider.as_deref()))
            .or_else(|| providers.first().copied())?;
        let id = provider.id.0.as_str();
        let picked = self.review_pick.on(id);
        let own = reviewer.filter(|r| r.provider_id.as_deref() == Some(id));
        let wanted = picked
            .and_then(|pick| pick.model.as_deref())
            .or_else(|| own.and_then(|r| r.model.as_deref()))
            .or_else(|| {
                (composer_provider.as_deref() == Some(id))
                    .then_some(composer_model.as_deref())
                    .flatten()
            });
        let model = provider
            .models
            .iter()
            .find(|model| Some(model.id.as_str()) == wanted)
            .or_else(|| provider.models.iter().find(|model| model.recommended))
            .or_else(|| provider.models.first());
        let effort = picked
            .and_then(|pick| pick.effort.as_deref())
            .or_else(|| own.and_then(|r| r.effort.as_deref()))
            .and_then(|effort| {
                let levels = &model?.effort_levels;
                (!levels.is_empty()).then(|| clamp_effort(effort, levels))
            });
        let account = provider
            .capabilities
            .supports_accounts
            .then(|| self.review_account(id, picked.and_then(|pick| pick.account.as_deref())))
            .flatten();
        Some(ReviewTarget {
            provider: provider.clone(),
            model: model.cloned(),
            effort,
            account,
        })
    }

    /// `runAgent`'s input; the effort only on the whole review.
    pub(crate) fn agent_run(
        &self,
        root: &str,
        prompt: String,
        as_reviewer: bool,
    ) -> Option<AgentRun> {
        let target = self.review_target()?;
        let provider_id = target.provider.id.0.clone();
        let reviewer = self.sources.reviewer.as_ref();
        let provider_runtime = (!cfg!(test))
            .then(|| crate::data::provider_runtime::for_provider(&provider_id))
            .flatten()
            .map(|mut runtime| {
                if target.provider.capabilities.supports_accounts {
                    runtime.account_id = target.account.clone();
                }
                runtime
            });
        Some(AgentRun {
            working_directory: root.to_string(),
            provider_runtime,
            provider_id,
            model: target.model.map(|model| model.id),
            prompt,
            agent_id: as_reviewer
                .then(|| reviewer.map(|r| r.id.clone()))
                .flatten(),
            effort: target.effort.filter(|_| as_reviewer),
        })
    }

    /// The account a review runs as: the panel's (`""` the system login),
    /// else the pick for new conversations; one that is gone is the system login.
    fn review_account(&self, provider_id: &str, picked: Option<&str>) -> Option<String> {
        let known = account_selection::known();
        let Some(picked) = picked else {
            return account_selection::resolve(
                provider_id,
                None,
                &self.account_picks,
                known.as_deref(),
            );
        };
        let exists = known.as_deref().is_none_or(|accounts| {
            accounts
                .iter()
                .any(|account| account.id == picked && account.provider_id.0 == provider_id)
        });
        (!picked.is_empty() && exists).then(|| picked.to_string())
    }

    /// The panel's pick, kept for the next reviews.
    pub(crate) fn set_review_pick(&mut self, pick: ReviewPick, cx: &mut Context<Self>) {
        crate::data::pr_review_pick::save(&pick);
        self.review_pick = pick;
        cx.notify();
    }

    /// `requestOpenPullRequest`: the hub on that pull request's code tab,
    /// under "Todos", once the list has it.
    pub fn take_requested(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = PrReviewJobs::take_pending_open(cx) {
            self.requested = Some(id);
        }
        self.follow_request(cx);
    }

    pub(crate) fn follow_request(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.requested.clone() else {
            return;
        };
        if self.filter != crate::data::pr_hub::Filter::All {
            self.filter = crate::data::pr_hub::Filter::All;
        }
        // The room's change card knows the pull request by its link.
        let found = self
            .visible(cx)
            .into_iter()
            .find(|item| item.id == id || item.url == id);
        if let Some(item) = found {
            self.requested = None;
            self.selected = Some(item.id);
            self.detail_open = true;
            self.tab = Tab::Code;
            self.ensure_detail(cx);
        } else if matches!(self.list, Some(Ok(_))) && self.search.read(cx).text().is_empty() {
            // Gone (merged, closed): stop waiting for it.
            self.requested = None;
        }
        cx.notify();
    }
}
