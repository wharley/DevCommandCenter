use std::path::Path;
use std::process::Command;

use super::*;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

fn write(dir: &Path, path: &str, text: &str) {
    let path = dir.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A repository with a worktree beside it, as a task gets one.
fn repo(gitignore: &str) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    write(&root, ".gitignore", gitignore);
    write(&root, ".env.example", "TRACKED=1");
    // Folders with tracked files, as in a monorepo.
    write(&root, "apps/web/package.json", "{}");
    write(&root, "keys/README", "keys");
    write(&root, "config/app.json", "{}");
    git(&root, &["add", "."]);
    git(
        &root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
    let worktree = temp.path().join("wt");
    git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            worktree.to_str().unwrap(),
        ],
    );
    (temp, root, worktree)
}

#[test]
fn copies_the_ignored_env_files_and_nothing_else_by_default() {
    let (_temp, root, worktree) =
        repo(".env\n.env.local\napps/web/.env.local\nnode_modules/\nsecret.txt\nscratch/.env\n");
    write(&root, ".env", "A=1");
    write(&root, ".env.local", "B=2");
    write(&root, "apps/web/.env.local", "C=3");
    write(&root, "node_modules/pkg/.env", "nope");
    write(&root, "secret.txt", "nope");
    write(&root, ".env.untracked", "not ignored, so not copied");
    // A folder with no tracked file comes as one entry: left out.
    write(&root, "scratch/.env", "nope");

    let mut report = copy_worktree_includes(&root, &worktree).unwrap();
    report.copied.sort();

    assert_eq!(report.copied, [".env", ".env.local", "apps/web/.env.local"]);
    assert_eq!(
        std::fs::read_to_string(worktree.join("apps/web/.env.local")).unwrap(),
        "C=3"
    );
    assert!(!worktree.join("node_modules").exists());
    assert!(!worktree.join("secret.txt").exists());
    assert!(!worktree.join(".env.untracked").exists());
    assert!(!worktree.join("scratch").exists());
    // The tracked example came with git, untouched.
    assert_eq!(
        std::fs::read_to_string(worktree.join(".env.example")).unwrap(),
        "TRACKED=1"
    );
}

#[test]
fn a_worktreeinclude_replaces_the_default_and_never_overwrites() {
    let (_temp, root, worktree) = repo(".env\nconfig/local.json\n*.pem\n");
    write(
        &root,
        ".worktreeinclude",
        "# ours\nconfig/local.json\n*.pem\n!keys/old.pem\n",
    );
    write(&root, ".env", "A=1");
    write(&root, "config/local.json", "{}");
    write(&root, "keys/dev.pem", "pem");
    write(&root, "keys/old.pem", "old");
    write(&worktree, "config/local.json", "mine");

    let mut report = copy_worktree_includes(&root, &worktree).unwrap();
    report.copied.sort();

    assert_eq!(report.copied, ["keys/dev.pem"]);
    assert_eq!(
        report.skipped,
        [("config/local.json".to_string(), "exists".to_string())]
    );
    assert_eq!(
        std::fs::read_to_string(worktree.join("config/local.json")).unwrap(),
        "mine"
    );
    assert!(!worktree.join(".env").exists());
    assert!(!worktree.join("keys/old.pem").exists());
}

#[test]
fn patterns_follow_gitignore_rules() {
    let rules = parse_rules(".env*\n/root.txt\nconf/\n**/deep/*.key\n");
    assert!(is_included(&rules, ".env"));
    assert!(is_included(&rules, "apps/api/.env.production.local"));
    assert!(is_included(&rules, "root.txt"));
    assert!(!is_included(&rules, "sub/root.txt"));
    assert!(is_included(&rules, "a/conf/x.json"));
    assert!(!is_included(&rules, "conf"));
    assert!(is_included(&rules, "x/y/deep/a.key"));
    assert!(is_included(&rules, "deep/a.key"));
    assert!(!is_included(&rules, "deep/sub/a.key"));
    assert!(!is_included(&rules, "envfile"));
}
