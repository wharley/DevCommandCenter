//! Leftover of the removed durable task objective.
//!
//! The DCC once kept its own per-session objective (intent, done criterion,
//! failure and turn budgets) and paused it automatically. That feature was
//! replaced by the providers' native `/goal` (see `goal.rs`). Sessions recorded
//! before the removal may still hold `objective_paused` events, so the reason
//! enum stays, with the same serde names, only to read them back.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Why an old `objective_paused` event paused the objective.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ObjectivePauseReason {
    Manual,
    ConsecutiveFailures,
    TurnBudget,
}
