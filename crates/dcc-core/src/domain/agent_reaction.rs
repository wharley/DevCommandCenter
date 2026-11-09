//! 👍 / 👎 on an agent's answer in the team room (Monocode): kept per answer,
//! counted in the weekly bulletin as useful or not, and a 👎 offers to keep
//! a rule the person writes in the agent's memory. One reaction per answer;
//! reacting again changes it, and taking it back removes it.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Reaction {
    Up,
    Down,
}

impl Reaction {
    pub fn key(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            _ => None,
        }
    }
}

/// The longest answer key kept (`<session>:<turn>`).
pub const MAX_LINE_KEY_CHARS: usize = 256;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reaction_reads_back_from_its_key() {
        for reaction in [Reaction::Up, Reaction::Down] {
            assert_eq!(Reaction::from_key(reaction.key()), Some(reaction));
        }
        assert_eq!(Reaction::from_key("meh"), None);
    }
}
