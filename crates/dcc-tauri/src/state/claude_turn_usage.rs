//! Claude Code's end-of-turn usage is the session's running total
//! (`dcc_core::domain::claude_usage`); what a turn stores is its share, so
//! the room, the monthly limits, the bulletin and the usage screen count
//! each token once.

use dcc_core::domain::claude_usage::{add_to_turn, turn_share};
use dcc_core::domain::usage::ModelTokenUsage;

use super::*;

impl SessionCommandState {
    /// The turn's share of each model's report, remembering the reports.
    pub(super) async fn claude_turn_usage(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        models: &[ModelTokenUsage],
    ) -> Vec<ModelTokenUsage> {
        let stored: Vec<ModelTokenUsage> = self
            .session_turn_usage(session_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|turn| turn.turn_id == turn_id.0)
            .map(|turn| turn.models)
            .unwrap_or_default();
        models
            .iter()
            .map(|usage| {
                let Some(model) = usage
                    .model
                    .as_deref()
                    .filter(|model| !model.trim().is_empty())
                else {
                    return usage.clone();
                };
                let mark = self
                    .session_repo
                    .usage_mark(&session_id.0, model)
                    .ok()
                    .flatten();
                let share = turn_share(mark.as_ref(), usage);
                let before = stored
                    .iter()
                    .find(|kept| kept.model.as_deref() == Some(model));
                let share = add_to_turn(before, share);
                if let Err(error) = self
                    .session_repo
                    .set_usage_mark(&session_id.0, model, usage)
                {
                    eprintln!("[DCC] usage mark not kept: {error}");
                }
                share
            })
            .collect()
    }
}
