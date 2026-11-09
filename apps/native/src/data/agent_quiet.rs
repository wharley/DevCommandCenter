//! When no trigger wakes an agent, whatever its ceilings say: "Pausar
//! todos" in the team room, and the quiet hours the person picked (Tucano
//! and Compozy both have them), in the person's local time. Kept with the
//! triggers in `agent-triggers.json`.

use chrono::{DateTime, Local, Timelike, Utc};
use serde::{Deserialize, Serialize};

use super::agent_triggers::{AgentTriggers, Held};

/// From `from` to `to` o'clock, local; `from > to` crosses midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietHours {
    pub from: u32,
    pub to: u32,
}

/// The quiet hours the room offers.
pub const QUIET_CHOICES: [QuietHours; 3] = [
    QuietHours { from: 22, to: 8 },
    QuietHours { from: 20, to: 8 },
    QuietHours { from: 23, to: 7 },
];

impl QuietHours {
    pub fn contains(self, hour: u32) -> bool {
        if self.from <= self.to {
            (self.from..self.to).contains(&hour)
        } else {
            hour >= self.from || hour < self.to
        }
    }
}

impl AgentTriggers {
    pub fn paused(&self) -> bool {
        self.stored.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        if self.stored.paused != paused {
            self.stored.paused = paused;
            self.save();
        }
    }

    pub fn quiet(&self) -> Option<QuietHours> {
        self.stored.quiet
    }

    pub fn set_quiet(&mut self, quiet: Option<QuietHours>) {
        if self.stored.quiet != quiet {
            self.stored.quiet = quiet;
            self.save();
        }
    }

    /// Whether the pause or the quiet hours hold every trigger now.
    pub fn quiet_check(&self, now: DateTime<Utc>) -> Result<(), Held> {
        if self.stored.paused {
            return Err(Held::Paused);
        }
        let hour = now.with_timezone(&Local).hour();
        if self.stored.quiet.is_some_and(|quiet| quiet.contains(hour)) {
            return Err(Held::Quiet);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agent_triggers::Trigger;

    #[test]
    fn the_pause_and_the_quiet_hours_hold_every_trigger() {
        let night = QuietHours { from: 22, to: 8 };
        assert!(
            night.contains(23) && night.contains(3) && !night.contains(8) && !night.contains(15)
        );
        let day = QuietHours { from: 9, to: 12 };
        assert!(day.contains(9) && !day.contains(12));

        let dir = tempfile::tempdir().unwrap();
        let mut triggers = AgentTriggers::load(dir.path().join("agent-triggers.json"));
        triggers.set("rev", Trigger::CiFailed, true);
        let now = Utc::now();
        assert_eq!(triggers.check("rev", Trigger::CiFailed, "t", now), Ok(()));
        triggers.set_paused(true);
        assert_eq!(
            triggers.check("rev", Trigger::CiFailed, "t", now),
            Err(Held::Paused)
        );
        triggers.set_paused(false);
        let hour = now.with_timezone(&Local).hour();
        triggers.set_quiet(Some(QuietHours {
            from: hour,
            to: (hour + 1) % 24,
        }));
        assert_eq!(
            triggers.check("rev", Trigger::CiFailed, "t", now),
            Err(Held::Quiet)
        );
        let again = AgentTriggers::load(dir.path().join("agent-triggers.json"));
        assert!(again.quiet().is_some() && !again.paused());
    }
}
