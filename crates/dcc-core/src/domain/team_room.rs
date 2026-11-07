//! The team room (Sala): one conversation with the resident agents. Each
//! message goes to one agent, picked by rule, never by a model: an
//! `@mention` first; then a clear sign of a role in the message (a pull
//! request, CI or a task for the Reviewer; an idea or research for the
//! Researcher); then whoever answered last, while the subject is fresh;
//! else the Reviewer. The agent answers in its own conversation, with its
//! memory and tools; the room shows them together.

use crate::domain::agent::{CHRONICLER_PRESET, RESEARCHER_PRESET, REVIEWER_PRESET};

/// How long the last agent keeps the subject.
pub const SUBJECT_MINUTES: i64 = 30;

/// An agent in the room.
#[derive(Clone, Copy, Debug)]
pub struct Teammate<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub preset: Option<&'a str>,
}

/// Why the room called that agent, shown under the person's message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteReason {
    Mention,
    Role,
    Continues,
    Default,
}

impl RouteReason {
    /// `agents.room.reason.*`
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Mention => "agents.room.reason.mention",
            Self::Role => "agents.room.reason.role",
            Self::Continues => "agents.room.reason.continues",
            Self::Default => "agents.room.reason.default",
        }
    }
}

/// Whether the agent takes part in the room (the Reporter runs no model).
pub fn talks(preset: Option<&str>) -> bool {
    preset != Some(CHRONICLER_PRESET)
}

const REVIEWER_WORDS: &[&str] = &[
    "pull request",
    "review",
    "revis",
    "diff",
    "commit",
    "merge",
    "check",
    "tarefa",
    "task",
    "bug",
    "teste",
    "falhou",
    "failing",
];
/// Short words that only count on their own.
const REVIEWER_TOKENS: &[&str] = &["pr", "prs", "ci"];
const RESEARCHER_WORDS: &[&str] = &[
    "ideia",
    "idea",
    "pesquis",
    "research",
    "investig",
    "compar",
    "biblioteca",
    "library",
    "alternativ",
];

fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// The agent the message names with `@`, the longest name winning
/// (`@Revisor`, `@revisor`, `@Pesquisador`).
pub fn mentioned<'a>(text: &str, team: &[Teammate<'a>]) -> Option<Teammate<'a>> {
    let folded = fold(text);
    let mut best: Option<Teammate<'a>> = None;
    for (at, _) in folded.match_indices('@') {
        let rest = &folded[at + 1..];
        for mate in team.iter().filter(|mate| talks(mate.preset)) {
            let name = fold(mate.name.trim());
            if name.is_empty() || !rest.starts_with(&name) {
                continue;
            }
            // The name ends there (`@Revisor,` but not `@Revisores`).
            let ends = rest[name.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric());
            if ends && best.is_none_or(|best| fold(best.name).len() < name.len()) {
                best = Some(*mate);
            }
        }
    }
    best
}

fn role_preset(text: &str) -> Option<&'static str> {
    let folded = fold(text);
    let tokens: Vec<&str> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    let research = RESEARCHER_WORDS.iter().any(|word| folded.contains(word));
    let review = REVIEWER_WORDS.iter().any(|word| folded.contains(word))
        || tokens.iter().any(|token| REVIEWER_TOKENS.contains(token));
    match (review, research) {
        (true, false) => Some(REVIEWER_PRESET),
        (false, true) => Some(RESEARCHER_PRESET),
        // Both or neither: no clear sign.
        _ => None,
    }
}

/// Who answers `text`: `last` is the agent that answered last and how many
/// minutes ago.
pub fn route<'a>(
    text: &str,
    team: &[Teammate<'a>],
    last: Option<(&str, i64)>,
) -> Option<(Teammate<'a>, RouteReason)> {
    let talking: Vec<Teammate<'a>> = team
        .iter()
        .copied()
        .filter(|mate| talks(mate.preset))
        .collect();
    if let Some(mate) = mentioned(text, &talking) {
        return Some((mate, RouteReason::Mention));
    }
    let by_role = role_preset(text)
        .and_then(|preset| talking.iter().find(|mate| mate.preset == Some(preset)));
    if let Some(mate) = by_role {
        return Some((*mate, RouteReason::Role));
    }
    let continues = last
        .filter(|(_, minutes)| *minutes <= SUBJECT_MINUTES)
        .and_then(|(id, _)| talking.iter().find(|mate| mate.id == id));
    if let Some(mate) = continues {
        return Some((*mate, RouteReason::Continues));
    }
    talking
        .iter()
        .find(|mate| mate.preset == Some(REVIEWER_PRESET))
        .or(talking.first())
        .map(|mate| (*mate, RouteReason::Default))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEAM: [Teammate<'static>; 4] = [
        Teammate {
            id: "rev",
            name: "Revisor",
            preset: Some(REVIEWER_PRESET),
        },
        Teammate {
            id: "res",
            name: "Pesquisador",
            preset: Some(RESEARCHER_PRESET),
        },
        Teammate {
            id: "rep",
            name: "Repórter",
            preset: Some(CHRONICLER_PRESET),
        },
        Teammate {
            id: "qa",
            name: "QA Mobile",
            preset: None,
        },
    ];

    fn who(text: &str, last: Option<(&str, i64)>) -> (&'static str, RouteReason) {
        let (mate, reason) = route(text, &TEAM, last).unwrap();
        (mate.id, reason)
    }

    #[test]
    fn a_mention_wins_and_the_reporter_never_answers() {
        assert_eq!(
            who("@pesquisador olha a API", None),
            ("res", RouteReason::Mention)
        );
        assert_eq!(
            who("ei @QA mobile, roda isso", None),
            ("qa", RouteReason::Mention)
        );
        // A mention beats a role sign.
        assert_eq!(
            who("@Pesquisador revisa o PR", None),
            ("res", RouteReason::Mention)
        );
        // Not a name that only starts the same.
        assert_ne!(who("@Revisores", None).1, RouteReason::Mention);
        assert_eq!(
            who("@Repórter como foi o dia", None),
            ("rev", RouteReason::Default)
        );
    }

    #[test]
    fn a_role_sign_comes_next_then_the_subject_then_the_reviewer() {
        assert_eq!(
            who("o CI falhou no checkout", None),
            ("rev", RouteReason::Role)
        );
        assert_eq!(
            who("revisa o PR 1014", Some(("res", 2))),
            ("rev", RouteReason::Role)
        );
        assert_eq!(
            who("pesquisa uma lib de filas", Some(("rev", 2))),
            ("res", RouteReason::Role)
        );
        // No sign: whoever answered last, while the subject is fresh.
        assert_eq!(
            who("e o outro?", Some(("res", 5))),
            ("res", RouteReason::Continues)
        );
        assert_eq!(
            who("e o outro?", Some(("res", 90))),
            ("rev", RouteReason::Default)
        );
        // "price" has no "pr" on its own.
        assert_eq!(who("qual o price?", None), ("rev", RouteReason::Default));
        // Both signs: no clear one.
        assert_eq!(
            who("pesquisa e revisa", Some(("qa", 1))),
            ("qa", RouteReason::Continues)
        );
    }
}
