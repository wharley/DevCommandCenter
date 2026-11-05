//! The review's delivery header (`GitSectionHeader`): the branch, the pull
//! request's link, and the button that takes the task's next step —
//! commit and push (after a preview of the message), push, create or open
//! the pull request, merge it — plus the bar to bring the base in when the
//! branch is behind. The steps run in the desktop (`workspace_delivery`);
//! the pull request's state is read here, from the forge's CLI, with the
//! status.

mod actions;
mod conflicts;
mod dialogs;
mod dock;
#[cfg(test)]
mod dock_tests;
mod draft;
#[cfg(test)]
mod draft_tests;
mod header;
mod overlay;
mod pull;
mod request;
#[cfg(test)]
mod tests;
mod toolbar;

pub use dock::DockControls;
pub(crate) use header::header;
pub use overlay::DeliveryOverlay;
pub(crate) use toolbar::toolbar;

use dcc_core::domain::commit_mode::{CommitButtonStatus, CommitMode, resolve_commit_mode};
use dcc_core::domain::workspace::Workspace;
use dcc_core::domain::workspace_git::WorkspacePrStatusOutput;
use dcc_core::domain::workspace_recap::RecapDeliveryPolicy;
use gpui::{Context, Entity};

use super::CodeColumn;
use crate::ui::text_area::TextArea;

/// How often the status loop reads the pull request again.
const PR_EVERY: std::time::Duration = std::time::Duration::from_secs(30);
/// A commit's or pull request's text scrolls inside its field past these
/// heights, so a long ✨ draft never stretches the dialog.
pub(super) const BODY_MAX_HEIGHT: f32 = 200.0;
pub(super) const SUBJECT_MAX_HEIGHT: f32 = 96.0;

/// A step that asks first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    Merge,
    SyncBase,
    /// "Concluir merge" with the conflicts resolved outside the resolver.
    CompleteMerge,
}

/// Which header opened the commit preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewOrigin {
    /// The review's "Commitar e enviar".
    Review,
    /// The conversation's menu: a commit, pushed or not.
    Dock { push: bool },
}

/// "Revisar mensagem do commit": the staged snapshot's message, editable.
pub(crate) struct Preview {
    pub origin: PreviewOrigin,
    pub subject: Entity<TextArea>,
    pub body: Entity<TextArea>,
    pub staged_files: u32,
    pub fingerprint: String,
}

#[derive(Default)]
pub(crate) struct Delivery {
    /// The task whose checkout this is (its id runs the steps; its state and
    /// worktree decide the completion after a merge).
    pub task: Option<Workspace>,
    pub pr: Option<WorkspacePrStatusOutput>,
    /// When and for which branch the pull request was last read.
    pub pr_read: Option<(std::time::Instant, Option<String>)>,
    /// A step is running (`isGitActionInProgress`).
    pub busy: bool,
    pub syncing: bool,
    /// The status is being read again.
    pub reading: bool,
    pub button: Option<CommitButtonStatus>,
    pub confirm: Option<Confirm>,
    pub preview: Option<Preview>,
    /// The conversation menu's preview is being prepared (it pushes or not).
    pub preparing: Option<bool>,
    /// "Criar PR" from the conversation's menu.
    pub request: Option<request::RequestDialog>,
    /// The ✨ writing a field of either dialog.
    pub drafting: Option<draft::Drafting>,
    /// The task completed after its merge, once.
    pub completed: Option<String>,
    /// `.dcc.toml`'s delivery policy and its before-merge checks, read with
    /// the pull request (`workspaceProjectAutomationConfig`).
    pub policy: Option<(RecapDeliveryPolicy, i64)>,
    /// The merge's conflict state, while git reports one (every 5s).
    pub conflicts: Option<dcc_core::domain::merge_conflict::WorkspaceGitConflictStateOutput>,
    pub _conflicts: Option<gpui::Task<()>>,
    /// When each task's own branch was last fetched.
    pub upstream: pull::UpstreamFetches,
}

impl Delivery {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn button(&self) -> CommitButtonStatus {
        self.button.unwrap_or(CommitButtonStatus::Idle)
    }

    /// "PR", or "MR" on GitLab.
    pub fn request_label(&self) -> &'static str {
        if self.pr.as_ref().and_then(|pr| pr.provider.as_deref()) == Some("gitlab") {
            "MR"
        } else {
            "PR"
        }
    }
}

impl CodeColumn {
    /// The branch the header names (`gitBranch ?? workspaceBranch`).
    pub(crate) fn current_branch(&self) -> Option<String> {
        self.status
            .as_ref()
            .and_then(|status| status.current_branch.clone())
            .filter(|branch| !branch.is_empty() && branch != "HEAD")
    }

    /// The task's own branch (`workspace.branch`: the imported source's
    /// head, else its base).
    pub(crate) fn task_branch(&self) -> Option<String> {
        let task = self.delivery.task.as_ref()?;
        Some(
            task.source
                .as_ref()
                .map(|source| source.head_branch.clone())
                .unwrap_or_else(|| task.base_branch.clone()),
        )
        .filter(|branch| !branch.is_empty())
    }

    pub(crate) fn commit_mode(&self) -> CommitMode {
        let branch = self
            .current_branch()
            .or_else(|| self.task_branch())
            .unwrap_or_default();
        resolve_commit_mode(
            &branch,
            self.status.as_ref(),
            self.delivery.pr.as_ref(),
            self.conflicts_ready(),
        )
    }

    /// The task and its pull request, the column shown or not: the
    /// conversation header's delivery needs them. With the status they are
    /// read every `PR_EVERY` (each read runs `gh`/`glab` several times), at
    /// once when the branch changes or a step asks (`force`).
    pub(crate) fn read_pull_request(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let (db_path, branch) = (self.db_path.clone(), self.current_branch());
        let fresh = self
            .delivery
            .pr_read
            .as_ref()
            .is_some_and(|(at, read)| at.elapsed() < PR_EVERY && *read == branch);
        if !force && fresh {
            return;
        }
        self.delivery.pr_read = Some((std::time::Instant::now(), branch.clone()));
        self._pull_request = Some(cx.spawn(async move |this, cx| {
            let path = root.clone();
            let (task, pr, policy) = cx
                .background_executor()
                // Straight on the background executor: the forge's context
                // blocks on its own futures inside, which a `block_on`
                // around it would refuse.
                .spawn(async move {
                    let task = match dcc_infra::db::SqliteWorkspaceRepo::open(&db_path) {
                        Ok(repo) => dcc_infra::workspace_git::find_workspace_by_root(&repo, &path)
                            .await
                            .ok()
                            .flatten(),
                        Err(_) => None,
                    };
                    let pr = dcc_infra::workspace_git::workspace_pr_status(
                        &db_path, &path, branch, None,
                    )
                    .await
                    .ok();
                    (task, pr, delivery_policy(&path))
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.root.as_deref() != Some(root.as_str()) {
                    return;
                }
                let task_changed = column.delivery.task.as_ref().map(|task| &task.id)
                    != task.as_ref().map(|task| &task.id);
                column.delivery.task = task;
                column.delivery.policy = policy;
                if let Some(pr) = pr {
                    column.delivery.pr = Some(pr);
                }
                column.complete_if_merged(cx);
                if task_changed {
                    column.read_last_turn(cx);
                    if !cfg!(test) {
                        column.fetch_upstream(cx);
                    }
                }
                cx.notify();
            });
        }));
    }
}

/// The project's delivery rules; none without a readable `.dcc.toml`.
fn delivery_policy(root: &str) -> Option<(RecapDeliveryPolicy, i64)> {
    let config =
        dcc_infra::git::read_workspace_automation_config(std::path::Path::new(root)).ok()??;
    let policy = config.delivery_policy;
    Some((
        RecapDeliveryPolicy {
            minimum_approvals: i64::from(policy.minimum_approvals),
            require_pipeline: policy.require_pipeline,
            require_resolved_discussions: policy.require_resolved_discussions,
            require_current_base: policy.require_current_base,
            require_before_merge_checks: policy.require_before_merge_checks,
        },
        config.before_merge.len() as i64,
    ))
}
