//! "Diagnóstico da interface" (`frontend-diagnostics.ts`): the last 10
//! failures of the native interface (Rust panics, the React errors' place)
//! kept on this device, scrubbed of credentials, and exported as the same
//! `{ schemaVersion: 1, errors }` JSON. Nothing leaves without the person.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// `MAX_RECORDS`
const MAX_RECORDS: usize = 10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorRecord {
    pub id: String,
    pub timestamp: String,
    pub kind: String,
    pub scope: String,
    pub message: String,
    pub app_version: String,
}

fn path() -> PathBuf {
    crate::prefs::native_file("ui-errors.json")
}

/// `scrub`: credentials in URLs, bearer tokens and secret query values.
pub fn scrub(text: &str, limit: usize) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, word) in text.split(' ').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let previous_is_bearer = out.trim_end().to_ascii_lowercase().ends_with("bearer");
        if previous_is_bearer && index > 0 {
            out.push_str("[redacted]");
            continue;
        }
        let mut word = word.to_string();
        // scheme://user:pass@host
        if let Some(scheme) = word.find("://") {
            let rest = &word[scheme + 3..];
            if let Some(at) = rest.find('@')
                && rest[..at].contains(':')
                && !rest[..at].contains('/')
            {
                word = format!("{}[redacted]@{}", &word[..scheme + 3], &rest[at + 1..]);
            }
        }
        for key in [
            "token=",
            "access_token=",
            "api_key=",
            "key=",
            "password=",
            "secret=",
        ] {
            for separator in ['?', '&'] {
                let marker = format!("{separator}{key}");
                if let Some(start) = word.find(&marker) {
                    let value = start + marker.len();
                    let end = word[value..]
                        .find(['&', '#'])
                        .map_or(word.len(), |offset| value + offset);
                    word.replace_range(value..end, "[redacted]");
                }
            }
        }
        out.push_str(&word);
    }
    out.chars().take(limit).collect()
}

/// `readFrontendErrors`
pub fn read() -> Vec<ErrorRecord> {
    std::fs::read(path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn record(message: &str, scope: &str) {
    let mut records = read();
    records.push(ErrorRecord {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        kind: "panic".into(),
        scope: scrub(scope, 300),
        message: scrub(message, 2000),
        app_version: crate::data::feedback::version(),
    });
    let keep = records.len().saturating_sub(MAX_RECORDS);
    records.drain(..keep);
    if let Ok(json) = serde_json::to_vec(&records) {
        let _ = std::fs::write(path(), json);
    }
}

/// `installFrontendErrorListeners`: every panic is kept, then handled as
/// before.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "Unknown panic".into());
        let scope = info
            .location()
            .map(|at| format!("{}:{}:{}", at.file(), at.line(), at.column()))
            .unwrap_or_else(|| "native".into());
        record(&message, &scope);
        previous(info);
    }));
}

/// `exportFrontendErrors`
pub fn export() -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "schemaVersion": 1,
        "errors": read(),
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_scrubbed() {
        assert_eq!(
            scrub("open https://ana:s3cret@host/x failed", 200),
            "open https://[redacted]@host/x failed"
        );
        assert_eq!(
            scrub("Authorization: Bearer abc.def", 200),
            "Authorization: Bearer [redacted]"
        );
        assert_eq!(
            scrub("GET /a?token=xyz&b=1", 200),
            "GET /a?token=[redacted]&b=1"
        );
        assert_eq!(scrub("abcdef", 3), "abc");
    }
}
