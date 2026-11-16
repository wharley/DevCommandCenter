//! Pull requests an agent asked DCC to review from its conversation
//! (`dcc_review_pull_request`): the sidebar's read brings the waiting ones
//! and each is taken once — marked in the desktop — and run as the hub's
//! own review ("Gerar revisão") on the agent's provider, in the person's
//! checkout of the repository. The team room shows it working and then its
//! result; its drafts wait in Pull Requests. No task, and nothing published
//! unless the person granted that agent sending in the project
//! (`review_auto_send`).

use std::collections::HashSet;
use std::sync::Arc;

use dcc_core::domain::pr_review_request::PrReviewRequest;
use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{App, Context, Global};

use super::Shell;
use super::review_auto_send::{ReviewOutcome, landed, sent_prompt};
use super::review_trigger::{Checkout, hub_item, local_repository, read_and_review, review_run};
use crate::data::agent_triggers::AgentTriggers;
use crate::data::desktop_api::DesktopApi;
use crate::data::pr_hub::Reviewed;
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
            let found = checkout.and_then(
                |Checkout {
                     root,
                     login,
                     project_id,
                 }| {
                    let item = hub_item(&pr, &root, login)?;
                    review_run(agent, &providers, &pick, &root, String::new())
                        .map(|_| (item, project_id))
                },
            );
            let Some((item, project_id)) = found else {
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
            crate::data::pr_reviews_running::add(crate::data::pr_reviews_running::RunningReview {
                pr_id: item.id.clone(),
                started_at: chrono::Utc::now().to_rfc3339(),
                resumed: self.resuming_review,
                started: crate::data::pr_reviews_running::Started::Asked {
                    request: request.clone(),
                },
            });
            let send = cx
                .try_global::<AgentTriggers>()
                .map(|triggers| triggers.review_send(&agent.id, &project_id))
                .unwrap_or_default();
            let (agent, providers, pick) = (agent.clone(), providers.clone(), pick.clone());
            // The room of the conversation it asked from; none: Geral.
            let room = agent
                .sessions
                .iter()
                .find(|session| session.session_id == request.session_id)
                .and_then(|session| session.room.clone());
            let (asker, url) = (agent.clone(), request.url.clone());
            cx.spawn(async move |shell, cx| {
                let id = item.id.clone();
                let name = agent.name.clone();
                let answer = cx
                    .background_executor()
                    .spawn(async move { read_and_review(&agent, &providers, &pick, &item, send) })
                    .await;
                let _ = shell.update(cx, |shell, cx| {
                    // Merged or closed since it was asked: no review, no card.
                    let Some(ReviewOutcome { answer, sent }) = answer else {
                        return PrReviewJobs::dismiss(cx, &id);
                    };
                    let ok = answer.is_ok();
                    PrReviewJobs::finish(cx, &id, answer);
                    let went = sent.as_ref().and_then(|sent| sent.as_ref().ok()).cloned();
                    if landed(cx, &id, &name, &label, sent) {
                        // Whoever asked for it through a connector is answered.
                        if let Some(went) = went {
                            let prompt = sent_prompt(&label, &url, &went);
                            shell.tell_agent(&asker, room.as_deref(), prompt, cx);
                        }
                        return;
                    }
                    let key = if ok {
                        "agents.room.card.reviewed"
                    } else {
                        "agents.room.card.reviewFailed"
                    };
                    let text = cx.t_with(
                        key,
                        &Args::default().with("agent", name).with("task", label),
                    );
                    crate::ui::attention::notify_away(&text, "");
                });
            })
            .detach();
        }
    }
}
