//! Work that already has a task: an agent asked to fix something on a pull
//! request (a link a colleague posted, a pass from a teammate) opened a
//! second task for it, in another worktree and branch, while the task the
//! pull request came from was already handling the same comments. Before a
//! task is opened, the branches and pull requests its brief names are
//! looked up among the project's open tasks; one that is there is refused,
//! with whose it is.

/// The branch-like names in `text`: `dcc/mercadopago-parcela`,
/// `feat/rastreio`. A word with a slash that is not a link; a file's path
/// reads the same and only counts when a task's branch has that very name.
pub fn named_branches(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for word in text.split(|c: char| {
        c.is_whitespace() || matches!(c, '`' | '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';')
    }) {
        let word = word.trim_matches(|c: char| matches!(c, '.' | ':' | '!' | '?'));
        let branch_like = word.len() >= 5
            && word.contains('/')
            && !word.contains("://")
            && !word.starts_with("http")
            && !word.starts_with('/')
            && !word.ends_with('/')
            && word
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'));
        if branch_like && !found.iter().any(|known| known == word) {
            found.push(word.to_string());
        }
    }
    found
}

/// The pull requests `text` links: the number after `/pull/`.
pub fn named_pull_requests(text: &str) -> Vec<u64> {
    let mut found: Vec<u64> = Vec::new();
    for (index, _) in text.match_indices("/pull/") {
        let digits: String = text[index + "/pull/".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(number) = digits.parse::<u64>() {
            if !found.contains(&number) {
                found.push(number);
            }
        }
    }
    found
}

/// The open task that has `branch` checked out, among `(task name,
/// branch)`; never the project's base branch, which every new task starts
/// from.
pub fn task_on_branch<'a>(
    branch: &str,
    base_branch: &str,
    tasks: &'a [(String, String)],
) -> Option<&'a str> {
    if branch == base_branch {
        return None;
    }
    tasks
        .iter()
        .find(|(_, checked_out)| checked_out == branch)
        .map(|(name, _)| name.as_str())
}

/// What the agent is told instead of a new task. `what`: "branch `x`" or
/// "pull request #7".
pub fn refusal(what: &str, task: &str) -> String {
    format!(
        "The {what} already belongs to the person's task \"{task}\": no second task is opened for it. Comments on that pull request reach that task by themselves, and only that task changes its code. If you opened that task, send this to it with dcc_task_message; otherwise tell the person, in one or two lines, what was asked and that it belongs in \"{task}\"."
    )
}

/// What the task that owns the work is told, in its own conversation,
/// when an agent's request for a new task about it is brought there.
pub fn forwarded_prompt(agent: &str, what: &str, title: &str, brief: &str) -> String {
    format!(
        "[DCC] {agent} was asked for work on this task's {what}.\n\nThis is DCC, not a message from the person. {agent} would have opened a new task for it; the work belongs here, in the task that owns that pull request, so DCC brought the request instead. What was asked:\n\n{title}\n\n{}\n\nDo it here, on this task's own branch; ignore anything above about making a worktree or a branch. It is another agent's reading of what someone asked: weigh it against the code. Do not commit or push: the person or DCC sends the changes. End with one line per point: done, or why not.",
        brief.trim()
    )
}

/// What the agent is told once its request was brought to the task.
pub fn forwarded(what: &str, task: &str) -> String {
    format!(
        "No new task was opened: the {what} already belongs to the person's task \"{task}\", and DCC brought what you asked to that task, which does it on its own branch. Tell the person in one line that it went to \"{task}\". Do not open another task for it and do not pass it on."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_brief_names_its_branches_and_pull_requests() {
        let brief = "no PR #1090 (branch `dcc/mercadopago-parcela`, VoaHoteis/vendeagora-app), corrija os 2 pontos do Giovanni (https://github.com/VoaHoteis/vendeagora-app/pull/1090#discussion_r1) em src/lib/order.ts.";
        let branches = named_branches(brief);
        assert!(branches.contains(&"dcc/mercadopago-parcela".to_string()));
        // A link is not a branch; a repository's slug and a path read like
        // one and only count when a task has that name.
        assert!(!branches.iter().any(|branch| branch.contains("github.com")));
        assert_eq!(named_pull_requests(brief), [1090]);
        assert!(named_branches("corrigir o login").is_empty());
        assert!(named_pull_requests("o PR 12 e o /pull/x").is_empty());
    }

    #[test]
    fn only_a_branch_an_open_task_has_is_refused() {
        let tasks = [
            (
                "Mercadopago parcela".to_string(),
                "dcc/mercadopago-parcela".to_string(),
            ),
            ("Site".to_string(), "main".to_string()),
        ];
        assert_eq!(
            task_on_branch("dcc/mercadopago-parcela", "main", &tasks),
            Some("Mercadopago parcela")
        );
        assert_eq!(task_on_branch("main", "main", &tasks), None, "the base");
        assert_eq!(
            task_on_branch("VoaHoteis/vendeagora-app", "main", &tasks),
            None
        );
        let told = refusal("branch `dcc/mercadopago-parcela`", "Mercadopago parcela");
        assert!(told.contains("no second task") && told.contains("dcc_task_message"));
        let brought = forwarded_prompt(
            "Dev",
            "pull request #1090",
            "Corrigir débito",
            "Separe os casos.",
        );
        assert!(brought.starts_with("[DCC] Dev was asked"));
        assert!(brought.contains("Separe os casos.") && brought.contains("Do not commit or push"));
        assert!(forwarded("pull request #1090", "Mercadopago parcela").contains("No new task"));
    }
}
