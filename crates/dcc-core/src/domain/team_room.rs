//! The team room (Sala): one conversation with the resident agents. Each
//! message goes to one agent, picked by rule, never by a model: an
//! `@mention` first; then a clear sign of the Reviewer's job in the
//! message (a pull request, CI, a task); then whoever answered last, while
//! the subject is fresh; else the Reviewer. The agent answers in its own conversation, with its
//! memory and tools; the room shows them together.

use crate::domain::agent::REVIEWER_PRESET;

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
        for mate in team {
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

/// Whether the message is about the Reviewer's job.
fn reviewer_job(text: &str) -> bool {
    let folded = fold(text);
    REVIEWER_WORDS.iter().any(|word| folded.contains(word))
        || folded
            .split(|c: char| !c.is_alphanumeric())
            .any(|token| REVIEWER_TOKENS.contains(&token))
}

/// Who answers `text`: `last` is the agent that answered last and how many
/// minutes ago.
pub fn route<'a>(
    text: &str,
    team: &[Teammate<'a>],
    last: Option<(&str, i64)>,
) -> Option<(Teammate<'a>, RouteReason)> {
    if let Some(mate) = mentioned(text, team) {
        return Some((mate, RouteReason::Mention));
    }
    let reviewer = team
        .iter()
        .find(|mate| mate.preset == Some(REVIEWER_PRESET));
    if let Some(mate) = reviewer.filter(|_| reviewer_job(text)) {
        return Some((*mate, RouteReason::Role));
    }
    let continues = last
        .filter(|(_, minutes)| *minutes <= SUBJECT_MINUTES)
        .and_then(|(id, _)| team.iter().find(|mate| mate.id == id));
    if let Some(mate) = continues {
        return Some((*mate, RouteReason::Continues));
    }
    reviewer
        .or(team.first())
        .map(|mate| (*mate, RouteReason::Default))
}

/// The mark that opens a message one agent passed to another.
pub const HANDOFF_MARK: &str = "↪ ";
/// How much of the first agent's answer goes along.
const HANDOFF_CONTEXT_CHARS: usize = 2_000;

/// What a turn in an agent's own conversation hears about its teammates:
/// who they are and how to suggest one takes over. The person decides.
pub fn handoff_block(teammates: &[&str], direct: &[&str]) -> Option<String> {
    if teammates.is_empty() {
        return None;
    }
    let who_decides = if direct.is_empty() {
        "The person decides whether to pass it on; never contact a teammate yourself.".to_string()
    } else {
        format!(
            "The person lets you pass to {} yourself: that line goes to them at once, with your answer as their brief — so end with it only when they should take over now. For anyone else the person decides; never contact a teammate any other way.",
            direct.join(", ")
        )
    };
    Some(format!(
        "You are part of the person's team in DCC's team room, with: {}. When one of them fits what the person needs better, end your answer with one line that starts with @ and their name and says what to ask them (\"@{} can you compare the two APIs?\"). {who_decides}",
        teammates.join(", "),
        teammates[0]
    ))
}

/// The teammate an agent's answer suggests passing to: its last line that
/// starts with `@` and names another agent, and that line.
pub fn handoff<'a>(
    answer: &str,
    team: &[Teammate<'a>],
    author: &str,
) -> Option<(Teammate<'a>, String)> {
    let others: Vec<Teammate<'a>> = team
        .iter()
        .copied()
        .filter(|mate| mate.id != author)
        .collect();
    answer.lines().rev().find_map(|line| {
        let line = line.trim().trim_start_matches(['-', '*', '>', ' ']).trim();
        if !line.starts_with('@') {
            return None;
        }
        let mate = mentioned(line, &others)?;
        // The name opens the line.
        let named = fold(line).starts_with(&format!("@{}", fold(mate.name.trim())));
        named.then(|| (mate, line.to_string()))
    })
}

/// The message the person passes to `to`: the request on the first line,
/// then what `from` said.
pub fn handoff_prompt(from: &str, to: &str, request: &str, answer: &str) -> String {
    let context: String = answer.chars().take(HANDOFF_CONTEXT_CHARS).collect();
    format!(
        "{HANDOFF_MARK}{from} → {to}: {request}\n\nWhat {from} said in the team room:\n{context}"
    )
}

/// The first line of a passed message (`↪ Revisor → Pesquisador: …`).
pub fn handoff_heading(text: &str) -> Option<&str> {
    text.starts_with(HANDOFF_MARK)
        .then(|| text.lines().next())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEAM: [Teammate<'static>; 3] = [
        Teammate {
            id: "rev",
            name: "Revisor",
            preset: Some(REVIEWER_PRESET),
        },
        Teammate {
            id: "res",
            name: "Pesquisa",
            preset: None,
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
    fn a_mention_wins_the_longest_name_first() {
        assert_eq!(
            who("@pesquisa olha a API", None),
            ("res", RouteReason::Mention)
        );
        assert_eq!(
            who("ei @QA mobile, roda isso", None),
            ("qa", RouteReason::Mention)
        );
        // A mention beats the Reviewer's job.
        assert_eq!(
            who("@Pesquisa revisa o PR", None),
            ("res", RouteReason::Mention)
        );
        // Not a name that only starts the same.
        assert_ne!(who("@Revisores", None).1, RouteReason::Mention);
    }

    #[test]
    fn the_reviewers_job_comes_next_then_the_subject_then_the_reviewer() {
        assert_eq!(
            who("o CI falhou no checkout", None),
            ("rev", RouteReason::Role)
        );
        assert_eq!(
            who("revisa o PR 1014", Some(("res", 2))),
            ("rev", RouteReason::Role)
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
        // Without a Reviewer, the first agent.
        assert_eq!(
            route("oi", &TEAM[1..], None).map(|(mate, reason)| (mate.id, reason)),
            Some(("res", RouteReason::Default))
        );
    }

    #[test]
    fn an_answer_can_suggest_a_teammate_and_the_person_passes_it() {
        let answer = "Revisei o PR, está ok.\n\n- @Pesquisa pode comparar as duas filas?";
        let (mate, line) = handoff(answer, &TEAM, "rev").unwrap();
        assert_eq!(mate.id, "res");
        assert_eq!(line, "@Pesquisa pode comparar as duas filas?");
        // Not to itself, not a name in the middle.
        assert!(handoff("@Revisor eu mesmo", &TEAM, "rev").is_none());
        assert!(handoff("como o @Pesquisa disse", &TEAM, "rev").is_none());
        let prompt = handoff_prompt("Revisor", "Pesquisa", &line, answer);
        assert_eq!(
            handoff_heading(&prompt),
            Some("↪ Revisor → Pesquisa: @Pesquisa pode comparar as duas filas?")
        );
        assert!(prompt.contains("Revisei o PR"));
        assert_eq!(handoff_heading("oi"), None);
        let block = handoff_block(&["Pesquisa", "QA"], &[]).unwrap();
        assert!(handoff_block(&["Dev"], &["Dev"])
            .unwrap()
            .contains("pass to Dev yourself"));
        assert!(block.contains("Pesquisa, QA") && block.contains("never contact"));
        assert!(handoff_block(&[], &[]).is_none());
    }
}
