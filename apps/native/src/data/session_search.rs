//! "Sessões passadas" (`SessionSearchDialog`, `search_sessions`): the
//! database's full-text search over titles and what the sessions said,
//! read directly; with no query, the recent sessions of every task.

use std::path::Path;

use dcc_core::domain::session::SessionSearchResult;
use dcc_infra::db::SqliteSessionRepo;

/// The dialog's `limit: 40`.
pub const LIMIT: usize = 40;

pub fn search(db_path: &Path, query: &str) -> anyhow::Result<Vec<SessionSearchResult>> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    Ok(SqliteSessionRepo::open_read_only(db_path)?.search_sessions(query, LIMIT)?)
}

/// `workspaceLabel`: "task · branch", either alone, else the task's id.
pub fn workspace_label(result: &SessionSearchResult) -> String {
    let name = result
        .workspace_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let branch = result
        .workspace_branch
        .as_deref()
        .map(str::trim)
        .filter(|branch| !branch.is_empty());
    match (name, branch) {
        (Some(name), Some(branch)) => format!("{name} · {branch}"),
        (Some(name), None) => name.to_string(),
        (None, Some(branch)) => branch.to_string(),
        (None, None) => result.workspace_id.0.clone(),
    }
}

const MONTHS_PT: [&str; 12] = [
    "jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.", "dez.",
];
const MONTHS_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `Intl.DateTimeFormat(…, { dateStyle: "medium", timeStyle: "short" })`.
pub fn updated_at_label(value: &str, english: bool) -> String {
    use chrono::{Datelike, Timelike};
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(value) else {
        return value.to_string();
    };
    let at = at.with_timezone(&chrono::Local);
    let month = at.month0() as usize;
    if english {
        let hour = match at.hour() % 12 {
            0 => 12,
            hour => hour,
        };
        let meridiem = if at.hour() < 12 { "AM" } else { "PM" };
        format!(
            "{} {}, {}, {hour}:{:02} {meridiem}",
            MONTHS_EN[month],
            at.day(),
            at.year(),
            at.minute()
        )
    } else {
        format!(
            "{} de {} de {}, {:02}:{:02}",
            at.day(),
            MONTHS_PT[month],
            at.year(),
            at.hour(),
            at.minute()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_date_reads_like_intl_medium_and_short() {
        let local = chrono::Local::now()
            .with_timezone(&chrono::Local)
            .fixed_offset()
            .offset()
            .to_string();
        let value = format!("2026-10-25T14:03:00{local}");
        assert_eq!(updated_at_label(&value, false), "25 de out. de 2026, 14:03");
        assert_eq!(updated_at_label(&value, true), "Oct 25, 2026, 2:03 PM");
        assert_eq!(updated_at_label("not a date", false), "not a date");
    }

    #[test]
    fn searching_reads_the_database_directly() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            search(&dir.path().join("none.sqlite"), "x")
                .unwrap()
                .is_empty()
        );
        let path = dir.path().join("database.sqlite");
        drop(SqliteSessionRepo::open(&path).unwrap());
        assert!(search(&path, "").unwrap().is_empty());
        assert!(search(&path, "checkout").unwrap().is_empty());
    }
}
