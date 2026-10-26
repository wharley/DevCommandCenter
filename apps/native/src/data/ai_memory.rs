//! What the header reads of ai-memory (`ai-memory-sources-menu.tsx`): the
//! sources the agent received on the session's latest turn and the
//! person's curation of them (pinned first, ignored apart, a correction in
//! their place), their text without the search markup, their kind and
//! when; whether the session still waits to be exported.

use std::collections::HashMap;

use chrono::{Datelike, Local, Timelike};
use serde::Deserialize;

/// `AiMemoryQueryHit`
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceHit {
    pub path: Option<String>,
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub created_at: Option<String>,
    /// `user-prompt` | `notification` | …
    pub kind: Option<String>,
}

impl SourceHit {
    /// `aiMemorySourceKey`: the identity the backend curates by.
    pub fn key(&self) -> String {
        dcc_infra::ai_memory::AiMemoryHit {
            path: self.path.clone(),
            title: self.title.clone(),
            snippet: self.snippet.clone(),
            rank: None,
            created_at: None,
            session_id: None,
            kind: None,
        }
        .source_key()
    }

    /// `sourceKind`
    pub fn kind(&self) -> SourceKind {
        if self.path.is_some() {
            SourceKind::Page
        } else {
            match self.kind.as_deref() {
                Some("user-prompt") => SourceKind::Prompt,
                Some("notification") => SourceKind::Answer,
                _ => SourceKind::Observation,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Prompt,
    Answer,
    Page,
    Observation,
}

impl SourceKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Answer => "answer",
            Self::Page => "page",
            Self::Observation => "observation",
        }
    }
}

/// `AiMemorySourceActionOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceAction {
    pub source_key: String,
    /// `pinned` | `ignored` | `corrected`
    pub action: String,
    pub correction: Option<String>,
}

/// `AiMemoryOutboxStatusOutput`: the export still waits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportStatus {
    pub attempts: u32,
    pub last_error: Option<String>,
}

/// The sources as the menu lists them: not ignored, the pinned first
/// (as in the context the agent receives); the ignored apart.
pub fn partition<'a>(
    sources: &'a [SourceHit],
    actions: &HashMap<String, SourceAction>,
) -> (Vec<&'a SourceHit>, Vec<&'a SourceHit>) {
    let action = |source: &SourceHit| {
        actions
            .get(&source.key())
            .map(|action| action.action.as_str())
    };
    let (mut visible, ignored): (Vec<&SourceHit>, Vec<&SourceHit>) = sources
        .iter()
        .partition(|source| action(source) != Some("ignored"));
    // Stable: the pinned lead, the rest keep their order.
    visible.sort_by_key(|source| action(source) != Some("pinned"));
    (visible, ignored)
}

/// The search over a source's title, path and excerpt, as plain text.
pub fn matches(source: &SourceHit, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || [&source.title, &source.path, &source.snippet]
            .into_iter()
            .flatten()
            .any(|value| clean_text(value).to_lowercase().contains(&query))
}

/// `cleanAiMemoryText`: without ai-memory's highlight and other markup,
/// the entities decoded, the spaces collapsed.
pub fn clean_text(value: &str) -> String {
    let mut plain = String::with_capacity(value.len());
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in value.chars() {
        match (in_tag, ch) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                // `<mark>` goes without a trace; any other tag is a space.
                let name = tag.trim_start_matches('/').to_ascii_lowercase();
                if !(name == "mark" || name.starts_with("mark ")) {
                    plain.push(' ');
                }
            }
            (true, ch) => tag.push(ch),
            (false, ch) => plain.push(ch),
        }
    }
    let decoded = decode_entities(&plain);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `formatAiMemorySourceTimestamp`: "8 de out., 15:22" (the year only when
/// it is not this one), or the value as it came.
pub fn timestamp(value: &str, english: bool) -> String {
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(value) else {
        return value.to_string();
    };
    let at = at.with_timezone(&Local);
    let this_year = at.year() == Local::now().year();
    const PT: [&str; 12] = [
        "jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.",
        "dez.",
    ];
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = at.month0() as usize;
    if english {
        let (pm, hour) = at.hour12();
        let clock = format!(
            "{hour:02}:{:02} {}",
            at.minute(),
            if pm { "PM" } else { "AM" }
        );
        if this_year {
            format!("{} {}, {clock}", EN[month], at.day())
        } else {
            format!("{} {}, {}, {clock}", EN[month], at.day(), at.year())
        }
    } else {
        let clock = format!("{:02}:{:02}", at.hour(), at.minute());
        if this_year {
            format!("{} de {}, {clock}", at.day(), PT[month])
        } else {
            format!("{} de {} de {}, {clock}", at.day(), PT[month], at.year())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(path: Option<&str>, title: &str, snippet: &str, kind: Option<&str>) -> SourceHit {
        SourceHit {
            path: path.map(str::to_string),
            title: Some(title.into()),
            snippet: Some(snippet.into()),
            created_at: None,
            kind: kind.map(str::to_string),
        }
    }

    #[test]
    fn the_key_and_kind_match_the_desktop() {
        let page = hit(Some("docs/a.md"), "A", "um", None);
        assert_eq!(page.key(), "docs/a.md|A|um");
        assert_eq!(page.kind(), SourceKind::Page);
        assert_eq!(
            hit(None, "p", "s", Some("user-prompt")).kind(),
            SourceKind::Prompt
        );
        assert_eq!(
            hit(None, "n", "s", Some("notification")).kind(),
            SourceKind::Answer
        );
        assert_eq!(hit(None, "o", "s", None).kind(), SourceKind::Observation);
        let long = "x".repeat(200);
        assert_eq!(
            hit(None, "t", &long, None).key(),
            format!("|t|{}", "x".repeat(160))
        );
    }

    #[test]
    fn pinned_lead_and_ignored_go_apart() {
        let sources = [
            hit(None, "a", "1", None),
            hit(None, "b", "2", None),
            hit(None, "c", "3", None),
        ];
        let actions: HashMap<String, SourceAction> = [("|b|2", "pinned"), ("|c|3", "ignored")]
            .into_iter()
            .map(|(key, action)| {
                (
                    key.to_string(),
                    SourceAction {
                        source_key: key.into(),
                        action: action.into(),
                        correction: None,
                    },
                )
            })
            .collect();
        let (visible, ignored) = partition(&sources, &actions);
        let titles: Vec<&str> = visible
            .iter()
            .map(|source| source.title.as_deref().unwrap())
            .collect();
        assert_eq!(titles, ["b", "a"]);
        assert_eq!(ignored.len(), 1);
        assert!(matches(&sources[0], "  "));
        assert!(!matches(&sources[0], "zz"));
    }

    #[test]
    fn markup_and_entities_go_away() {
        assert_eq!(
            clean_text("o <mark>checkout</mark> com <b>retry</b> &amp; &#39;x&#39;\n  fim"),
            "o checkout com retry & 'x' fim"
        );
        assert_eq!(clean_text("a &unknown; b"), "a &unknown; b");
    }

    #[test]
    fn the_timestamp_reads_like_intl() {
        let year = Local::now().year();
        let at = Local
            .with_ymd_and_hms(year, 10, 8, 15, 22, 0)
            .unwrap()
            .to_rfc3339();
        assert_eq!(timestamp(&at, false), "8 de out., 15:22");
        assert_eq!(timestamp(&at, true), "Oct 8, 03:22 PM");
        let old = Local
            .with_ymd_and_hms(2020, 1, 2, 9, 5, 0)
            .unwrap()
            .to_rfc3339();
        assert_eq!(timestamp(&old, false), "2 de jan. de 2020, 09:05");
        assert_eq!(timestamp("ontem", false), "ontem");
    }

    use chrono::TimeZone;
}
