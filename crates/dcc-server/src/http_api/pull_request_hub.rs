//! The "Pull Requests" hub for the native app (`pull-requests-hub.tsx`): the
//! open pull requests across the projects, one's summary or code, a
//! comment, the merge, a review with inline comments, a conversation's reply
//! and resolution, and the Reviewer's read-only pass over the patches. They
//! run the desktop's own commands. A pull request is addressed by its
//! repository's root, which must be a registered project (or a task's
//! checkout); the relay never forwards `/api/v1/pull-request-hub/`.
use super::*;
use dcc_tauri::commands::forge_commands::{
    self, PullRequestHubCommentInput, PullRequestHubDetailInput, PullRequestHubListInput,
    PullRequestHubMergeInput, PullRequestHubSubmitReviewInput, PullRequestHubThreadReplyInput,
    PullRequestHubThreadResolveInput,
};
use dcc_tauri::commands::session_commands::{self, RunPullRequestReviewAgentInput};

pub(super) fn routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/pull-request-hub/list", post(list))
        .route("/api/v1/pull-request-hub/reviewed", post(reviewed))
        .route("/api/v1/pull-request-hub/detail", post(detail))
        .route("/api/v1/pull-request-hub/comment", post(comment))
        .route("/api/v1/pull-request-hub/merge", post(merge))
        .route("/api/v1/pull-request-hub/action", post(action))
        .route("/api/v1/pull-request-hub/check-log", post(check_log))
        .route("/api/v1/pull-request-hub/check-rerun", post(check_rerun))
        .route(
            "/api/v1/pull-request-hub/submit-review",
            post(submit_review),
        )
        .route("/api/v1/pull-request-hub/reply-thread", post(reply_thread))
        .route(
            "/api/v1/pull-request-hub/resolve-thread",
            post(resolve_thread),
        )
        .route("/api/v1/pull-request-hub/review-agent", post(review_agent))
}

fn reply<T: serde::Serialize>(result: Result<T, String>) -> Result<Json<Value>, HttpApiError> {
    result
        .map(|output| Json(json!(output)))
        .map_err(HttpApiError::bad_request)
}

/// The pull request's repository: a project DCC knows, or a task's checkout.
pub(super) async fn known_repository(
    backend: &HttpBackend,
    root: &str,
) -> Result<(), HttpApiError> {
    let root = root.trim();
    let repo = dcc_infra::db::SqliteWorkspaceRepo::open(&backend.workspace.db_path)
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let registered = dcc_core::ports::RepositoryRepo::list_repositories(&repo)
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .iter()
        .any(|repository| repository.root_path.trim() == root);
    if registered {
        return Ok(());
    }
    git_changes::task_checkout(backend, root)
        .await
        .map_err(|_| HttpApiError::bad_request("not a project's repository"))
}

async fn list(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PullRequestHubListInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(forge_commands::pull_request_hub_list_with_state(&backend.workspace, input).await)
}

/// The open pull requests the person reviewed, with what came since: one
/// search, for the app's count beside "Pull Requests".
async fn reviewed(State(backend): State<Arc<HttpBackend>>) -> Result<Json<Value>, HttpApiError> {
    reply(
        dcc_tauri::commands::pr_hub_since::pull_request_hub_reviewed_with_state(&backend.workspace)
            .await,
    )
}

async fn detail(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PullRequestHubDetailInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.repository_root).await?;
    reply(forge_commands::pull_request_hub_detail_with_state(&backend.workspace, input).await)
}

async fn comment(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PullRequestHubCommentInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.repository_root).await?;
    reply(forge_commands::pull_request_hub_comment_with_state(&backend.workspace, input).await)
}

async fn check_log(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<dcc_tauri::commands::pr_hub_checks::PullRequestHubCheckInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.repository_root).await?;
    reply(
        dcc_tauri::commands::pr_hub_checks::pull_request_hub_check_log_with_state(
            &backend.workspace,
            input,
        )
        .await,
    )
}

async fn check_rerun(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<dcc_tauri::commands::pr_hub_checks::PullRequestHubCheckInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.repository_root).await?;
    reply(
        dcc_tauri::commands::pr_hub_checks::pull_request_hub_check_rerun_with_state(
            &backend.workspace,
            input,
        )
        .await
        .map(|()| json!({ "ok": true })),
    )
}

async fn action(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<dcc_tauri::commands::pr_hub_merge_extras::PullRequestHubActionInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.repository_root).await?;
    reply(
        dcc_tauri::commands::pr_hub_merge_extras::pull_request_hub_action_with_state(
            &backend.workspace,
            input,
        )
        .await,
    )
}

async fn merge(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PullRequestHubMergeInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.repository_root).await?;
    reply(forge_commands::pull_request_hub_merge_with_state(&backend.workspace, input).await)
}

async fn submit_review(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PullRequestHubSubmitReviewInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.repository_root).await?;
    reply(
        forge_commands::pull_request_hub_submit_review_with_state(&backend.workspace, input).await,
    )
}

async fn reply_thread(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PullRequestHubThreadReplyInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.repository_root).await?;
    reply(forge_commands::pull_request_hub_reply_thread_with_state(&backend.workspace, input).await)
}

async fn resolve_thread(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PullRequestHubThreadResolveInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.repository_root).await?;
    reply(
        forge_commands::pull_request_hub_resolve_thread_with_state(&backend.workspace, input).await,
    )
}

/// The Reviewer's pass: a read-only, ephemeral turn in the repository.
async fn review_agent(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<RunPullRequestReviewAgentInput>,
) -> Result<Json<Value>, HttpApiError> {
    known_repository(&backend, &input.working_directory).await?;
    reply(
        session_commands::run_pull_request_review_agent_with_state(&backend.sessions, input).await,
    )
}
