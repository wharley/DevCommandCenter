//! A new worktree has only the files git tracks, so the `.env` a project
//! needs to run is missing. Right after `git worktree add`, the files the
//! source checkout ignores and that match `.env*` (or the project's
//! `.worktreeinclude`, the file Conductor and Claude Code read too) are copied
//! over.
//!
//! Creating a task must stay instant, on the desktop and from the phone, so:
//! - one `git ls-files … --directory` lists the ignored entries without
//!   walking into ignored folders (0.15 s beside 1.5 GB of `node_modules`);
//! - only loose files are copied, never a whole ignored folder, up to
//!   [`MAX_TOTAL_BYTES`]; a folder without any tracked file comes from git as
//!   one entry and is left out too;
//! - a file already in the worktree is never overwritten;
//! - nothing here can fail the creation: the caller only logs the report.

use std::path::Path;

use crate::git_command::{run_git_stdout_in_dir, GIT_LOCAL_TIMEOUT};

pub const WORKTREE_INCLUDE_FILE: &str = ".worktreeinclude";
const DEFAULT_PATTERNS: &str = ".env*";
const MAX_TOTAL_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct WorktreeIncludeReport {
    pub copied: Vec<String>,
    /// Matching files left out, with why ("exists", "too large", an error).
    pub skipped: Vec<(String, String)>,
}

/// Copies `source`'s ignored files that match the include patterns into
/// `worktree`. Only reads names: the files' contents are never looked at.
pub fn copy_worktree_includes(
    source: &Path,
    worktree: &Path,
) -> Result<WorktreeIncludeReport, String> {
    let patterns = match std::fs::read_to_string(source.join(WORKTREE_INCLUDE_FILE)) {
        Ok(text) => text,
        Err(_) => DEFAULT_PATTERNS.to_string(),
    };
    let rules = parse_rules(&patterns);
    if rules.is_empty() {
        return Ok(WorktreeIncludeReport::default());
    }
    let listing = run_git_stdout_in_dir(
        [
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "--no-empty-directory",
        ],
        Some(source),
        GIT_LOCAL_TIMEOUT,
        "failed to list ignored files",
    )?;
    let mut report = WorktreeIncludeReport::default();
    let mut total = 0u64;
    for path in listing.split('\0').filter(|entry| !entry.is_empty()) {
        // A trailing slash is a whole ignored folder (`node_modules/`).
        if path.ends_with('/') || !is_included(&rules, path) {
            continue;
        }
        let from = source.join(path);
        let to = worktree.join(path);
        let Ok(metadata) = std::fs::metadata(&from) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        if to.exists() {
            report
                .skipped
                .push((path.to_string(), "exists".to_string()));
            continue;
        }
        if total + metadata.len() > MAX_TOTAL_BYTES {
            report
                .skipped
                .push((path.to_string(), "too large".to_string()));
            continue;
        }
        let copied = to
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::copy(&from, &to));
        match copied {
            Ok(bytes) => {
                total += bytes;
                report.copied.push(path.to_string());
            }
            Err(error) => report.skipped.push((path.to_string(), error.to_string())),
        }
    }
    Ok(report)
}

#[derive(Debug, PartialEq, Eq)]
struct Rule {
    pattern: String,
    negated: bool,
    /// Has a slash before its end: matched against the whole path.
    anchored: bool,
    /// Ends with a slash: matches folders, so the files inside them.
    folder: bool,
}

/// `.gitignore` syntax: comments, `!` negation, a leading `/` anchors, a
/// trailing `/` means a folder, `*`, `?` and `**`. Character classes are
/// taken literally.
fn parse_rules(text: &str) -> Vec<Rule> {
    text.lines()
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (negated, line) = match line.strip_prefix('!') {
                Some(rest) => (true, rest),
                None => (false, line.strip_prefix('\\').unwrap_or(line)),
            };
            let (folder, line) = match line.strip_suffix('/') {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            let anchored = line.contains('/');
            Rule {
                pattern: line.trim_start_matches('/').to_string(),
                negated,
                anchored,
                folder,
            }
        })
        .filter(|rule| !rule.pattern.is_empty())
        .collect()
}

/// The last rule that matches decides, as in `.gitignore`.
fn is_included(rules: &[Rule], path: &str) -> bool {
    let mut included = false;
    for rule in rules {
        if rule_matches(rule, path) {
            included = !rule.negated;
        }
    }
    included
}

fn rule_matches(rule: &Rule, path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    // A file pattern also matches the folders above the file; a folder
    // pattern only those.
    let last = if rule.folder {
        segments.len() - 1
    } else {
        segments.len()
    };
    (1..=last).any(|depth| {
        let candidate = if rule.anchored {
            segments[..depth].join("/")
        } else {
            segments[depth - 1].to_string()
        };
        glob(rule.pattern.as_bytes(), candidate.as_bytes())
    })
}

/// `*` and `?` stay inside one path segment; `**` crosses them.
fn glob(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.first() {
        None => text.is_empty(),
        Some(b'*') if pattern.get(1) == Some(&b'*') => {
            let rest = &pattern[2..];
            let rest = rest.strip_prefix(b"/").unwrap_or(rest);
            (0..=text.len()).any(|skip| {
                (skip == 0 || text[skip - 1] == b'/' || rest.is_empty())
                    && glob(rest, &text[skip..])
            })
        }
        Some(b'*') => {
            let rest = &pattern[1..];
            (0..=text.len())
                .take_while(|&skip| skip == 0 || text[skip - 1] != b'/')
                .any(|skip| glob(rest, &text[skip..]))
        }
        Some(b'?') => text.first().is_some_and(|&c| c != b'/') && glob(&pattern[1..], &text[1..]),
        Some(&c) => text.first() == Some(&c) && glob(&pattern[1..], &text[1..]),
    }
}

#[cfg(test)]
#[path = "worktree_include_tests.rs"]
mod tests;
