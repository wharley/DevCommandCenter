//! The team room's inbox: what the agents did on their own or finished
//! elsewhere, as cards among the room's lines — an agent's conversation in a
//! task that finished or waits on the person (marked "Gatilho · CI falhou"
//! when a trigger woke it), and the Reviewer's pull request reviews. Built
//! from what DCC already keeps (the agents' sessions, the trigger runs, the
//! review jobs and the reading markers); nothing new is stored.

use std::collections::HashMap;

use dcc_core::domain::agent_conversation::home_agent_id;

use super::agent_triggers::{Run, Trigger};
use super::agents::{AgentRecord, AgentState, REVIEWER_PRESET};
use super::pr_review_jobs::{JobStatus, PrReviewJob};
use super::time::parse_ms;

/// Cards older than this leave the room.
const KEEP_DAYS: i64 = 7;
/// A trigger run this close before a result is what woke the agent.
const WOKEN_WITHIN_MS: i64 = 2 * 60 * 60 * 1000;

#[derive(Clone, Debug, PartialEq)]
pub enum CardKind {
    /// The agent's conversation in a task finished a turn.
    TaskResult {
        workspace_id: String,
        session_id: String,
        completed_at: String,
    },
    /// The agent waits on the person in a task's conversation.
    TaskWaiting {
        workspace_id: String,
        session_id: String,
    },
    /// A pull request review of the Reviewer's (or of the agent that asked
    /// for it), working until it finishes.
    PrReview {
        pr_id: String,
        failed: bool,
        running: bool,
        /// Why it failed, as the provider said it.
        reason: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct InboxCard {
    pub key: String,
    pub agent_id: String,
    pub kind: CardKind,
    /// The task's name or the pull request (`acme/app#1014`).
    pub title: String,
    /// The trigger that woke the agent, if one did.
    pub trigger: Option<Trigger>,
    /// When it happened (RFC 3339); a waiting one, when it last moved.
    pub at: String,
    pub unread: bool,
}

/// What the cards are made from.
pub struct InboxSources<'a> {
    pub agents: &'a [AgentRecord],
    pub task_names: &'a HashMap<String, String>,
    pub runs: &'a [Run],
    pub jobs: &'a [PrReviewJob],
    /// Whether a session's result at that time is still unread.
    pub unread: &'a dyn Fn(&str, &str) -> bool,
    pub now_ms: i64,
}

fn woken_by(runs: &[Run], agent: &str, target: &str, at_ms: i64) -> Option<Trigger> {
    runs.iter()
        .filter(|run| run.agent_id == agent && run.target == target)
        .filter_map(|run| parse_ms(&run.at).map(|ran| (ran, run.trigger)))
        .filter(|(ran, _)| *ran <= at_ms && at_ms - ran <= WOKEN_WITHIN_MS)
        .max_by_key(|(ran, _)| *ran)
        .map(|(_, trigger)| trigger)
}

/// The cards, oldest first.
pub fn cards(sources: &InboxSources<'_>) -> Vec<InboxCard> {
    let cutoff = sources.now_ms - KEEP_DAYS * 24 * 60 * 60 * 1000;
    let recent = |at: &str| parse_ms(at).is_some_and(|at| at >= cutoff);
    let mut cards = Vec::new();
    for agent in sources.agents {
        for session in &agent.sessions {
            // Its own conversation is the room itself.
            if home_agent_id(&session.project_id).is_some() {
                continue;
            }
            let title = sources
                .task_names
                .get(&session.workspace_id)
                .cloned()
                .or_else(|| session.title.clone())
                .unwrap_or_default();
            if session.state == AgentState::NeedsYou {
                cards.push(InboxCard {
                    key: format!("wait:{}", session.session_id),
                    agent_id: agent.id.clone(),
                    kind: CardKind::TaskWaiting {
                        workspace_id: session.workspace_id.clone(),
                        session_id: session.session_id.clone(),
                    },
                    title,
                    trigger: None,
                    at: session.updated_at.clone(),
                    unread: true,
                });
                continue;
            }
            let Some(completed) = session.completed_at.as_deref() else {
                continue;
            };
            if session.state == AgentState::Working || !recent(completed) {
                continue;
            }
            let trigger = parse_ms(completed)
                .and_then(|at| woken_by(sources.runs, &agent.id, &session.workspace_id, at));
            cards.push(InboxCard {
                key: format!("done:{}:{completed}", session.session_id),
                agent_id: agent.id.clone(),
                kind: CardKind::TaskResult {
                    workspace_id: session.workspace_id.clone(),
                    session_id: session.session_id.clone(),
                    completed_at: completed.to_string(),
                },
                title,
                trigger,
                at: completed.to_string(),
                unread: (sources.unread)(&session.session_id, completed),
            });
        }
    }
    let reviewer = sources
        .agents
        .iter()
        .find(|agent| agent.preset.as_deref() == Some(REVIEWER_PRESET));
    for job in sources.jobs {
        let asked = job
            .agent_id
            .as_deref()
            .and_then(|id| sources.agents.iter().find(|agent| agent.id == id));
        let Some(agent) = asked.or(reviewer) else {
            continue;
        };
        let running = job.status == JobStatus::Running;
        let at = if running {
            job.started_at.as_deref()
        } else {
            job.finished_at.as_deref()
        };
        let Some(at) = at.filter(|at| recent(at)) else {
            continue;
        };
        let title = if job.title.is_empty() {
            job.label.clone()
        } else {
            format!("{} · {}", job.label, job.title)
        };
        cards.push(InboxCard {
            key: format!("pr:{}:{at}", job.pr_id),
            agent_id: agent.id.clone(),
            kind: CardKind::PrReview {
                pr_id: job.pr_id.clone(),
                failed: job.status == JobStatus::Failed,
                running,
                reason: (job.status == JobStatus::Failed)
                    .then(|| job.error.clone())
                    .flatten(),
            },
            title,
            trigger: job.by_trigger.then_some(Trigger::PrUpdated),
            at: at.to_string(),
            unread: !running && !job.seen,
        });
    }
    cards.sort_by(|left, right| left.at.cmp(&right.at));
    cards
}

/// Beside "Agentes": the unread cards, and whether one waits on the person.
pub fn unread_count(cards: &[InboxCard]) -> (usize, bool) {
    let unread = cards.iter().filter(|card| card.unread).count();
    let waiting = cards
        .iter()
        .any(|card| matches!(card.kind, CardKind::TaskWaiting { .. }));
    (unread, waiting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agents::AgentSession;

    fn session(
        id: &str,
        task: &str,
        project: &str,
        state: AgentState,
        done: Option<&str>,
    ) -> AgentSession {
        AgentSession {
            session_id: id.into(),
            workspace_id: task.into(),
            state,
            completed_at: done.map(str::to_string),
            project_id: project.into(),
            title: Some(format!("Conversa {id}")),
            updated_at: "2026-11-07T11:00:00Z".into(),
            room: None,
        }
    }

    fn reviewer(sessions: Vec<AgentSession>) -> AgentRecord {
        AgentRecord {
            id: "rev".into(),
            name: "Revisor".into(),
            preset: Some(REVIEWER_PRESET.into()),
            avatar: dcc_core::domain::agent::AgentAvatar {
                color: "blue".into(),
                arms: 3,
                eyes: "round".into(),
                shape: "octopus".into(),
                accessory: "glasses".into(),
            },
            provider_id: None,
            model: None,
            effort: None,
            kickoff_prompt: String::new(),
            role: String::new(),
            offer_prompt: String::new(),
            extra_instructions: String::new(),
            projects: Vec::new(),
            monthly_token_limit: None,
            tool_access: Default::default(),
            auto_start_tasks: false,
            may_call: Vec::new(),
            account_id: None,
            sessions,
        }
    }

    #[test]
    fn results_waits_and_reviews_become_cards_with_what_woke_them() {
        let agents = vec![reviewer(vec![
            session(
                "s-ci",
                "w-1",
                "p-1",
                AgentState::Done,
                Some("2026-11-07T10:30:00Z"),
            ),
            session("s-ask", "w-2", "p-1", AgentState::NeedsYou, None),
            session(
                "s-run",
                "w-3",
                "p-1",
                AgentState::Working,
                Some("2026-11-07T09:00:00Z"),
            ),
            session(
                "s-home",
                "w-h",
                "dcc-agent-rev",
                AgentState::Done,
                Some("2026-11-07T10:40:00Z"),
            ),
            session(
                "s-old",
                "w-4",
                "p-1",
                AgentState::Done,
                Some("2026-10-01T10:00:00Z"),
            ),
        ])];
        let names = HashMap::from([("w-1".to_string(), "Checkout".to_string())]);
        let runs = vec![Run {
            agent_id: "rev".into(),
            trigger: Trigger::CiFailed,
            target: "w-1".into(),
            at: "2026-11-07T10:20:00Z".into(),
        }];
        let jobs = vec![PrReviewJob {
            pr_id: "pr-1".into(),
            label: "app#1014".into(),
            title: "Pagamentos".into(),
            status: JobStatus::Done,
            response: None,
            error: None,
            finished: Some(1),
            seen: false,
            finished_at: Some("2026-11-07T10:50:00Z".into()),
            by_trigger: true,
            started_at: Some("2026-11-07T10:45:00Z".into()),
            agent_id: None,
        }];
        let unread = |session: &str, _: &str| session == "s-ci";
        let now = parse_ms("2026-11-07T11:00:00Z").unwrap();
        let cards = cards(&InboxSources {
            agents: &agents,
            task_names: &names,
            runs: &runs,
            jobs: &jobs,
            unread: &unread,
            now_ms: now,
        });
        let keys: Vec<&str> = cards.iter().map(|card| card.title.as_str()).collect();
        assert_eq!(
            keys,
            ["Checkout", "app#1014 · Pagamentos", "Conversa s-ask"]
        );
        assert_eq!(cards[0].trigger, Some(Trigger::CiFailed));
        assert!(cards[0].unread);
        assert_eq!(cards[1].trigger, Some(Trigger::PrUpdated));
        assert!(matches!(cards[2].kind, CardKind::TaskWaiting { .. }));
        assert_eq!(unread_count(&cards), (3, true));

        // One the agent asked for shows working, unread only once done.
        let mut asked = jobs[0].clone();
        asked.pr_id = "pr-2".into();
        asked.status = JobStatus::Running;
        asked.finished_at = None;
        asked.title = String::new();
        asked.agent_id = Some(agents[0].id.clone());
        let jobs = [asked];
        let asked_cards = super::cards(&InboxSources {
            agents: &agents,
            task_names: &names,
            runs: &runs,
            jobs: &jobs,
            unread: &unread,
            now_ms: now,
        });
        let working = asked_cards
            .iter()
            .find(|card| matches!(card.kind, CardKind::PrReview { running: true, .. }))
            .unwrap();
        assert_eq!(
            (working.title.as_str(), working.unread, working.at.as_str()),
            ("app#1014", false, "2026-11-07T10:45:00Z")
        );
    }
}
