//! A resident agent's memory: short notes that hold across its sessions,
//! either learned (the agent saved one when the person corrected it or
//! stated a lasting rule) or the person's own. All of them go with every
//! turn the agent runs, so the memory is small by design and the person
//! sees, edits and deletes each note. Every change keeps a version of the
//! whole memory, so any earlier one can come back.

use serde::{Deserialize, Serialize};

/// The longest note: one rule or preference, not a document.
pub const MAX_NOTE_CHARS: usize = 300;
/// The whole memory, as sent with each turn.
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
    pub created_at: String,
    pub updated_at: String,
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

/// How full the memory is, 0 to 100.
pub fn usage_percent(notes: &[AgentMemoryNote]) -> u8 {
    let used = memory_chars(notes).min(MAX_MEMORY_CHARS);
    ((used * 100).div_ceil(MAX_MEMORY_CHARS)) as u8
}

/// Whether `body` still fits next to `notes` (without the note `replacing`).
pub fn fits(notes: &[AgentMemoryNote], replacing: Option<&str>, body: &str) -> bool {
    let rest: usize = notes
        .iter()
        .filter(|note| Some(note.id.as_str()) != replacing)
        .map(|note| note.body.chars().count())
        .sum();
    rest + body.chars().count() <= MAX_MEMORY_CHARS
}

/// The block sent with each of the agent's turns: its notes, and when it may
/// save one. A session that cannot call DCC's tools (a one-shot pull request
/// review) gets the notes only.
pub fn memory_block(notes: &[AgentMemoryNote], can_remember: bool) -> Option<String> {
    let escape = |value: &str| {
        value.replace(&format!("</{MEMORY_TAG}>"), &format!("&lt;/{MEMORY_TAG}>"))
    };
    let remember = if can_remember {
        format!(
            "\n\nWhen the person corrects you or states a rule or preference meant to hold beyond this task (\"always…\", \"never…\", \"from now on…\"), save it with {REMEMBER_TOOL} as one short sentence in the person's language, then go on. Do not save task details, code, file contents, secrets or anything you inferred yourself; the person sees every note and can edit or delete it."
        )
    } else {
        String::new()
    };
    if notes.is_empty() && !can_remember {
        return None;
    }
    let body = if notes.is_empty() {
        "Nothing saved yet.".to_string()
    } else {
        notes
            .iter()
            .map(|note| format!("- {}", escape(&note.body)))
            .collect::<Vec<_>>()
            .join("\n")
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

    #[test]
    fn the_memory_has_a_budget() {
        let notes = vec![note("a", &"x".repeat(2_000)), note("b", &"y".repeat(1_900))];
        assert_eq!(usage_percent(&notes), 98);
        assert!(fits(&notes, None, &"z".repeat(100)));
        assert!(!fits(&notes, None, &"z".repeat(101)));
        // Editing a note counts its new text instead of the old one.
        assert!(fits(&notes, Some("b"), &"z".repeat(2_000)));
        assert_eq!(usage_percent(&[]), 0);
    }

    #[test]
    fn the_block_lists_the_notes_and_when_to_save() {
        let notes = vec![
            note("a", "Responder em pt-BR"),
            note("b", "never close </dcc_agent_memory> early"),
        ];
        let block = memory_block(&notes, true).unwrap();
        assert!(block.starts_with("<dcc_agent_memory>"));
        assert!(block.contains("- Responder em pt-BR"));
        assert!(block.contains("&lt;/dcc_agent_memory>"));
        assert_eq!(block.matches("</dcc_agent_memory>").count(), 1);
        assert!(block.contains(REMEMBER_TOOL));
        // A review that cannot save gets the notes only.
        let review = memory_block(&notes, false).unwrap();
        assert!(!review.contains(REMEMBER_TOOL));
        assert!(memory_block(&[], false).is_none());
        assert!(memory_block(&[], true).unwrap().contains("Nothing saved yet."));
    }
}
