//! Rooms (`docs/PLANO_SALA_POR_PROJETO`): which room an agent's conversation
//! is in, what a turn there hears about its project, and the teammate's
//! conversation in the same room for a pass.

use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::room_context::{context_block, AGENT_FILES, FALLBACK_FILE};

use super::*;

impl SessionCommandState {
    pub fn set_session_room(&self, session_id: &SessionId, project_id: &str) -> Result<()> {
        self.session_repo.set_session_room(session_id, project_id)
    }

    pub fn session_room(&self, session_id: &SessionId) -> Result<Option<String>> {
        self.session_repo.session_room(session_id)
    }

    /// What a turn in a project's room hears: which project it is, so work
    /// asked there goes to it without the person naming it.
    pub(super) fn room_block(&self, session_id: &SessionId) -> Result<Option<String>> {
        let Some(project) = self.session_room(session_id)? else {
            return Ok(None);
        };
        let (name, path) = self
            .session_repo
            .project_name_and_path(&project)
            .unwrap_or_else(|| (project.clone(), String::new()));
        let folder = if path.is_empty() {
            String::new()
        } else {
            format!(" Its folder is {path}.")
        };
        let room = format!(
            "<dcc_room project=\"{name}\">\nThis conversation is the team room of the project \"{name}\".{folder} What the person asks here is about it unless they name another project: open its tasks in \"{name}\" and, for where things stand, look at its tasks first.\n</dcc_room>"
        );
        Ok(Some(match context_block(&name, &project_files(&path)) {
            Some(context) => format!("{room}\n\n{context}"),
            None => room,
        }))
    }

    /// The project whose notes a turn of `session_id` carries: a room's
    /// project first, else the task's.
    pub fn memory_project_of_session(&self, session_id: &SessionId) -> Result<Option<String>> {
        if let Some(room) = self.session_room(session_id)? {
            return Ok(Some(room));
        }
        self.session_repo.memory_project_of_session(&session_id.0)
    }

    /// `agent_id`'s conversation in the room of `room` (none: the Geral
    /// room), the most recent one.
    pub fn agent_room_session(&self, agent_id: &str, room: Option<&str>) -> Result<Option<String>> {
        let home = home_project_id(agent_id);
        Ok(self
            .list_agent_session_bindings()?
            .into_iter()
            .filter(|binding| {
                binding.agent_id == agent_id
                    && binding.project_id == home
                    && binding.room.as_deref() == room
            })
            .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
            .map(|binding| binding.session_id))
    }
}

/// A file bigger than this is not read at all (a generated README).
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// The project's notes for agents, `(name, text)`: `CLAUDE.md` and
/// `AGENTS.md`, else `README.md`; only files really inside the folder (a
/// link pointing out of it is left out, as in Tucano).
pub(crate) fn project_files(root: &str) -> Vec<(String, String)> {
    let Ok(root) = std::fs::canonicalize(root) else {
        return Vec::new();
    };
    let read = |name: &str| -> Option<(String, String)> {
        let real = std::fs::canonicalize(root.join(name)).ok()?;
        let metadata = std::fs::metadata(&real).ok()?;
        if !real.starts_with(&root) || !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return None;
        }
        std::fs::read_to_string(&real)
            .ok()
            .map(|text| (name.to_string(), text))
    };
    let mut files: Vec<(String, String)> =
        AGENT_FILES.iter().filter_map(|name| read(name)).collect();
    if files.is_empty() {
        files.extend(read(FALLBACK_FILE));
    }
    files
}

#[cfg(test)]
mod tests {
    use super::project_files;

    #[test]
    fn the_projects_notes_are_read_from_inside_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("checkout");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("README.md"), "readme").unwrap();
        // Only the README: it is the fallback.
        let names = |files: Vec<(String, String)>| -> Vec<String> {
            files.into_iter().map(|(name, _)| name).collect()
        };
        assert_eq!(names(project_files(root.to_str().unwrap())), ["README.md"]);
        std::fs::write(root.join("AGENTS.md"), "agents").unwrap();
        // A link out of the folder is left out.
        let outside = dir.path().join("secret.md");
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("CLAUDE.md")).unwrap();
        assert_eq!(names(project_files(root.to_str().unwrap())), ["AGENTS.md"]);
        assert!(project_files("/nope/nowhere").is_empty());
    }
}
