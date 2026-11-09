//! Pull requests an agent asked DCC to review from its conversation
//! (`dcc_review_pull_request`): the sidebar's read brings the waiting ones
//! and each is taken once — marked in the desktop — and run as the hub's
//! own review ("Gerar revisão") on the agent's provider, in the person's
//! checkout of the repository. The team room shows it working and then its
//! result; its drafts wait in Pull Requests. No task, nothing published.

use std::collections::HashSet;
use std::sync::Arc;

use dcc_core::domain::pr_review_request::PrReviewRequest;
use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{App, Context, Global};

use super::Shell;
use super::review_trigger::{Checkout, hub_item, local_repository, review_run};
use crate::data::desktop_api::DesktopApi;
use crate::data::pr_hub::Reviewed;
use crate::data::pr_review_agent::review_prompt;
use crate::data::pr_review_jobs::PrReviewJobs;
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

/// The requests this run of the app took, so a read before the desktop
/// marks one does not start it twice.
#[derive(Default)]
struct Taken(HashSet<String>);

impl Global for Taken {}

/// Tells the desktop the request is taken, off the UI thread.
fn mark_taken(request: &PrReviewRequest, cx: &mut App) {
    let (agent, id) = (request.agent_id.clone(), request.id.clone());
    cx.background_executor()
        .spawn(async move {
            if let Some(api) = DesktopApi::load() {
                let _ = api.take_review_request(&agent, &id);
            }
        })
        .detach();
}

impl Shell {
    /// Starts the review of each request not taken yet.
    pub(super) fn review_requests_arrived(
        &mut self,
        requests: &[PrReviewRequest],
        cx: &mut Context<Self>,
    ) {
        let agents = self.sidebar.read(cx).agents().to_vec();
        let composer = self.conversation.read(cx).composer.read(cx);
        let providers: Arc<Vec<ProviderDescriptor>> = composer.catalog.clone().unwrap_or_default();
        let pick = (
            composer.session.provider_id.clone(),
            composer.session.model.clone(),
        );
        let db = crate::data::loader::database_path();
        for request in requests {
            if !cx.default_global::<Taken>().0.insert(request.id.clone()) {
                continue;
            }
            mark_taken(request, cx);
            // An agent deleted meanwhile asks nothing.
            let Some(agent) = agents.iter().find(|agent| agent.id == request.agent_id) else {
                continue;
            };
            let checkout = local_repository(&db, &request.repository);
            let pr = Reviewed {
                url: request.url.clone(),
                number: request.number as u32,
                repository: request.repository.clone(),
                ..Reviewed::default()
            };
            let found = checkout.and_then(|Checkout { root, login, .. }| {
                let item = hub_item(&pr, &root, login)?;
                review_run(agent, &providers, &pick, &root, String::new()).map(|_| item)
            });
            let Some(item) = found else {
                Toaster::error(
                    cx.t_with(
                        "agents.reviewRequest.noCheckout",
                        &Args::default()
                            .with("agent", agent.name.clone())
                            .with("repository", request.repository.clone()),
                    ),
                    cx,
                );
                continue;
            };
            let label = format!("{}#{}", item.repository_name, item.number);
            if !PrReviewJobs::start(cx, &item.id, label.clone(), String::new()) {
                // Already under review: that one's result answers this too.
                continue;
            }
            PrReviewJobs::asked_by(cx, &item.id, &agent.id);
            let (agent, providers, pick) = (agent.clone(), providers.clone(), pick.clone());
            cx.spawn(async move |_, cx| {
                let id = item.id.clone();
                let name = agent.name.clone();
                let answer = cx
                    .background_executor()
                    .spawn(async move {
                        let api =
                            DesktopApi::load().ok_or_else(|| "DCC is not running".to_string())?;
                        let detail = api
                            .pr_hub_detail(&item, true)
                            .map_err(|error| error.to_string())?;
                        let prompt = review_prompt(&item, &detail, "");
                        let run =
                            review_run(&agent, &providers, &pick, &item.repository_root, prompt)
                                .ok_or_else(|| "no provider can review".to_string())?;
                        api.pr_review_agent(&run).map_err(|error| error.to_string())
                    })
                    .await;
                let _ = cx.update(|cx| {
                    let key = if answer.is_ok() {
                        "agents.room.card.reviewed"
                    } else {
                        "agents.room.card.reviewFailed"
                    };
                    let text = cx.t_with(
                        key,
                        &Args::default().with("agent", name).with("task", label),
                    );
                    crate::ui::attention::notify_away(&text, "");
                    PrReviewJobs::finish(cx, &id, answer);
                });
            })
            .detach();
        }
    }
}
