//! "Buscar na conversa" (`thread-find.logic.ts`): the rows that hold the
//! text, in timeline order, case-insensitive, at least two characters, at
//! most 500; each with a 96-character excerpt around the first hit. Rows
//! still streaming are skipped.

use dcc_core::domain::timeline::TimelineMessage;

pub const MIN_QUERY_CHARS: usize = 2;
pub const SNIPPET_CHARS: usize = 96;
pub const MAX_MATCHES: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMatch {
    pub message_id: String,
    pub snippet: String,
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(ch);
            space = false;
        }
    }
    out
}

/// `findInThread`
pub fn find_in_thread(rows: &[TimelineMessage], query: &str) -> Vec<FindMatch> {
    let needle: Vec<char> = query.trim().to_lowercase().chars().collect();
    if needle.len() < MIN_QUERY_CHARS {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for row in rows.iter().filter(|row| !row.streaming) {
        let content: Vec<char> = row.content.chars().collect();
        let haystack: Vec<char> = content
            .iter()
            .map(|ch| ch.to_lowercase().next().unwrap_or(*ch))
            .collect();
        let Some(at) = haystack
            .windows(needle.len())
            .position(|window| window == needle.as_slice())
        else {
            continue;
        };
        let start = at.saturating_sub(SNIPPET_CHARS.saturating_sub(needle.len()) / 2);
        let end = (start + SNIPPET_CHARS).min(content.len());
        let excerpt: String = content[start..end].iter().collect();
        let snippet = format!(
            "{}{}{}",
            if start > 0 { "…" } else { "" },
            collapse_whitespace(&excerpt),
            if end < content.len() { "…" } else { "" }
        );
        matches.push(FindMatch {
            message_id: row.id.clone(),
            snippet,
        });
        if matches.len() >= MAX_MATCHES {
            break;
        }
    }
    matches
}

/// `stepThreadFindIndex`: wraps both ways; 0 for an empty list.
pub fn step(current: usize, total: usize, forward: bool) -> usize {
    if total == 0 {
        return 0;
    }
    if forward {
        (current + 1) % total
    } else {
        (current + total - 1) % total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::timeline::Role;

    fn message(id: &str, role: Role, content: &str, streaming: bool) -> TimelineMessage {
        TimelineMessage {
            id: id.into(),
            role,
            turn_id: None,
            turn_settled: true,
            turn_started_at: None,
            turn_ended_at: None,
            assistant_phase: None,
            content: content.into(),
            label: String::new(),
            model: None,
            streaming,
            created_at: None,
            status: None,
            annotations: Vec::new(),
            plan_mode: false,
            answers_plan_mode: false,
            evidence: None,
            retry_of_turn_id: None,
            action: None,
            delegation: None,
            delegation_turn_id: None,
            hand_back: None,
        }
    }

    fn thread() -> Vec<TimelineMessage> {
        vec![
            message("u1", Role::User, "Add retries to the Checkout flow", false),
            message(
                "a1",
                Role::Assistant,
                "I added a retry helper in checkout.ts and wired it.",
                false,
            ),
            message("u2", Role::User, "Now the timeout case", false),
            message("a2", Role::Assistant, "checkout retry streaming…", true),
        ]
    }

    #[test]
    fn matches_in_order_ignoring_case_and_streaming_rows() {
        let matches = find_in_thread(&thread(), "checkout");
        let ids: Vec<&str> = matches.iter().map(|m| m.message_id.as_str()).collect();
        assert_eq!(ids, ["u1", "a1"]);
        assert!(matches[1].snippet.contains("checkout.ts"));
    }

    #[test]
    fn short_or_blank_queries_find_nothing() {
        assert!(find_in_thread(&thread(), "c").is_empty());
        assert!(find_in_thread(&thread(), "   ").is_empty());
        assert!(find_in_thread(&thread(), "nothing-here").is_empty());
    }

    #[test]
    fn snippets_are_bounded_around_the_first_hit() {
        let long = format!("{} needle {}", "x".repeat(300), "y".repeat(300));
        let matches = find_in_thread(&[message("m", Role::Assistant, &long, false)], "needle");
        let snippet = &matches[0].snippet;
        assert!(snippet.chars().count() <= 100);
        assert!(snippet.starts_with('…') && snippet.ends_with('…'));
        assert!(snippet.contains("needle"));
    }

    #[test]
    fn stepping_wraps_both_ways() {
        assert_eq!(step(0, 3, true), 1);
        assert_eq!(step(2, 3, true), 0);
        assert_eq!(step(0, 3, false), 2);
        assert_eq!(step(5, 0, true), 0);
    }
}
