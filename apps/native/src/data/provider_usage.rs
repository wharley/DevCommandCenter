//! How much of a sign-in's plan is left (`provider-account-usage.ts`,
//! `provider-account-usage-panel.tsx`): the tightest window, its severity,
//! the windows' names and reset times, the saved resets the person can
//! spend; and Antigravity's runtime and Google sign-in. Read and acted on
//! over the desktop's loopback API.

use std::time::Duration;

use dcc_core::domain::provider::{
    ProviderAccountUsage, ProviderAccountUsageState, ProviderModelDescriptor, ProviderResetCredits,
    ProviderResetOutcome, ProviderUsageWindow,
};
use dcc_core::ports::provider::ProviderRuntimeConfig;
use serde::Deserialize;
use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi};

/// Installing downloads the runtime; connecting waits on the browser.
const LONG_WAIT: Duration = Duration::from_secs(15 * 60);

/// `ProviderUsageSeverity`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Critical,
}

/// `mostConstrainedUsageWindow`: an exhausted window first, else the one
/// with the least left.
pub fn tightest(usage: Option<&ProviderAccountUsage>) -> Option<&ProviderUsageWindow> {
    let usage = usage?;
    if !matches!(usage.state, ProviderAccountUsageState::Available) {
        return None;
    }
    usage.windows.iter().reduce(|current, candidate| {
        if candidate.is_exhausted != current.is_exhausted {
            return if candidate.is_exhausted {
                candidate
            } else {
                current
            };
        }
        if candidate.remaining_percent < current.remaining_percent {
            candidate
        } else {
            current
        }
    })
}

/// `providerUsageSeverity`
pub fn severity(window: &ProviderUsageWindow) -> Option<Severity> {
    if window.is_exhausted || window.remaining_percent <= 5.0 {
        Some(Severity::Critical)
    } else if window.remaining_percent <= 20.0 {
        Some(Severity::Warning)
    } else {
        None
    }
}

/// `windowLabel`: the key under `settings.model`, or the id spelled out.
pub fn window_label(window: &ProviderUsageWindow) -> Result<&'static str, String> {
    let known = match window.id.as_str() {
        "primary" => Some("usageWindowPrimary"),
        "secondary" => Some("usageWindowSecondary"),
        "five_hour" => Some("usageWindowFiveHour"),
        "seven_day" => Some("usageWindowSevenDay"),
        "seven_day_opus" => Some("usageWindowSevenDayOpus"),
        "seven_day_sonnet" => Some("usageWindowSevenDaySonnet"),
        "overage" => Some("usageWindowOverage"),
        "subscription" => Some("usageWindowSubscription"),
        _ => match window.window_duration_minutes {
            Some(300) => Some("usageWindowFiveHour"),
            Some(10_080) => Some("usageWindowSevenDay"),
            _ => None,
        },
    };
    known.ok_or_else(|| window.id.replace('_', " "))
}

fn local(at: &str) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&chrono::Local))
}

/// `Intl.DateTimeFormat(locale, { dateStyle: "short", timeStyle: "short" })`.
pub fn short_date_time(at: &str, english: bool) -> Option<String> {
    use chrono::Timelike;
    let at = local(at)?;
    Some(if english {
        let (pm, hour) = at.hour12();
        format!(
            "{}, {hour}:{:02} {}",
            at.format("%-m/%-d/%y"),
            at.minute(),
            if pm { "PM" } else { "AM" }
        )
    } else {
        at.format("%d/%m/%Y, %H:%M").to_string()
    })
}

/// `Intl.DateTimeFormat(locale, { timeStyle: "short" })`.
pub fn short_time(at: &str, english: bool) -> Option<String> {
    use chrono::Timelike;
    let at = local(at)?;
    Some(if english {
        let (pm, hour) = at.hour12();
        format!("{hour}:{:02} {}", at.minute(), if pm { "PM" } else { "AM" })
    } else {
        at.format("%H:%M").to_string()
    })
}

/// `formatResetExpiry`'s relative part: days, or hours within a day, as
/// `Intl.RelativeTimeFormat(locale, { numeric: "auto" })` says them.
pub fn relative_expiry(at_ms: i64, now_ms: i64, english: bool) -> String {
    let diff = (at_ms - now_ms) as f64;
    let days = (diff / 86_400_000.0).round() as i64;
    if days.abs() >= 1 {
        return match (days, english) {
            (1, true) => "tomorrow".into(),
            (-1, true) => "yesterday".into(),
            (1, false) => "amanhã".into(),
            (-1, false) => "ontem".into(),
            (2, false) => "depois de amanhã".into(),
            (-2, false) => "anteontem".into(),
            (n, true) if n > 0 => format!("in {n} days"),
            (n, true) => format!("{} days ago", -n),
            (n, false) if n > 0 => format!("em {n} dias"),
            (n, false) => format!("há {} dias", -n),
        };
    }
    let hours = (diff / 3_600_000.0).round() as i64;
    match (hours, english) {
        (0, true) => "this hour".into(),
        (0, false) => "esta hora".into(),
        (1, true) => "in 1 hour".into(),
        (1, false) => "em 1 hora".into(),
        (-1, true) => "1 hour ago".into(),
        (-1, false) => "há 1 hora".into(),
        (n, true) if n > 0 => format!("in {n} hours"),
        (n, true) => format!("{} hours ago", -n),
        (n, false) if n > 0 => format!("em {n} horas"),
        (n, false) => format!("há {} horas", -n),
    }
}

/// `RedeemableReset`: `id` is none when Codex only reports a count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redeemable {
    pub id: Option<String>,
    /// Empty: the caller names it (`usageResetNext` / `usageResetLabel`).
    pub title: String,
    pub expires_at: Option<String>,
    pub expires_ms: Option<i64>,
}

/// `redeemableResets`: unexpired credits, soonest first; a bare count is
/// one unnamed "next" reset. Titles left empty get their position (0 for
/// the bare count, else 1, 2…) in `position`.
pub fn redeemable(credits: Option<&ProviderResetCredits>, now_ms: i64) -> Vec<(usize, Redeemable)> {
    let Some(credits) = credits.filter(|credits| credits.available_count > 0) else {
        return Vec::new();
    };
    let mut listed: Vec<Redeemable> = credits
        .credits
        .iter()
        .map(|credit| {
            let expires_ms = credit
                .expires_at
                .as_deref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| at.timestamp_millis());
            Redeemable {
                id: Some(credit.id.clone()),
                title: credit
                    .title
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                expires_at: credit.expires_at.clone(),
                expires_ms,
            }
        })
        .filter(|credit| credit.expires_ms.is_none_or(|at| at > now_ms))
        .collect();
    listed.sort_by_key(|credit| credit.expires_ms.unwrap_or(i64::MAX));
    if listed.is_empty() {
        return vec![(
            0,
            Redeemable {
                id: None,
                title: String::new(),
                expires_at: None,
                expires_ms: None,
            },
        )];
    }
    listed
        .into_iter()
        .enumerate()
        .map(|(index, credit)| (index + 1, credit))
        .collect()
}

/// `AntigravityStatusOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityStatus {
    pub managed_runtime_installed: bool,
    pub runtime_version: Option<String>,
    pub signed_in: bool,
    pub cached_model_count: usize,
}

fn runtime_body(provider_id: &str, runtime: Option<&ProviderRuntimeConfig>) -> Value {
    json!({ "providerId": provider_id, "providerRuntime": runtime })
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    /// `getProviderAccountUsage`: `runtime` carries the account.
    pub fn provider_usage(
        &self,
        provider_id: &str,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<Option<ProviderAccountUsage>, ApiError> {
        let value = self.post(
            "/api/v1/provider-accounts/usage",
            &runtime_body(provider_id, runtime),
        )?;
        decode(value.get("usage").cloned().unwrap_or(Value::Null))
    }

    /// `consumeProviderAccountReset`
    pub fn provider_reset(
        &self,
        provider_id: &str,
        credit_id: Option<&str>,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<ProviderResetOutcome, ApiError> {
        let mut body = runtime_body(provider_id, runtime);
        body["creditId"] = json!(credit_id);
        let value = self.post("/api/v1/provider-accounts/reset", &body)?;
        decode(value.get("outcome").cloned().unwrap_or(Value::Null))
    }

    /// `getAntigravityStatus`
    pub fn antigravity_status(
        &self,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<AntigravityStatus, ApiError> {
        decode(self.post(
            "/api/v1/antigravity/status",
            &json!({ "providerRuntime": runtime }),
        )?)
    }

    /// `installAntigravity`: the version installed.
    pub fn install_antigravity(&self) -> Result<String, ApiError> {
        let value = self.request_waiting(
            "POST",
            "/api/v1/antigravity/install",
            Some(&json!({})),
            LONG_WAIT,
        )?;
        Ok(value
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    /// `connectAntigravity`: the account's models.
    pub fn connect_antigravity(
        &self,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<Vec<ProviderModelDescriptor>, ApiError> {
        let value = self.request_waiting(
            "POST",
            "/api/v1/antigravity/connect",
            Some(&json!({ "providerRuntime": runtime })),
            LONG_WAIT,
        )?;
        decode(value.get("models").cloned().unwrap_or(Value::Null))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(windows: Value) -> ProviderAccountUsage {
        serde_json::from_value(json!({
            "providerId": "codex", "state": "available", "windows": windows,
            "updatedAt": "2026-10-27T12:00:00Z", "isCached": false,
        }))
        .unwrap()
    }

    #[test]
    fn the_tightest_window_and_its_severity() {
        let usage = usage(json!([
            { "id": "five_hour", "usedPercent": 70.0, "remainingPercent": 30.0, "isExhausted": false },
            { "id": "seven_day", "usedPercent": 85.0, "remainingPercent": 15.0, "isExhausted": false },
            { "id": "x_y", "usedPercent": 10.0, "remainingPercent": 90.0, "isExhausted": false, "windowDurationMinutes": 300 },
        ]));
        let tight = tightest(Some(&usage)).unwrap();
        assert_eq!(tight.id, "seven_day");
        assert_eq!(severity(tight), Some(Severity::Warning));
        assert_eq!(window_label(&usage.windows[2]), Ok("usageWindowFiveHour"));
        let mut odd = usage.windows[2].clone();
        odd.window_duration_minutes = None;
        assert_eq!(window_label(&odd), Err("x y".into()));
        let exhausted = self::usage(json!([
            { "id": "a", "usedPercent": 10.0, "remainingPercent": 90.0, "isExhausted": true },
            { "id": "b", "usedPercent": 99.0, "remainingPercent": 1.0, "isExhausted": false },
        ]));
        assert_eq!(tightest(Some(&exhausted)).unwrap().id, "a");
        assert_eq!(severity(&exhausted.windows[1]), Some(Severity::Critical));
    }

    #[test]
    fn resets_are_soonest_first_and_relative_times_read_like_intl() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-27T12:00:00Z")
            .unwrap()
            .timestamp_millis();
        let credits: ProviderResetCredits = serde_json::from_value(json!({
            "availableCount": 3,
            "credits": [
                { "id": "late", "title": "Full reset", "expiresAt": "2026-11-20T12:00:00Z" },
                { "id": "gone", "expiresAt": "2026-10-01T12:00:00Z" },
                { "id": "soon", "expiresAt": "2026-10-29T12:00:00Z" },
            ],
        }))
        .unwrap();
        let resets = redeemable(Some(&credits), now);
        let ids: Vec<_> = resets
            .iter()
            .map(|(_, reset)| reset.id.as_deref().unwrap())
            .collect();
        assert_eq!(ids, ["soon", "late"]);
        assert_eq!(resets[0].0, 1);
        assert_eq!(resets[1].1.title, "Full reset");
        let count_only: ProviderResetCredits =
            serde_json::from_value(json!({ "availableCount": 1 })).unwrap();
        assert_eq!(redeemable(Some(&count_only), now)[0].1.id, None);
        let day = 86_400_000;
        assert_eq!(
            relative_expiry(now + 2 * day, now, false),
            "depois de amanhã"
        );
        assert_eq!(relative_expiry(now + 5 * day, now, true), "in 5 days");
        assert_eq!(
            relative_expiry(now + 3 * 3_600_000, now, false),
            "em 3 horas"
        );
        assert_eq!(short_date_time("nope", false), None);
    }
}
