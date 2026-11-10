//! The state line under an agent's name in the team's column: what it is
//! doing, picked by priority from plain facts (no reads here, so the column
//! can ask every frame). Compozy shows the "needs you" reason the same way,
//! in place of the row's usual second line.

use crate::agent::Mood;

/// What the column knows about one agent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentFacts<'a> {
    pub mood: Option<Mood>,
    /// A task it works in or opened that runs: its name and start.
    pub task: Option<(&'a str, Option<&'a str>)>,
    /// A pull request review it runs, as `repository#number`.
    pub review: Option<&'a str>,
    /// Its own conversation is mid-turn.
    pub replying: bool,
    pub at_ceiling: bool,
    /// Enabled sources.
    pub sources: usize,
    pub triggers_on: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AgentLine<'a> {
    NeedsYou,
    Task {
        name: &'a str,
        started_at: Option<&'a str>,
    },
    Reviewing(&'a str),
    Replying,
    AtCeiling,
    Sources(usize),
    Triggers,
    NewResult,
    Idle,
}

pub fn line<'a>(facts: &AgentFacts<'a>) -> AgentLine<'a> {
    if facts.mood == Some(Mood::NeedsYou) {
        return AgentLine::NeedsYou;
    }
    if let Some((name, started_at)) = facts.task {
        return AgentLine::Task { name, started_at };
    }
    if let Some(label) = facts.review {
        return AgentLine::Reviewing(label);
    }
    if facts.replying {
        return AgentLine::Replying;
    }
    if facts.at_ceiling {
        return AgentLine::AtCeiling;
    }
    if facts.sources > 0 {
        return AgentLine::Sources(facts.sources);
    }
    if facts.triggers_on {
        return AgentLine::Triggers;
    }
    if facts.mood == Some(Mood::Done) {
        return AgentLine::NewResult;
    }
    AgentLine::Idle
}

#[cfg(test)]
mod tests {
    use super::*;

    fn busy() -> AgentFacts<'static> {
        AgentFacts {
            mood: Some(Mood::Working),
            task: Some(("Corrigir domínio", Some("2026-11-10T10:00:00Z"))),
            review: Some("app#9"),
            replying: true,
            at_ceiling: true,
            sources: 2,
            triggers_on: true,
        }
    }

    #[test]
    fn each_priority_beats_the_ones_below_it() {
        let mut facts = AgentFacts {
            mood: Some(Mood::NeedsYou),
            ..busy()
        };
        assert_eq!(line(&facts), AgentLine::NeedsYou);
        facts.mood = Some(Mood::Working);
        assert!(matches!(line(&facts), AgentLine::Task { name, .. } if name == "Corrigir domínio"));
        facts.task = None;
        assert_eq!(line(&facts), AgentLine::Reviewing("app#9"));
        facts.review = None;
        assert_eq!(line(&facts), AgentLine::Replying);
        facts.replying = false;
        assert_eq!(line(&facts), AgentLine::AtCeiling);
        facts.at_ceiling = false;
        assert_eq!(line(&facts), AgentLine::Sources(2));
        facts.sources = 0;
        assert_eq!(line(&facts), AgentLine::Triggers);
        facts.triggers_on = false;
        facts.mood = Some(Mood::Done);
        assert_eq!(line(&facts), AgentLine::NewResult);
        facts.mood = Some(Mood::Asleep);
        assert_eq!(line(&facts), AgentLine::Idle);
        assert_eq!(line(&AgentFacts::default()), AgentLine::Idle);
    }
}
