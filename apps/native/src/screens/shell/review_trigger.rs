//! The Reviewer's first trigger: a pull request the person reviewed got
//! commits or replies (`PrWatch` announces it), so the Reviewer reviews it
//! again by itself — when the person turned that on for it and its ceilings
//! allow (`AgentTriggers`). The review is the hub's own ("Gerar revisão"):
//! its job shows "resultado pronto" and its drafts wait in the hub when the
//! person opens the pull request. Nothing is published.
//!
//! No `gh` is added to find out what changed: the news comes from the one
//! search `PrWatch` already makes. The run reads that pull request (its
//! patches) and runs the review, like the button does.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use dcc_core::domain::provider::ProviderDescriptor;
use dcc_core::domain::repository::{Repository, github_slug};
use dcc_core::ports::RepositoryRepo;
use dcc_infra::db::SqliteWorkspaceRepo;
use gpui::{App, BorrowAppContext, Context};

use super::Shell;
use crate::data::agent_triggers::{AgentTriggers, Trigger};
use crate::data::agents::AgentRecord;
use crate::data::composer_settings::clamp_effort;
use crate::data::desktop_api::DesktopApi;
use crate::data::pr_hub::{Actor, Item, Reviewed};
use crate::data::pr_review::AgentRun;
use crate::data::pr_review_agent::review_prompt;
use crate::data::pr_review_jobs::PrReviewJobs;
use crate::i18n::{Args, Translate};

/// The host of a pull request's URL (`github.com`).
fn url_host(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/')
        .next()
        .filter(|host| !host.is_empty())
        .map(str::to_string)
}

/// A checkout of a pull request's repository: its root, its forge login
/// and its project.
#[derive(Debug, PartialEq)]
pub(super) struct Checkout {
    pub root: String,
    pub login: Option<String>,
    pub project_id: String,
}

/// The person's checkout of `repository` (`owner/name`); the first one added
/// when there are several.
pub(super) fn pick_repository(repositories: &[Repository], repository: &str) -> Option<Checkout> {
    let wanted = repository.to_lowercase();
    repositories
        .iter()
        .filter(|candidate| {
            candidate
                .remote_url
                .as_deref()
                .and_then(github_slug)
                .as_deref()
                == Some(wanted.as_str())
        })
        .min_by(|left, right| left.created_at.cmp(&right.created_at))
        .map(|found| Checkout {
            root: found.root_path.clone(),
            login: found.forge_login.clone(),
            project_id: found.project_id.0.clone(),
        })
}

/// `pick_repository` over DCC's database.
pub(super) fn local_repository(db: &Path, repository: &str) -> Option<Checkout> {
    let repo = SqliteWorkspaceRepo::open(db).ok()?;
    let repositories = futures::executor::block_on(repo.list_repositories()).ok()?;
    pick_repository(&repositories, repository)
}

/// The hub's item for a pull request of the search, with the same id the
/// hub gives it (`github:<host>:<repository root>:<number>`), so the job's
/// drafts land there.
pub(super) fn hub_item(pr: &Reviewed, root: &str, login: Option<String>) -> Option<Item> {
    let host = url_host(&pr.url)?;
    let name = pr.repository.rsplit('/').next().unwrap_or(&pr.repository);
    Some(Item {
        id: format!("github:{host}:{root}:{}", pr.number),
        provider: "github".to_string(),
        repository_name: name.to_string(),
        repository_root: root.to_string(),
        forge_login: login,
        number: pr.number,
        title: pr.title.clone(),
        url: pr.url.clone(),
        author: pr.author.clone().map(|login| Actor {
            login,
            ..Actor::default()
        }),
        ..Item::default()
    })
}

/// What the review runs on, as the hub picks it without its panel: the
/// Reviewer's own provider, model and effort, else the composer's.
pub(super) fn review_run(
    reviewer: &AgentRecord,
    providers: &[ProviderDescriptor],
    pick: &(Option<String>, Option<String>),
    root: &str,
    prompt: String,
) -> Option<AgentRun> {
    let usable: Vec<&ProviderDescriptor> = providers
        .iter()
        .filter(|provider| provider.capabilities.supports_read_only_delegation)
        .collect();
    let find = |id: Option<&str>| {
        usable
            .iter()
            .find(|provider| Some(provider.id.0.as_str()) == id)
            .copied()
    };
    let provider = find(reviewer.provider_id.as_deref())
        .or_else(|| find(pick.0.as_deref()))
        .or_else(|| usable.first().copied())?;
    let id = provider.id.0.as_str();
    let own = reviewer.provider_id.as_deref() == Some(id);
    let wanted = if own {
        reviewer.model.as_deref()
    } else {
        (pick.0.as_deref() == Some(id))
            .then_some(pick.1.as_deref())
            .flatten()
    };
    let model = provider
        .models
        .iter()
        .find(|model| Some(model.id.as_str()) == wanted)
        .or_else(|| provider.models.iter().find(|model| model.recommended))
        .or_else(|| provider.models.first());
    let effort = reviewer
        .effort
        .as_deref()
        .filter(|_| own)
        .and_then(|effort| {
            let levels = &model?.effort_levels;
            (!levels.is_empty()).then(|| clamp_effort(effort, levels))
        });
    let provider_runtime = (!cfg!(test))
        .then(|| crate::data::provider_runtime::for_provider(id))
        .flatten();
    Some(AgentRun {
        working_directory: root.to_string(),
        provider_id: id.to_string(),
        model: model.map(|model| model.id.clone()),
        provider_runtime,
        prompt,
        agent_id: Some(reviewer.id.clone()),
        effort,
    })
}

/// "Revisor revisou de novo" — "acme/app #1014 · 3 comentários para revisar".
fn announce(
    trigger: Trigger,
    reviewer: &str,
    pr: &Reviewed,
    answer: &Result<String, String>,
    cx: &App,
) {
    let key = |what: &str| format!("agents.triggers.{}.{what}", trigger.id());
    let title = cx.t_with(
        &key("done"),
        &Args::default().with("agent", reviewer.to_string()),
    );
    let what = match answer {
        Ok(_) => cx.t(&key("ready")).to_string(),
        Err(_) => cx.t(&key("failed")).to_string(),
    };
    let body = format!("{} #{} · {what}", pr.repository, pr.number);
    crate::ui::attention::notify_away(&title, &body);
}

impl Shell {
    /// News from `PrWatch`: the Reviewer reviews again what its trigger,
    /// its ceilings and the person's checkouts allow.
    pub(super) fn pr_news_arrived(&mut self, news: &[Reviewed], cx: &mut Context<Self>) {
        self.review_pull_requests(Trigger::PrUpdated, news, cx);
    }

    /// The hub's review of each pull request, started by `trigger` for the
    /// Reviewer when it watches for it.
    pub(super) fn review_pull_requests(
        &mut self,
        trigger: Trigger,
        news: &[Reviewed],
        cx: &mut Context<Self>,
    ) {
        let Some(reviewer) = self.watching(trigger, cx).into_iter().next() else {
            return;
        };
        let composer = self.conversation.read(cx).composer.read(cx);
        let providers: Arc<Vec<ProviderDescriptor>> = composer.catalog.clone().unwrap_or_default();
        let pick = (
            composer.session.provider_id.clone(),
            composer.session.model.clone(),
        );
        let db = crate::data::loader::database_path();
        for pr in news {
            let now = Utc::now();
            let allowed = cx
                .global::<AgentTriggers>()
                .check(&reviewer.id, trigger, &pr.url, now);
            if allowed.is_err() {
                continue;
            }
            // Only the pull requests of the agent's projects.
            let Some(Checkout { root, login, .. }) = local_repository(&db, &pr.repository)
                .filter(|checkout| reviewer.works_in(&checkout.project_id))
            else {
                continue;
            };
            let Some(item) = hub_item(pr, &root, login) else {
                continue;
            };
            // The prompt needs the patches, read in the background below.
            if review_run(&reviewer, &providers, &pick, &root, String::new()).is_none() {
                continue;
            }
            let label = format!("{}#{}", item.repository_name, item.number);
            if !PrReviewJobs::start(cx, &item.id, label, item.title.clone()) {
                continue;
            }
            PrReviewJobs::started_by_trigger(cx, &item.id);
            cx.update_global::<AgentTriggers, _>(|triggers, _| {
                triggers.record(&reviewer.id, trigger, &pr.url, now)
            });
            let (reviewer, providers, pick, pr) = (
                reviewer.clone(),
                providers.clone(),
                pick.clone(),
                pr.clone(),
            );
            cx.spawn(async move |_, cx| {
                let id = item.id.clone();
                let name = reviewer.name.clone();
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
                            review_run(&reviewer, &providers, &pick, &item.repository_root, prompt)
                                .ok_or_else(|| "no provider can review".to_string())?;
                        api.pr_review_agent(&run).map_err(|error| error.to_string())
                    })
                    .await;
                let _ = cx.update(|cx| {
                    announce(trigger, &name, &pr, &answer, cx);
                    PrReviewJobs::finish(cx, &id, answer);
                });
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pull_request_finds_the_persons_checkout_and_the_hubs_id() {
        let repository =
            |root: &str, login: Option<&str>, remote: Option<&str>, at: &str| Repository {
                id: dcc_core::domain::repository::RepositoryId(root.into()),
                project_id: dcc_core::domain::project::ProjectId("p".into()),
                name: "app".into(),
                display_name: None,
                icon: None,
                color: None,
                logo: None,
                pinned_at: None,
                root_path: root.into(),
                base_branch: "main".into(),
                remote: None,
                remote_url: remote.map(str::to_string),
                forge_provider: None,
                forge_login: login.map(str::to_string),
                created_at: at.into(),
                updated_at: at.into(),
            };
        let repositories = [
            repository("/w/app-2", None, Some("git@github.com:acme/app.git"), "2"),
            repository("/home", None, None, "0"),
            repository(
                "/w/app",
                Some("me"),
                Some("git@github.com:acme/app.git"),
                "1",
            ),
        ];
        assert_eq!(
            pick_repository(&repositories, "Acme/App"),
            Some(Checkout {
                root: "/w/app".to_string(),
                login: Some("me".to_string()),
                project_id: "p".to_string(),
            })
        );
        assert_eq!(pick_repository(&repositories, "acme/other"), None);

        let pr = Reviewed {
            url: "https://github.com/acme/app/pull/1014".into(),
            number: 1014,
            repository: "acme/app".into(),
            title: "perf: cache".into(),
            author: Some("ana".into()),
            ..Reviewed::default()
        };
        let item = hub_item(&pr, "/w/app", Some("me".into())).unwrap();
        assert_eq!(item.id, "github:github.com:/w/app:1014");
        assert_eq!(item.repository_name, "app");
        assert_eq!(item.author.unwrap().login, "ana");
    }
}
