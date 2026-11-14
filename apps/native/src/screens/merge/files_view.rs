//! How a conflicted file is called by what happened to it on each side.

use dcc_core::domain::merge_conflict::{WorkspaceGitConflictEntry, WorkspaceGitConflictKind};
use gpui::SharedString;

use crate::i18n::Translate;

pub(crate) fn kind_label(entry: &WorkspaceGitConflictEntry, cx: &gpui::App) -> SharedString {
    cx.t(match entry.kind {
        WorkspaceGitConflictKind::BothModified => "mergeConflict.conflictKind.bothModified",
        WorkspaceGitConflictKind::BothAdded => "mergeConflict.conflictKind.bothAdded",
        WorkspaceGitConflictKind::DeletedByCurrent => "mergeConflict.conflictKind.deletedByCurrent",
        WorkspaceGitConflictKind::DeletedByIncoming => {
            "mergeConflict.conflictKind.deletedByIncoming"
        }
        WorkspaceGitConflictKind::BothDeleted => "mergeConflict.conflictKind.bothDeleted",
        WorkspaceGitConflictKind::Other => "mergeConflict.conflictKind.other",
    })
}
