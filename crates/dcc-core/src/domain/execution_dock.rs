//! The delivery controls over a task's conversation (`ExecutionDock.actions`
//! and `ExecutionDock.logic`): what the changes button counts and which
//! steps the delivery menu offers, from the commit mode the header already
//! resolved. A presentation map: git state is not read a second time.

use super::commit_mode::CommitMode;

/// What a step runs: a commit mode, a commit alone, bringing the base in,
/// or bringing in what someone else pushed to the same branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockRunMode {
    Mode(CommitMode),
    Commit,
    SyncBase,
    Pull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockActionId {
    Primary,
    Commit,
    CommitAndPush,
    Push,
    SyncBase,
    Pull,
    CreatePr,
    CreateDraftPr,
    OpenPr,
    Merge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockActionKind {
    Execute,
    /// Opens the changes (fix CI, conflicts).
    Review,
    /// Opens the pull request dialog.
    CreateRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockAction {
    pub id: DockActionId,
    pub mode: Option<DockRunMode>,
    pub kind: DockActionKind,
    pub disabled: bool,
    pub primary: bool,
}

/// `ExecutionDockChangeSummary`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DockChangeSummary {
    /// Files changed in the working tree (staged or not).
    pub files: u32,
    pub additions: u32,
    pub deletions: u32,
    /// Files changed by the branch's commits against its base.
    pub branch_files: u32,
    pub branch_additions: u32,
    pub branch_deletions: u32,
    pub ahead_of_remote_count: u32,
    /// Commits on the branch's remote copy this checkout does not have
    /// (someone else pushed), as of its last fetch.
    pub behind_of_remote_count: u32,
    /// False when the repository has no remote; `None` counts as present.
    pub has_remote: Option<bool>,
    pub pull_request_state: Option<String>,
    pub pull_request_number: Option<u32>,
}

impl DockChangeSummary {
    fn request_state(&self) -> Option<String> {
        self.pull_request_state
            .as_deref()
            .map(str::trim)
            .filter(|state| !state.is_empty())
            .map(str::to_lowercase)
    }
}

/// `ExecutionDockGitState`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DockGitState {
    Loading,
    #[default]
    Ready,
    Error,
}

/// `ExecutionDockStatus`
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockStatus {
    Loading,
    Error,
    None,
    Merged {
        pull_request_number: Option<u32>,
    },
    Local {
        files: u32,
        additions: u32,
        deletions: u32,
    },
    Branch {
        files: u32,
        additions: u32,
        deletions: u32,
    },
    LocalAndBranch {
        local_files: u32,
        local_additions: u32,
        local_deletions: u32,
        branch_files: u32,
    },
    Ahead {
        commits: u32,
    },
}

impl DockStatus {
    /// The +/− the changes button shows (`localAdditions`/`localDeletions`).
    pub fn counts(&self) -> (u32, u32) {
        match *self {
            Self::Local {
                additions,
                deletions,
                ..
            }
            | Self::Branch {
                additions,
                deletions,
                ..
            } => (additions, deletions),
            Self::LocalAndBranch {
                local_additions,
                local_deletions,
                ..
            } => (local_additions, local_deletions),
            _ => (0, 0),
        }
    }
}

/// `resolveExecutionDockStatus`
pub fn resolve_dock_status(summary: Option<&DockChangeSummary>, git: DockGitState) -> DockStatus {
    match git {
        DockGitState::Loading => return DockStatus::Loading,
        DockGitState::Error => return DockStatus::Error,
        DockGitState::Ready => {}
    }
    let Some(summary) = summary else {
        return DockStatus::None;
    };
    let local = summary.files > 0;
    let branch = summary.branch_files > 0;
    // Uncommitted work is never hidden behind a delivery status.
    if local && branch {
        return DockStatus::LocalAndBranch {
            local_files: summary.files,
            local_additions: summary.additions,
            local_deletions: summary.deletions,
            branch_files: summary.branch_files,
        };
    }
    if local {
        return DockStatus::Local {
            files: summary.files,
            additions: summary.additions,
            deletions: summary.deletions,
        };
    }
    if summary.request_state().as_deref() == Some("merged") {
        return DockStatus::Merged {
            pull_request_number: summary.pull_request_number,
        };
    }
    if branch {
        return DockStatus::Branch {
            files: summary.branch_files,
            additions: summary.branch_additions,
            deletions: summary.branch_deletions,
        };
    }
    if summary.ahead_of_remote_count > 0 {
        return DockStatus::Ahead {
            commits: summary.ahead_of_remote_count,
        };
    }
    DockStatus::None
}

/// `ExecutionDockActionContext`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DockActionContext {
    pub mode: Option<CommitMode>,
    pub loading: bool,
    pub multi_project: bool,
    pub has_local_changes: bool,
    pub has_branch_changes: bool,
    pub has_ahead_commits: bool,
    /// Someone else pushed to this branch: pulling comes first.
    pub behind_remote: u32,
    pub has_change_request: bool,
    pub has_open_request: bool,
    /// False when the repository has no remote; `None` counts as present.
    pub has_remote: Option<bool>,
}

impl DockActionContext {
    /// The context `WorkspaceDeliveryControls` builds from the summary.
    pub fn from_summary(
        mode: Option<CommitMode>,
        loading: bool,
        multi_project: bool,
        summary: Option<&DockChangeSummary>,
    ) -> Self {
        let state = summary.and_then(DockChangeSummary::request_state);
        Self {
            mode,
            loading,
            multi_project,
            has_local_changes: summary.is_some_and(|summary| summary.files > 0),
            has_branch_changes: summary.is_some_and(|summary| summary.branch_files > 0),
            has_ahead_commits: summary.is_some_and(|summary| summary.ahead_of_remote_count > 0),
            behind_remote: summary.map_or(0, |summary| summary.behind_of_remote_count),
            has_change_request: state.is_some(),
            has_open_request: state.as_deref() == Some("open"),
            has_remote: summary.and_then(|summary| summary.has_remote),
        }
    }
}

fn action(
    id: DockActionId,
    mode: Option<DockRunMode>,
    kind: DockActionKind,
    disabled: bool,
) -> DockAction {
    DockAction {
        id,
        mode,
        kind,
        disabled,
        primary: false,
    }
}

/// `resolveExecutionDockActions`: the primary step first, then the others
/// without repeating its mode.
pub fn resolve_dock_actions(context: &DockActionContext) -> Vec<DockAction> {
    use DockActionId as Id;
    use DockActionKind as Kind;
    let loading = context.loading;
    let primary = |mode, kind, disabled| DockAction {
        id: Id::Primary,
        mode,
        kind,
        disabled,
        primary: true,
    };
    if context.multi_project {
        return vec![primary(None, Kind::Execute, loading)];
    }
    let no_remote = context.has_remote == Some(false);
    let mode = context.mode;
    // Without a remote, push and pull requests have nowhere to go: delivery
    // stops at a local commit. Conflict work still leads, since it is local.
    let local_only = no_remote
        && !matches!(
            mode,
            Some(CommitMode::ResolveConflicts | CommitMode::CompleteMerge)
        );
    // Someone else pushed to this branch: their commits come in before any
    // push (which would be refused) or merge. Conflict work still leads.
    let pull_first = !local_only
        && context.behind_remote > 0
        && !matches!(
            mode,
            Some(CommitMode::ResolveConflicts | CommitMode::CompleteMerge)
        );
    let first = if pull_first {
        primary(Some(DockRunMode::Pull), Kind::Execute, loading)
    } else if local_only {
        primary(
            Some(DockRunMode::Commit),
            Kind::Execute,
            loading || !context.has_local_changes,
        )
    } else {
        match mode {
            Some(CommitMode::CreatePr) if !context.has_branch_changes => {
                primary(Some(DockRunMode::SyncBase), Kind::Execute, loading)
            }
            Some(CommitMode::CreatePr) => primary(
                Some(DockRunMode::Mode(CommitMode::CreatePr)),
                Kind::CreateRequest,
                loading,
            ),
            Some(CommitMode::Merge) => primary(
                Some(DockRunMode::Mode(CommitMode::Merge)),
                Kind::Execute,
                loading,
            ),
            Some(
                mode @ (CommitMode::Fix | CommitMode::ResolveConflicts | CommitMode::CompleteMerge),
            ) => primary(Some(DockRunMode::Mode(mode)), Kind::Review, loading),
            mode => primary(mode.map(DockRunMode::Mode), Kind::Execute, loading),
        }
    };
    let create = loading || no_remote || context.has_open_request || !context.has_branch_changes;
    let requests = if context.has_change_request {
        [
            action(
                Id::OpenPr,
                Some(DockRunMode::Mode(CommitMode::OpenPr)),
                Kind::Execute,
                loading,
            ),
            action(
                Id::Merge,
                Some(DockRunMode::Mode(CommitMode::Merge)),
                Kind::Execute,
                loading || mode != Some(CommitMode::Merge),
            ),
        ]
    } else {
        let create_pr = Some(DockRunMode::Mode(CommitMode::CreatePr));
        [
            action(Id::CreatePr, create_pr, Kind::CreateRequest, create),
            action(Id::CreateDraftPr, create_pr, Kind::CreateRequest, create),
        ]
    };
    let others = [
        action(
            Id::CommitAndPush,
            Some(DockRunMode::Mode(CommitMode::CommitAndPush)),
            Kind::Execute,
            loading || no_remote,
        ),
        action(
            Id::Commit,
            Some(DockRunMode::Commit),
            Kind::Execute,
            loading || !context.has_local_changes,
        ),
        action(
            Id::Push,
            Some(DockRunMode::Mode(CommitMode::Push)),
            Kind::Execute,
            loading || no_remote || !context.has_ahead_commits,
        ),
        action(
            Id::SyncBase,
            Some(DockRunMode::SyncBase),
            Kind::Execute,
            loading,
        ),
        action(
            Id::Pull,
            Some(DockRunMode::Pull),
            Kind::Execute,
            loading || no_remote,
        ),
    ];
    std::iter::once(first)
        .chain(
            others
                .into_iter()
                .chain(requests)
                .filter(|action| action.mode != first.mode),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(mode: CommitMode) -> DockActionContext {
        DockActionContext {
            mode: Some(mode),
            ..DockActionContext::default()
        }
    }

    #[test]
    fn conflicts_and_fix_route_to_review() {
        let actions = resolve_dock_actions(&DockActionContext {
            has_local_changes: true,
            has_branch_changes: true,
            ..context(CommitMode::ResolveConflicts)
        });
        assert_eq!(actions[0].kind, DockActionKind::Review);
        assert!(actions[0].primary);
    }

    #[test]
    fn what_someone_else_pushed_is_pulled_first() {
        let behind = resolve_dock_actions(&DockActionContext {
            has_local_changes: true,
            behind_remote: 2,
            ..context(CommitMode::CommitAndPush)
        });
        assert_eq!(behind[0].mode, Some(DockRunMode::Pull));
        assert!(behind[0].primary);
        assert_eq!(
            behind
                .iter()
                .filter(|action| action.mode == Some(DockRunMode::Pull))
                .count(),
            1,
            "listed once"
        );
        let current = resolve_dock_actions(&context(CommitMode::CommitAndPush));
        assert_ne!(current[0].mode, Some(DockRunMode::Pull));
        let pull = current
            .iter()
            .find(|action| action.id == DockActionId::Pull)
            .unwrap();
        assert!(!pull.disabled, "still in the menu");
        let conflicts = resolve_dock_actions(&DockActionContext {
            behind_remote: 2,
            ..context(CommitMode::ResolveConflicts)
        });
        assert_eq!(conflicts[0].kind, DockActionKind::Review, "conflicts first");
    }

    #[test]
    fn merge_runs_from_the_menu() {
        let actions = resolve_dock_actions(&DockActionContext {
            has_branch_changes: true,
            has_change_request: true,
            has_open_request: true,
            ..context(CommitMode::Merge)
        });
        assert_eq!(actions[0].mode, Some(DockRunMode::Mode(CommitMode::Merge)));
        assert_eq!(actions[0].kind, DockActionKind::Execute);
    }

    #[test]
    fn create_pr_opens_the_dialog_or_syncs_without_branch_changes() {
        let with_changes = resolve_dock_actions(&DockActionContext {
            has_branch_changes: true,
            ..context(CommitMode::CreatePr)
        });
        assert_eq!(with_changes[0].kind, DockActionKind::CreateRequest);
        let without = resolve_dock_actions(&context(CommitMode::CreatePr));
        assert_eq!(without[0].mode, Some(DockRunMode::SyncBase));
        assert_eq!(without[0].kind, DockActionKind::Execute);
    }

    #[test]
    fn the_others_stay_and_the_primary_is_not_repeated() {
        let push = resolve_dock_actions(&DockActionContext {
            has_branch_changes: true,
            has_ahead_commits: true,
            ..context(CommitMode::Push)
        });
        assert!(push
            .iter()
            .any(|action| action.id == DockActionId::CreatePr));
        let commit = resolve_dock_actions(&DockActionContext {
            has_local_changes: true,
            has_branch_changes: true,
            ..context(CommitMode::CommitAndPush)
        });
        let repeated = commit
            .iter()
            .filter(|action| action.mode == Some(DockRunMode::Mode(CommitMode::CommitAndPush)))
            .count();
        assert_eq!(repeated, 1);
    }

    #[test]
    fn an_existing_request_offers_open_and_merge() {
        let actions = resolve_dock_actions(&DockActionContext {
            has_branch_changes: true,
            has_change_request: true,
            has_open_request: true,
            ..context(CommitMode::Fix)
        });
        let find = |id| actions.iter().find(|action| action.id == id);
        assert!(!find(DockActionId::OpenPr).unwrap().disabled);
        assert!(find(DockActionId::Merge).unwrap().disabled);
        assert!(find(DockActionId::CreatePr).is_none());
    }

    #[test]
    fn without_a_remote_delivery_stops_at_a_local_commit() {
        let base = DockActionContext {
            has_local_changes: true,
            has_branch_changes: true,
            has_ahead_commits: true,
            has_remote: Some(false),
            ..context(CommitMode::CommitAndPush)
        };
        let actions = resolve_dock_actions(&base);
        assert_eq!(actions[0].mode, Some(DockRunMode::Commit));
        assert!(!actions[0].disabled);
        let disabled = |id| {
            actions
                .iter()
                .find(|action| action.id == id)
                .unwrap()
                .disabled
        };
        assert!(disabled(DockActionId::CommitAndPush));
        assert!(disabled(DockActionId::Push));
        assert!(disabled(DockActionId::CreatePr));
        assert!(disabled(DockActionId::CreateDraftPr));
        let commits = actions
            .iter()
            .filter(|action| action.mode == Some(DockRunMode::Commit))
            .count();
        assert_eq!(commits, 1);
        let clean = resolve_dock_actions(&DockActionContext {
            mode: Some(CommitMode::CreatePr),
            has_local_changes: false,
            ..base
        });
        assert_eq!(clean[0].mode, Some(DockRunMode::Commit));
        assert!(clean[0].disabled);
        let conflicts = resolve_dock_actions(&DockActionContext {
            mode: Some(CommitMode::ResolveConflicts),
            ..base
        });
        assert_eq!(conflicts[0].kind, DockActionKind::Review);
        let unknown = resolve_dock_actions(&DockActionContext {
            has_remote: None,
            ..base
        });
        assert_eq!(
            unknown[0].mode,
            Some(DockRunMode::Mode(CommitMode::CommitAndPush))
        );
    }

    #[test]
    fn the_status_counts_what_it_should() {
        let clean = DockChangeSummary::default();
        assert_eq!(
            resolve_dock_status(Some(&clean), DockGitState::Ready),
            DockStatus::None
        );
        let branch = DockChangeSummary {
            branch_files: 4,
            branch_additions: 20,
            branch_deletions: 3,
            ..clean.clone()
        };
        assert_eq!(
            resolve_dock_status(Some(&branch), DockGitState::Ready),
            DockStatus::Branch {
                files: 4,
                additions: 20,
                deletions: 3
            }
        );
        let both = DockChangeSummary {
            files: 2,
            additions: 8,
            branch_files: 5,
            ..clean.clone()
        };
        let status = resolve_dock_status(Some(&both), DockGitState::Ready);
        assert_eq!(
            status,
            DockStatus::LocalAndBranch {
                local_files: 2,
                local_additions: 8,
                local_deletions: 0,
                branch_files: 5
            }
        );
        assert_eq!(status.counts(), (8, 0));
        let merged = DockChangeSummary {
            pull_request_state: Some("MERGED".into()),
            pull_request_number: Some(42),
            ..clean
        };
        assert_eq!(
            resolve_dock_status(Some(&merged), DockGitState::Ready),
            DockStatus::Merged {
                pull_request_number: Some(42)
            }
        );
        assert_eq!(
            resolve_dock_status(None, DockGitState::Loading),
            DockStatus::Loading
        );
    }
}
