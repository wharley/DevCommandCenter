//! The agents' check-in, with no model: a pull request asking for the
//! person's review, an issue assigned to them, a task whose turn is open
//! with no sign of life for `STALLED_MINUTES`. The desktop finds them and
//! keeps the cards in its own file (`check-ins.json`); the team room shows
//! each in its project's room, on the desktop and on the phone.

use serde::{Deserialize, Serialize};

/// A turn open this long with no event is stalled.
pub const STALLED_MINUTES: i64 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckInKind {
    /// A pull request asks for the person's review.
    ReviewRequested,
    /// An issue was assigned to the person.
    IssueAssigned,
    /// A task's turn is open with no sign of life.
    TaskStalled,
}

impl CheckInKind {
    /// `agents.room.checkIn.<id>`
    pub fn id(self) -> &'static str {
        match self {
            Self::ReviewRequested => "reviewRequested",
            Self::IssueAssigned => "issueAssigned",
            Self::TaskStalled => "taskStalled",
        }
    }
}

/// One thing the check-in found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckIn {
    /// What makes it the same thing next time.
    pub key: String,
    pub kind: CheckInKind,
    pub title: String,
    /// `acme/app #12`, or the stalled task's project.
    pub place: String,
    pub url: Option<String>,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub project_id: Option<String>,
    /// When it was found.
    pub at: String,
    pub unread: bool,
}

/// The cards of the desktop's `check-ins.json`; none when it is unreadable.
pub fn stored_cards(raw: &str) -> Vec<CheckIn> {
    #[derive(Deserialize)]
    struct Stored {
        #[serde(default)]
        cards: Vec<CheckIn>,
    }
    serde_json::from_str::<Stored>(raw)
        .map(|stored| stored.cards)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktops_file_gives_its_cards() {
        let raw = r#"{"started":["taskStalled"],"seen":{"k":"2026-11-10T10:00:00Z"},"cards":[
            {"key":"k","kind":"taskStalled","title":"Checkout","place":"app","url":null,
             "workspaceId":"w-1","sessionId":"s-1","projectId":"p-1",
             "at":"2026-11-10T10:00:00Z","unread":true}]}"#;
        let cards = stored_cards(raw);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].kind, CheckInKind::TaskStalled);
        assert_eq!(cards[0].project_id.as_deref(), Some("p-1"));
        assert!(stored_cards("not json").is_empty());
        assert!(stored_cards("{}").is_empty());
    }
}
