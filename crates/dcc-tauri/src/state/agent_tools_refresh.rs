//! A changed access reaches the agent's conversations: the tools a provider
//! lists are fixed when it attaches to a session, so a conversation that
//! stayed attached kept the old list ("Ler e escrever" on the Browser did
//! not show dcc_browser_open). An idle one is let go — the provider process
//! stops, nothing is written to the conversation — and attaches again,
//! resuming the same thread with the new tools, on the person's next
//! message. One at work keeps its tools until it is let go another way.

use super::*;

impl SessionCommandState {
    /// Lets go of `agent_id`'s idle conversations; how many were.
    pub async fn refresh_agent_tools(&self, agent_id: &str) -> Result<usize> {
        let sessions: Vec<SessionId> = self
            .list_agent_session_bindings()?
            .into_iter()
            .filter(|binding| binding.agent_id == agent_id)
            .map(|binding| SessionId(binding.session_id))
            .collect();
        let mut released = 0;
        for session_id in sessions {
            if self.provider_binding(&session_id)?.is_none() {
                continue;
            }
            // Under the session's lock no turn starts meanwhile.
            let _transition = self.acquire_provider_transition(&session_id).await?;
            let Some(binding) = self.provider_binding(&session_id)? else {
                continue;
            };
            if binding.current_turn_id.lock().await.is_some() {
                continue;
            }
            self.cancel_provider_session(&session_id).await?;
            released += 1;
        }
        Ok(released)
    }
}
