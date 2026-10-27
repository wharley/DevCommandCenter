//! `workspace-command-palette.logic.ts`: what the palette shows for a
//! query — actions whose label or keywords hold every word, the tasks
//! ranked (exact name, then starts with, contains, branch; the open one by
//! a hair), and the sessions, recent ones by their title or an `@` search
//! over the conversations' text.

/// `PALETTE_SESSION_SEARCH_MIN_LENGTH`
pub const SESSION_SEARCH_MIN: usize = 2;
/// `MAX_RECENT_PALETTE_SESSIONS`
pub const MAX_RECENT_SESSIONS: usize = 40;

/// `resolvePaletteSessionSearch`: `@ term` searches the conversations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSearch {
    pub query: String,
    /// The query starts with `@`.
    pub explicit: bool,
    /// …and has at least two letters after it.
    pub enabled: bool,
}

pub fn session_search(query: &str) -> SessionSearch {
    let rest = query.trim_start().strip_prefix('@');
    let term = rest.map(|rest| rest.trim().to_string()).unwrap_or_default();
    SessionSearch {
        enabled: rest.is_some() && term.chars().count() >= SESSION_SEARCH_MIN,
        explicit: rest.is_some(),
        query: term,
    }
}

/// `queryTokens`: lowercase words.
fn tokens(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// `matchesPaletteAction`
pub fn matches_action(label: &str, keywords: &str, query: &str) -> bool {
    let haystack = format!("{label} {keywords}").to_lowercase();
    tokens(query).iter().all(|token| haystack.contains(token))
}

/// One task as the palette ranks it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteTask {
    pub id: String,
    pub name: String,
    pub branch: String,
}

/// `rankPaletteWorkspaces` over tasks that can be opened.
pub fn rank_tasks(tasks: &[PaletteTask], query: &str, selected: Option<&str>) -> Vec<PaletteTask> {
    let tokens = tokens(query);
    let mut scored: Vec<(&PaletteTask, i64)> = tasks
        .iter()
        .filter_map(|task| {
            let haystack = format!("{} {} {}", task.name, task.branch, task.id).to_lowercase();
            if tokens.iter().any(|token| !haystack.contains(token)) {
                return None;
            }
            let name = task.name.to_lowercase();
            let branch = task.branch.to_lowercase();
            let exact = if tokens.len() == 1 && name == tokens[0] {
                100
            } else {
                0
            };
            let words: i64 = tokens
                .iter()
                .map(|token| {
                    (if name.starts_with(token.as_str()) {
                        20
                    } else {
                        0
                    }) + (if name.contains(token.as_str()) { 8 } else { 0 })
                        + (if branch.contains(token.as_str()) {
                            4
                        } else {
                            0
                        })
                })
                .sum();
            let open = i64::from(selected == Some(task.id.as_str()));
            Some((task, exact + words + open))
        })
        .collect();
    scored.sort_by(|(a, a_score), (b, b_score)| {
        b_score
            .cmp(a_score)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.branch.to_lowercase().cmp(&b.branch.to_lowercase()))
    });
    scored.into_iter().map(|(task, _)| task.clone()).collect()
}

/// A session the palette lists: a recent one of the open task, or an
/// `@` search hit (with its excerpt).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteSession {
    pub session_id: String,
    pub task_id: String,
    pub title: String,
    pub task_name: Option<String>,
    pub branch: Option<String>,
    pub snippet: String,
    pub archived: bool,
    pub updated_at: String,
}

impl PaletteSession {
    /// `[workspaceName, workspaceBranch].join(" · ") || workspaceId`
    pub fn workspace_label(&self) -> String {
        let parts: Vec<&str> = [self.task_name.as_deref(), self.branch.as_deref()]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            self.task_id.clone()
        } else {
            parts.join(" · ")
        }
    }
}

/// `recentPaletteSessions`: newest first, 40 at most.
pub fn recent_sessions(mut sessions: Vec<PaletteSession>) -> Vec<PaletteSession> {
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions.truncate(MAX_RECENT_SESSIONS);
    sessions
}

/// `filterPaletteSessionsByMetadata`: title, task and branch hold every word.
pub fn filter_sessions(sessions: &[PaletteSession], query: &str) -> Vec<PaletteSession> {
    let tokens = tokens(query);
    sessions
        .iter()
        .filter(|session| {
            let metadata = format!(
                "{} {} {}",
                session.title,
                session.task_name.as_deref().unwrap_or(""),
                session.branch.as_deref().unwrap_or("")
            )
            .to_lowercase();
            tokens.iter().all(|token| metadata.contains(token))
        })
        .cloned()
        .collect()
}

/// `groupPaletteSessions`: the open task's first, then the others.
pub fn group_sessions(
    sessions: Vec<PaletteSession>,
    selected: Option<&str>,
) -> (Vec<PaletteSession>, Vec<PaletteSession>) {
    match selected {
        None => (Vec::new(), sessions),
        Some(selected) => sessions
            .into_iter()
            .partition(|session| session.task_id == selected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, name: &str, branch: &str) -> PaletteTask {
        PaletteTask {
            id: id.into(),
            name: name.into(),
            branch: branch.into(),
        }
    }

    fn session(id: &str, task: &str, title: &str, updated: &str) -> PaletteSession {
        PaletteSession {
            session_id: id.into(),
            task_id: task.into(),
            title: title.into(),
            task_name: Some("Checkout".into()),
            branch: Some("main".into()),
            snippet: String::new(),
            archived: false,
            updated_at: updated.into(),
        }
    }

    #[test]
    fn an_at_starts_a_conversation_search_from_two_letters() {
        assert_eq!(
            session_search("  @ re"),
            SessionSearch {
                query: "re".into(),
                explicit: true,
                enabled: true
            }
        );
        assert!(!session_search("@r").enabled);
        assert!(session_search("@r").explicit);
        assert!(!session_search("retry").explicit);
    }

    #[test]
    fn actions_need_every_word() {
        assert!(matches_action(
            "Abrir terminal",
            "shell console",
            "term shell"
        ));
        assert!(!matches_action("Abrir terminal", "shell", "term git"));
        assert!(matches_action("Qualquer", "", ""));
    }

    #[test]
    fn tasks_rank_by_name_then_branch() {
        let tasks = [
            task("1", "Login retry", "feat/x"),
            task("2", "Checkout", "fix/login"),
            task("3", "login", "main"),
        ];
        let ranked: Vec<String> = rank_tasks(&tasks, "login", Some("2"))
            .into_iter()
            .map(|task| task.id)
            .collect();
        // Exact name, then starts-with, then branch only.
        assert_eq!(ranked, ["3", "1", "2"]);
        // No query: alphabetical, the open one breaking no tie here.
        let ranked: Vec<String> = rank_tasks(&tasks, "", None)
            .into_iter()
            .map(|task| task.name)
            .collect();
        assert_eq!(ranked, ["Checkout", "login", "Login retry"]);
        assert!(rank_tasks(&tasks, "nada", None).is_empty());
    }

    #[test]
    fn sessions_filter_sort_and_group() {
        let sessions = recent_sessions(vec![
            session("a", "t1", "Retry no checkout", "2026-10-01"),
            session("b", "t2", "Ajuste de CSS", "2026-10-03"),
        ]);
        assert_eq!(sessions[0].session_id, "b");
        let filtered = filter_sessions(&sessions, "retry");
        assert_eq!(filtered.len(), 1);
        let (mine, others) = group_sessions(sessions.clone(), Some("t1"));
        assert_eq!(mine[0].session_id, "a");
        assert_eq!(others[0].session_id, "b");
        let (mine, others) = group_sessions(sessions, None);
        assert!(mine.is_empty());
        assert_eq!(others.len(), 2);
        let label = session("a", "t1", "x", "1").workspace_label();
        assert_eq!(label, "Checkout · main");
    }
}
