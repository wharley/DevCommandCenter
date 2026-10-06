use tauri::State;

use dcc_core::domain::agent::{
    ChroniclerPresetText, ResearcherPresetText, ResidentAgent, ReviewerPresetText,
};
use dcc_infra::db::ActivityRecap;
use dcc_tauri::{
    commands::agent_commands::{
        self as agent_command_impl, ActivityRecapInput, AgentsOverview, BindSessionAgentInput,
        SaveAgentInput,
    },
    state::SessionCommandState,
};

#[tauri::command]
pub async fn agents_overview(
    state: State<'_, SessionCommandState>,
    reviewer: ReviewerPresetText,
    researcher: Option<ResearcherPresetText>,
    chronicler: Option<ChroniclerPresetText>,
) -> Result<AgentsOverview, String> {
    agent_command_impl::agents_overview(state, reviewer, researcher, chronicler).await
}

#[tauri::command]
pub async fn agents_activity_recap(
    state: State<'_, SessionCommandState>,
    input: ActivityRecapInput,
) -> Result<ActivityRecap, String> {
    agent_command_impl::agents_activity_recap(state, input).await
}

#[tauri::command]
pub async fn agents_save(
    state: State<'_, SessionCommandState>,
    input: SaveAgentInput,
) -> Result<ResidentAgent, String> {
    agent_command_impl::agents_save(state, input).await
}

#[tauri::command]
pub async fn agents_bind_session(
    state: State<'_, SessionCommandState>,
    input: BindSessionAgentInput,
) -> Result<(), String> {
    agent_command_impl::agents_bind_session(state, input).await
}
