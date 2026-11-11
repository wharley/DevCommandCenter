//! The live part's buttons: "Parar" stops a conversation's turn (the
//! composer's stop), "Tentar de novo" sends a stopped turn's prompt again to
//! that same conversation, on the agent's own provider, model and mode as
//! the room sends (`agent_turn`, shared with `deliver`). Both call the
//! desktop off the UI thread and say a failure in a toast.

use gpui::{Context, SharedString};

use super::execution::agent_execution;
use super::room::TeamRoom;
use crate::data::agents::AgentRecord;
use crate::data::desktop_api::TurnOptions;
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

impl TeamRoom {
    /// What a message to `agent` runs on: its provider, its model and the
    /// turn's options (its effort, the mode set for its conversations, its
    /// own account on its provider).
    pub(super) fn agent_turn(
        &self,
        agent: &AgentRecord,
        cx: &gpui::App,
    ) -> Result<(String, Option<String>, TurnOptions), SharedString> {
        let (provider, model) = agent_execution(agent, self.composer.read(cx), cx)?;
        let effort = dcc_core::domain::agent_run::agent_run_effort(
            agent.provider_id.as_deref(),
            agent.effort.as_deref(),
        );
        // A message the person sends runs in the mode set for the agent.
        let mode = super::editor_task_mode::wire_mode(agent.tool_access.conversation_approval);
        // Its own account on its own provider (`provider_runtime::for_agent`).
        let runtime = (!cfg!(test))
            .then(|| crate::data::provider_runtime::for_agent(agent, &provider))
            .flatten();
        let options = TurnOptions {
            provider_id: Some(provider.clone()),
            model: model.clone(),
            effort,
            approval_policy: Some(mode),
            plan_mode: false,
            fast_mode: false,
            tool_instructions: None,
            provider_runtime: runtime,
        };
        Ok((provider, model, options))
    }

    /// "Parar": the conversation's running turn stops.
    pub(super) fn stop_turn(&mut self, session_id: String, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        if !self.live.stopping.insert(session_id.clone()) {
            return;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let stopped = cx
                .background_executor()
                .spawn({
                    let session_id = session_id.clone();
                    async move { api.abort(&session_id) }
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                if let Err(error) = stopped {
                    room.live.stopping.remove(&session_id);
                    Toaster::error(
                        cx.t_with(
                            "agents.room.live.stopFailed",
                            &Args::default().with("error", error.to_string()),
                        ),
                        cx,
                    );
                } else if !cfg!(test) {
                    room.sidebar
                        .update(cx, |sidebar, cx| sidebar.reload_now(cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Tentar de novo": `prompt` again in `session_id`, as a retry of
    /// `turn_id` (the conversation reopened first when the stop closed it).
    pub(super) fn retry_turn(
        &mut self,
        agent: AgentRecord,
        session_id: String,
        turn_id: String,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        let (_, _, options) = match self.agent_turn(&agent, cx) {
            Ok(turn) => turn,
            Err(reason) => {
                Toaster::error(reason, cx);
                return;
            }
        };
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        if !self.live.retried.insert(turn_id.clone()) {
            return;
        }
        // The new turn shows at the bottom.
        self.list.follow();
        cx.notify();
        let turn = turn_id.clone();
        cx.spawn(async move |this, cx| {
            let sent = cx
                .background_executor()
                .spawn(async move {
                    // Not closed by the stop: nothing to reopen.
                    let _ = api.resume(&session_id);
                    api.send_turn(&session_id, &prompt, Some(&turn_id), &options)
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                match sent {
                    Err(error) => {
                        room.live.retried.remove(&turn);
                        Toaster::error(
                            cx.t_with(
                                "agents.room.live.retryFailed",
                                &Args::default().with("error", error.to_string()),
                            ),
                            cx,
                        );
                    }
                    Ok(()) if !cfg!(test) => {
                        room.sidebar
                            .update(cx, |sidebar, cx| sidebar.reload_now(cx));
                    }
                    Ok(()) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
}
