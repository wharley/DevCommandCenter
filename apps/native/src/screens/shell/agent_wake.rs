//! Waking an agent in its own conversation with a request DCC writes (a
//! routine's, a source's news): its conversation in the room the request
//! belongs to (a source's room, else Geral), or a new one there, on its own
//! provider (else the composer's), with approval "ask". Shared by the
//! routines' and the sources' clocks.

use gpui::App;

use super::Shell;
use super::review_trigger::review_run;
use crate::data::agent_conversation::ConversationStart;
use crate::data::agents::AgentRecord;
use crate::data::desktop_api::{DesktopApi, TurnOptions};
use crate::data::pr_review::AgentRun;
use crate::i18n::{Args, Translate};

/// Everything a send needs, taken on the UI thread.
pub(super) struct HomeSend {
    agent_id: String,
    agent_name: String,
    existing: Option<String>,
    /// The project whose room it wakes in; none: Geral.
    room: Option<String>,
    home_task: Option<String>,
    title: String,
    run: AgentRun,
    pub prompt: String,
}

impl Shell {
    /// What sending `prompt` to `agent` needs; none when no provider can run it.
    pub(super) fn home_send(
        &self,
        agent: &AgentRecord,
        room: Option<&str>,
        prompt: String,
        cx: &App,
    ) -> Option<HomeSend> {
        let composer = self.conversation.read(cx).composer.read(cx);
        let providers = composer.catalog.clone().unwrap_or_default();
        let pick = (
            composer.session.provider_id.clone(),
            composer.session.model.clone(),
        );
        let run = review_run(agent, &providers, &pick, "", String::new())?;
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
        // A project's room names its conversation after the project too.
        let project = room.and_then(|room| {
            self.sidebar
                .read(cx)
                .projects()
                .iter()
                .find(|project| project.project_id == room)
                .map(|project| project.display_name.clone())
        });
        let title = match project {
            Some(project) => format!("{title} · {project}"),
            None => title,
        };
        let home = dcc_core::domain::agent_conversation::home_project_id(&agent.id);
        let existing = agent
            .sessions
            .iter()
            .filter(|session| session.project_id == home && session.room.as_deref() == room)
            .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
            .map(|session| session.session_id.clone());
        Some(HomeSend {
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            existing,
            room: room.map(str::to_string),
            home_task,
            title,
            run,
            prompt,
        })
    }
}

/// Sends it; blocking, off the UI thread.
pub(super) fn send_home(api: &DesktopApi, send: HomeSend) -> Result<(), String> {
    let session = match send.existing {
        Some(session) => session,
        None => {
            api.start_agent_conversation(&ConversationStart {
                agent_id: &send.agent_id,
                agent_name: &send.agent_name,
                home_task: send.home_task.as_deref(),
                title: &send.title,
                provider: &send.run.provider_id,
                model: send.run.model.as_deref(),
                runtime: send.run.provider_runtime.clone(),
                room: send.room.as_deref(),
            })
            .map_err(|error| error.to_string())?
            .1
        }
    };
    let options = TurnOptions {
        provider_id: Some(send.run.provider_id.clone()),
        model: send.run.model.clone(),
        effort: send.run.effort.clone(),
        approval_policy: Some("ask".into()),
        plan_mode: false,
        fast_mode: false,
        tool_instructions: None,
        provider_runtime: send.run.provider_runtime.clone(),
    };
    api.send_turn(&session, &send.prompt, None, &options)
        .map_err(|error| error.to_string())
}
