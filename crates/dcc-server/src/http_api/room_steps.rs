//! An answer's steps, listed when the person opens its folded line in the
//! phone's room ("Trabalhou 4 min · leu 6 arquivos"): each tool call of that
//! turn with its kind, what it acted on and whether it failed
//! (`dcc_core::domain::team_room_steps`), read from the conversation the
//! room already keeps projected.

use dcc_core::domain::team_room_steps::{steps_of, Step};
use dcc_infra::db::SqliteSessionRepo;

use super::room_feed::messages;
use super::*;

/// The most steps one answer lists.
const STEPS_LISTED: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StepsQuery {
    session_id: String,
    turn: String,
}

pub(super) fn step_json(step: &Step) -> Value {
    json!({
        "kind": step.kind.key(),
        "target": step.target,
        "failed": step.failed,
    })
}

/// `{steps, more}`: the first `STEPS_LISTED` and how many were left out.
pub(super) fn steps_json(steps: &[Step]) -> Value {
    json!({
        "steps": steps.iter().take(STEPS_LISTED).map(step_json).collect::<Vec<_>>(),
        "more": steps.len().saturating_sub(STEPS_LISTED),
    })
}

/// `GET /api/v1/mobile/room/steps?sessionId=&turn=`
pub(super) async fn steps(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<StepsQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let db_path = backend.workspace.db_path.clone();
    let value = tokio::task::spawn_blocking(move || {
        let repo =
            SqliteSessionRepo::open_read_only(&db_path).map_err(|error| error.to_string())?;
        let messages = messages(&repo, &query.session_id)?;
        Ok::<_, String>(steps_json(&steps_of(&messages, &query.turn)))
    })
    .await
    .map_err(|error| HttpApiError::internal(error.to_string()))?
    .map_err(HttpApiError::internal)?;
    Ok(Json(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::timeline::activity::ToolKind;

    #[test]
    fn each_step_names_its_kind_and_target() {
        let steps = vec![
            Step {
                kind: ToolKind::Read,
                target: Some("src/cache.rs".into()),
                failed: false,
            },
            Step {
                kind: ToolKind::Command,
                target: Some("yarn test".into()),
                failed: true,
            },
        ];
        assert_eq!(
            steps_json(&steps),
            json!({ "steps": [
                { "kind": "read", "target": "src/cache.rs", "failed": false },
                { "kind": "command", "target": "yarn test", "failed": true }
            ], "more": 0 })
        );
        let many = vec![steps[0].clone(); STEPS_LISTED + 3];
        assert_eq!(steps_json(&many)["more"], 3);
    }
}
