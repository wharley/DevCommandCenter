//! Merge conflicts (`features/merge`): the state's contract, the blocks in a
//! file, when a merge resolved outside the app can be completed from
//! "Mudanças" (`isMergeConflictResolutionReady`), and the resolver's labels.

mod hunks;
mod types;

pub use hunks::*;
pub use types::*;

/// `isMergeConflictResolutionReady`: every result a readable text file with
/// no marker left, in a merge (binary, big, deleted… stay explicit).
pub fn conflict_resolution_ready(state: Option<&WorkspaceGitConflictStateOutput>) -> bool {
    let Some(state) = state else {
        return false;
    };
    state.operation == WorkspaceGitConflictOperation::Merge
        && !state.conflicts.is_empty()
        && state.conflicts.iter().all(|conflict| {
            matches!(
                conflict.kind,
                WorkspaceGitConflictKind::BothModified | WorkspaceGitConflictKind::BothAdded
            ) && conflict.result.exists
                && !conflict.result.binary
                && !conflict.result.truncated
                && conflict
                    .result
                    .text
                    .as_deref()
                    .is_some_and(|text| !has_conflict_marker_fragments(text))
        })
}

/// `shortRef`: a branch without `refs/heads/`, `refs/remotes/` or `remotes/`.
pub fn short_ref(value: Option<&str>, fallback: &str) -> String {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return fallback.to_string();
    };
    let value = value
        .strip_prefix("refs/heads/")
        .or_else(|| value.strip_prefix("refs/remotes/"))
        .unwrap_or(value);
    value.strip_prefix("remotes/").unwrap_or(value).to_string()
}

/// `editableGitMode`: a regular file (or no mode known).
pub fn editable_git_mode(mode: Option<&str>) -> bool {
    mode.is_none_or(|mode| mode.starts_with("100"))
}

/// The result can't be edited as text (`textUnavailable`).
pub fn conflict_text_unavailable(entry: &WorkspaceGitConflictEntry) -> bool {
    entry.result.binary
        || entry.result.truncated
        || !editable_git_mode(entry.current.mode.as_deref())
        || !editable_git_mode(entry.incoming.mode.as_deref())
        || (entry.current.text.is_none() && entry.incoming.text.is_none())
}

/// What the resolver starts editing: the result, else a side (`openEntry`).
pub fn conflict_initial_result(entry: &WorkspaceGitConflictEntry) -> String {
    entry
        .result
        .text
        .clone()
        .or_else(|| entry.current.text.clone())
        .or_else(|| entry.incoming.text.clone())
        .unwrap_or_default()
}

/// An empty result where a side had code (`suspiciousEmptyResult`).
pub fn conflict_result_suspiciously_empty(entry: &WorkspaceGitConflictEntry, result: &str) -> bool {
    result.trim().is_empty()
        && [&entry.current.text, &entry.incoming.text]
            .iter()
            .any(|side| side.as_deref().is_some_and(|text| !text.trim().is_empty()))
}

/// `resultLineCount`: lines by `\r\n`, `\r` or `\n`; none when empty.
pub fn result_line_count(result: &str) -> usize {
    if result.is_empty() {
        return 0;
    }
    result.replace("\r\n", "\n").split(['\n', '\r']).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(text: Option<&str>) -> WorkspaceGitConflictContent {
        WorkspaceGitConflictContent {
            exists: true,
            byte_count: text.map_or(0, str::len) as u64,
            mode: Some("100644".into()),
            text: text.map(str::to_string),
            ..Default::default()
        }
    }

    fn state(result: Option<&str>) -> WorkspaceGitConflictStateOutput {
        WorkspaceGitConflictStateOutput {
            operation: WorkspaceGitConflictOperation::Merge,
            current_branch: Some("feature/example".into()),
            incoming_ref: Some("origin/main".into()),
            conflicts: vec![WorkspaceGitConflictEntry {
                path: "src/example.ts".into(),
                kind: WorkspaceGitConflictKind::BothModified,
                base: content(Some("base")),
                current: content(Some("current")),
                incoming: content(Some("incoming")),
                result: WorkspaceGitConflictContent {
                    mode: None,
                    ..content(result)
                },
            }],
        }
    }

    #[test]
    fn a_marker_free_text_result_is_ready() {
        assert!(conflict_resolution_ready(Some(&state(Some(
            "current + incoming\n"
        )))));
        assert!(!conflict_resolution_ready(Some(&state(Some(
            "<<<<<<< HEAD\ncurrent\n=======\nincoming\n>>>>>>> main\n"
        )))));
        assert!(!conflict_resolution_ready(Some(&state(None))));
        let mut deleted = state(Some("kept content\n"));
        deleted.conflicts[0].kind = WorkspaceGitConflictKind::DeletedByIncoming;
        assert!(!conflict_resolution_ready(Some(&deleted)));
        assert!(!conflict_resolution_ready(None));
    }

    #[test]
    fn labels_and_the_result_checks() {
        assert_eq!(
            short_ref(Some("refs/remotes/origin/main"), "x"),
            "origin/main"
        );
        assert_eq!(short_ref(Some("refs/heads/feature"), "x"), "feature");
        assert_eq!(short_ref(None, "base"), "base");
        assert!(editable_git_mode(Some("100755")) && !editable_git_mode(Some("120000")));
        let entry = &state(None).conflicts[0];
        assert_eq!(conflict_initial_result(entry), "current");
        assert!(conflict_result_suspiciously_empty(entry, "  \n"));
        assert!(!conflict_text_unavailable(entry));
        assert_eq!(result_line_count(""), 0);
        assert_eq!(result_line_count("a\r\nb\nc"), 3);
    }
}
