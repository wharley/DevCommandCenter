//! The cards of a room on the phone, the same ones the desktop's room shows
//! there (`dcc_core::domain::room_inbox`): an agent's conversation in a task
//! that finished or waits on the person, a task an agent opened once its
//! turn ends, each in its project's room (else Geral) and left out of the
//! room it was asked in; the check-ins in their project's room; and, in
//! Geral only, an agent past its monthly limit and the agents and routines
//! agents proposed — a source's proposal in the room it was asked in.
//!
//! Read from what the desktop already keeps: DCC's database, and its own
//! files beside it (`agent-seen-results.json` for what was opened,
//! `agent-triggers.json` for what woke an agent, `check-ins.json`), as
//! `room_handoffs` reads `room-handoffs.json`. The pull request reviews the
//! Reviewer runs from the hub live only in the desktop's memory, so they
//! are not here.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Local;
use dcc_core::domain::agent_budget::{month_key, month_start, over_limit};
use dcc_core::domain::agent_proposal::AgentProposal;
use dcc_core::domain::agent_reply::AgentReply;
use dcc_core::domain::agent_routine::{schedule_label, Routine};
use dcc_core::domain::agent_source::AgentSource;
use dcc_core::domain::agent_trigger::TriggerRun;
use dcc_core::domain::check_in::{stored_cards, CheckIn, STALLED_MINUTES};
use dcc_core::domain::room_inbox::{
    opened_task_card, result_unread, shows_in, task_card, TaskCard, TaskCardKind, TaskSession,
};
use dcc_core::domain::room_presence::room_for;
use dcc_core::ports::{DelegationRepo, WorkspaceRepo};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};

use super::room_feed::Team;
use super::*;

/// How much of a proposed agent's role the card shows, as the desktop's.
const ROLE_PREVIEW_CHARS: usize = 220;

/// The desktop's reading marker of the agents' results.
#[derive(Debug, Default, Deserialize)]
struct SeenMarks {
    baseline: Option<String>,
    #[serde(default)]
    seen: HashMap<String, String>,
}

/// A task an agent opened from one of its conversations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Opened {
    pub agent_id: String,
    /// The room of the conversation that asked for it.
    pub asked_from: Option<String>,
    pub task_id: String,
    pub task_session: String,
    pub open: bool,
    pub completed: Option<String>,
}

/// What the cards are made from.
#[derive(Default)]
pub(super) struct CardSources {
    /// The projects with a room.
    pub rooms: Vec<String>,
    /// Every project, `(id, name)`.
    pub projects: Vec<(String, String)>,
    /// Task id → its name (if it has one) and project.
    pub tasks: HashMap<String, (Option<String>, String)>,
    seen: SeenMarks,
    pub runs: Vec<TriggerRun>,
    pub opened: Vec<Opened>,
    pub check_ins: Vec<CheckIn>,
    /// The agents past their monthly limit.
    pub over_limit: Vec<String>,
    pub agent_proposals: Vec<AgentProposal>,
    pub routines: Vec<Routine>,
    pub sources: Vec<AgentSource>,
    /// The replies agents proposed, waiting for the person.
    pub replies: Vec<AgentReply>,
    pub now_ms: i64,
}

fn read_file<T: serde::de::DeserializeOwned + Default>(app_data: &Path, name: &str) -> T {
    std::fs::read(app_data.join(name))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// The trigger runs of the desktop's `agent-triggers.json`.
fn read_runs(app_data: &Path) -> Vec<TriggerRun> {
    #[derive(Default, Deserialize)]
    struct Stored {
        #[serde(default)]
        runs: Vec<TriggerRun>,
    }
    read_file::<Stored>(app_data, "agent-triggers.json").runs
}

/// The tasks agents opened from their conversations lately, with how each
/// one's own conversation is.
fn read_opened(repo: &SqliteSessionRepo, team: &Team, now_ms: i64) -> Result<Vec<Opened>, String> {
    let cutoff = now_ms - dcc_core::domain::room_inbox::KEEP_DAYS * 24 * 60 * 60 * 1000;
    let runtime = tokio::runtime::Handle::current();
    let delegations = runtime
        .block_on(repo.list_delegations(None, None))
        .map_err(|error| error.to_string())?;
    let mut opened = Vec::new();
    let mut sessions_of: HashMap<String, Vec<(String, bool, Option<String>)>> = HashMap::new();
    for delegation in delegations {
        // One that has not moved for longer than a card lives has none.
        if dcc_core::domain::room_inbox::parse_ms(&delegation.updated_at)
            .is_some_and(|at| at < cutoff)
        {
            continue;
        }
        let Some(parent) = team
            .bindings
            .iter()
            .find(|binding| binding.session_id == delegation.parent_session_id.0)
        else {
            continue;
        };
        let (Some(task_id), Some(task_session)) = (
            delegation
                .budget
                .new_task
                .as_ref()
                .and_then(|task| task.workspace_id.clone()),
            delegation.child_session_id.as_ref().map(|id| id.0.clone()),
        ) else {
            continue;
        };
        if !sessions_of.contains_key(&task_id) {
            let sessions = repo
                .list_workspace_sessions(&dcc_core::domain::workspace::WorkspaceId(task_id.clone()))
                .map_err(|error| error.to_string())?
                .iter()
                .map(|summary| {
                    let input =
                        dcc_core::domain::attention::SessionActivityInput::from_summary(summary);
                    (
                        summary.session.id.0.clone(),
                        input.is_running(),
                        summary.last_turn_completed_at.clone(),
                    )
                })
                .collect();
            sessions_of.insert(task_id.clone(), sessions);
        }
        let Some((_, open, completed)) = sessions_of[&task_id]
            .iter()
            .find(|(id, _, _)| *id == task_session)
        else {
            continue;
        };
        opened.push(Opened {
            agent_id: parent.agent_id.clone(),
            asked_from: parent.room.clone(),
            task_id,
            task_session,
            open: *open,
            completed: completed.clone(),
        });
    }
    Ok(opened)
}

/// The agents with a limit that are past it this month.
fn read_over_limit(repo: &SqliteSessionRepo, team: &Team) -> Result<Vec<String>, String> {
    let now = Local::now();
    let (month, since) = (month_key(now), month_start(now));
    let mut over = Vec::new();
    for agent in &team.agents {
        let Some(limit) = agent.monthly_token_limit else {
            continue;
        };
        let spend = repo
            .agent_spend_since(&agent.id, &since)
            .map_err(|error| error.to_string())?;
        if over_limit(
            &spend,
            Some(limit),
            agent.limit_override_month.as_deref(),
            &month,
        ) {
            over.push(agent.id.clone());
        }
    }
    Ok(over)
}

/// Everything the cards are made from, read once per room read.
pub(super) fn read_sources(
    db_path: &Path,
    app_data: &Path,
    repo: &SqliteSessionRepo,
    team: &Team,
) -> Result<CardSources, String> {
    let projects = super::mobile_rooms::project_names(db_path)?;
    let rooms = super::mobile_rooms::listed_rooms_of(app_data, team, &projects)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let workspaces = SqliteWorkspaceRepo::open(db_path).map_err(|error| error.to_string())?;
    let tasks = tokio::runtime::Handle::current()
        .block_on(workspaces.list_workspaces())
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|task| (task.id.0, (task.name, task.project_id.0)))
        .collect();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let check_ins = std::fs::read_to_string(app_data.join("check-ins.json"))
        .map(|raw| stored_cards(&raw))
        .unwrap_or_default();
    Ok(CardSources {
        rooms,
        projects,
        tasks,
        seen: read_file(app_data, "agent-seen-results.json"),
        runs: read_runs(app_data),
        opened: read_opened(repo, team, now_ms)?,
        check_ins,
        over_limit: read_over_limit(repo, team)?,
        agent_proposals: repo
            .waiting_agent_proposals()
            .map_err(|error| error.to_string())?,
        routines: repo
            .list_agent_routines()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|routine| routine.proposed)
            .collect(),
        sources: repo
            .list_agent_sources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|source| source.proposed)
            .collect(),
        replies: repo
            .list_pending_agent_replies()
            .map_err(|error| error.to_string())?,
        now_ms,
    })
}

fn preview(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= ROLE_PREVIEW_CHARS {
        return text;
    }
    let cut: String = text.chars().take(ROLE_PREVIEW_CHARS).collect();
    format!("{}…", cut.trim_end())
}

fn task_json(card: TaskCard) -> Value {
    let (kind, actions) = match card.kind {
        TaskCardKind::Result { .. } => ("taskResult", json!(["open", "delivery"])),
        TaskCardKind::Waiting => ("taskWaiting", json!(["open"])),
    };
    json!({
        "key": card.key,
        "kind": kind,
        "agentId": card.agent_id,
        "title": card.title,
        "at": card.at,
        "unread": card.unread,
        "trigger": card.trigger.map(|trigger| trigger.id()),
        "verdict": Value::Null,
        "workspaceId": card.workspace_id,
        "sessionId": card.session_id,
        "prId": Value::Null,
        "actions": actions,
    })
}

fn check_in_json(check: &CheckIn) -> Value {
    json!({
        "key": format!("check:{}", check.key),
        "kind": "checkIn",
        "agentId": Value::Null,
        "title": check.title,
        "at": check.at,
        "unread": check.unread,
        "trigger": Value::Null,
        "verdict": Value::Null,
        "workspaceId": check.workspace_id,
        "sessionId": check.session_id,
        "prId": Value::Null,
        "actions": ["open"],
        "checkIn": {
            "kind": check.kind.id(),
            "place": check.place,
            "url": check.url,
            "minutes": STALLED_MINUTES,
        },
    })
}

/// A card DCC itself has for the person, at the room's end.
fn own_json(key: String, kind: &str, agent_id: &str, title: &str, at: Option<&str>) -> Value {
    json!({
        "key": key,
        "kind": kind,
        "agentId": agent_id,
        "title": title,
        "at": at,
        "unread": false,
        "trigger": Value::Null,
        "verdict": Value::Null,
        "workspaceId": Value::Null,
        "sessionId": Value::Null,
        "prId": Value::Null,
        "actions": ["create", "discard"],
    })
}

/// The cards of `room` (none: Geral): the inbox's and the check-ins in
/// time order, then DCC's own (the limits, the proposals) as the desktop
/// shows them after everything.
pub(super) fn cards(team: &Team, sources: &CardSources, room: Option<&str>) -> Vec<Value> {
    let unread = |session: &str, completed: &str| {
        result_unread(
            sources.seen.baseline.as_deref(),
            sources.seen.seen.get(session).map(String::as_str),
            Some(completed),
        )
    };
    let project_of = |task: &str| sources.tasks.get(task).map(|(_, project)| project.clone());
    let room_of = |task: &str| room_for(project_of(task), &sources.rooms);
    // A result of a task an agent opened: the room it was asked in.
    let asked_from = |card: &TaskCard| match card.kind {
        TaskCardKind::Result { .. } => sources
            .opened
            .iter()
            .find(|opened| opened.task_id == card.workspace_id)
            .map(|opened| opened.asked_from.as_deref()),
        TaskCardKind::Waiting => None,
    };
    let mut inbox: Vec<(String, Value)> = Vec::new();
    let mut keys: HashSet<String> = HashSet::new();
    let known = |agent: &str| team.agents.iter().any(|known| known.id == agent);
    for binding in &team.bindings {
        let input = team.activity.get(&binding.session_id);
        let waiting = team.waiting(binding).is_some();
        let changed = team.changed_at(binding);
        let session = TaskSession {
            agent_id: &binding.agent_id,
            session_id: &binding.session_id,
            workspace_id: &binding.workspace_id,
            project_id: &binding.project_id,
            title: binding.title.as_deref(),
            waiting,
            working: team.running(binding) && !waiting,
            completed_at: input.and_then(|input| input.last_turn_completed_at.as_deref()),
            updated_at: &changed,
        };
        let name = sources
            .tasks
            .get(&binding.workspace_id)
            .and_then(|(name, _)| name.as_deref());
        let Some(card) = task_card(&session, name, &sources.runs, &unread, sources.now_ms) else {
            continue;
        };
        if shows_in(
            room_of(&card.workspace_id).as_deref(),
            asked_from(&card),
            room,
        ) && keys.insert(card.key.clone())
        {
            inbox.push((card.at.clone(), task_json(card)));
        }
    }
    for opened in &sources.opened {
        let name = sources
            .tasks
            .get(&opened.task_id)
            .and_then(|(name, _)| name.clone())
            .unwrap_or_default();
        let Some(card) = opened_task_card(
            &opened.agent_id,
            &opened.task_id,
            &name,
            &opened.task_session,
            opened.open,
            opened.completed.as_deref(),
            &unread,
            sources.now_ms,
        ) else {
            continue;
        };
        if shows_in(
            room_of(&card.workspace_id).as_deref(),
            asked_from(&card),
            room,
        ) && keys.insert(card.key.clone())
        {
            inbox.push((card.at.clone(), task_json(card)));
        }
    }
    for check in &sources.check_ins {
        if room_for(check.project_id.clone(), &sources.rooms).as_deref() == room {
            inbox.push((check.at.clone(), check_in_json(check)));
        }
    }
    inbox.retain(|(_, card)| card["agentId"].as_str().is_none_or(known));
    inbox.sort_by(|left, right| left.0.cmp(&right.0));
    let mut all: Vec<Value> = inbox.into_iter().map(|(_, card)| card).collect();
    if room.is_none() {
        for agent in team
            .agents
            .iter()
            .filter(|agent| sources.over_limit.contains(&agent.id))
        {
            let mut card = own_json(
                format!("limit:{}", agent.id),
                "limit",
                &agent.id,
                &agent.name,
                None,
            );
            card["actions"] = json!(["continue"]);
            card["limit"] = json!(agent.monthly_token_limit);
            all.push(card);
        }
        for routine in sources
            .routines
            .iter()
            .filter(|routine| known(&routine.agent_id))
        {
            let mut card = own_json(
                format!("routine:{}", routine.id),
                "routineProposal",
                &routine.agent_id,
                &routine.prompt,
                Some(&routine.created_at),
            );
            card["routine"] = json!({
                "id": routine.id,
                "when": schedule_label(routine, false),
            });
            all.push(card);
        }
        for proposal in sources
            .agent_proposals
            .iter()
            .filter(|proposal| known(&proposal.agent_id))
        {
            let draft = &proposal.draft;
            let names: Vec<&str> = draft
                .projects
                .iter()
                .filter_map(|id| {
                    sources
                        .projects
                        .iter()
                        .find(|(project, _)| project == id)
                        .map(|(_, name)| name.as_str())
                })
                .collect();
            let mut card = own_json(
                format!("proposal:{}", proposal.id),
                "agentProposal",
                &proposal.agent_id,
                &draft.name,
                Some(&proposal.created_at),
            );
            card["proposal"] = json!({
                "id": proposal.id,
                "name": draft.name,
                "role": preview(&draft.role),
                "avatar": draft.avatar,
                "projects": names,
                "firstRequest": draft.kickoff_prompt,
            });
            all.push(card);
        }
    }
    for source in sources
        .sources
        .iter()
        .filter(|source| source.room.as_deref() == room && known(&source.agent_id))
    {
        let mut card = own_json(
            format!("source:{}", source.id),
            "sourceProposal",
            &source.agent_id,
            &source.watch,
            Some(&source.created_at),
        );
        card["source"] = json!({
            "id": source.id,
            "connector": source.connector,
            "watch": source.watch,
            "every": source.every_minutes,
        });
        all.push(card);
    }
    for reply in sources
        .replies
        .iter()
        .filter(|reply| reply.room.as_deref() == room && known(&reply.agent_id))
    {
        all.push(reply_json(reply));
    }
    all
}

/// A reply an agent proposed: it needs the person until sent or discarded.
fn reply_json(reply: &AgentReply) -> Value {
    let mut card = own_json(
        format!("reply:{}", reply.id),
        "replyProposal",
        &reply.agent_id,
        &reply.destination,
        Some(&reply.created_at),
    );
    card["unread"] = json!(true);
    card["actions"] = json!(["send", "discard"]);
    card["reply"] = json!({
        "id": reply.id,
        "connector": reply.connector,
        "destination": reply.destination,
        "text": reply.text,
        "error": reply.error,
    });
    card
}

/// What moves the room when its cards change.
pub(super) fn digest(cards: &[Value], digest: &mut Sha256) {
    for card in cards {
        digest.update(card.to_string().as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::super::room_feed::tests::{agent, binding, input, team};
    use super::*;
    use dcc_core::domain::check_in::CheckInKind;

    fn done(at: &str) -> dcc_core::domain::attention::SessionActivityInput {
        let mut input = input(false, None);
        input.last_turn_completed_at = Some(at.into());
        input
    }

    fn sources() -> CardSources {
        CardSources {
            rooms: vec!["checkout".into()],
            projects: vec![("checkout".into(), "vendeagora-app".into())],
            tasks: HashMap::from([
                (
                    "w-s-done".to_string(),
                    (Some("Checkout".to_string()), "checkout".to_string()),
                ),
                (
                    "w-s-omni".to_string(),
                    (Some("Omni".to_string()), "omni".to_string()),
                ),
            ]),
            seen: SeenMarks {
                baseline: Some("2026-11-01T00:00:00Z".into()),
                seen: HashMap::new(),
            },
            runs: vec![TriggerRun {
                agent_id: "dev".into(),
                trigger: dcc_core::domain::agent_trigger::Trigger::CiFailed,
                target: "w-s-done".into(),
                at: "2026-11-10T09:50:00Z".into(),
            }],
            now_ms: dcc_core::domain::room_inbox::parse_ms("2026-11-10T12:00:00Z").unwrap(),
            ..CardSources::default()
        }
    }

    fn team_with_results() -> Team {
        let mut team = team();
        team.bindings
            .push(binding("s-done", "dev", None, "2026-11-10T09:00:00Z"));
        team.bindings
            .push(binding("s-omni", "rev", None, "2026-11-10T09:00:00Z"));
        team.activity
            .insert("s-done".into(), done("2026-11-10T10:00:00Z"));
        team.activity
            .insert("s-omni".into(), done("2026-11-10T10:30:00Z"));
        team
    }

    #[test]
    fn a_task_card_goes_to_its_projects_room_else_geral_with_what_woke_it() {
        let team = team_with_results();
        let sources = sources();
        let checkout = cards(&team, &sources, Some("checkout"));
        let keys: Vec<&str> = checkout
            .iter()
            .map(|card| card["key"].as_str().unwrap())
            .collect();
        assert_eq!(keys, ["done:s-done:2026-11-10T10:00:00Z"]);
        let card = &checkout[0];
        assert_eq!(card["kind"], "taskResult");
        assert_eq!(card["title"], "Checkout");
        assert_eq!(card["trigger"], "ciFailed");
        assert_eq!(card["unread"], true);
        assert_eq!(card["workspaceId"], "w-s-done");
        assert_eq!(card["sessionId"], "s-done");
        assert_eq!(card["actions"], json!(["open", "delivery"]));
        // A project with no room: Geral. The task at work (`s-task`) has none.
        let geral = cards(&team, &sources, None);
        assert_eq!(geral.len(), 1);
        assert_eq!(geral[0]["title"], "Omni");
        assert_eq!(geral[0]["trigger"], Value::Null);
    }

    #[test]
    fn an_opened_task_stays_out_of_the_room_it_was_asked_in() {
        let mut team = team();
        team.agents.push(agent("qa"));
        let mut sources = sources();
        sources.tasks.insert(
            "w-opened".into(),
            (Some("Corrigir domínio".into()), "omni".into()),
        );
        sources.opened = vec![Opened {
            agent_id: "dev".into(),
            asked_from: None,
            task_id: "w-opened".into(),
            task_session: "s-opened".into(),
            open: false,
            completed: Some("2026-11-10T11:00:00Z".into()),
        }];
        // Asked in Geral, its project has no room: Geral already reads it.
        assert!(cards(&team, &sources, None).is_empty());
        // Asked in the checkout room: Geral shows it.
        sources.opened[0].asked_from = Some("checkout".into());
        let geral = cards(&team, &sources, None);
        assert_eq!(geral[0]["title"], "Corrigir domínio");
        assert!(cards(&team, &sources, Some("checkout")).is_empty());
    }

    #[test]
    fn check_ins_go_to_their_projects_room_and_own_cards_to_geral() {
        let team = team();
        let mut sources = sources();
        sources.check_ins = vec![CheckIn {
            key: "stalled:w-1".into(),
            kind: CheckInKind::TaskStalled,
            title: "Checkout".into(),
            place: "vendeagora-app".into(),
            url: None,
            workspace_id: Some("w-1".into()),
            session_id: Some("s-1".into()),
            project_id: Some("checkout".into()),
            at: "2026-11-10T11:00:00Z".into(),
            unread: true,
        }];
        sources.over_limit = vec!["rev".into()];
        sources.routines = vec![Routine {
            id: "r-1".into(),
            agent_id: "dev".into(),
            days: vec![0, 1, 2, 3, 4],
            hour: 9,
            minute: 0,
            prompt: "Resumo do dia".into(),
            enabled: false,
            proposed: true,
            last_run_at: None,
            created_at: "2026-11-10T08:00:00Z".into(),
        }];
        let checkout = cards(&team, &sources, Some("checkout"));
        assert_eq!(checkout.len(), 1);
        assert_eq!(checkout[0]["kind"], "checkIn");
        assert_eq!(checkout[0]["agentId"], Value::Null);
        assert_eq!(checkout[0]["checkIn"]["kind"], "taskStalled");
        assert_eq!(checkout[0]["checkIn"]["minutes"], 20);
        let geral = cards(&team, &sources, None);
        let kinds: Vec<&str> = geral
            .iter()
            .map(|card| card["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["limit", "routineProposal"]);
        assert_eq!(geral[0]["actions"], json!(["continue"]));
        assert_eq!(geral[1]["routine"]["when"], "Dias úteis · 09:00");
        assert_eq!(geral[1]["actions"], json!(["create", "discard"]));
    }

    #[test]
    fn a_proposed_reply_is_a_card_in_its_room_that_needs_the_person() {
        let team = team();
        let mut sources = sources();
        sources.replies = vec![AgentReply {
            id: "rp-1".into(),
            agent_id: "rev".into(),
            session_id: "s-rev".into(),
            connector_id: "slack".into(),
            connector: "Slack".into(),
            tool: "slack_send_message".into(),
            arguments: "{}".into(),
            text_argument: "message".into(),
            destination: "#suporte · thread do Guilherme".into(),
            text: "Foi na loja 374.".into(),
            state: dcc_core::domain::agent_reply::ReplyState::Pending,
            error: None,
            created_at: "2026-11-10T11:00:00Z".into(),
            sent_at: None,
            room: Some("checkout".into()),
        }];
        assert!(cards(&team, &sources, None)
            .iter()
            .all(|card| card["kind"] != "replyProposal"));
        let checkout = cards(&team, &sources, Some("checkout"));
        let reply = checkout
            .iter()
            .find(|card| card["kind"] == "replyProposal")
            .unwrap();
        assert_eq!(reply["unread"], true);
        assert_eq!(reply["actions"], json!(["send", "discard"]));
        assert_eq!(reply["reply"]["text"], "Foi na loja 374.");
        assert_eq!(reply["reply"]["connector"], "Slack");
    }

    #[test]
    fn the_digest_moves_with_the_cards() {
        let team = team_with_results();
        let mut sources = sources();
        let hash = |sources: &CardSources| {
            let mut digest = Sha256::new();
            super::digest(&cards(&team, sources, None), &mut digest);
            format!("{:x}", digest.finalize())
        };
        let before = hash(&sources);
        assert_eq!(before, hash(&sources));
        sources
            .seen
            .seen
            .insert("s-omni".into(), "2026-11-10T10:30:00Z".into());
        assert_ne!(before, hash(&sources));
    }
}
