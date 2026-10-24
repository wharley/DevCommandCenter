//! The code column's file tree (`code-files-tree.logic.ts`): folders before
//! files, a folder that only holds one folder shown as one row
//! (`src/features`), the rows on screen for the folders that are open, and
//! each file's git status (the worktree side wins over the index).

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodeTreeNode {
    pub name: String,
    pub path: String,
    pub children: BTreeMap<String, CodeTreeNode>,
    pub file: bool,
}

/// `buildCodeTree`
pub fn build_code_tree(paths: &[String]) -> CodeTreeNode {
    let mut root = CodeTreeNode::default();
    for file_path in paths {
        let parts: Vec<&str> = file_path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let Some((file_name, folders)) = parts.split_last() else {
            continue;
        };
        let mut current = &mut root;
        for (index, part) in folders.iter().enumerate() {
            current = current
                .children
                .entry(part.to_string())
                .or_insert_with(|| CodeTreeNode {
                    name: part.to_string(),
                    path: parts[..=index].join("/"),
                    ..CodeTreeNode::default()
                });
        }
        current.children.insert(
            file_name.to_string(),
            CodeTreeNode {
                name: file_name.to_string(),
                path: file_path.clone(),
                children: BTreeMap::new(),
                file: true,
            },
        );
    }
    root
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// `expanded` while open; `path` is the deepest folder of a chain.
    Folder {
        expanded: bool,
    },
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeTreeRow {
    pub kind: RowKind,
    pub path: String,
    /// `src/features` for a compacted chain.
    pub label: String,
    pub depth: usize,
    pub parent_path: Option<String>,
}

impl CodeTreeRow {
    pub fn is_folder(&self) -> bool {
        matches!(self.kind, RowKind::Folder { .. })
    }
}

/// ICU's root order for the punctuation names use, then digits, then
/// letters without case (`localeCompare`).
fn weight(character: char) -> (u8, u32) {
    const PUNCTUATION: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";
    if let Some(index) = PUNCTUATION.find(character) {
        return (0, index as u32);
    }
    if character.is_whitespace() {
        return (0, 0);
    }
    if character.is_ascii_digit() {
        return (1, character as u32);
    }
    (
        2,
        character.to_lowercase().next().unwrap_or(character) as u32,
    )
}

/// `left.name.localeCompare(right.name)`, close enough for file names:
/// case only breaks a tie, lowercase first.
pub fn locale_compare(left: &str, right: &str) -> Ordering {
    let primary = left.chars().map(weight).cmp(right.chars().map(weight));
    primary.then_with(|| {
        left.chars()
            .map(|character| character.is_uppercase())
            .cmp(right.chars().map(|character| character.is_uppercase()))
    })
}

fn sorted_children(node: &CodeTreeNode) -> Vec<&CodeTreeNode> {
    let mut children: Vec<&CodeTreeNode> = node.children.values().collect();
    children.sort_by(|left, right| match (left.file, right.file) {
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        _ => locale_compare(&left.name, &right.name),
    });
    children
}

/// `visibleCodeTreeRows`: top to bottom, for the folders that are open.
pub fn visible_rows(root: &CodeTreeNode, expanded: &HashSet<String>) -> Vec<CodeTreeRow> {
    fn walk(
        node: &CodeTreeNode,
        depth: usize,
        parent: Option<&str>,
        expanded: &HashSet<String>,
        rows: &mut Vec<CodeTreeRow>,
    ) {
        for child in sorted_children(node) {
            if child.file {
                rows.push(CodeTreeRow {
                    kind: RowKind::File,
                    path: child.path.clone(),
                    label: child.name.clone(),
                    depth,
                    parent_path: parent.map(str::to_string),
                });
                continue;
            }
            // A folder whose only entry is another folder reads as one row.
            let mut folder = child;
            let mut label = child.name.clone();
            while folder.children.len() == 1 {
                let only = folder.children.values().next().expect("one child");
                if only.file {
                    break;
                }
                folder = only;
                label = format!("{label}/{}", only.name);
            }
            let open = expanded.contains(&folder.path);
            rows.push(CodeTreeRow {
                kind: RowKind::Folder { expanded: open },
                path: folder.path.clone(),
                label,
                depth,
                parent_path: parent.map(str::to_string),
            });
            if open {
                walk(folder, depth + 1, Some(&folder.path), expanded, rows);
            }
        }
    }
    let mut rows = Vec::new();
    walk(root, 0, None, expanded, &mut rows);
    rows
}

/// `ancestorFolderPaths`: every folder above a file.
pub fn ancestor_folder_paths(file_path: &str) -> Vec<String> {
    let parts: Vec<&str> = file_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    (1..parts.len())
        .map(|index| parts[..index].join("/"))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Modified,
    Added,
    Deleted,
    Renamed,
    Conflict,
}

/// `CodeTreeGitStatus`: the letter shown and its color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeStatus {
    pub letter: char,
    pub tone: Tone,
}

fn tree_status(status: &str) -> Option<TreeStatus> {
    let (letter, tone) = match status {
        "M" => ('M', Tone::Modified),
        "A" => ('A', Tone::Added),
        "?" => ('U', Tone::Added),
        "D" => ('D', Tone::Deleted),
        "R" => ('R', Tone::Renamed),
        "C" => ('C', Tone::Renamed),
        "U" => ('!', Tone::Conflict),
        _ => return None,
    };
    Some(TreeStatus { letter, tone })
}

/// `codeTreeGitStatus`: per file, and the folders that hold a change.
pub fn git_status<'a>(
    staged: impl IntoIterator<Item = (&'a str, &'a str)>,
    unstaged: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> (HashMap<String, TreeStatus>, HashSet<String>) {
    let mut by_path = HashMap::new();
    for (path, status) in staged.into_iter().chain(unstaged) {
        if let Some(status) = tree_status(status) {
            by_path.insert(path.to_string(), status);
        }
    }
    let changed = by_path
        .keys()
        .flat_map(|path| ancestor_folder_paths(path))
        .collect();
    (by_path, changed)
}

/// `fileSetSignature`: moves when a file is created or deleted, not when one
/// is edited — then the file list is stale.
pub fn file_set_signature<'a>(changes: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut entries: Vec<String> = changes
        .into_iter()
        .filter(|(_, status)| *status != "M")
        .map(|(path, status)| format!("{status}:{path}"))
        .collect();
    entries.sort();
    entries.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Vec<String> {
        [
            "README.md",
            "apps/desktop/src/App.tsx",
            "apps/desktop/src/main.tsx",
            "apps/desktop/package.json",
            "crates/core/src/lib.rs",
        ]
        .map(str::to_string)
        .to_vec()
    }

    #[test]
    fn folders_come_before_files_everything_collapsed() {
        let rows = visible_rows(&build_code_tree(&paths()), &HashSet::new());
        let shown: Vec<(bool, &str)> = rows
            .iter()
            .map(|row| (row.is_folder(), row.label.as_str()))
            .collect();
        assert_eq!(
            shown,
            vec![
                (true, "apps/desktop"),
                (true, "crates/core/src"),
                (false, "README.md")
            ]
        );
    }

    #[test]
    fn a_compacted_chain_is_keyed_on_its_deepest_folder() {
        let expanded = HashSet::from(["apps/desktop".to_string()]);
        let rows = visible_rows(&build_code_tree(&paths()), &expanded);
        let shown: Vec<(&str, usize, Option<&str>)> = rows
            .iter()
            .map(|row| (row.label.as_str(), row.depth, row.parent_path.as_deref()))
            .collect();
        assert_eq!(
            shown,
            vec![
                ("apps/desktop", 0, None),
                ("src", 1, Some("apps/desktop")),
                ("package.json", 1, Some("apps/desktop")),
                ("crates/core/src", 0, None),
                ("README.md", 0, None),
            ]
        );
    }

    #[test]
    fn revealing_a_file_opens_the_way_down() {
        let expanded: HashSet<String> = ancestor_folder_paths("apps/desktop/src/App.tsx")
            .into_iter()
            .collect();
        let rows = visible_rows(&build_code_tree(&paths()), &expanded);
        assert!(
            rows.iter()
                .any(|row| row.path == "apps/desktop/src/App.tsx")
        );
    }

    #[test]
    fn the_worktree_status_wins_and_folders_above_a_change_are_marked() {
        let (by_path, changed) = git_status(
            [("apps/desktop/src/App.tsx", "A")],
            [("apps/desktop/src/App.tsx", "M"), ("notes/new.md", "?")],
        );
        assert_eq!(
            by_path["apps/desktop/src/App.tsx"],
            TreeStatus {
                letter: 'M',
                tone: Tone::Modified
            }
        );
        assert_eq!(
            by_path["notes/new.md"],
            TreeStatus {
                letter: 'U',
                tone: Tone::Added
            }
        );
        let mut changed: Vec<String> = changed.into_iter().collect();
        changed.sort();
        assert_eq!(
            changed,
            vec!["apps", "apps/desktop", "apps/desktop/src", "notes"]
        );
    }

    #[test]
    fn the_file_set_moves_only_when_a_file_comes_or_goes() {
        let base = file_set_signature([("a.ts", "M")]);
        assert_eq!(file_set_signature([("a.ts", "M"), ("b.ts", "M")]), base);
        assert_ne!(file_set_signature([("c.ts", "?")]), base);
    }

    #[test]
    fn names_sort_like_locale_compare() {
        let mut names = vec![
            "build.rs",
            "Cargo.toml",
            "_private",
            "-dash",
            "10.txt",
            "a",
            "B",
        ];
        names.sort_by(|left, right| locale_compare(left, right));
        assert_eq!(
            names,
            vec![
                "_private",
                "-dash",
                "10.txt",
                "a",
                "B",
                "build.rs",
                "Cargo.toml"
            ]
        );
    }
}
