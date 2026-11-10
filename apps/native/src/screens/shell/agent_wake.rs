//! Waking an agent in its own conversation with a request DCC writes (a
//! routine's, a source's news): the conversation it has, or a new one in its
//! home, on its own provider (else the composer's), with approval "ask".
//! Shared by the routines' and the sources' clocks.

use gpui::App;

use super::Shell;
use super::check_in::home_session;
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
        Some(HomeSend {
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            existing: home_session(agent),
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
                // Events wake an agent in the Geral room.
                room: None,
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
