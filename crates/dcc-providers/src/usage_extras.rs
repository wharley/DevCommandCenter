//! What a plan has beyond its windows, as each maker says it: Codex's
//! credits (`rateLimits.credits`: `hasCredits`, `unlimited`, `balance`),
//! shown only where the account has them.

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
}
