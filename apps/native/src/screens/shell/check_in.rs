//! The agents' check-in as `PrWatch` brings it (`data::check_ins`): every
//! new item becomes a card in the team room, with no model; then the agents
//! that watch for it wake, within their ceilings, the pause and the quiet
//! hours (`AgentTriggers`):
//! - a pull request asking for the person's review: the Reviewer reviews it
//!   in the hub, like a reviewed pull request that moved;
//! - an issue assigned to the person: the agent reads it in its own
//!   conversation and, when it is work for one of the person's projects,
//!   proposes the task by card (`dcc_open_task`);
//! - a task whose turn has no sign of life: the agent looks at it,
//!   read-only, in its conversation in that task.

use chrono::Utc;
use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{App, BorrowAppContext, Context};

use super::Shell;
use super::pr_watch::PrNewsArrived;
use super::review_trigger::review_run;
use super::reviewer_in_task::TaskAsk;
use crate::data::agent_conversation::ConversationStart;
use crate::data::agent_triggers::{AgentTriggers, Trigger};
use crate::data::agents::{AgentRecord, REVIEWER_PRESET};
use crate::data::check_ins::{CheckIn, CheckInKind, CheckIns, asked};
use crate::data::desktop_api::{DesktopApi, TurnOptions};
use crate::data::pr_hub::Reviewed;
use crate::i18n::{Args, Translate};

/// What an agent is asked about an issue assigned to the person.
pub(super) fn issue_prompt(issue: &CheckIn) -> String {
    format!(
        "[DCC] An issue was assigned to the person: {} — \"{}\" ({}).\n\nRead it. If it is work for one of the person's projects, open the task with dcc_open_task and a self-contained brief (it starts, or waits as a card, as the person set for you); if it is not, say why in one line. Answer in the person's language, short.",
        issue.place,
        issue.title,
        issue.url.as_deref().unwrap_or_default()
    )
}

/// What an agent is asked about a task with no sign of life.
pub(super) fn stalled_prompt(task: &CheckIn) -> String {
    format!(
        "[DCC] The turn of this task (\"{}\") has been open with no sign of life for {} minutes. Look at what it was doing and say in a few lines whether it is stuck and what would unblock it. Work read-only: do not edit files, stop the turn or send it anything; the person decides. Answer in the person's language, short.",
        task.title,
        crate::data::check_ins::STALLED_MINUTES
    )
}

fn announce(trigger: Trigger, agent: &str, item: &CheckIn, done: &Result<(), String>, cx: &App) {
    let key = |what: &str| format!("agents.triggers.{}.{what}", trigger.id());
    let title = cx.t_with(
        &key("done"),
        &Args::default().with("agent", agent.to_string()),
    );
    let what = match done {
        Ok(()) => cx.t(&key("ready")).to_string(),
        Err(_) => cx.t(&key("failed")).to_string(),
    };
    crate::ui::attention::notify_away(&title, &format!("{} · {what}", item.place));
}

/// The agent's own conversation in the Geral room, when it has one.
pub(super) fn home_session(agent: &AgentRecord) -> Option<String> {
    let home = home_project_id(&agent.id);
    agent
        .sessions
        .iter()
        .filter(|session| session.project_id == home && session.room.is_none())
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
        .map(|session| session.session_id.clone())
}

impl Shell {
    /// The agents watching for `trigger`; the hub's review triggers are the
    /// Reviewer's alone.
    pub(super) fn watching(&self, trigger: Trigger, cx: &App) -> Vec<AgentRecord> {
        let Some(triggers) = cx.try_global::<AgentTriggers>() else {
            return Vec::new();
        };
        self.sidebar
            .read(cx)
            .agents()
            .iter()
            .filter(|agent| {
                !trigger.reviewer_only() || agent.preset.as_deref() == Some(REVIEWER_PRESET)
            })
            .filter(|agent| triggers.is_on(&agent.id, trigger))
            .cloned()
            .collect()
    }

    /// This pass's check-in: new items become cards, then wake who watches.
    pub(super) fn check_in_arrived(&mut self, event: &PrNewsArrived, cx: &mut Context<Self>) {
        if !cx.has_global::<CheckIns>() {
            return;
        }
        let now = Utc::now();
        let requested = event
            .requested
            .iter()
            .map(|pr| asked(CheckInKind::ReviewRequested, pr))
            .collect();
        let issues = event
            .issues
            .iter()
            .map(|issue| asked(CheckInKind::IssueAssigned, issue))
            .collect();
        let (requested, issues, stalled) = cx.update_global::<CheckIns, _>(|checks, _| {
            (
                checks.take(CheckInKind::ReviewRequested, requested, now),
                checks.take(CheckInKind::IssueAssigned, issues, now),
                checks.take(CheckInKind::TaskStalled, event.stalled.clone(), now),
            )
        });
        if requested.is_empty() && issues.is_empty() && stalled.is_empty() {
            return;
        }
        // The count beside "Agentes".
        self.sidebar.update(cx, |_, cx| cx.notify());
        let asked_prs: Vec<Reviewed> = event
            .requested
            .iter()
            .filter(|pr| {
                requested
                    .iter()
                    .any(|card| card.url.as_deref() == Some(&pr.url))
            })
            .map(|pr| Reviewed {
                url: pr.url.clone(),
                number: pr.number,
                repository: pr.repository.clone(),
                title: pr.title.clone(),
                author: pr.author.clone(),
                ..Reviewed::default()
            })
            .collect();
        self.review_pull_requests(Trigger::ReviewRequested, &asked_prs, cx);
        for issue in &issues {
            for agent in self.watching(Trigger::IssueAssigned, cx) {
                self.ask_about_issue(agent, issue, cx);
            }
        }
        for task in &stalled {
            for agent in self.watching(Trigger::TaskStalled, cx) {
                self.ask_about_stalled(agent, task, cx);
            }
        }
    }

    fn run_for(
        &self,
        agent: &AgentRecord,
        root: &str,
        cx: &App,
    ) -> Option<crate::data::pr_review::AgentRun> {
        let composer = self.conversation.read(cx).composer.read(cx);
        let providers: std::sync::Arc<Vec<ProviderDescriptor>> =
            composer.catalog.clone().unwrap_or_default();
        let pick = (
            composer.session.provider_id.clone(),
            composer.session.model.clone(),
        );
        review_run(agent, &providers, &pick, root, String::new())
    }

    /// Whether `trigger` may wake `agent` for `target` in `project`; records
    /// the run when it may.
    fn allow(
        &self,
        agent: &AgentRecord,
        trigger: Trigger,
        target: &str,
        project: Option<&str>,
        cx: &mut App,
    ) -> bool {
        // Outside the agent's projects nothing wakes it.
        if project.is_some_and(|project| !agent.works_in(project)) {
            return false;
        }
        let now = Utc::now();
        if cx
            .global::<AgentTriggers>()
            .check(&agent.id, trigger, target, now)
            .is_err()
        {
            return false;
        }
        cx.update_global::<AgentTriggers, _>(|triggers, _| {
            triggers.record(&agent.id, trigger, target, now)
        });
        true
    }

    fn ask_about_issue(&mut self, agent: AgentRecord, issue: &CheckIn, cx: &mut Context<Self>) {
        let target = issue.url.clone().unwrap_or_else(|| issue.key.clone());
        if !self.allow(
            &agent,
            Trigger::IssueAssigned,
            &target,
            issue.project_id.as_deref(),
            cx,
        ) {
            return;
        }
        let Some(run) = self.run_for(&agent, "", cx) else {
            return;
        };
        let existing = home_session(&agent);
        let home_task = self
            .sidebar
            .read(cx)
            .agent_page(&agent.id)
            .and_then(|data| data.home_task);
        let title = cx
            .t_with(
                "agents.conversation.taskName",
                &Args::default().with("agent", agent.name.clone()),
            )
            .to_string();
        let (issue, prompt) = (issue.clone(), issue_prompt(issue));
        cx.spawn(async move |_, cx| {
            let name = agent.name.clone();
            let done = cx
                .background_executor()
                .spawn(async move {
                    let api = DesktopApi::load().ok_or_else(|| "DCC is not running".to_string())?;
                    let session = match existing {
                        Some(session) => session,
                        None => {
                            api.start_agent_conversation(&ConversationStart {
                                agent_id: &agent.id,
                                agent_name: &agent.name,
                                home_task: home_task.as_deref(),
                                title: &title,
                                provider: &run.provider_id,
                                model: run.model.as_deref(),
                                runtime: run.provider_runtime.clone(),
                                // Events wake an agent in the Geral room.
                                room: None,
                            })
                            .map_err(|error| error.to_string())?
                            .1
                        }
                    };
                    let options = TurnOptions {
                        provider_id: Some(run.provider_id.clone()),
                        model: run.model.clone(),
                        effort: run.effort.clone(),
                        approval_policy: Some("ask".into()),
                        plan_mode: false,
                        fast_mode: false,
                        tool_instructions: None,
                        provider_runtime: run.provider_runtime.clone(),
                    };
                    api.send_turn(&session, &prompt, None, &options)
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|cx| announce(Trigger::IssueAssigned, &name, &issue, &done, cx));
        })
        .detach();
    }

    fn ask_about_stalled(&mut self, agent: AgentRecord, task: &CheckIn, cx: &mut Context<Self>) {
        let (Some(workspace), Some(project)) = (task.workspace_id.clone(), task.project_id.clone())
        else {
            return;
        };
        if !self.allow(&agent, Trigger::TaskStalled, &workspace, Some(&project), cx) {
            return;
        }
        let path = self
            .sidebar
            .read(cx)
            .tasks()
            .iter()
            .find(|candidate| candidate.id == workspace)
            .map(|candidate| {
                candidate
                    .worktree_path
                    .clone()
                    .unwrap_or_else(|| candidate.root_path.clone())
            })
            .unwrap_or_default();
        let Some(run) = self.run_for(&agent, &path, cx) else {
            return;
        };
        let ask = TaskAsk::new(&agent, &workspace, &project, run);
        let (task, prompt, name) = (task.clone(), stalled_prompt(task), agent.name.clone());
        cx.spawn(async move |_, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    let api = DesktopApi::load().ok_or_else(|| "DCC is not running".to_string())?;
                    ask.ask(&api, &prompt)
                })
                .await;
            let _ = cx.update(|cx| announce(Trigger::TaskStalled, &name, &task, &done, cx));
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompts_say_what_was_found_and_keep_the_person_in_charge() {
        let issue = asked(
            CheckInKind::IssueAssigned,
            &crate::data::pr_hub::Asked {
                url: "https://github.com/acme/app/issues/4".into(),
                number: 4,
                repository: "acme/app".into(),
                title: "Bug no frete".into(),
                ..Default::default()
            },
        );
        let prompt = issue_prompt(&issue);
        assert!(prompt.starts_with("[DCC] An issue was assigned to the person: acme/app #4"));
        assert!(prompt.contains("dcc_open_task") && prompt.contains("as the person set for you"));
        let stalled = stalled_prompt(&issue);
        assert!(stalled.contains("read-only") && stalled.contains("20 minutes"));
    }
}
