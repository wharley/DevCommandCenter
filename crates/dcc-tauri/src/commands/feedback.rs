//! Global DCC feedback, in `dcc_infra::feedback`; the desktop's account
//! comes from its workspace state.
pub use dcc_infra::feedback::{
    create, list, FeedbackContext, FeedbackInput, FeedbackIssue, FeedbackPage,
};

use crate::state::WorkspaceCommandState;

pub fn context(state: &WorkspaceCommandState, version: String) -> Result<FeedbackContext, String> {
    dcc_infra::feedback::context(&state.db_path, version)
}
