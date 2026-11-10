//! The phone's list of rooms (`docs/PLANO_MOBILE_SALAS`): Geral, then the
//! project rooms in the desktop's order (`dcc_core::domain::room_list`: the
//! ones the person made, kept in the desktop's `agent-rooms.json` beside
//! DCC's database, then the projects agents talk in), each with its last
//! line, who works there and the desktop's dot (`room_presence`): something
//! runs there, or something waits on the person. Like the attention
//! snapshot, a phone that sends back the `snapshotId` it holds as `after`
//! gets `{unchanged, snapshotId}` without a single event read.

use std::collections::HashSet;
use std::path::Path;

use dcc_core::domain::agent_conversation::home_agent_id;
use dcc_core::domain::attention::{
    derive_agent_activity, AgentActivityState, SessionActivityInput,
};
use dcc_core::domain::repository::repository_display_name;
use dcc_core::domain::room_list::rooms;
use dcc_core::domain::room_presence::{presence, Presence, Where, Work};
use dcc_core::domain::team_room_lines::{RoomLine, Speaker};
use dcc_core::domain::workspace::WorkspaceState;
use dcc_core::ports::{DelegationRepo, RepositoryRepo, WorkspaceRepo};
use dcc_infra::db::{AgentSessionBinding, SqliteSessionRepo, SqliteWorkspaceRepo};

use super::room_feed::{conversation_lines, read_team, Team};
use super::*;

/// How long a room's last line is on the list.
const LAST_CHARS: usize = 140;

#[derive(Debug, Default, Deserialize)]
pub(super) struct RoomsQuery {
    /// The `snapshotId` the phone already holds.
    after: Option<String>,
}

/// The projects as the desktop's column names them, `(id, name)`: one per
/// project, after its first checkout; the agents' homes left out.
pub(super) fn project_names(db_path: &Path) -> Result<Vec<(String, String)>, String> {
    let repo = SqliteWorkspaceRepo::open(db_path).map_err(|error| error.to_string())?;
    let repositories = tokio::runtime::Handle::current()
        .block_on(repo.list_repositories())
        .map_err(|error| error.to_string())?;
    let named: Vec<(String, String)> = repositories
        .iter()
        .filter(|repository| home_agent_id(&repository.project_id.0).is_none())
        .map(|repository| {
            (
                repository.project_id.0.clone(),
                repository_display_name(repository),
            )
        })
        .collect();
    Ok(dcc_core::domain::room_list::project_names(
        named.iter().map(|(id, name)| (id.as_str(), name.as_str())),
    ))
}

/// The rooms the person made, from the desktop's file; none when it is
/// missing or unreadable.
fn saved_rooms(app_data: &Path) -> Vec<String> {
    std::fs::read_to_string(app_data.join("agent-rooms.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// One room as the list shows it, before its last line is read.
struct RoomState<'a> {
    id: Option<String>,
    name: String,
    homes: Vec<&'a AgentSessionBinding>,
    working: Vec<String>,
    presence: Option<Presence>,
}

/// Geral and the listed project rooms with what goes on in each:
/// `running_tasks` are the projects of the running tasks agents opened.
fn room_states<'a>(
    team: &'a Team,
    listed: &[(String, String)],
    running_tasks: &[String],
) -> Vec<RoomState<'a>> {
    let ids: Vec<String> = listed.iter().map(|(id, _)| id.clone()).collect();
    let live = team
        .bindings
        .iter()
        .filter(|binding| team.running(binding))
        .map(|binding| Work {
            // A task's session counts in its project; a conversation in the
            // room it is held in.
            at: if Team::is_home(binding) {
                Where::Room(binding.room.as_deref())
            } else {
                Where::Project(&binding.project_id)
            },
            waiting: team.waiting(binding).is_some(),
        });
    let tasks = running_tasks.iter().map(|project| Work {
        at: Where::Project(project),
        waiting: false,
    });
    let found = presence(live.chain(tasks), &ids);
    std::iter::once((None, "Geral".to_string()))
        .chain(
            listed
                .iter()
                .map(|(id, name)| (Some(id.clone()), name.clone())),
        )
        .map(|(id, name)| {
            let homes = team.room_homes(id.as_deref());
            RoomState {
                working: team.working(&homes),
                presence: found.get(&id).copied(),
                id,
                name,
                homes,
            }
        })
        .collect()
}

fn snapshot_id(team: &Team, states: &[RoomState<'_>]) -> String {
    let mut digest = Sha256::new();
    for state in states {
        digest.update(state.id.as_deref().unwrap_or("-").as_bytes());
        digest.update(state.name.as_bytes());
        digest.update(state.working.join(",").as_bytes());
        digest.update(format!("{:?}", state.presence).as_bytes());
        team.digest(&state.homes, &mut digest);
    }
    digest.finalize()[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `text` on one line, at most `LAST_CHARS` characters.
fn one_line(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= LAST_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(LAST_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

fn last_json(line: &RoomLine, team: &Team) -> Value {
    let name = team
        .agents
        .iter()
        .find(|agent| agent.id == line.agent_id)
        .map(|agent| agent.name.as_str())
        .unwrap_or_default();
    json!({
        "agentId": line.agent_id,
        "agentName": name,
        "speaker": match line.speaker {
            Speaker::Person => "person",
            Speaker::Agent => "agent",
        },
        "text": one_line(&super::room_line_json::plain(&line.text)),
        "at": line.at,
    })
}

/// The projects of the running tasks agents opened from their
/// conversations, as the desktop's column counts them.
fn running_opened_tasks(db_path: &Path, team: &Team) -> Result<Vec<String>, String> {
    let sessions = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let runtime = tokio::runtime::Handle::current();
    let delegations = runtime
        .block_on(sessions.list_delegations(None, None))
        .map_err(|error| error.to_string())?;
    let agents_sessions: HashSet<&str> = team
        .bindings
        .iter()
        .map(|binding| binding.session_id.as_str())
        .collect();
    let opened: HashSet<String> = delegations
        .iter()
        .filter(|delegation| agents_sessions.contains(delegation.parent_session_id.0.as_str()))
        .filter_map(|delegation| delegation.budget.new_task.as_ref()?.workspace_id.clone())
        .collect();
    if opened.is_empty() {
        return Ok(Vec::new());
    }
    let workspaces = SqliteWorkspaceRepo::open(db_path).map_err(|error| error.to_string())?;
    let workspaces = runtime
        .block_on(workspaces.list_workspaces())
        .map_err(|error| error.to_string())?;
    let mut projects = Vec::new();
    for workspace in workspaces {
        if !opened.contains(&workspace.id.0)
            || matches!(
                workspace.state,
                WorkspaceState::Archived | WorkspaceState::Completed
            )
        {
            continue;
        }
        let inputs: Vec<SessionActivityInput> = sessions
            .list_workspace_sessions(&workspace.id)
            .map_err(|error| error.to_string())?
            .iter()
            .map(SessionActivityInput::from_summary)
            .collect();
        if derive_agent_activity(&inputs)
            .is_some_and(|activity| activity.state == AgentActivityState::Active)
        {
            projects.push(workspace.project_id.0);
        }
    }
    Ok(projects)
}

fn read_rooms(db_path: &Path, app_data: &Path, after: Option<&str>) -> Result<Value, String> {
    let repo = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let team = read_team(&repo)?;
    // Agent by agent, as the desktop's column reads them.
    let talked = team.agents.iter().flat_map(|agent| {
        team.bindings
            .iter()
            .filter(move |binding| binding.agent_id == agent.id)
            .filter_map(|binding| binding.room.clone())
    });
    let listed = rooms(&saved_rooms(app_data), talked, &project_names(db_path)?);
    let running = running_opened_tasks(db_path, &team)?;
    let states = room_states(&team, &listed, &running);
    let snapshot = snapshot_id(&team, &states);
    if after == Some(snapshot.as_str()) {
        return Ok(json!({ "unchanged": true, "snapshotId": snapshot }));
    }
    let mut list = Vec::new();
    for state in &states {
        // The newest conversation holds the room's last line.
        let last = match state.homes.last() {
            // What a person would read: not a wake nor a quiet answer.
            Some(home) => super::room_line_json::shown(conversation_lines(
                &repo,
                &home.agent_id,
                &home.session_id,
            )?)
            .into_iter()
            .rev()
            .find(|line| super::room_line_json::wake(line).is_none()),
            None => None,
        };
        let at = last
            .as_ref()
            .map(|line| line.at.clone())
            .or_else(|| state.homes.last().map(|home| team.changed_at(home)));
        list.push(json!({
            "id": state.id,
            "name": state.name,
            "last": last.as_ref().map(|line| last_json(line, &team)),
            "working": state.working,
            "needsYou": state.presence == Some(Presence::Waiting),
            "running": state.presence.is_some(),
            "at": at,
        }));
    }
    Ok(json!({ "snapshotId": snapshot, "rooms": list }))
}

/// `GET /api/v1/mobile/rooms`
pub(super) async fn list(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<RoomsQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let db_path = backend.workspace.db_path.clone();
    let app_data = db_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let value = tokio::task::spawn_blocking(move || {
        read_rooms(&db_path, &app_data, query.after.as_deref())
    })
    .await
    .map_err(|error| HttpApiError::internal(error.to_string()))?
    .map_err(HttpApiError::internal)?;
    Ok(Json(value))
}

#[cfg(test)]
mod tests {
    use super::super::room_feed::tests::team;
    use super::*;

    #[test]
    fn geral_first_then_each_room_with_what_goes_on_there() {
        let team = team();
        let listed = vec![
            ("checkout".to_string(), "Loja".to_string()),
            ("site".to_string(), "Site".to_string()),
        ];
        let states = room_states(&team, &listed, &["site".to_string()]);
        let names: Vec<&str> = states.iter().map(|state| state.name.as_str()).collect();
        assert_eq!(names, ["Geral", "Loja", "Site"]);
        assert_eq!(states[0].presence, None);
        assert!(states[0].working.is_empty());
        // Dev waits on a permission in the checkout room; its task runs there.
        assert_eq!(states[1].working, ["dev"]);
        assert_eq!(states[1].presence, Some(Presence::Waiting));
        assert_eq!(states[1].homes.len(), 3);
        // A running task an agent opened shows in its project's room.
        assert_eq!(states[2].presence, Some(Presence::Running));
        let same = snapshot_id(&team, &states);
        assert_eq!(
            same,
            snapshot_id(&team, &room_states(&team, &listed, &["site".to_string()]))
        );
        assert_ne!(same, snapshot_id(&team, &room_states(&team, &listed, &[])));
    }

    #[test]
    fn the_last_line_is_one_short_line() {
        assert_eq!(one_line("  Feito.\n\nAbri o PR  "), "Feito. Abri o PR");
        let long = "a".repeat(200);
        let cut = one_line(&long);
        assert_eq!(cut.chars().count(), LAST_CHARS);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn the_rooms_made_come_from_the_desktops_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(saved_rooms(dir.path()).is_empty());
        std::fs::write(
            dir.path().join("agent-rooms.json"),
            r#"["checkout","site"]"#,
        )
        .unwrap();
        assert_eq!(saved_rooms(dir.path()), ["checkout", "site"]);
    }
}
