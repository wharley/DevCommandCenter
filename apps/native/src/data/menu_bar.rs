//! The menu bar panel's snapshot (`menu-bar/api.ts`), read from the desktop
//! (`GET /api/v1/menu-bar`): the tasks running, waiting or just finished,
//! the DCC processes' CPU and memory, and the keep-awake hold.

use serde::Deserialize;

use super::desktop_api::{ApiError, DesktopApi};
use super::keep_awake::Hold;

/// `MenuBarTask`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Task {
    pub workspace_id: String,
    pub session_id: String,
    pub title: String,
    pub project: String,
    /// running, permission, input, completed or aborted.
    pub status: String,
    pub updated_at: String,
}

impl Task {
    /// `activeTask`
    pub fn active(&self) -> bool {
        matches!(self.status.as_str(), "running" | "permission" | "input")
    }

    pub fn waiting(&self) -> bool {
        matches!(self.status.as_str(), "permission" | "input")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Metrics {
    pub cpu_percent: Option<f64>,
    pub memory_bytes: u64,
    pub process_count: usize,
}

/// `MenuBarSnapshot`
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Snapshot {
    pub tasks: Vec<Task>,
    pub metrics: Option<Metrics>,
    pub keep_awake: Option<Hold>,
}

/// `Intl.NumberFormat(…, { maximumFractionDigits })`: no trailing zeros, a
/// comma in Portuguese.
pub fn number(value: f64, digits: usize, english: bool) -> String {
    // Half away from zero, like `Intl` (`format!` rounds half to even).
    let scale = 10f64.powi(digits as i32);
    let value = (value * scale).round() / scale;
    let fixed = format!("{value:.digits$}");
    let trimmed = if fixed.contains('.') {
        fixed
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        fixed
    };
    let (whole, fraction) = trimmed.split_once('.').unwrap_or((&trimmed, ""));
    let negative = whole.starts_with('-');
    let digits_only = whole.trim_start_matches('-');
    let mut grouped = String::new();
    for (index, digit) in digits_only.chars().enumerate() {
        if index > 0 && (digits_only.len() - index) % 3 == 0 {
            grouped.push(if english { ',' } else { '.' });
        }
        grouped.push(digit);
    }
    let sign = if negative { "-" } else { "" };
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!(
            "{sign}{grouped}{}{fraction}",
            if english { '.' } else { ',' }
        )
    }
}

/// CPU as the panel shows it: `12,3%`, or `—` while the sampler warms up.
pub fn cpu(metrics: Option<&Metrics>, english: bool) -> String {
    metrics.and_then(|metrics| metrics.cpu_percent).map_or_else(
        || "—".to_string(),
        |cpu| format!("{}%", number(cpu, 1, english)),
    )
}

/// Memory in MiB, GiB past 1024.
pub fn memory(metrics: Option<&Metrics>, english: bool) -> String {
    let Some(metrics) = metrics else {
        return "—".into();
    };
    let mib = metrics.memory_bytes as f64 / 1024.0 / 1024.0;
    if mib >= 1024.0 {
        format!("{} GiB", number(mib / 1024.0, 1, english))
    } else {
        format!("{} MiB", number(mib, 0, english))
    }
}

impl DesktopApi {
    /// `menu_bar_snapshot`
    pub fn menu_bar_snapshot(&self) -> Result<Snapshot, ApiError> {
        let value = self.get("/api/v1/menu-bar")?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_like_intl() {
        assert_eq!(number(12.34, 1, false), "12,3");
        assert_eq!(number(12.0, 1, false), "12");
        assert_eq!(number(1234.0, 0, false), "1.234");
        assert_eq!(number(1234.5, 1, true), "1,234.5");
        let metrics = Metrics {
            cpu_percent: Some(3.25),
            memory_bytes: 512 * 1024 * 1024,
            process_count: 4,
        };
        assert_eq!(cpu(Some(&metrics), false), "3,3%");
        assert_eq!(memory(Some(&metrics), false), "512 MiB");
        let big = Metrics {
            memory_bytes: 1536 * 1024 * 1024,
            ..metrics.clone()
        };
        assert_eq!(memory(Some(&big), true), "1.5 GiB");
        assert_eq!(cpu(None, false), "—");
        assert_eq!(cpu(Some(&Metrics::default()), false), "—", "warming up");
    }

    #[test]
    fn the_snapshot_reads_the_desktops_shape() {
        let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
            "tasks": [
                { "workspaceId": "w", "sessionId": "s", "title": "Login", "project": "app",
                  "status": "permission", "updatedAt": "2026-10-28T10:00:00Z" },
                { "workspaceId": "w2", "sessionId": "s2", "title": "Docs", "project": "app",
                  "status": "completed", "updatedAt": "2026-10-28T09:00:00Z" }
            ],
            "metrics": { "cpuPercent": null, "memoryBytes": 1048576, "processCount": 2 },
            "keepAwake": { "reason": { "kind": "agentsWorking", "count": 2 }, "label": "x" }
        }))
        .unwrap();
        assert!(snapshot.tasks[0].active() && snapshot.tasks[0].waiting());
        assert!(!snapshot.tasks[1].active());
        assert!(snapshot.keep_awake.is_some());
    }
}
