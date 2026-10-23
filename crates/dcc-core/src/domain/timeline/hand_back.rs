//! The `[DCC] …` turns the backend writes into a parent conversation when a
//! delegation finishes or its edits are applied/discarded
//! (`delegation-hand-back.ts`). Addressed to the agent, so the conversation
//! shows them as a one-line note instead of a message the person wrote.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandBackOutcome {
    Finished,
    Failed,
    Applied,
    Discarded,
    TaskTurn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationHandBack {
    pub delegation_id: String,
    pub outcome: HandBackOutcome,
}

/// `(task <id>).` somewhere after `before`, the id without spaces or `)`.
fn task_id_after<'a>(line: &'a str, before: &str) -> Option<&'a str> {
    let rest = line.get(line.find(before)? + before.len()..)?;
    let start = rest.rfind("(task ")? + "(task ".len();
    let tail = &rest[start..];
    let end = tail.find(')')?;
    let id = &tail[..end];
    let valid =
        !id.is_empty() && !id.chars().any(char::is_whitespace) && tail[end..].starts_with(").");
    valid.then_some(id)
}

/// One word of `\w+`: letters, digits or underscores.
fn one_word(text: &str) -> Option<(&str, &str)> {
    let end = text
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(text.len(), |(index, _)| index);
    (end > 0).then(|| text.split_at(end))
}

pub fn parse_delegation_hand_back(content: &str) -> Option<DelegationHandBack> {
    let line = content.trim_start().split('\n').next().unwrap_or_default();
    let hand_back = |id: &str, outcome| {
        Some(DelegationHandBack {
            delegation_id: id.to_string(),
            outcome,
        })
    };
    // `[DCC] Delegated \w+ task (finished|failed) — …(task id).`
    if let Some(rest) = line.strip_prefix("[DCC] Delegated ") {
        if let Some((_, rest)) = one_word(rest) {
            for (word, outcome) in [
                ("finished", HandBackOutcome::Finished),
                ("failed", HandBackOutcome::Failed),
            ] {
                if rest.starts_with(&format!(" task {word} — ")) {
                    if let Some(id) = task_id_after(rest, " — ") {
                        return hand_back(id, outcome);
                    }
                }
            }
        }
    }
    // `[DCC] Task ".*" ended a turn — …(task id).`
    if line.starts_with("[DCC] Task \"") && line.contains("\" ended a turn — ") {
        if let Some(id) = task_id_after(line, "\" ended a turn — ") {
            return hand_back(id, HandBackOutcome::TaskTurn);
        }
    }
    // `[DCC] The human (APPLIED|DISCARDED) the edits from your delegated \w+ task — …`
    for (word, outcome) in [
        ("APPLIED", HandBackOutcome::Applied),
        ("DISCARDED", HandBackOutcome::Discarded),
    ] {
        let prefix = format!("[DCC] The human {word} the edits from your delegated ");
        if let Some(rest) = line.strip_prefix(&prefix) {
            if let Some((_, rest)) = one_word(rest) {
                if rest.starts_with(" task — ") {
                    if let Some(id) = task_id_after(rest, " task — ") {
                        return hand_back(id, outcome);
                    }
                }
            }
        }
    }
    None
}
