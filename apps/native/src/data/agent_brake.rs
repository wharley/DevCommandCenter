//! The brake on runs without the person (Compozy's unattended-turn brake):
//! after `UNATTENDED_LIMIT` turns an agent started on its own — triggers and
//! routines — since the person last talked to it, nothing more wakes it
//! until they do, or press "Retomar" on its card in the team room. Kept
//! with the triggers in `agent-triggers.json`.

use chrono::{DateTime, Utc};

use super::agent_triggers::{AgentTriggers, Held};

pub const UNATTENDED_LIMIT: usize = 30;

impl AgentTriggers {
    /// The person talked to the agent (the room, a passed turn, "Retomar").
    pub fn attend(&mut self, agent_id: &str, now: DateTime<Utc>) {
        self.stored
            .attended
            .insert(agent_id.to_string(), now.to_rfc3339());
        self.save();
    }

    /// The agent's runs since the person last talked to it.
    pub fn unattended(&self, agent_id: &str) -> usize {
        let since = self.stored.attended.get(agent_id);
        self.runs()
            .iter()
            .filter(|run| run.agent_id == agent_id)
            .filter(|run| since.is_none_or(|since| run.at.as_str() > since.as_str()))
            .count()
    }

    pub fn brake_check(&self, agent_id: &str) -> Result<(), Held> {
        if self.unattended(agent_id) >= UNATTENDED_LIMIT {
            Err(Held::Unattended)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agent_triggers::Trigger;

    #[test]
    fn runs_without_the_person_stop_at_the_brake_until_they_talk() {
        let dir = tempfile::tempdir().unwrap();
        let mut triggers = AgentTriggers::load(dir.path().join("agent-triggers.json"));
        let now = Utc::now();
        for index in 0..UNATTENDED_LIMIT {
            triggers.record("qa", Trigger::Routine, &format!("r{index}"), now);
        }
        assert_eq!(triggers.brake_check("qa"), Err(Held::Unattended));
        // Routines do not eat the triggers' daily ceiling.
        assert_eq!(triggers.runs_today("qa", now), 0);
        triggers.attend("qa", now + chrono::Duration::seconds(1));
        assert_eq!(triggers.unattended("qa"), 0);
        assert_eq!(triggers.brake_check("qa"), Ok(()));
    }
}
