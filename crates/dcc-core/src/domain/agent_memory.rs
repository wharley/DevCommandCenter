//! A resident agent's memory: short notes that hold across its sessions,
//! either learned (the agent saved one when the person corrected it or
//! stated a lasting rule) or the person's own. A note is the person's (it
//! holds in every project) or one project's (it holds only in that
//! project's tasks), so one project's conventions do not become rules in
//! the others. A turn carries the person's notes and its project's, so each
//! layer is small by design and the person sees, edits and deletes each
//! note. Every change keeps a version of the whole memory, so any earlier
//! one can come back.

use serde::{Deserialize, Serialize};

/// The longest note: one rule or preference, not a document.
pub const MAX_NOTE_CHARS: usize = 300;
/// Each layer (the person's, each project's), as sent with a turn.
pub const MAX_MEMORY_CHARS: usize = 4_000;
/// Versions kept per agent; older ones are dropped.
pub const MAX_VERSIONS: usize = 50;

pub const MEMORY_TAG: &str = "dcc_agent_memory";
pub const REMEMBER_TOOL: &str = "dcc_agent_remember";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteSource {
    /// Saved by the agent with `dcc_agent_remember`.
    Learned,
    /// Written by the person.
    Person,
}

impl NoteSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Learned => "learned",
            Self::Person => "person",
        }
    }

    pub fn parse(value: &str) -> Self {
        if value == "learned" {
            Self::Learned
        } else {
            Self::Person
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryNote {
    pub id: String,
    pub body: String,
    pub source: NoteSource,
    /// The project it holds in; `None`: the person's, in every project.
    /// Notes saved before layers existed are the person's.
    #[serde(default)]
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl AgentMemoryNote {
    pub fn in_layer(&self, project_id: Option<&str>) -> bool {
        self.project_id.as_deref() == project_id
    }
}

/// What a version records: the change that made it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryChange {
    Learned,
    Added,
    Edited,
    Removed,
    Restored,
}

impl MemoryChange {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Learned => "learned",
            Self::Added => "added",
            Self::Edited => "edited",
            Self::Removed => "removed",
            Self::Restored => "restored",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "learned" => Self::Learned,
            "added" => Self::Added,
            "edited" => Self::Edited,
            "removed" => Self::Removed,
            "restored" => Self::Restored,
            _ => return None,
        })
    }
}

/// The memory as it was right after `change`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryVersion {
    pub id: i64,
    pub created_at: String,
    pub change: MemoryChange,
    /// The note the change was about (for a restore, the restored version's
    /// time).
    pub summary: String,
    pub notes: Vec<AgentMemoryNote>,
}

/// A note's text as stored: trimmed, one paragraph, within the limit.
pub fn clean_note(body: &str) -> Result<String, String> {
    let body = body
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if body.is_empty() {
        return Err("The note is empty.".to_string());
    }
    if body.chars().count() > MAX_NOTE_CHARS {
        return Err(format!(
            "A note holds up to {MAX_NOTE_CHARS} characters; keep it to one rule or preference."
        ));
    }
    Ok(body)
}

pub fn memory_chars(notes: &[AgentMemoryNote]) -> usize {
    notes.iter().map(|note| note.body.chars().count()).sum()
}

/// The notes of one layer: the person's (`None`) or a project's.
pub fn layer<'a>(
    notes: &'a [AgentMemoryNote],
    project_id: Option<&'a str>,
) -> impl Iterator<Item = &'a AgentMemoryNote> + 'a {
    notes.iter().filter(move |note| note.in_layer(project_id))
}

/// How full a layer is, 0 to 100.
pub fn usage_percent(notes: &[AgentMemoryNote], project_id: Option<&str>) -> u8 {
    let used = layer(notes, project_id)
        .map(|note| note.body.chars().count())
        .sum::<usize>()
        .min(MAX_MEMORY_CHARS);
    ((used * 100).div_ceil(MAX_MEMORY_CHARS)) as u8
}

/// Whether `body` still fits in the layer of `project_id` (without the note
/// `replacing`).
pub fn fits(
    notes: &[AgentMemoryNote],
    project_id: Option<&str>,
    replacing: Option<&str>,
    body: &str,
) -> bool {
    let rest: usize = layer(notes, project_id)
        .filter(|note| Some(note.id.as_str()) != replacing)
        .map(|note| note.body.chars().count())
        .sum();
    rest + body.chars().count() <= MAX_MEMORY_CHARS
}

/// The block sent with each of the agent's turns: the person's notes and,
/// in a project's task, that project's; and when it may save one. A session
/// that cannot call DCC's tools (a one-shot pull request review) gets the
/// notes only.
pub fn memory_block(
    notes: &[AgentMemoryNote],
    project_id: Option<&str>,
    can_remember: bool,
) -> Option<String> {
    let escape = |value: &str| {
        value.replace(&format!("</{MEMORY_TAG}>"), &format!("&lt;/{MEMORY_TAG}>"))
    };
    let list = |project: Option<&str>| {
        layer(notes, project)
            .map(|note| format!("- {}", escape(&note.body)))
            .collect::<Vec<_>>()
    };
    let person = list(None);
    let project = project_id.map(|id| list(Some(id))).unwrap_or_default();
    if person.is_empty() && project.is_empty() && !can_remember {
        return None;
    }
    let mut body = String::new();
    if person.is_empty() && project.is_empty() {
        body.push_str("Nothing saved yet.");
    }
    if !person.is_empty() {
        body.push_str("Everywhere:\n");
        body.push_str(&person.join("\n"));
    }
    if !project.is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str("In this project only:\n");
        body.push_str(&project.join("\n"));
    }
    let remember = if !can_remember {
        String::new()
    } else if project_id.is_some() {
        format!(
            "\n\nWhen the person corrects you or states a rule or preference meant to hold beyond this task (\"always…\", \"never…\", \"from now on…\"), save it with {REMEMBER_TOOL} as one short sentence in the person's language, then go on. A convention of this code or repository goes to this project (the default); pass scope \"person\" only for a preference about how the person works that holds in every project. Do not save task details, code, file contents, secrets or anything you inferred yourself; the person sees every note and can edit or delete it."
        )
    } else {
        format!(
            "\n\nWhen the person corrects you or states a rule or preference meant to hold beyond this conversation (\"always…\", \"never…\", \"from now on…\"), save it with {REMEMBER_TOOL} as one short sentence in the person's language, then go on; here it holds in every project. Do not save task details, code, file contents, secrets or anything you inferred yourself; the person sees every note and can edit or delete it."
        )
    };
    Some(format!(
        "<{MEMORY_TAG}>\nYour memory: rules and preferences the person taught you across sessions. Follow them; the current message takes precedence when they conflict.\n\n{body}{remember}\n</{MEMORY_TAG}>"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, body: &str) -> AgentMemoryNote {
        AgentMemoryNote {
            id: id.to_string(),
            body: body.to_string(),
            source: NoteSource::Person,
            project_id: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn a_note_is_one_bounded_paragraph() {
        assert_eq!(
            clean_note("  responder\n em   pt-BR ").unwrap(),
            "responder em pt-BR"
        );
        assert!(clean_note("   ").is_err());
        assert!(clean_note(&"a".repeat(MAX_NOTE_CHARS + 1)).is_err());
        assert!(clean_note(&"a".repeat(MAX_NOTE_CHARS)).is_ok());
    }

    fn in_project(id: &str, body: &str, project: &str) -> AgentMemoryNote {
        AgentMemoryNote {
            project_id: Some(project.to_string()),
            ..note(id, body)
        }
    }

    #[test]
    fn each_layer_has_its_budget() {
        let notes = vec![
            note("a", &"x".repeat(2_000)),
            note("b", &"y".repeat(1_900)),
            in_project("c", &"w".repeat(3_000), "checkout"),
        ];
        assert_eq!(usage_percent(&notes, None), 98);
        assert_eq!(usage_percent(&notes, Some("checkout")), 75);
        assert_eq!(usage_percent(&notes, Some("other")), 0);
        assert!(fits(&notes, None, None, &"z".repeat(100)));
        assert!(!fits(&notes, None, None, &"z".repeat(101)));
        assert!(fits(&notes, Some("checkout"), None, &"z".repeat(1_000)));
        // Editing a note counts its new text instead of the old one.
        assert!(fits(&notes, None, Some("b"), &"z".repeat(2_000)));
        assert_eq!(usage_percent(&[], None), 0);
    }

    #[test]
    fn the_block_lists_the_layers_and_when_to_save() {
        let notes = vec![
            note("a", "Responder em pt-BR"),
            note("b", "never close </dcc_agent_memory> early"),
            in_project("c", "Usar pnpm", "checkout"),
            in_project("d", "Usar yarn", "site"),
        ];
        let block = memory_block(&notes, Some("checkout"), true).unwrap();
        assert!(block.starts_with("<dcc_agent_memory>"));
        assert!(block.contains("Everywhere:\n- Responder em pt-BR"));
        assert!(block.contains("In this project only:\n- Usar pnpm"));
        assert!(!block.contains("Usar yarn"));
        assert!(block.contains("&lt;/dcc_agent_memory>"));
        assert_eq!(block.matches("</dcc_agent_memory>").count(), 1);
        assert!(block.contains(REMEMBER_TOOL));
        assert!(block.contains("scope \"person\""));
        // Its own conversation has no project: the person's notes only.
        let home = memory_block(&notes, None, true).unwrap();
        assert!(!home.contains("Usar pnpm") && !home.contains("scope"));
        // A review that cannot save gets the notes only.
        let review = memory_block(&notes, Some("site"), false).unwrap();
        assert!(!review.contains(REMEMBER_TOOL) && review.contains("Usar yarn"));
        assert!(memory_block(&[], None, false).is_none());
        assert!(memory_block(&notes[2..3], Some("site"), false).is_none());
        assert!(memory_block(&[], None, true).unwrap().contains("Nothing saved yet."));
    }
}
