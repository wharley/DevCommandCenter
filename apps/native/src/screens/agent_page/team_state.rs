//! The state line under each agent in the team's column (`data::agent_line`
//! picks it); this gathers the facts once per frame from what the sidebar
//! already read, in O(agents + running tasks), with no reads of its own.

use dcc_core::domain::agent::REVIEWER_PRESET;
use dcc_core::domain::agent_conversation::home_project_id;
use gpui::{App, SharedString};

use super::room_running::elapsed;
use crate::agent::Mood;
use crate::data::agent_line::{AgentFacts, AgentLine, line};
use crate::data::agent_sources::Sources;
use crate::data::agent_triggers::AgentTriggers;
use crate::data::agents::{AgentRecord, AgentState};
use crate::data::pr_review_jobs::{JobStatus, PrReviewJobs};
use crate::i18n::{Args, Translate};
use crate::screens::sidebar::{AgentAtWork, RunningTask, Sidebar};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tone {
    Urgent,
    Active,
    Quiet,
}

/// What the sidebar knows, read once for all the rows.
pub(super) struct TeamFacts {
    running: Vec<RunningTask>,
    at_work: Vec<AgentAtWork>,
    now_ms: i64,
}

impl TeamFacts {
    pub(super) fn read(sidebar: &Sidebar) -> Self {
        Self {
            running: sidebar.running_agent_tasks(),
            at_work: sidebar.agents_at_work(),
            now_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// The running task of `agent`: one it opened, else one it works in.
    fn task(&self, agent: &str) -> Option<(&str, Option<&str>)> {
        self.running
            .iter()
            .find(|task| task.agent_id == agent && !task.name.is_empty())
            .map(|task| (task.name.as_str(), task.started_at.as_deref()))
            .or_else(|| {
                self.at_work
                    .iter()
                    .find(|work| work.agent_id == agent && !work.task_name.is_empty())
                    .map(|work| (work.task_name.as_str(), work.started_at.as_deref()))
            })
    }
}

fn replying(agent: &AgentRecord) -> bool {
    let home = home_project_id(&agent.id);
    agent
        .sessions
        .iter()
        .any(|session| session.project_id == home && session.state == AgentState::Working)
}

/// `agent`'s line and its tone.
pub(super) fn state_line(
    agent: &AgentRecord,
    mood: Mood,
    facts: &TeamFacts,
    cx: &App,
) -> (SharedString, Tone) {
    let jobs = PrReviewJobs::get(cx).map(|jobs| jobs.jobs.as_slice());
    let review = jobs.and_then(|jobs| {
        jobs.iter()
            .find(|job| {
                job.status == JobStatus::Running
                    && match &job.agent_id {
                        Some(id) => *id == agent.id,
                        None => agent.preset.as_deref() == Some(REVIEWER_PRESET),
                    }
            })
            .map(|job| job.label.as_str())
    });
    let triggers = cx.try_global::<AgentTriggers>();
    let triggers_on = triggers.is_some_and(|triggers| triggers.any_on(&agent.id));
    let facts_of = AgentFacts {
        mood: Some(mood),
        task: facts.task(&agent.id),
        review,
        replying: replying(agent),
        // The ceiling only matters with a trigger on.
        at_ceiling: triggers_on
            && triggers.is_some_and(|triggers| triggers.at_ceiling(&agent.id, chrono::Utc::now())),
        sources: cx.try_global::<Sources>().map_or(0, |sources| {
            sources.of(&agent.id).filter(|s| s.enabled).count()
        }),
        triggers_on,
    };
    let text = |key: &str| cx.t(key);
    match line(&facts_of) {
        AgentLine::NeedsYou => (text("agents.team.line.needsYou"), Tone::Urgent),
        AgentLine::Task { name, started_at } => {
            let time = elapsed(started_at, facts.now_ms).unwrap_or_default();
            let args = Args::default()
                .with("task", name)
                .with("time", time.clone());
            let key = if time.is_empty() {
                "agents.team.line.taskNow"
            } else {
                "agents.team.line.task"
            };
            (cx.t_with(key, &args), Tone::Active)
        }
        AgentLine::Reviewing(label) => (
            cx.t_with(
                "agents.team.line.reviewing",
                &Args::default().with("label", label),
            ),
            Tone::Active,
        ),
        AgentLine::Replying => (text("agents.team.line.replying"), Tone::Active),
        AgentLine::AtCeiling => (text("agents.team.line.ceiling"), Tone::Quiet),
        AgentLine::Sources(count) => (
            cx.t_with("agents.team.line.sources", &Args::count(count as i64)),
            Tone::Quiet,
        ),
        AgentLine::Triggers => (text("agents.team.line.triggers"), Tone::Quiet),
        AgentLine::NewResult => (text("agents.team.line.newResult"), Tone::Quiet),
        AgentLine::Idle => (text("agents.team.line.idle"), Tone::Quiet),
    }
}
