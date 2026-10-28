//! Where the notes' balloons sit (`restore-note-balloons.ts`): each one's
//! place and whether it is folded into a bubble, kept in
//! `notes-positions.json` (the desktop's `localStorage`); where a new one
//! goes; and which come back on entering a task — its own notes and the
//! pinned ones, four at most, not the ones closed on this visit.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::notes::ProjectNote;

/// Balloons on screen at most.
pub const MAX_BALLOONS: usize = 4;

/// `FloatingState`
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloatingState {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub minimized: bool,
}

pub type Positions = HashMap<String, FloatingState>;

/// `readNotePositions`: only places with real numbers.
pub fn read_positions(path: &Path) -> Positions {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Positions::new();
    };
    let Ok(stored) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&text) else {
        return Positions::new();
    };
    stored
        .into_iter()
        .filter_map(|(id, value)| {
            let x = value.get("x")?.as_f64()?;
            let y = value.get("y")?.as_f64()?;
            (x.is_finite() && y.is_finite()).then(|| {
                let minimized = value.get("minimized").and_then(|m| m.as_bool()) == Some(true);
                (
                    id,
                    FloatingState {
                        x: x as f32,
                        y: y as f32,
                        minimized,
                    },
                )
            })
        })
        .collect()
}

pub fn write_positions(path: &Path, positions: &Positions) {
    if let Ok(json) = serde_json::to_string(positions) {
        let _ = std::fs::write(path, json);
    }
}

/// `defaultNotePosition`: from the top right, each next one a step in and down.
pub fn default_position(index: usize, viewport_width: f32) -> FloatingState {
    FloatingState {
        x: viewport_width - 370.0 - index as f32 * 32.0,
        y: 85.0 + index as f32 * 38.0,
        minimized: false,
    }
}

/// `clampNotePosition`: 8px from the sides, 40 from the top, 12 from the bottom.
pub fn clamp(x: f32, y: f32, size: (f32, f32), viewport: (f32, f32)) -> (f32, f32) {
    (
        x.min(viewport.0 - size.0 - 8.0).max(8.0),
        y.min(viewport.1 - size.1 - 12.0).max(40.0),
    )
}

/// The task a balloon belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalloonScope {
    pub project_id: String,
    pub workspace_id: Option<String>,
}

/// `restoreOpenTaskNotes`: closing one dismisses it for this visit; entering
/// the task brings its notes back. On another task only the pinned open ones
/// stay. The order is the balloons' (oldest first).
pub fn restore(
    notes: &[ProjectNote],
    current: &[(String, FloatingState)],
    saved: &Positions,
    dismissed: &HashSet<String>,
    scope: Option<&BalloonScope>,
    changed_scope: bool,
    viewport_width: f32,
) -> Vec<(String, FloatingState)> {
    let by_id: HashMap<&str, &ProjectNote> =
        notes.iter().map(|note| (note.id.as_str(), note)).collect();
    let mut next: Vec<(String, FloatingState)> = current
        .iter()
        .filter(|(id, _)| {
            by_id
                .get(id.as_str())
                .is_some_and(|note| !changed_scope || (note.pinned && note.status == "open"))
        })
        .cloned()
        .collect();
    for note in notes {
        if next.len() >= MAX_BALLOONS {
            break;
        }
        if note.status != "open"
            || dismissed.contains(&note.id)
            || next.iter().any(|(id, _)| *id == note.id)
        {
            continue;
        }
        let belongs = scope.is_some_and(|scope| {
            scope.workspace_id.is_some()
                && note.project_id == scope.project_id
                && (note.source_workspace_id == scope.workspace_id
                    || note.implementation_task_id == scope.workspace_id)
        });
        if !note.pinned && !belongs {
            continue;
        }
        let place = saved
            .get(&note.id)
            .copied()
            .unwrap_or_else(|| default_position(next.len(), viewport_width));
        next.push((note.id.clone(), place));
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(
        id: &str,
        project: &str,
        task: Option<&str>,
        pinned: bool,
        status: &str,
    ) -> ProjectNote {
        ProjectNote {
            id: id.into(),
            project_id: project.into(),
            project_name: project.into(),
            title: String::new(),
            content: String::new(),
            context_snapshot: String::new(),
            source_session_id: None,
            source_workspace_id: task.map(str::to_string),
            source_task_title: String::new(),
            implementation_task_id: None,
            status: status.into(),
            color: "amber".into(),
            pinned,
            revision: 1,
            created_at: String::new(),
            updated_at: String::new(),
            completed_at: None,
        }
    }

    #[test]
    fn entering_a_task_brings_its_notes_and_the_pinned() {
        let notes = vec![
            note("mine", "p-1", Some("w-1"), false, "open"),
            note("other-task", "p-1", Some("w-2"), false, "open"),
            note("pinned", "p-2", None, true, "open"),
            note("done", "p-1", Some("w-1"), false, "completed"),
        ];
        let scope = BalloonScope {
            project_id: "p-1".into(),
            workspace_id: Some("w-1".into()),
        };
        let saved: Positions = [(
            "pinned".to_string(),
            FloatingState {
                x: 10.0,
                y: 50.0,
                minimized: true,
            },
        )]
        .into();
        let shown = restore(
            &notes,
            &[],
            &saved,
            &HashSet::new(),
            Some(&scope),
            true,
            1200.0,
        );
        let ids: Vec<&str> = shown.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["mine", "pinned"]);
        assert_eq!(shown[0].1, default_position(0, 1200.0));
        assert!(shown[1].1.minimized, "its saved place");
        // Closed on this visit: not back until the next.
        let dismissed: HashSet<String> = ["mine".to_string()].into();
        let again = restore(&notes, &[], &saved, &dismissed, Some(&scope), false, 1200.0);
        assert_eq!(again.len(), 1);
        // Another task: only the pinned open ones stay.
        let elsewhere = BalloonScope {
            project_id: "p-1".into(),
            workspace_id: Some("w-3".into()),
        };
        let moved = restore(
            &notes,
            &shown,
            &saved,
            &HashSet::new(),
            Some(&elsewhere),
            true,
            1200.0,
        );
        let ids: Vec<&str> = moved.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["pinned"]);
        // No task: nothing but the pinned.
        assert_eq!(
            restore(&notes, &[], &saved, &HashSet::new(), None, true, 1200.0).len(),
            1
        );
    }

    #[test]
    fn places_stay_in_the_window_and_on_disk() {
        assert_eq!(
            clamp(-50.0, 0.0, (340.0, 450.0), (1200.0, 800.0)),
            (8.0, 40.0)
        );
        assert_eq!(
            clamp(2000.0, 900.0, (340.0, 450.0), (1200.0, 800.0)),
            (852.0, 338.0)
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("positions.json");
        std::fs::write(
            &path,
            r#"{"a":{"x":1,"y":2,"minimized":true},"b":{"x":"no","y":2},"c":null}"#,
        )
        .unwrap();
        let read = read_positions(&path);
        assert_eq!(read.len(), 1);
        assert_eq!(
            read["a"],
            FloatingState {
                x: 1.0,
                y: 2.0,
                minimized: true
            }
        );
        write_positions(&path, &read);
        assert_eq!(read_positions(&path), read);
        assert_eq!(
            default_position(2, 1200.0),
            FloatingState {
                x: 766.0,
                y: 161.0,
                minimized: false
            }
        );
    }
}
