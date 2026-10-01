use tauri::State;

use dcc_core::domain::agent::{ResidentAgent, ReviewerPresetText};
use dcc_tauri::{
    commands::agent_commands::{
        self as agent_command_impl, AgentsOverview, BindSessionAgentInput, SaveAgentInput,
    },
    state::SessionCommandState,
};

#[tauri::command]
pub async fn agents_overview(
    state: State<'_, SessionCommandState>,
    reviewer: ReviewerPresetText,
) -> Result<AgentsOverview, String> {
    agent_command_impl::agents_overview(state, reviewer).await
}

#[tauri::command]
pub async fn agents_save(
    state: State<'_, SessionCommandState>,
    input: SaveAgentInput,
) -> Result<ResidentAgent, String> {
    agent_command_impl::agents_save(state, input).await
}

#[tauri::command]
pub async fn agents_delete(
    state: State<'_, SessionCommandState>,
    agent_id: String,
) -> Result<bool, String> {
    agent_command_impl::agents_delete(state, agent_id).await
}

#[tauri::command]
pub async fn agents_bind_session(
    state: State<'_, SessionCommandState>,
    input: BindSessionAgentInput,
) -> Result<(), String> {
    agent_command_impl::agents_bind_session(state, input).await
}
