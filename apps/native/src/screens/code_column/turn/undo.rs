//! Guarded Undo from "Último turno": prepare (read-only: what would come
//! back, with a token that expires), then restore once the person
//! confirms — for the task's one project, or project after project.

use chrono::{DateTime, Utc};
use dcc_core::domain::turn_review::{
    TurnUndoPreparation, TurnUndoProject, execute_turn_undo, is_guarded_undo_preview_expired,
    plan_turn_undo, prepare_turn_undo, ready_preview,
};
use gpui::Context;

use super::super::CodeColumn;
use super::ProjectsUndo;
use crate::data::desktop_api::{ApiError, DesktopApi};

impl CodeColumn {
    fn turn_api(&self) -> Result<DesktopApi, ApiError> {
        self.endpoint
            .clone()
            .or_else(DesktopApi::load)
            .ok_or(ApiError::DesktopClosed)
    }

    /// The projects `TurnUndoAcrossProjects` covers, with their reviews.
    pub(crate) fn turn_projects(&self) -> Vec<TurnUndoProject> {
        self.turn
            .members
            .iter()
            .enumerate()
            .map(|(index, (id, name))| TurnUndoProject {
                workspace_id: id.clone(),
                name: name.clone(),
                review: self.turn.member_reviews.get(index).cloned().flatten(),
            })
            .collect()
    }

    /// "Desfazer": prepares, and opens the confirmation when it is ready.
    pub(crate) fn prepare_undo(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.turn.summary().map(|review| review.snapshot_id.clone()) else {
            return;
        };
        if self.turn.single.preparing {
            return;
        }
        self.turn.single.preparing = true;
        let api = self.turn_api();
        cx.notify();
        self.turn._undo = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.prepare_guarded_undo(&snapshot) })
                .await;
            let _ = this.update(cx, |column, cx| {
                let single = &mut column.turn.single;
                single.preparing = false;
                match result {
                    Ok(output) => {
                        single.prepare_failed = false;
                        single.outcome = None;
                        single.problem = None;
                        match ready_preview(output) {
                            Ok(preview) => {
                                let expires_at = preview.expires_at.clone();
                                single.expired =
                                    is_guarded_undo_preview_expired(&expires_at, Utc::now());
                                single.prepared = Some(preview);
                                single.dialog = true;
                                column.watch_expiry(expires_at, false, cx);
                            }
                            Err((result, reason)) => {
                                single.prepared = None;
                                single.problem = Some((result, reason));
                            }
                        }
                    }
                    Err(_) => single.prepare_failed = true,
                }
                cx.notify();
            });
        }));
    }

    /// The preview stops being usable when its time comes.
    fn watch_expiry(&mut self, expires_at: String, projects: bool, cx: &mut Context<Self>) {
        let Ok(at) = DateTime::parse_from_rfc3339(&expires_at) else {
            return;
        };
        let wait = (at.with_timezone(&Utc) - Utc::now())
            .to_std()
            .unwrap_or_default();
        self.turn._expiry = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |column, cx| {
                if projects {
                    column.turn.projects.expired = true;
                } else {
                    column.turn.single.expired = true;
                }
                cx.notify();
            });
        }));
    }

    /// "Restaurar": the prepared token, once.
    pub(crate) fn execute_undo(&mut self, cx: &mut Context<Self>) {
        let single = &mut self.turn.single;
        let Some(token) = single
            .prepared
            .as_ref()
            .map(|prepared| prepared.preview_token.clone())
        else {
            return;
        };
        if single.executing || single.expired {
            return;
        }
        single.executing = true;
        let api = self.turn_api();
        cx.notify();
        self.turn._undo = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.execute_guarded_undo(&token) })
                .await;
            let _ = this.update(cx, |column, cx| {
                let single = &mut column.turn.single;
                single.executing = false;
                match result {
                    Ok(outcome) => {
                        let (status, reason) = outcome.status();
                        single.outcome = Some((status, reason.map(str::to_string)));
                    }
                    Err(_) => single.execute_failed = true,
                }
                column.after_undo(cx);
            });
        }));
    }

    /// `closeUndoDialog`: a result that needs recovery stays under the header.
    pub(crate) fn close_undo(&mut self, cx: &mut Context<Self>) {
        let single = &mut self.turn.single;
        if single.executing {
            return;
        }
        single.dialog = false;
        single.execute_failed = false;
        if single
            .outcome
            .as_ref()
            .is_none_or(|(status, _)| *status != "recovery_required")
        {
            single.prepared = None;
            single.outcome = None;
        }
        cx.notify();
    }

    /// "Desfazer o turno · N projetos": every project prepared first.
    pub(crate) fn prepare_projects_undo(&mut self, cx: &mut Context<Self>) {
        if self.turn.projects.preparing {
            return;
        }
        let targets = plan_turn_undo(&self.turn_projects()).targets;
        if targets.is_empty() {
            return;
        }
        self.turn.projects.preparing = true;
        let api = self.turn_api();
        cx.notify();
        self.turn._undo = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    prepare_turn_undo(&targets, |snapshot| api.prepare_guarded_undo(snapshot))
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                let projects = &mut column.turn.projects;
                projects.preparing = false;
                projects.results = None;
                match result {
                    Ok(TurnUndoPreparation::Ready(prepared)) => {
                        projects.blocked = None;
                        projects.expired = prepared.iter().any(|project| {
                            is_guarded_undo_preview_expired(&project.preview.expires_at, Utc::now())
                        });
                        let soonest = prepared
                            .iter()
                            .map(|project| project.preview.expires_at.clone())
                            .min();
                        projects.prepared = Some(prepared);
                        projects.dialog = true;
                        if let Some(at) = soonest {
                            column.watch_expiry(at, true, cx);
                        }
                    }
                    Ok(TurnUndoPreparation::Blocked {
                        target,
                        reason_code,
                        ..
                    }) => {
                        projects.prepared = None;
                        projects.blocked = Some((target.name, reason_code));
                    }
                    // A request that fails is a project that cannot be undone now.
                    Err(_) => {
                        projects.prepared = None;
                        projects.blocked = None;
                    }
                }
                cx.notify();
            });
        }));
    }

    /// "Restaurar N projetos", one after the other.
    pub(crate) fn execute_projects_undo(&mut self, cx: &mut Context<Self>) {
        let projects = &mut self.turn.projects;
        let Some(prepared) = projects.prepared.clone() else {
            return;
        };
        if projects.executing || projects.expired {
            return;
        }
        projects.executing = true;
        let api = self.turn_api();
        cx.notify();
        self.turn._undo = Some(cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    Ok::<_, ApiError>(execute_turn_undo(&prepared, |token| {
                        api.execute_guarded_undo(token)
                    }))
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                let projects = &mut column.turn.projects;
                projects.executing = false;
                match results {
                    Ok(results) => projects.results = Some(results),
                    Err(_) => projects.execute_failed = true,
                }
                column.after_undo(cx);
            });
        }));
    }

    pub(crate) fn close_projects_undo(&mut self, cx: &mut Context<Self>) {
        let projects = &mut self.turn.projects;
        if projects.executing {
            return;
        }
        *projects = ProjectsUndo {
            blocked: projects.blocked.take(),
            ..ProjectsUndo::default()
        };
        cx.notify();
    }

    /// The turn, the status and the diffs read again after a restore.
    fn after_undo(&mut self, cx: &mut Context<Self>) {
        self.read_last_turn(cx);
        self.refresh_git(cx);
        cx.notify();
    }
}
