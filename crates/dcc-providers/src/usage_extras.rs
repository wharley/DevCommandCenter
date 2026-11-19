//! What a plan has beyond its windows, as each maker says it: Codex's
//! credits (`rateLimits.credits`: `hasCredits`, `unlimited`, `balance`),
//! shown only where the account has them, and Claude's extra usage
//! (`extra_usage`, in cents, where it is on): what was used of its monthly
//! cap, or what was spent where it has none, in dollars.

use dcc_core::domain::provider::ProviderUsageExtra;
use serde_json::Value;

/// Codex's credits, from `account/rateLimits/read`'s `rateLimits`.
pub fn codex(limits: &Value) -> Vec<ProviderUsageExtra> {
    let credits = &limits["credits"];
    let unlimited = credits["unlimited"].as_bool() == Some(true);
    let balance = match &credits["balance"] {
        Value::String(text) => text.trim().parse::<f64>().ok(),
        other => other.as_f64(),
    };
    let has = credits["hasCredits"].as_bool() == Some(true) || balance.is_some_and(|b| b > 0.0);
    if !unlimited && !has {
        return Vec::new();
    }
    vec![ProviderUsageExtra {
        id: "credits".to_string(),
        kind: "left".to_string(),
        amount: if unlimited { None } else { balance },
        limit: None,
        unit: "credits".to_string(),
        unlimited,
    }]
}

/// Claude's extra usage, from its OAuth usage answer.
pub fn claude(usage: &Value) -> Vec<ProviderUsageExtra> {
    let extra = &usage["extra_usage"];
    if extra["is_enabled"].as_bool() != Some(true) {
        return Vec::new();
    }
    let cents = |v: &Value| v.as_f64().filter(|n| n.is_finite());
    let used = cents(&extra["used_credits"]).unwrap_or(0.0).max(0.0) / 100.0;
    let cap = cents(&extra["monthly_limit"])
        .filter(|n| *n > 0.0)
        .map(|n| n / 100.0);
    vec![ProviderUsageExtra {
        id: "extra_usage".to_string(),
        kind: if cap.is_some() { "capped" } else { "spent" }.to_string(),
        amount: Some(used),
        limit: cap,
        unit: "usd".to_string(),
        unlimited: false,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_credits_show_only_where_the_account_has_them() {
        let none =
            json!({ "credits": { "hasCredits": false, "unlimited": false, "balance": "0" } });
        assert!(codex(&none).is_empty());
        let some =
            json!({ "credits": { "hasCredits": true, "unlimited": false, "balance": "1250" } });
        let extras = codex(&some);
        assert_eq!(
            (
                extras[0].kind.as_str(),
                extras[0].amount,
                extras[0].unlimited
            ),
            ("left", Some(1250.0), false)
        );
        let all = json!({ "credits": { "hasCredits": true, "unlimited": true, "balance": null } });
        assert!(codex(&all)[0].unlimited && codex(&all)[0].amount.is_none());
        assert!(codex(&json!({})).is_empty());
    }

    #[test]
    fn claude_extra_usage_is_in_dollars_capped_or_spent() {
        let off = json!({ "extra_usage": { "is_enabled": false, "used_credits": 900 } });
        assert!(claude(&off).is_empty());
        assert!(claude(&json!({})).is_empty());
        let capped = json!({ "extra_usage": { "is_enabled": true, "used_credits": 1250, "monthly_limit": 5000 } });
        let extra = &claude(&capped)[0];
        assert_eq!(
            (extra.kind.as_str(), extra.amount, extra.limit),
            ("capped", Some(12.5), Some(50.0))
        );
        let open = json!({ "extra_usage": { "is_enabled": true, "used_credits": 300, "monthly_limit": null } });
        assert_eq!(
            (claude(&open)[0].kind.as_str(), claude(&open)[0].amount),
            ("spent", Some(3.0))
        );
    }
}
