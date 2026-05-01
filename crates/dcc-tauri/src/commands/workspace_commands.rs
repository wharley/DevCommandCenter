use crate::commands::common::CmdResult;

pub async fn prepare_workspace_from_repo() -> CmdResult<String> {
	Ok("phase_0a_prepare_workspace".to_string())
}

pub async fn finalize_workspace_from_repo() -> CmdResult<String> {
	Ok("phase_0a_finalize_workspace".to_string())
}
