//! The Reviewer's third trigger: someone else reviewed or commented on one
//! of the person's task pull requests. The same search `PrWatch` already
//! makes lists the latest review activity on the person's own pull requests
//! (`pr_hub_received`), so no `gh` is added to notice it. The first sight
//! only sets where it starts; what comes after wakes the Reviewer, when the
//! person turned this on and its ceilings allow: it reads the open review
//! conversations (the code column's read) and, read-only in its own
//! conversation in that task, says for each whether it holds and what to
//! change.

use chrono::Utc;
use dcc_core::domain::provider::ProviderDescriptor;
use dcc_core::domain::review_threads::WorkspacePrReviewComment;
use gpui::{App, BorrowAppContext, Context};

use super::Shell;
use super::review_trigger::review_run;
use super::reviewer_in_task::TaskAsk;
use crate::data::agent_triggers::{AgentTriggers, Trigger};
use crate::data::desktop_api::DesktopApi;
use crate::data::pr_hub::Received;
use crate::i18n::{Args, Translate};

/// The open conversations sent along, at most.
const MAX_CHARS: usize = 12_000;

/// The open review conversations, root first, as the Reviewer reads them.
fn open_threads(comments: &[WorkspacePrReviewComment]) -> String {
    let mut threads: Vec<(&str, Vec<&WorkspacePrReviewComment>)> = Vec::new();
    for comment in comments {
        if comment.resolved == Some(true) || comment.outdated == Some(true) {
            continue;
        }
        match threads
            .iter_mut()
            .find(|(thread, _)| *thread == comment.thread_id)
        {
            Some((_, list)) => list.push(comment),
            None => threads.push((comment.thread_id.as_str(), vec![comment])),
        }
    }
    let mut text = String::new();
    for (_, list) in threads {
        for (index, comment) in list.iter().enumerate() {
            let author = comment
                .author
                .as_ref()
                .and_then(|author| author.login.as_deref())
                .unwrap_or("someone");
            let body = comment.body.trim();
            let line = if index == 0 {
                let place = match comment.line.or(comment.original_line) {
                    Some(line) => format!("{}:{line}", comment.path),
                    None => comment.path.clone(),
                };
                format!("- {place} — @{author}: {body}\n")
            } else {
                format!("  ↳ @{author}: {body}\n")
            };
            if text.len() + line.len() > MAX_CHARS {
                text.push_str("- … more open conversations on the pull request\n");
                return text;
            }
            text.push_str(&line);
        }
    }
    text
}

/// What the Reviewer is asked; `comments` is the pull request's review
/// conversations as DCC read them.
pub(super) fn review_prompt(
    pr: &Received,
    comments: Option<&[WorkspacePrReviewComment]>,
) -> String {
    let from = pr
        .from
        .iter()
        .map(|login| format!("@{login}"))
        .collect::<Vec<_>>()
        .join(", ");
    let threads = comments.map(open_threads).unwrap_or_default();
    let evidence = if threads.is_empty() {
        "DCC found no open inline review conversation; the new review or comment is in the pull request's conversation.".to_string()
    } else {
        format!("The open review conversations:\n\n{threads}")
    };
    format!(
        "[DCC] New review activity on this task's pull request {}#{} (branch {}) from {from}.\n\nAs the Reviewer, read what they asked: for each point, say whether it holds and what to change, the smallest fix first; say so when a point is already handled or does not apply. Work read-only: do not edit files, reply on the pull request, commit or push; the person decides what to do. Answer in the person's language, short.\n\n{evidence}",
        pr.repository, pr.number, pr.head_branch
    )
}

fn announce(reviewer: &str, pr: &Received, done: &Result<(), String>, cx: &App) {
    let title = cx.t_with(
        "agents.triggers.reviewReceived.done",
        &Args::default().with("agent", reviewer.to_string()),
    );
    let what = match done {
        Ok(()) => cx.t("agents.triggers.reviewReceived.ready").to_string(),
        Err(_) => cx.t("agents.triggers.reviewReceived.failed").to_string(),
    };
    let body = format!("{} #{} · {what}", pr.repository, pr.number);
    crate::ui::attention::notify_away(&title, &body);
}

impl Shell {
    /// The reviews on the person's pull requests in this search: the
    /// Reviewer reads what came after the last look, when its trigger, its
    /// ceilings and the task allow.
    pub(super) fn reviews_received(&mut self, received: &[Received], cx: &mut Context<Self>) {
        let Some(reviewer) = self.sidebar.read(cx).reviewer() else {
            return;
        };
        if !cx.has_global::<AgentTriggers>()
            || !cx
                .global::<AgentTriggers>()
                .is_on(&reviewer.id, Trigger::ReviewReceived)
        {
            return;
        }
        let composer = self.conversation.read(cx).composer.read(cx);
        let providers: std::sync::Arc<Vec<ProviderDescriptor>> =
            composer.catalog.clone().unwrap_or_default();
        let pick = (
            composer.session.provider_id.clone(),
            composer.session.model.clone(),
        );
        for pr in received {
            let (Some(task), Some(project), Some(path)) = (
                pr.workspace_id.clone(),
                pr.project_id.clone(),
                pr.workspace_path.clone(),
            ) else {
                continue;
            };
            let news = cx.update_global::<AgentTriggers, _>(|triggers, _| {
                triggers.advance_received(&reviewer.id, &pr.url, &pr.last_activity)
            });
            // Outside the agent's projects the news is only marked seen.
            if !news || !reviewer.works_in(&project) {
                continue;
            }
            let now = Utc::now();
            if cx
                .global::<AgentTriggers>()
                .check(&reviewer.id, Trigger::ReviewReceived, &task, now)
                .is_err()
            {
                continue;
            }
            let Some(run) = review_run(&reviewer, &providers, &pick, &path, String::new()) else {
                continue;
            };
            cx.update_global::<AgentTriggers, _>(|triggers, _| {
                triggers.record(&reviewer.id, Trigger::ReviewReceived, &task, now)
            });
            let ask = TaskAsk::new(&reviewer, &task, &project, run);
            let (name, pr) = (reviewer.name.clone(), pr.clone());
            cx.spawn(async move |_, cx| {
                let done = cx
                    .background_executor()
                    .spawn({
                        let pr = pr.clone();
                        async move {
                            let api = DesktopApi::load()
                                .ok_or_else(|| "DCC is not running".to_string())?;
                            let comments = api
                                .review_comments(&path, Some(&pr.head_branch), None)
                                .ok()
                                .map(|read| read.comments);
                            ask.ask(&api, &review_prompt(&pr, comments.as_deref()))
                        }
                    })
                    .await;
                let _ = cx.update(|cx| announce(&name, &pr, &done, cx));
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::review_threads::WorkspacePrReviewCommentAuthor;

    fn comment(id: &str, thread: &str, author: &str, body: &str) -> WorkspacePrReviewComment {
        WorkspacePrReviewComment {
            id: id.into(),
            thread_id: thread.into(),
            path: "src/checkout.ts".into(),
            body: body.into(),
            line: Some(42),
            author: Some(WorkspacePrReviewCommentAuthor {
                login: Some(author.into()),
                ..WorkspacePrReviewCommentAuthor::default()
            }),
            ..WorkspacePrReviewComment::default()
        }
    }

    #[test]
    fn the_reviewer_reads_only_the_open_conversations() {
        let pr = Received {
            repository: "acme/app".into(),
            number: 7,
            head_branch: "feat/rastreio".into(),
            from: vec!["giovanni".into(), "daniel".into()],
            ..Received::default()
        };
        let mut resolved = comment("c3", "t2", "daniel", "Já resolvido");
        resolved.resolved = Some(true);
        let mut outdated = comment("c4", "t3", "daniel", "Código mudou");
        outdated.outdated = Some(true);
        let comments = [
            comment("c1", "t1", "giovanni", "Falta tratar o erro aqui"),
            comment("c2", "t1", "wharley", "Vou ver"),
            resolved,
            outdated,
        ];
        let prompt = review_prompt(&pr, Some(&comments));
        assert!(prompt.starts_with(
            "[DCC] New review activity on this task's pull request acme/app#7 (branch feat/rastreio) from @giovanni, @daniel."
        ));
        assert!(prompt.contains(
            "- src/checkout.ts:42 — @giovanni: Falta tratar o erro aqui\n  ↳ @wharley: Vou ver"
        ));
        assert!(!prompt.contains("Já resolvido"));
        assert!(!prompt.contains("Código mudou"));
        assert!(prompt.contains("read-only"));
        assert!(review_prompt(&pr, None).contains("no open inline review conversation"));
    }
}
