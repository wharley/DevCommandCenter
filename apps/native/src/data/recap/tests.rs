use dcc_core::domain::workspace_git::{
    WorkspaceGitChangeEntry, WorkspaceGitStatusOutput, WorkspacePrStatusOutput,
};

use super::*;

fn git(changes: &[(&str, u32, u32)], ahead: u32) -> WorkspaceGitStatusOutput {
    WorkspaceGitStatusOutput {
        staged: vec![],
        unstaged: changes
            .iter()
            .map(|(path, insertions, deletions)| WorkspaceGitChangeEntry {
                path: path.to_string(),
                name: path.to_string(),
                absolute_path: format!("/repo/{path}"),
                status: "M".into(),
                insertions: *insertions,
                deletions: *deletions,
            })
            .collect(),
        staged_fingerprint: String::new(),
        current_branch: Some("dcc/task".into()),
        ahead_of_remote_count: ahead,
        behind_of_remote_count: 0,
        conflict_count: 0,
        merge_in_progress: false,
        has_remote: true,
    }
}

fn pr(number: u32, state: &str, merge_state: Option<&str>) -> WorkspacePrStatusOutput {
    WorkspacePrStatusOutput {
        checks_failing: None,
        provider: Some("github".into()),
        host: Some("github.com".into()),
        number: Some(number),
        title: Some("Title".into()),
        url: None,
        head_branch: Some("dcc/task".into()),
        base_branch: Some("main".into()),
        state: Some(state.into()),
        is_draft: false,
        mergeable: Some("MERGEABLE".into()),
        merge_state_status: merge_state.map(str::to_string),
    }
}

fn input<'a>(
    git: Option<&'a WorkspaceGitStatusOutput>,
    pr: Option<&'a WorkspacePrStatusOutput>,
) -> RecapInput<'a> {
    RecapInput {
        branch: "dcc/task",
        turn_running: false,
        has_activity: true,
        git,
        pr,
        committed_vs_base: 0,
        pending_delegations: 0,
    }
}

fn key(recap: Option<Recap>) -> Option<String> {
    recap.map(|recap| recap.message_key)
}

#[test]
fn an_open_pr_with_clean_checks_is_ready_to_merge() {
    let status = git(&[], 0);
    let open = pr(886, "OPEN", Some("CLEAN"));
    assert_eq!(
        key(build_recap(&input(Some(&status), Some(&open)))).as_deref(),
        Some("mergeReady")
    );
    assert_eq!(
        next_action_key("mergeReady").as_deref(),
        Some("sidebar.nextAction.mergeReady")
    );
}

#[test]
fn uncommitted_work_waits_for_a_commit() {
    let status = git(&[("a.rs", 30, 2), ("b.rs", 8, 4)], 0);
    let recap = build_recap(&input(Some(&status), None)).unwrap();
    assert_eq!((recap.message_key.as_str(), recap.count), ("changes", 2));
    let meta = build_meta("dcc/task", Some(&status), None).unwrap();
    assert_eq!(
        (meta.additions, meta.deletions, meta.pr_number),
        (38, 6, None)
    );
}

#[test]
fn failing_checks_and_pushes_are_spotted() {
    let status = git(&[], 0);
    let failing = pr(12, "open", Some("UNSTABLE"));
    assert_eq!(
        key(build_recap(&input(Some(&status), Some(&failing)))).as_deref(),
        Some("checksFailing")
    );
    let ahead = git(&[], 3);
    let recap = build_recap(&input(Some(&ahead), None)).unwrap();
    assert_eq!((recap.message_key.as_str(), recap.count), ("ahead", 3));
}

#[test]
fn a_running_turn_is_the_agents_line_not_a_next_action() {
    let status = git(&[("a.rs", 1, 0)], 0);
    let mut running = input(Some(&status), None);
    running.turn_running = true;
    assert_eq!(key(build_recap(&running)).as_deref(), Some("working"));
    assert_eq!(next_action_key("working"), None);
}

#[test]
fn committed_work_without_a_pr_is_ready_for_one() {
    let status = git(&[], 0);
    let mut ready = input(Some(&status), None);
    ready.committed_vs_base = 4;
    let recap = build_recap(&ready).unwrap();
    assert_eq!(
        (recap.message_key.as_str(), recap.count, recap.request_label),
        ("readyForPr", 4, "PR")
    );
}

#[test]
fn a_new_idle_task_says_nothing() {
    let status = git(&[], 0);
    let mut idle = input(Some(&status), None);
    idle.has_activity = false;
    assert_eq!(build_recap(&idle), None);
}

#[test]
fn a_pr_from_another_branch_does_not_belong_to_the_task() {
    let mut other = pr(5, "open", None);
    other.head_branch = Some("release".into());
    assert_eq!(build_meta("dcc/task", None, Some(&other)), None);
    let mine = pr(6, "merged", None);
    let meta = build_meta("dcc/task", None, Some(&mine)).unwrap();
    assert_eq!(
        (meta.pr_number, meta.pr_state),
        (Some(6), Some(PrState::Merged))
    );
}

#[test]
fn branch_names_still_hint_when_git_is_silent() {
    assert_eq!(
        resolve_commit_mode("fix/login", None, None),
        CommitMode::Fix
    );
    assert_eq!(
        resolve_commit_mode("feature", None, None),
        CommitMode::CreatePr
    );
}
