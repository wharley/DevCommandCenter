//! A resident agent's memory, as its page reads and changes it
//! (`/api/v1/agents/:id/memory`): the notes of each layer (the person's,
//! each project's), how full each is, and the versions the person can bring
//! back.

use serde::Deserialize;
use serde_json::json;

use crate::data::desktop_api::{ApiError, DesktopApi, encode};

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNote {
    pub id: String,
    pub body: String,
    /// `learned` (the agent saved it) or `person`.
    pub source: String,
    /// The project it holds in; `None`: the person's, everywhere.
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub updated_at: String,
}

impl MemoryNote {
    pub fn learned(&self) -> bool {
        self.source == "learned"
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryVersion {
    pub id: i64,
    pub created_at: String,
    /// `learned`, `added`, `edited`, `removed` or `restored`.
    pub change: String,
    pub summary: String,
    pub notes: Vec<MemoryNote>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemory {
    /// How full the person's layer is.
    pub usage: u8,
    /// How full each project's layer is, by project id.
    #[serde(default)]
    pub project_usage: std::collections::HashMap<String, u8>,
    pub notes: Vec<MemoryNote>,
    pub versions: Vec<MemoryVersion>,
}

/// The notes the filter and the search leave.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoteFilter {
    #[default]
    All,
    Learned,
    Person,
}

impl AgentMemory {
    /// How full `layer` is (`None`: the person's).
    pub fn usage_of(&self, layer: Option<&str>) -> u8 {
        match layer {
            None => self.usage,
            Some(project) => self.project_usage.get(project).copied().unwrap_or(0),
        }
    }
}

pub fn visible<'a>(
    notes: &'a [MemoryNote],
    layer: Option<&str>,
    filter: NoteFilter,
    query: &str,
) -> Vec<&'a MemoryNote> {
    let query = query.trim().to_lowercase();
    notes
        .iter()
        .filter(|note| note.project_id.as_deref() == layer)
        .filter(|note| match filter {
            NoteFilter::All => true,
            NoteFilter::Learned => note.learned(),
            NoteFilter::Person => !note.learned(),
        })
        .filter(|note| query.is_empty() || note.body.to_lowercase().contains(&query))
        .collect()
}

fn parse(value: serde_json::Value) -> Result<AgentMemory, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    fn memory_path(agent_id: &str) -> String {
        format!("/api/v1/agents/{}/memory", encode(agent_id))
    }

    pub fn agent_memory(&self, agent_id: &str) -> Result<AgentMemory, ApiError> {
        parse(self.get(&Self::memory_path(agent_id))?)
    }

    /// A note for the person (`project_id` `None`) or for a project.
    pub fn add_memory_note(
        &self,
        agent_id: &str,
        body: &str,
        project_id: Option<&str>,
    ) -> Result<AgentMemory, ApiError> {
        parse(self.post(
            &Self::memory_path(agent_id),
            &json!({ "body": body, "projectId": project_id }),
        )?)
    }

    pub fn edit_memory_note(
        &self,
        agent_id: &str,
        note_id: &str,
        body: &str,
    ) -> Result<AgentMemory, ApiError> {
        let path = format!("{}/{}", Self::memory_path(agent_id), encode(note_id));
        parse(self.post(&path, &json!({ "body": body }))?)
    }

    pub fn remove_memory_note(
        &self,
        agent_id: &str,
        note_id: &str,
    ) -> Result<AgentMemory, ApiError> {
        let path = format!("{}/{}", Self::memory_path(agent_id), encode(note_id));
        parse(self.delete(&path)?)
    }

    pub fn restore_memory_version(
        &self,
        agent_id: &str,
        version_id: i64,
    ) -> Result<AgentMemory, ApiError> {
        let path = format!(
            "{}/versions/{version_id}/restore",
            Self::memory_path(agent_id)
        );
        parse(self.post(&path, &json!({}))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(body: &str, source: &str) -> MemoryNote {
        MemoryNote {
            id: body.to_string(),
            body: body.to_string(),
            source: source.to_string(),
            project_id: None,
            updated_at: String::new(),
        }
    }

    #[test]
    fn the_filter_and_the_search_narrow_the_notes() {
        let notes = vec![
            note("Responder em pt-BR", "learned"),
            note("Nunca fazer merge", "person"),
            MemoryNote {
                project_id: Some("checkout".into()),
                ..note("Usar pnpm", "learned")
            },
        ];
        assert_eq!(visible(&notes, None, NoteFilter::All, "").len(), 2);
        assert_eq!(
            visible(&notes, None, NoteFilter::Learned, "")[0].body,
            "Responder em pt-BR"
        );
        assert_eq!(
            visible(&notes, None, NoteFilter::Person, "")[0].body,
            "Nunca fazer merge"
        );
        assert_eq!(visible(&notes, None, NoteFilter::All, " MERGE ").len(), 1);
        assert!(visible(&notes, None, NoteFilter::Learned, "merge").is_empty());
        // A project's layer shows its notes only.
        let checkout = visible(&notes, Some("checkout"), NoteFilter::All, "");
        assert_eq!(checkout.len(), 1);
        assert_eq!(checkout[0].body, "Usar pnpm");
    }

    #[test]
    fn the_answer_reads_as_the_server_sends_it() {
        let memory = parse(json!({
            "usage": 3,
            "notes": [{"id": "n", "body": "b", "source": "learned", "createdAt": "t", "updatedAt": "t"}],
            "versions": [{"id": 4, "createdAt": "t", "change": "learned", "summary": "b",
                          "notes": [{"id": "n", "body": "b", "source": "learned", "createdAt": "t", "updatedAt": "t"}]}],
        }))
        .unwrap();
        assert_eq!(memory.usage, 3);
        assert_eq!(memory.usage_of(Some("checkout")), 0);
        assert!(memory.notes[0].learned());
        assert_eq!(memory.versions[0].notes.len(), 1);
    }
}
