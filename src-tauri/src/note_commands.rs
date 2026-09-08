use dcc_infra::notes::{CreateProjectNote, ProjectNote, UpdateProjectNote};
use dcc_tauri::state::SessionCommandState;
use tauri::State;

#[tauri::command]
pub fn list_project_notes(
    state: State<'_, SessionCommandState>,
) -> Result<Vec<ProjectNote>, String> {
    state.list_project_notes()
}
#[tauri::command]
pub fn create_project_note(
    state: State<'_, SessionCommandState>,
    input: CreateProjectNote,
) -> Result<ProjectNote, String> {
    state.create_project_note(input)
}
#[tauri::command]
pub fn update_project_note(
    state: State<'_, SessionCommandState>,
    input: UpdateProjectNote,
) -> Result<ProjectNote, String> {
    state.update_project_note(input)
}
#[tauri::command]
pub fn delete_project_notes(
    state: State<'_, SessionCommandState>,
    ids: Vec<String>,
) -> Result<(), String> {
    state.delete_project_notes(&ids)
}
