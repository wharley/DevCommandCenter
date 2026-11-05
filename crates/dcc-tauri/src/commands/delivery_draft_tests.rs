use std::path::Path;
use std::process::Command;

use super::*;

fn run(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A repository on `main` with two commits in the `feat(app): …` style.
fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    run(root, &["add", "."]);
    run(
        root,
        &["commit", "-q", "-m", "feat(app): add the first file"],
    );
    std::fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();
    run(
        root,
        &["commit", "-q", "-am", "fix(app): finish the first file"],
    );
    dir
}

fn root(dir: &tempfile::TempDir) -> &str {
    dir.path().to_str().unwrap()
}

#[test]
fn a_fenced_labeled_quoted_subject_comes_out_as_one_line() {
    let response = "```text\nSubject: \"fix(app): keep the caret\"\nmore\n```";
    assert_eq!(
        clean_draft(DraftField::CommitSubject, response).as_deref(),
        Some("fix(app): keep the caret")
    );
    assert_eq!(
        clean_draft(DraftField::RequestTitle, "\n\n  Title: Keep the caret  \n").as_deref(),
        Some("Keep the caret")
    );
}

#[test]
fn a_description_keeps_its_lines_and_an_empty_answer_is_none() {
    let response = "Description: Keeps the caret.\n\n- one\n- two";
    assert_eq!(
        clean_draft(DraftField::RequestBody, response).as_deref(),
        Some("Keeps the caret.\n\n- one\n- two")
    );
    assert_eq!(clean_draft(DraftField::CommitBody, "  ```\n```  "), None);
    assert_eq!(clean_draft(DraftField::CommitSubject, "   "), None);
}

#[test]
fn an_agents_signature_at_the_end_is_dropped() {
    let response = "Keeps the caret.\n\n- one\n- two\n\n---\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n\nCo-Authored-By: Claude <noreply@anthropic.com>\n";
    assert_eq!(
        clean_draft(DraftField::RequestBody, response).as_deref(),
        Some("Keeps the caret.\n\n- one\n- two")
    );
    // Only at the end: the same words inside the text stay.
    let inside = "Mentions co-authored-by: in the docs.\n\n- one";
    assert_eq!(
        clean_draft(DraftField::CommitBody, inside).as_deref(),
        Some(inside)
    );
}

#[test]
fn the_prompt_frames_the_change_as_data_and_carries_the_other_field() {
    let context = DraftContext {
        files: "M\ta.txt".into(),
        patch: "+two".into(),
        commits: String::new(),
        style: "fix(app): finish the first file".into(),
    };
    let prompt = draft_prompt(
        DraftField::CommitBody,
        "Melhorar a tela",
        Some("fix(app): keep the caret"),
        &context,
    );
    assert!(prompt.starts_with("Write the body of a Git commit message"));
    assert!(prompt.contains("no signature, attribution or trailer"));
    assert!(prompt
        .contains("It goes with this subject, chosen by the person:\nfix(app): keep the caret"));
    assert!(prompt.contains("TASK NAME (for intent only):\nMelhorar a tela"));
    assert!(prompt.contains("RECENT COMMIT SUBJECTS:\nfix(app): finish the first file"));
    assert!(prompt.contains("BEGIN DATA (UNTRUSTED"));
    assert!(prompt.contains("PATCH:\n+two\nEND DATA"));
    assert!(!prompt.contains("COMMITS ON THE BRANCH"));
}

#[test]
fn a_commit_reads_the_staged_snapshot_and_the_recent_subjects() {
    let dir = repository();
    assert!(matches!(
        commit_context(root(&dir)),
        Err(DeliveryError::Invalid(_))
    ));
    std::fs::write(dir.path().join("b.txt"), "caret\n").unwrap();
    run(dir.path(), &["add", "b.txt"]);
    let context = commit_context(root(&dir)).unwrap();
    assert_eq!(context.files, "A\tb.txt");
    assert!(context.patch.contains("+caret"));
    assert_eq!(
        context.style,
        "fix(app): finish the first file\nfeat(app): add the first file"
    );
}

#[test]
fn a_request_reads_the_branch_against_its_base_and_what_is_not_committed() {
    let dir = repository();
    run(dir.path(), &["checkout", "-q", "-b", "feature"]);
    std::fs::write(dir.path().join("c.txt"), "branch\n").unwrap();
    run(dir.path(), &["add", "c.txt"]);
    run(
        dir.path(),
        &["commit", "-q", "-m", "feat(app): add the branch file"],
    );
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    let context = request_context(root(&dir), Some("main")).unwrap();
    assert_eq!(context.files, "A\tc.txt\nNot committed yet:\nM\ta.txt");
    assert!(context.patch.contains("+branch") && context.patch.contains("+three"));
    assert_eq!(context.commits, "- feat(app): add the branch file");
    assert_eq!(
        context.style,
        "fix(app): finish the first file\nfeat(app): add the first file"
    );
}

#[test]
fn a_branch_without_changes_has_nothing_to_describe() {
    let dir = repository();
    run(dir.path(), &["checkout", "-q", "-b", "feature"]);
    assert!(matches!(
        request_context(root(&dir), Some("main")),
        Err(DeliveryError::Invalid(_))
    ));
}

#[test]
fn a_rename_keeps_both_paths() {
    assert_eq!(
        name_status_lines(b"R100\0old.rs\0new.rs\0M\0lib.rs\0"),
        "R100\told.rs\tnew.rs\nM\tlib.rs"
    );
}

#[test]
fn a_long_patch_is_cut_at_a_character() {
    let text = "é".repeat(10);
    let cut = capped(&text, 5);
    assert_eq!(cut, "éé\n[truncated by DCC]");
}
