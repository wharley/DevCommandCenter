//! A trigger any agent can have on, from the server: someone else reviewed
//! or commented on one of the person's task pull requests
//! (`docs/PLANO_MAQUINA_SEM_TELA`, slice 5c). The pull requests' search
//! (`pr_watch`) already lists the latest review activity on the person's
//! own pull requests (`pr_hub_received`), so no `gh` is added to notice it.
//! The first sight only sets where it starts; what comes after wakes the
//! agent, when the person turned this on and its ceilings allow: it reads
//! the open review conversations (the code column's read) and, read-only in
//! its own conversation in that task, says for each whether it holds and
//! what to change. Where the person chose "Corrigir na tarefa" for that
//! agent, the conversations go to the task's own conversation to be fixed
//! instead (`review_fix`). It ran in the native app's window
//! (`review_received`).

use dcc_core::domain::review_threads::WorkspacePrReviewComment;
use dcc_tauri::agent_triggers::Trigger;
use dcc_tauri::commands::forge_commands::{self, WorkspacePrReviewCommentsInput};
use dcc_tauri::commands::pr_hub_received::PullRequestHubReceived;

use super::*;

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

/// What the agent is asked; `comments` is the pull request's review
/// conversations as DCC read them.
pub(super) fn review_prompt(
    pr: &PullRequestHubReceived,
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
        "[DCC] New review activity on this task's pull request {}#{} (branch {}) from {from}.\n\nRead what they asked: for each point, say whether it holds and what to change, the smallest fix first; say so when a point is already handled or does not apply. Work read-only: do not edit files, reply on the pull request, commit or push; the person decides what to do. Answer in the person's language, short.\n\n{evidence}",
        pr.repository, pr.number, pr.head_branch
    )
}

/// The open review conversations of the task's pull request; none when
/// they could not be read.
async fn read_comments(
    backend: &HttpBackend,
    path: &str,
    branch: &str,
) -> Option<Vec<WorkspacePrReviewComment>> {
    forge_commands::workspace_pr_review_comments_with_state(
        &backend.workspace,
        WorkspacePrReviewCommentsInput {
            workspace_root: path.to_string(),
            branch: Some(branch.to_string()),
            forge_login: None,
        },
    )
    .await
    .ok()
    .map(|read| read.comments)
}

/// The reviews on the person's pull requests in this search: each watching
/// agent reads what came after its last look, when its ceilings and the
/// task allow.
pub(super) async fn react(backend: &Arc<HttpBackend>, received: &[PullRequestHubReceived]) {
    if received.is_empty() {
        return;
    }
    let watching = match super::ci_failing::watching(backend, Trigger::ReviewReceived).await {
        Ok(watching) => watching,
        Err(error) => {
            eprintln!("[DCC][review-received] agents unreadable: {error}");
            return;
        }
    };
    for agent in &watching {
        for pr in received {
            let (Some(task), Some(project), Some(path)) = (
                pr.workspace_id.clone(),
                pr.project_id.clone(),
                pr.workspace_path.clone(),
            ) else {
                continue;
            };
            let now = chrono::Utc::now();
            // What was seen before this look (only what came after is new),
            // and whether the task's own conversation gets it.
            let woken = super::agent_clock::with_triggers(backend, |triggers| {
                let seen = triggers.received_at(&agent.id, &pr.url);
                let news = triggers.advance_received(&agent.id, &pr.url, &pr.last_activity);
                // Outside the agent's projects the news is only marked seen.
                if !news
                    || !agent.works_in(&project)
                    || triggers
                        .check(&agent.id, Trigger::ReviewReceived, &task, now)
                        .is_err()
                {
                    return None;
                }
                Some((triggers.fix_in_task(&agent.id), seen))
            })
            .flatten();
            let Some((fix, seen)) = woken else {
                continue;
            };
            let (backend, agent, pr) = (backend.clone(), agent.clone(), pr.clone());
            crate::runtime::spawn(async move {
                // Whether an agent got something to do.
                let done = if fix {
                    // "Corrigir na tarefa": the task's own agent gets them,
                    // not this one; nothing open is nothing sent.
                    super::review_fix::fix_in_task(&backend, &pr, &task, &path, seen.as_deref())
                        .await
                } else {
                    let comments = read_comments(&backend, &path, &pr.head_branch).await;
                    let prompt = review_prompt(&pr, comments.as_deref());
                    super::agent_task_ask::ask_in_task(&backend, &agent, &task, &project, &prompt)
                        .await
                        .map(|_| true)
                };
                match done {
                    // Counted once sent: the app announces what is recorded,
                    // so a failure or an empty read is never told as done.
                    Ok(true) => {
                        super::agent_clock::with_triggers(&backend, |triggers| {
                            triggers.record(&agent.id, Trigger::ReviewReceived, &task, now)
                        });
                    }
                    Ok(false) => {}
                    Err(error) => eprintln!(
                        "[DCC][review-received] {}#{}: {error}",
                        pr.repository, pr.number
                    ),
                }
            });
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
        let pr = PullRequestHubReceived {
            url: String::new(),
            number: 7,
            repository: "acme/app".into(),
            title: String::new(),
            head_branch: "feat/rastreio".into(),
            last_activity: String::new(),
            from: vec!["giovanni".into(), "daniel".into()],
            workspace_id: None,
            project_id: None,
            workspace_path: None,
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
