//! Text search across a checkout's tracked files (`git grep`), for the
//! "search in files" dialog: a fixed string, case-insensitive, binary files
//! skipped, at most 200 lines, each cut at 400 characters.

use crate::git_command::run_git_output;

/// `SEARCH_WORKSPACE_MAX_RESULTS`
pub const MAX_RESULTS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub path: String,
    pub line: u32,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub matches: Vec<SearchMatch>,
    /// More lines matched than were kept.
    pub truncated: bool,
}

/// `git grep -z -n -I -i -F` for `query` under `root`. An empty root or
/// query finds nothing; no match is not an error (git exits 1, empty).
pub fn search_workspace(root: &str, query: &str) -> Result<SearchResults, String> {
    let (root, query) = (root.trim(), query.trim());
    if root.is_empty() || query.is_empty() {
        return Ok(SearchResults::default());
    }
    // `-z` makes the path delimiter NUL, so paths containing `:` parse cleanly.
    let output = run_git_output(
        root,
        &["grep", "-z", "-n", "-I", "-i", "-F", "-e", query, "--"],
    )?;
    Ok(parse_git_grep_z(&output.stdout, MAX_RESULTS))
}

/// Records are `<path>\0<line>\0<text>`, one per line (git 2.50 ends the
/// line number with NUL too under `-z`; older ones with `:`).
pub fn parse_git_grep_z(stdout: &[u8], max_results: usize) -> SearchResults {
    let mut matches = Vec::new();
    let mut truncated = false;
    let stdout = String::from_utf8_lossy(stdout);
    for record in stdout.split('\n') {
        if record.is_empty() {
            continue;
        }
        let Some((path, rest)) = record.split_once('\0') else {
            continue;
        };
        let Some((line_str, text)) = rest.split_once(['\0', ':']) else {
            continue;
        };
        let Ok(line) = line_str.trim().parse::<u32>() else {
            continue;
        };
        if matches.len() >= max_results {
            truncated = true;
            break;
        }
        matches.push(SearchMatch {
            path: path.to_string(),
            line,
            text: text.chars().take(400).collect(),
        });
    }
    SearchResults { matches, truncated }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colon_paths_parse_and_the_cap_truncates() {
        let mut input = Vec::new();
        input.extend_from_slice(b"src/with:colon.rs");
        input.push(0);
        input.extend_from_slice(b"12:let value = 1;\nsrc/lib.rs");
        input.push(0);
        input.extend_from_slice(b"3:needle\n");
        let output = parse_git_grep_z(&input, 1);

        assert!(output.truncated);
        assert_eq!(output.matches.len(), 1);
        assert_eq!(output.matches[0].path, "src/with:colon.rs");
        assert_eq!(output.matches[0].line, 12);
        assert_eq!(output.matches[0].text, "let value = 1;");
    }

    #[test]
    fn the_line_number_ends_with_nul_too() {
        let output = parse_git_grep_z(b"b.rs\x001\x00needle: here\nsrc/a.rs\x002\x00let x;\n", 10);
        assert!(!output.truncated);
        assert_eq!(output.matches.len(), 2);
        assert_eq!(output.matches[0].path, "b.rs");
        assert_eq!(output.matches[0].line, 1);
        assert_eq!(output.matches[0].text, "needle: here");
        assert_eq!(output.matches[1].line, 2);
    }

    #[test]
    fn a_real_checkout_is_searched() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.path().join("a.rs"), "one\nlet Needle = 1;\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "first"]);
        let root = dir.path().to_string_lossy().into_owned();
        let found = search_workspace(&root, "needle").unwrap();
        assert_eq!(found.matches.len(), 1);
        assert_eq!(found.matches[0].line, 2);
        assert_eq!(found.matches[0].text, "let Needle = 1;");
        assert!(search_workspace(&root, "absent")
            .unwrap()
            .matches
            .is_empty());
    }

    #[test]
    fn nothing_to_look_for_finds_nothing() {
        assert_eq!(search_workspace(" ", "x"), Ok(SearchResults::default()));
        assert_eq!(search_workspace("/tmp", "  "), Ok(SearchResults::default()));
    }
}
