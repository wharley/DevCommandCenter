//! The resident agents as the sidebar knows them: each one's state across its
//! sessions, what the agent's page reads, and what "Agentes" counts.

use std::path::Path;

use gpui::Context;

use super::Sidebar;
use crate::data::agents::{AgentRecord, AgentState, REVIEWER_PRESET, aggregate, lead};
use crate::data::daily_recap::DailyRecap;
use crate::data::pr_review_jobs::{JobStatus, PrReviewJob};
use crate::data::seen::SeenResults;

/// The reading markers the agents' states depend on.
pub struct AgentMarkers {
    /// Finished agent results the person opened (`agent-seen-results.ts`).
    pub seen: SeenResults,
    /// The daily recap's window (`daily-recap.ts`), a card in the team room.
    pub recap: DailyRecap,
    /// The weekly bulletin's last week opened, a card in the team room.
    pub bulletin: crate::data::weekly_bulletin::WeeklyBulletin,
}

impl AgentMarkers {
    pub fn load(dir: &Path) -> Self {
        let now = chrono::Local::now();
        Self {
            seen: SeenResults::load(
                &dir.join("agent-seen-results.json"),
                &now.to_utc().to_rfc3339(),
            ),
            recap: DailyRecap::load(&dir.join("daily-recap.json"), now),
            bulletin: crate::data::weekly_bulletin::WeeklyBulletin::load(
                &dir.join("weekly-bulletin.json"),
                now,
            ),
        }
    }
}

/// One agent and the state its mascot shows.
pub struct AgentView<'a> {
    pub agent: &'a AgentRecord,
    pub state: AgentState,
}

impl Sidebar {
    /// `useAgentsQuery`: each agent's state across its sessions; the
    /// reviewer's pull request reviews count too.
    pub(super) fn agent_views(&self) -> Vec<AgentView<'_>> {
        let running = self.running_agent_tasks();
        self.state
            .data
            .agents
            .iter()
            .map(|agent| {
                let mut sessions: Vec<(AgentState, bool)> = agent
                    .sessions
                    .iter()
                    .map(|session| {
                        let unread = session.state == AgentState::Done
                            && self
                                .agent_markers
                                .seen
                                .is_unread(&session.session_id, session.completed_at.as_deref());
                        (session.state, unread)
                    })
                    .collect();
                if agent.preset.as_deref() == Some(REVIEWER_PRESET) {
                    sessions.extend(self.pr_reviews.iter().map(|job| {
                        let state = if job.status == JobStatus::Running {
                            AgentState::Working
                        } else {
                            AgentState::Done
                        };
                        (state, job.unread())
                    }));
                }
                // A task it opened that still runs: it is at work, not done.
                sessions.extend(
                    running
                        .iter()
                        .filter(|task| task.agent_id == agent.id)
                        .map(|_| (AgentState::Working, false)),
                );
                AgentView {
                    agent,
                    state: aggregate(&sessions),
                }
            })
            .collect()
    }
}

/// What the agent's page reads (`AgentView` in `use-agents.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct AgentPageData {
    pub agent: AgentRecord,
    pub state: AgentState,
    /// Per session, whether its finished result is unread.
    pub unread: Vec<bool>,
    /// Task names by id (`workspaceNames`).
    pub task_names: std::collections::HashMap<String, String>,
    /// Project names by project id (`projectLabels`).
    pub project_labels: std::collections::HashMap<String, String>,
    /// The reviewer's pull request reviews (`prReviews`).
    pub pr_reviews: Vec<PrReviewJob>,
    /// The agent's own conversation task, once it has one.
    pub home_task: Option<String>,
}

impl Sidebar {
    pub fn agent_page(&self, id: &str) -> Option<AgentPageData> {
        let view = self
            .agent_views()
            .into_iter()
            .find(|view| view.agent.id == id)?;
        let unread = view
            .agent
            .sessions
            .iter()
            .map(|session| {
                session.state == AgentState::Done
                    && self
                        .agent_markers
                        .seen
                        .is_unread(&session.session_id, session.completed_at.as_deref())
            })
            .collect();
        let data = &self.state.data;
        let task_names: std::collections::HashMap<String, String> = data
            .tasks
            .iter()
            .map(|task| (task.id.clone(), task.name.clone()))
            .collect();
        Some(AgentPageData {
            agent: view.agent.clone(),
            state: view.state,
            unread,
            task_names,
            project_labels: data
                .projects
                .iter()
                .map(|project| (project.project_id.clone(), project.display_name.clone()))
                .collect(),
            home_task: data.agent_homes.get(id).cloned(),
            pr_reviews: if view.agent.preset.as_deref() == Some(REVIEWER_PRESET) {
                self.pr_reviews.clone()
            } else {
                Vec::new()
            },
        })
    }

    /// The built-in reviewer, for the pull requests hub.
    pub fn reviewer(&self) -> Option<AgentRecord> {
        self.state
            .data
            .agents
            .iter()
            .find(|agent| agent.preset.as_deref() == Some(REVIEWER_PRESET))
            .cloned()
    }

    /// The daily recap's current window.
    pub fn recap_window(&self) -> Option<crate::data::daily_recap::RecapWindow> {
        self.agent_markers.recap.current.clone()
    }

    pub fn recap_unread(&self) -> bool {
        self.agent_markers.recap.is_unread()
    }

    pub fn bulletin_unread(&self) -> bool {
        self.agent_markers.bulletin.is_unread(chrono::Local::now())
    }

    /// The bulletin's card opened in the team room.
    pub fn mark_bulletin_seen(&mut self, cx: &mut Context<Self>) {
        if self.agent_markers.bulletin.mark_seen(chrono::Local::now()) {
            cx.notify();
        }
    }

    /// `markDailyRecapSeen`: its card is open in the team room.
    pub fn mark_recap_seen(&mut self, cx: &mut Context<Self>) {
        if self.agent_markers.recap.mark_seen() {
            cx.notify();
        }
    }

    /// `RECAP_BLOCKING_REASONS`: what blocks the person now (an unread result
    /// is "moved" instead).
    pub fn recap_blocked(&self) -> Vec<dcc_core::domain::attention::AttentionItem> {
        use dcc_core::domain::attention::AttentionReason as R;
        self.state
            .attention()
            .into_iter()
            .filter(|item| {
                matches!(
                    item.reason,
                    R::Permission
                        | R::Input
                        | R::Conflicts
                        | R::PrConflicts
                        | R::ChecksFailing
                        | R::DelegatedReview
                        | R::Setup
                )
            })
            .collect()
    }

    /// A listed task's project id.
    pub fn task_project(&self, id: &str) -> Option<String> {
        self.state.task(id).map(|task| task.project_id.clone())
    }

    /// The agent the team's screen opens on: the one that needs the person,
    /// else one with a result unread, else the first.
    pub fn lead_agent(&self) -> Option<String> {
        let views = self.agent_views();
        lead(&views, |view| view.state)
            .or(views.first())
            .map(|view| view.agent.id.clone())
    }
}
