//! The project rooms down the team's column (`docs/PLANO_SALA_POR_PROJETO`):
//! the ones the person made with "Nova sala", kept in `agent-rooms.json`, and
//! any project an agent already talks in; each project once, and only
//! projects that still exist.

use std::path::Path;

use super::agents::AgentRecord;

fn path() -> std::path::PathBuf {
    crate::prefs::native_file("agent-rooms.json")
}

fn load_from(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The rooms the person made (none under test: never the real file).
pub fn load() -> Vec<String> {
    if cfg!(test) {
        return Vec::new();
    }
    load_from(&path())
}

pub fn save(rooms: &[String]) {
    if cfg!(test) {
        return;
    }
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_string(rooms).unwrap_or_default());
}

/// The rooms to list, `(project id, name)`: the order the phone's list
/// shares (`dcc_core::domain::room_list`).
pub use dcc_core::domain::room_list::rooms;

/// The projects agents talk in, from their conversations.
pub fn talked(agents: &[AgentRecord]) -> Vec<String> {
    agents
        .iter()
        .flat_map(|agent| agent.sessions.iter())
        .filter_map(|session| session.room.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_holds_no_rooms() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_from(&dir.path().join("none.json")).is_empty());
    }
}
