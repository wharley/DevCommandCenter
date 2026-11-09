//! The agents' routines as the app knows them (`dcc_core::domain::agent_routine`):
//! read from DCC's database once a minute by the scheduler (`shell::routines`)
//! and kept in a global the team room and the agent's page read; changes go
//! through the desktop API, which owns the table.

use dcc_core::domain::agent_routine::{Routine, RoutineDraft};
use gpui::Global;
use serde_json::json;

use crate::data::desktop_api::{ApiError, DesktopApi, encode};

#[derive(Default)]
pub struct Routines {
    pub list: Vec<Routine>,
}

impl Global for Routines {}

impl Routines {
    pub fn of<'a>(&'a self, agent_id: &'a str) -> impl Iterator<Item = &'a Routine> {
        self.list
            .iter()
            .filter(move |routine| routine.agent_id == agent_id)
    }

    /// The routines agents proposed, waiting for the person.
    pub fn proposed(&self) -> impl Iterator<Item = &Routine> {
        self.list.iter().filter(|routine| routine.proposed)
    }
}

/// Every live agent's routines from DCC's database.
pub fn read() -> Vec<Routine> {
    let db = crate::data::loader::database_path();
    dcc_infra::db::SqliteSessionRepo::open_read_only(&db)
        .and_then(|repo| repo.list_agent_routines())
        .unwrap_or_default()
}

fn routine_path(agent_id: &str, routine_id: &str, action: &str) -> String {
    format!(
        "/api/v1/agents/{}/routines/{}/{action}",
        encode(agent_id),
        encode(routine_id)
    )
}

impl DesktopApi {
    pub fn save_routine(
        &self,
        agent_id: &str,
        id: Option<&str>,
        draft: &RoutineDraft,
        enabled: bool,
    ) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/agents/{}/routines", encode(agent_id)),
            &json!({
                "id": id,
                "days": draft.days,
                "hour": draft.hour,
                "minute": draft.minute,
                "prompt": draft.prompt,
                "enabled": enabled,
            }),
        )
        .map(|_| ())
    }

    pub fn delete_routine(&self, agent_id: &str, id: &str) -> Result<(), ApiError> {
        self.post(&routine_path(agent_id, id, "delete"), &json!({}))
            .map(|_| ())
    }

    pub fn accept_routine(&self, agent_id: &str, id: &str) -> Result<(), ApiError> {
        self.post(&routine_path(agent_id, id, "accept"), &json!({}))
            .map(|_| ())
    }

    pub fn routine_ran(&self, agent_id: &str, id: &str, at: &str) -> Result<(), ApiError> {
        self.post(&routine_path(agent_id, id, "ran"), &json!({ "at": at }))
            .map(|_| ())
    }
}

/// `Seg, Qua · 09:00` / `Mon, Wed · 09:00`; every day: `Todo dia`.
pub fn schedule_label(routine: &Routine, english: bool) -> String {
    const PT: [&str; 7] = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
    const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let names = if english { EN } else { PT };
    let days = match routine.days.as_slice() {
        [0, 1, 2, 3, 4, 5, 6] => (if english { "Every day" } else { "Todo dia" }).to_string(),
        [0, 1, 2, 3, 4] => (if english { "Weekdays" } else { "Dias úteis" }).to_string(),
        days => days
            .iter()
            .filter_map(|day| names.get(*day as usize))
            .copied()
            .collect::<Vec<_>>()
            .join(", "),
    };
    format!("{days} · {:02}:{:02}", routine.hour, routine.minute)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_schedule_reads_short() {
        let routine = |days: Vec<u8>| Routine {
            id: "r".into(),
            agent_id: "a".into(),
            days,
            hour: 9,
            minute: 5,
            prompt: "x".into(),
            enabled: true,
            proposed: false,
            last_run_at: None,
            created_at: String::new(),
        };
        assert_eq!(
            schedule_label(&routine(vec![0, 2]), false),
            "Seg, Qua · 09:05"
        );
        assert_eq!(
            schedule_label(&routine(vec![0, 1, 2, 3, 4]), false),
            "Dias úteis · 09:05"
        );
        assert_eq!(
            schedule_label(&routine((0..7).collect()), true),
            "Every day · 09:05"
        );
    }
}
