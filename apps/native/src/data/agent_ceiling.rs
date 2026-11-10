//! How many times a day triggers may wake each agent: `DAILY_RUNS` (5) by
//! default, or what the person picks on the agent's page — 10, 20 or no
//! ceiling. Routines, sources and the person's own messages never count;
//! the 2 a day for one pull request or task stay, against loops.

use chrono::{DateTime, Utc};

use super::agent_triggers::{AgentTriggers, DAILY_RUNS};

/// The choices on the agent's page; `None` is no ceiling.
pub const CEILING_CHOICES: [Option<usize>; 4] = [Some(DAILY_RUNS), Some(10), Some(20), None];

impl AgentTriggers {
    /// The agent's ceiling a day; `None`: no ceiling.
    pub fn daily_ceiling(&self, agent_id: &str) -> Option<usize> {
        match self.stored.ceilings.get(agent_id) {
            None => Some(DAILY_RUNS),
            Some(0) => None,
            Some(ceiling) => Some(*ceiling),
        }
    }

    pub fn set_daily_ceiling(&mut self, agent_id: &str, ceiling: Option<usize>) {
        match ceiling {
            Some(DAILY_RUNS) => {
                self.stored.ceilings.remove(agent_id);
            }
            Some(ceiling) => {
                self.stored.ceilings.insert(agent_id.to_string(), ceiling);
            }
            None => {
                self.stored.ceilings.insert(agent_id.to_string(), 0);
            }
        }
        self.save();
    }

    /// Triggers woke the agent as often as its ceiling allows today.
    pub fn at_ceiling(&self, agent_id: &str, now: DateTime<Utc>) -> bool {
        self.daily_ceiling(agent_id)
            .is_some_and(|ceiling| self.runs_today(agent_id, now) >= ceiling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agent_triggers::Trigger;

    #[test]
    fn the_ceiling_is_five_unless_the_person_picks_another() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent-triggers.json");
        let mut triggers = AgentTriggers::load(path.clone());
        let now = Utc::now();
        for index in 0..DAILY_RUNS {
            triggers.record("rev", Trigger::PrUpdated, &format!("pr/{index}"), now);
        }
        assert_eq!(triggers.daily_ceiling("rev"), Some(5));
        assert!(triggers.at_ceiling("rev", now));
        triggers.set_daily_ceiling("rev", Some(10));
        assert!(!triggers.at_ceiling("rev", now));
        triggers.set_daily_ceiling("rev", None);
        assert_eq!(triggers.daily_ceiling("rev"), None);
        assert!(!triggers.at_ceiling("rev", now));
        // Kept across a restart.
        assert_eq!(AgentTriggers::load(path).daily_ceiling("rev"), None);
    }
}
