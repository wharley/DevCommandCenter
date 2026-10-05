use tauri::State;

use dcc_core::domain::idea::Idea;
use dcc_tauri::{
    commands::research_commands::{
        self as research_command_impl, IdeaRootInput, PublishIdeaInput, PublishedIdea,
    },
    state::WorkspaceCommandState,
};

#[tauri::command]
pub async fn research_create_idea(state: State<'_, WorkspaceCommandState>) -> Result<Idea, String> {
    research_command_impl::research_create_idea(state).await
}

#[tauri::command]
pub async fn research_list_ideas(
    state: State<'_, WorkspaceCommandState>,
) -> Result<Vec<Idea>, String> {
    research_command_impl::research_list_ideas(state).await
}

#[tauri::command]
pub async fn research_publish_idea(
    state: State<'_, WorkspaceCommandState>,
    input: PublishIdeaInput,
) -> Result<PublishedIdea, String> {
    research_command_impl::research_publish_idea(state, input).await
}

#[tauri::command]
pub async fn research_discard_idea(
    state: State<'_, WorkspaceCommandState>,
    input: IdeaRootInput,
) -> Result<(), String> {
    research_command_impl::research_discard_idea(state, input).await
}
