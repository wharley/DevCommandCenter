//! `@arquivo`: typing `@` opens the workspace's files above the composer
//! (`FileMentionPlugin`), ranked by name; ↑↓ move, Enter or Tab pick, and
//! the pick becomes a file chip. Files come from `git ls-files` like the
//! desktop's (`dcc_infra::workspace_git::git_tracked_files`).

use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::data::prompt::basename;
use crate::ui::text_area::CHIP;

pub const MAX_VISIBLE_OPTIONS: usize = 50;
const MAX_FILE_MENTION_LENGTH: usize = 200;
/// The list is read again after this (`staleTime`).
const STALE: Duration = Duration::from_secs(60);

/// `rankFile`
pub fn rank(path: &str, query: &str) -> u8 {
    if query.is_empty() {
        return 1;
    }
    let q = query.to_lowercase();
    let name = basename(path).to_lowercase();
    let path = path.to_lowercase();
    if name == q || path == q {
        5
    } else if path.ends_with(&format!("/{q}")) {
        4
    } else if name.starts_with(&q) {
        3
    } else if name.contains(&q) {
        2
    } else if path.contains(&q) {
        1
    } else {
        0
    }
}

/// `filterFiles`: an exact name wins alone; otherwise the best ranked.
pub fn filter(files: &[String], query: &str) -> Vec<String> {
    if query.is_empty() {
        return files.iter().take(MAX_VISIBLE_OPTIONS).cloned().collect();
    }
    let q = query.to_lowercase();
    let exact: Vec<String> = files
        .iter()
        .filter(|path| basename(path).to_lowercase() == q || path.to_lowercase() == q)
        .take(MAX_VISIBLE_OPTIONS)
        .cloned()
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    let mut ranked: Vec<(u8, usize, &String)> = files
        .iter()
        .enumerate()
        .map(|(index, path)| (rank(path, query), index, path))
        .filter(|(score, ..)| *score > 0)
        .collect();
    // Stable like `Array.sort`: equal scores keep the list's order.
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    ranked
        .into_iter()
        .take(MAX_VISIBLE_OPTIONS)
        .map(|(.., path)| path.clone())
        .collect()
}

/// `matchFileMentionTrigger` on the text before the caret: the `@query`
/// range and the query, when the caret ends one.
pub fn trigger(before_caret: &str) -> Option<(Range<usize>, String)> {
    let at = before_caret.rfind('@')?;
    let query = &before_caret[at + 1..];
    if query.chars().count() > MAX_FILE_MENTION_LENGTH
        || query
            .chars()
            .any(|c| c.is_whitespace() || c == '@' || c == CHIP)
    {
        return None;
    }
    let lead = before_caret[..at].chars().next_back();
    if !lead.is_none_or(|c| c.is_whitespace() || c == '(' || c == CHIP) {
        return None;
    }
    Some((at..before_caret.len(), query.to_string()))
}

/// The files of one checkout, read once a minute at most.
#[derive(Clone, Debug)]
pub struct FileList {
    pub root: String,
    pub files: Arc<Vec<String>>,
    pub read_at: Instant,
}

impl FileList {
    pub fn fresh_for(&self, root: &str) -> bool {
        self.root == root && self.read_at.elapsed() < STALE
    }
}

/// One row of the popup: a file for `@`, a command for `/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem {
    File(String),
    Command(super::slash::Command),
}

/// The open popup.
#[derive(Clone, Debug, Default)]
pub struct MentionMenu {
    /// The typed `@query` or `/query` in the field.
    pub range: Range<usize>,
    pub options: Vec<MenuItem>,
    pub selected: usize,
}

impl MentionMenu {
    pub fn commands(&self) -> bool {
        matches!(self.options.first(), Some(MenuItem::Command(_)))
    }
}

impl MentionMenu {
    pub fn move_by(&mut self, delta: isize) {
        if self.options.is_empty() {
            return;
        }
        let count = self.options.len() as isize;
        self.selected = ((self.selected as isize + delta).rem_euclid(count)) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<String> {
        [
            "src/app.rs",
            "src/payment.ts",
            "src/paymentPix.ts",
            "lib/app.rs",
            "docs/readme.md",
        ]
        .iter()
        .map(|path| path.to_string())
        .collect()
    }

    #[test]
    fn ranks_like_the_desktop() {
        assert_eq!(
            filter(&files(), "payment.ts"),
            vec!["src/payment.ts".to_string()]
        );
        assert_eq!(
            filter(&files(), "pay"),
            vec![
                "src/payment.ts".to_string(),
                "src/paymentPix.ts".to_string()
            ]
        );
        assert_eq!(filter(&files(), "app.rs").len(), 2);
        assert_eq!(filter(&files(), "").len(), 5);
        assert_eq!(rank("src/a/b.rs", "a/b.rs"), 4);
        assert!(filter(&files(), "zzz").is_empty());
    }

    #[test]
    fn the_trigger_needs_a_boundary_before_the_at() {
        assert_eq!(trigger("veja @src/a"), Some((5..11, "src/a".into())));
        assert_eq!(trigger("@"), Some((0..1, String::new())));
        assert_eq!(trigger("(@app"), Some((1..5, "app".into())));
        assert_eq!(trigger("email@host"), None);
        assert_eq!(trigger("@a b"), None);
        let after_chip = format!("x{CHIP}@lib");
        assert!(trigger(&after_chip).is_some());
    }

    #[test]
    fn the_highlight_wraps() {
        let mut menu = MentionMenu {
            options: vec![MenuItem::File("a".into()), MenuItem::File("b".into())],
            ..MentionMenu::default()
        };
        menu.move_by(-1);
        assert_eq!(menu.selected, 1);
        menu.move_by(1);
        assert_eq!(menu.selected, 0);
    }
}
