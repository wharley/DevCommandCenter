//! A resident agent's monthly spend (Tucano's "Limite mensal"): the tokens
//! its conversations used this month, the cached reads counted at a tenth,
//! against the limit the person set (none by default). A turn of a
//! conversation bound to an agent past its limit is refused before it goes,
//! unless the person chose "Continuar mesmo assim" for this month.

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

/// The start of the error a refused turn carries; the desktop and the phone
/// recognise it.
pub const AGENT_LIMIT_ERROR: &str = "agent monthly limit reached";

/// The limits the editor offers, in tokens.
pub const LIMIT_CHOICES: [u64; 4] = [1_000_000, 2_000_000, 5_000_000, 10_000_000];

/// What an agent used this month.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentSpend {
    /// Cached reads count at a tenth.
    pub tokens: u64,
    /// Only what the providers reported (Claude does; Codex and Cursor do not).
    pub cost_usd: Option<f64>,
    pub turns: u64,
}

/// `2026-11`, in the person's local time.
pub fn month_key(now: DateTime<Local>) -> String {
    format!("{:04}-{:02}", now.year(), now.month())
}

/// The first instant of the person's month, as stored timestamps compare.
pub fn month_start(now: DateTime<Local>) -> String {
    let first = Local
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .earliest()
        .unwrap_or(now);
    first
        .with_timezone(&Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// One turn's tokens as the limit counts them.
pub fn counted_tokens(total: u64, cached: u64) -> u64 {
    total.saturating_sub(cached - cached / 10)
}

/// Whether a turn of the agent waits: past the limit and not let through
/// for this month.
pub fn over_limit(
    spend: &AgentSpend,
    limit: Option<u64>,
    override_month: Option<&str>,
    month: &str,
) -> bool {
    limit.is_some_and(|limit| spend.tokens >= limit) && override_month != Some(month)
}

/// The error of a refused turn: `<AGENT_LIMIT_ERROR>: <agent name>`.
pub fn limit_error(agent: &str) -> String {
    format!("{AGENT_LIMIT_ERROR}: {agent}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_waits_past_the_limit_unless_let_through_this_month() {
        assert_eq!(counted_tokens(1_000, 900), 190);
        assert_eq!(counted_tokens(100, 0), 100);
        let spend = AgentSpend {
            tokens: 2_000_000,
            ..AgentSpend::default()
        };
        assert!(!over_limit(&spend, None, None, "2026-11"), "no limit");
        assert!(over_limit(&spend, Some(2_000_000), None, "2026-11"));
        assert!(!over_limit(
            &spend,
            Some(2_000_000),
            Some("2026-11"),
            "2026-11"
        ));
        assert!(over_limit(
            &spend,
            Some(2_000_000),
            Some("2026-10"),
            "2026-11"
        ));
        assert!(!over_limit(&spend, Some(5_000_000), None, "2026-11"));
        let now = Local.with_ymd_and_hms(2026, 11, 8, 15, 0, 0).unwrap();
        assert_eq!(month_key(now), "2026-11");
        assert!(month_start(now).starts_with("2026-1"));
        assert!(limit_error("QA").starts_with(AGENT_LIMIT_ERROR));
    }
}
