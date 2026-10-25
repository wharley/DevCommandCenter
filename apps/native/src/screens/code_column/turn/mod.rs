//! "Último turno" in "Mudanças" (`TurnReviewSurface`): what the session's
//! last turn changed in this task, file by file from the turn's capture,
//! and Guarded Undo to put it back — one project, or every project of a
//! multi-project task in turn. Everything comes from the desktop, which
//! holds the capture and the undo journal.

mod dialogs;
mod undo;
mod view;

pub(crate) use dialogs::dialogs;
pub(crate) use view::surface;

use std::time::Duration;

use dcc_core::domain::turn_review::{
    PreparedTurnUndo, ReadyPreview, TurnReviewSummary, TurnUndoStepResult,
};
use gpui::{AppContext, Context, Entity, Task};

use super::CodeColumn;
use super::changes::{CARDS_OPEN_LIMIT, Scope};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::screens::diff_view::DiffView;

/// `turnReviewQueryOptions`' polling while a project's capture finishes.
const MEMBER_POLL: Duration = Duration::from_millis(1500);
const MEMBER_POLLS: usize = 5;

pub(crate) enum TurnDiff {
    Idle,
    Loading(#[allow(dead_code)] Task<()>),
    Ready(Entity<DiffView>),
    /// No patch kept for the file.
    Unavailable,
    Failed,
}

pub(crate) struct TurnCard {
    pub path: String,
    pub open: bool,
    pub diff: TurnDiff,
}

/// One project's Undo, `TurnReviewSurface`'s own state.
#[derive(Default)]
pub(crate) struct SingleUndo {
    pub preparing: bool,
    pub prepare_failed: bool,
    /// `blocked` / `unavailable` and the reason.
    pub problem: Option<(&'static str, String)>,
    pub prepared: Option<ReadyPreview>,
    pub dialog: bool,
    pub expired: bool,
    pub executing: bool,
    pub execute_failed: bool,
    /// The execute status and its reason.
    pub outcome: Option<(&'static str, Option<String>)>,
}

/// `TurnUndoAcrossProjects`' state.
#[derive(Default)]
pub(crate) struct ProjectsUndo {
    pub preparing: bool,
    /// The first project that cannot be undone: name, reason code.
    pub blocked: Option<(String, String)>,
    pub prepared: Option<Vec<PreparedTurnUndo>>,
    pub dialog: bool,
    pub expired: bool,
    pub executing: bool,
    pub execute_failed: bool,
    pub results: Option<Vec<TurnUndoStepResult>>,
}

#[derive(Default)]
pub(crate) struct LastTurn {
    /// The session and task it was read for.
    pub read_for: Option<(String, String)>,
    /// `None` until the first answer; then the review (or none yet).
    pub review: Option<Result<Option<TurnReviewSummary>, ()>>,
    /// A read in flight (the chip shows no count meanwhile).
    pub fetching: bool,
    pub cards: Vec<TurnCard>,
    /// The header's "expand / collapse all".
    pub all_open: Option<bool>,
    /// The task's projects (id, name) when it has more than one.
    pub members: Vec<(String, String)>,
    /// Each project's review of the same turn, in `members` order.
    pub member_reviews: Vec<Option<TurnReviewSummary>>,
    pub single: SingleUndo,
    pub projects: ProjectsUndo,
    _read: Option<Task<()>>,
    _members: Option<Task<()>>,
    _member_reviews: Option<Task<()>>,
    _diffs: Vec<Task<()>>,
    _undo: Option<Task<()>>,
    _expiry: Option<Task<()>>,
}

impl LastTurn {
    /// The review shown, once read for the current session and task.
    pub fn summary(&self) -> Option<&TurnReviewSummary> {
        match &self.review {
            Some(Ok(Some(review))) => Some(review),
            _ => None,
        }
    }

    /// The chip's count: none while it reads (`visibleTurnReviewSummary`).
    pub fn count(&self) -> Option<usize> {
        (!self.fetching)
            .then(|| self.summary().map(|review| review.files.len()))
            .flatten()
    }

    /// `reviewCardsStartOpen`, unless the header said otherwise.
    pub fn cards_open(&self) -> bool {
        self.all_open
            .unwrap_or(self.cards.len() <= CARDS_OPEN_LIMIT)
    }
}

impl CodeColumn {
    /// The session and task the turn review reads (`sessionId && workspaceId`).
    pub(crate) fn last_turn_context(&self) -> Option<(String, String)> {
        let session = self.session_id.clone()?;
        let task = self.delivery.task.as_ref()?.id.0.clone();
        Some((session, task))
    }

    /// `resolveInspectorReviewScope`: "Último turno" only with a session.
    pub(crate) fn review_scope(&self) -> Scope {
        match self.changes.scope() {
            Scope::LastTurn if self.last_turn_context().is_none() => self.changes.default_scope(),
            scope => scope,
        }
    }

    /// Reads the last turn's review again (`useCachedTurnReviewSummary`):
    /// on a new session or task, a finished turn, or an undo.
    pub(crate) fn read_last_turn(&mut self, cx: &mut Context<Self>) {
        let Some((session, task)) = self.last_turn_context() else {
            self.turn = LastTurn::default();
            return;
        };
        if self.turn.read_for.as_ref() != Some(&(session.clone(), task.clone())) {
            self.turn = LastTurn {
                read_for: Some((session.clone(), task.clone())),
                ..LastTurn::default()
            };
            self.read_members(&task, cx);
        }
        let api = self.endpoint.clone().or_else(DesktopApi::load);
        self.turn.fetching = true;
        self.turn._read = Some(cx.spawn(async move |this, cx| {
            let (read_session, read_task) = (session.clone(), task.clone());
            let result = cx
                .background_executor()
                .spawn(async move {
                    api.ok_or(ApiError::DesktopClosed)?.last_turn_review(
                        &read_session,
                        &read_task,
                        None,
                    )
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.turn.read_for != Some((session, task)) {
                    return;
                }
                column.turn.fetching = false;
                let result = result.map_err(|_| ());
                column.take_review(result, cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// A new answer: the cards follow its files (a new snapshot is a new
    /// set of cards), and the projects read the same turn.
    fn take_review(
        &mut self,
        result: Result<Option<TurnReviewSummary>, ()>,
        cx: &mut Context<Self>,
    ) {
        let (old, new) = (
            self.turn.summary().map(|review| review.snapshot_id.clone()),
            result
                .as_ref()
                .ok()
                .and_then(|review| review.as_ref().map(|review| review.snapshot_id.clone())),
        );
        if old != new || self.turn.review.is_none() {
            let files: Vec<String> = result
                .as_ref()
                .ok()
                .and_then(Option::as_ref)
                .map(|review| review.files.iter().map(|file| file.path.clone()).collect())
                .unwrap_or_default();
            let open = self
                .turn
                .all_open
                .unwrap_or(files.len() <= CARDS_OPEN_LIMIT);
            self.turn.cards = files
                .into_iter()
                .map(|path| TurnCard {
                    path,
                    open,
                    diff: TurnDiff::Idle,
                })
                .collect();
            self.turn._diffs.clear();
        }
        self.turn.review = Some(result);
        self.read_member_reviews(cx);
    }

    /// The task's projects, when it has more than one (`turnUndoWorkspaces`).
    fn read_members(&mut self, task: &str, cx: &mut Context<Self>) {
        let (db_path, task) = (self.db_path.clone(), task.to_string());
        self.turn._members = Some(cx.spawn(async move |this, cx| {
            let read_task = task.clone();
            let members = cx
                .background_executor()
                .spawn(async move { bundle_members(&db_path, &read_task) })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.turn.read_for.as_ref().map(|(_, task)| task) != Some(&task) {
                    return;
                }
                column.turn.members = members;
                column.read_member_reviews(cx);
                cx.notify();
            });
        }));
    }

    /// Each project's review of this turn, read again while a capture
    /// finishes (`turnReviewQueryOptions`' polling).
    fn read_member_reviews(&mut self, cx: &mut Context<Self>) {
        let (Some(review), true) = (self.turn.summary(), self.turn.members.len() > 1) else {
            return;
        };
        let (session, turn) = (review.session_id.clone(), review.turn_id.clone());
        let members: Vec<String> = self.turn.members.iter().map(|(id, _)| id.clone()).collect();
        let api = self.endpoint.clone().or_else(DesktopApi::load);
        self.turn._member_reviews = Some(cx.spawn(async move |this, cx| {
            for poll in 0..MEMBER_POLLS {
                if poll > 0 {
                    cx.background_executor().timer(MEMBER_POLL).await;
                }
                let (api, members, read_session, read_turn) =
                    (api.clone(), members.clone(), session.clone(), turn.clone());
                let reviews = cx
                    .background_executor()
                    .spawn(async move {
                        members
                            .iter()
                            .map(|id| {
                                api.as_ref()?
                                    .last_turn_review(&read_session, id, Some(&read_turn))
                                    .ok()
                                    .flatten()
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;
                let pending = reviews.iter().any(|review| {
                    review
                        .as_ref()
                        .is_none_or(|review| review.state == "collecting")
                });
                let kept = this.update(cx, |column, cx| {
                    let same = column.turn.summary().map(|review| review.turn_id.as_str())
                        == Some(turn.as_str());
                    if same {
                        column.turn.member_reviews = reviews;
                        cx.notify();
                    }
                    same
                });
                if !matches!(kept, Ok(true)) || !pending {
                    return;
                }
            }
        }));
    }

    /// One card's patch, read once it opens.
    pub(crate) fn load_turn_diff(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(review) = self.turn.summary() else {
            return;
        };
        let snapshot = review.snapshot_id.clone();
        let Some(card) = self.turn.cards.get(at) else {
            return;
        };
        if !matches!(card.diff, TurnDiff::Idle) {
            return;
        }
        let path = card.path.clone();
        let unavailable = review
            .files
            .iter()
            .find(|file| file.path == path)
            .is_none_or(|file| file.preview_unavailable);
        if unavailable {
            self.turn.cards[at].diff = TurnDiff::Unavailable;
            return;
        }
        let api = self.endpoint.clone().or_else(DesktopApi::load);
        let dark = self.changes.dark();
        let read_path = path.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api.ok_or(ApiError::DesktopClosed)?
                        .turn_review_file_diff(&snapshot, &read_path)
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                let Some(card) = column.turn.cards.iter_mut().find(|card| card.path == path) else {
                    return;
                };
                card.diff = match result {
                    Ok(output) => match output.diff.filter(|_| !output.preview_unavailable) {
                        Some(patch) => TurnDiff::Ready(
                            cx.new(|cx| DiffView::from_patch(path.clone(), patch, dark, cx)),
                        ),
                        None => TurnDiff::Unavailable,
                    },
                    Err(_) => TurnDiff::Failed,
                };
                cx.notify();
            });
        });
        self.turn.cards[at].diff = TurnDiff::Loading(task);
    }

    /// "Tentar novamente" on a patch that failed.
    pub(crate) fn retry_turn_diff(&mut self, at: usize, cx: &mut Context<Self>) {
        if let Some(card) = self.turn.cards.get_mut(at) {
            card.diff = TurnDiff::Idle;
        }
        self.load_turn_diff(at, cx);
        cx.notify();
    }

    pub(crate) fn toggle_turn_card(&mut self, at: usize, cx: &mut Context<Self>) {
        if let Some(card) = self.turn.cards.get_mut(at) {
            card.open = !card.open;
        }
        cx.notify();
    }

    pub(crate) fn toggle_turn_cards(&mut self, cx: &mut Context<Self>) {
        let open = !self.turn.cards_open();
        self.turn.all_open = Some(open);
        for card in &mut self.turn.cards {
            card.open = open;
        }
        cx.notify();
    }
}

/// `selectedBundleMembers` with `repositoryDisplayName`: every project of a
/// multi-project task, in the bundle's order; empty for a single project.
fn bundle_members(db_path: &std::path::Path, task: &str) -> Vec<(String, String)> {
    use dcc_core::domain::workspace::WorkspaceId;
    use dcc_core::ports::{RepositoryRepo, WorkspaceBundleRepo, WorkspaceRepo};
    use futures::executor::block_on;
    let Ok(repo) = dcc_infra::db::SqliteWorkspaceRepo::open(db_path) else {
        return Vec::new();
    };
    let Ok(Some(mut bundle)) =
        block_on(repo.get_workspace_bundle_for_workspace(&WorkspaceId(task.to_string())))
    else {
        return Vec::new();
    };
    bundle.members.sort_by_key(|member| member.position);
    let repositories = block_on(repo.list_repositories()).unwrap_or_default();
    bundle
        .members
        .iter()
        .filter_map(|member| {
            let workspace = block_on(repo.get_workspace(&member.workspace_id)).ok()??;
            let root = workspace.root_path.trim();
            let name = repositories
                .iter()
                .find(|repository| repository.root_path.trim() == root)
                .map(|repository| {
                    repository
                        .display_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_string)
                        .or_else(|| {
                            Some(repository.name.trim().to_string()).filter(|name| !name.is_empty())
                        })
                        .unwrap_or_else(|| {
                            std::path::Path::new(root)
                                .file_name()
                                .map(|leaf| leaf.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "Project".to_string())
                        })
                })
                .unwrap_or_else(|| workspace.name.clone().unwrap_or_default());
            Some((member.workspace_id.0.clone(), name))
        })
        .collect()
}
