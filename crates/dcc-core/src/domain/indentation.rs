//! How a file is indented when typing in it (`indentation.ts`): the
//! project's `.editorconfig` first (its last matching section wins), then
//! what the file already does, then two spaces; and what Enter inserts.

/// Tabs or a number of spaces, and how wide a tab is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIndentation {
    pub use_tabs: bool,
    /// Columns per level; also a tab's width.
    pub size: usize,
}

pub const DEFAULT_INDENTATION: FileIndentation = FileIndentation {
    use_tabs: false,
    size: 2,
};

#[derive(Default)]
struct Partial {
    use_tabs: Option<bool>,
    size: Option<usize>,
}

/// The sections of an `.editorconfig`, in order: (glob, properties).
fn parse_editor_config(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
            sections.push((line[1..line.len() - 1].trim().to_string(), Vec::new()));
            continue;
        }
        let (Some(section), Some(at)) = (sections.last_mut(), line.find('=')) else {
            continue;
        };
        section.1.push((
            line[..at].trim().to_lowercase(),
            line[at + 1..].trim().to_lowercase(),
        ));
    }
    sections
}

/// `{a,b}` spelled out: every alternative of the glob.
fn expand_braces(glob: &str) -> Vec<String> {
    let Some(open) = glob.find('{') else {
        return vec![glob.to_string()];
    };
    let Some(close) = glob[open..].find('}').map(|at| open + at) else {
        return vec![glob.to_string()];
    };
    let (head, body, tail) = (&glob[..open], &glob[open + 1..close], &glob[close + 1..]);
    body.split(',')
        .flat_map(|option| expand_braces(&format!("{head}{option}{tail}")))
        .collect()
}

/// `*` within a segment, `**` across them, `?` one character, `[…]` a class.
fn matches(glob: &[char], text: &[char]) -> bool {
    match glob.first() {
        None => text.is_empty(),
        Some('*') if glob.get(1) == Some(&'*') => {
            (0..=text.len()).any(|skip| matches(&glob[2..], &text[skip..]))
        }
        Some('*') => {
            let limit = text.iter().position(|c| *c == '/').unwrap_or(text.len());
            (0..=limit).any(|skip| matches(&glob[1..], &text[skip..]))
        }
        Some('?') => text.first().is_some_and(|c| *c != '/') && matches(&glob[1..], &text[1..]),
        Some('[') => {
            let Some(close) = glob.iter().position(|c| *c == ']') else {
                return text.first() == Some(&'[') && matches(&glob[1..], &text[1..]);
            };
            let Some(first) = text.first() else {
                return false;
            };
            let class = &glob[1..close];
            let (negated, class) = match class.first() {
                Some('!') => (true, &class[1..]),
                _ => (false, class),
            };
            let mut found = false;
            let mut index = 0;
            while index < class.len() {
                if index + 2 < class.len() && class[index + 1] == '-' {
                    found |= (class[index]..=class[index + 2]).contains(first);
                    index += 3;
                } else {
                    found |= class[index] == *first;
                    index += 1;
                }
            }
            found != negated && matches(&glob[close + 1..], &text[1..])
        }
        Some(c) => text.first() == Some(c) && matches(&glob[1..], &text[1..]),
    }
}

/// `matchesEditorConfigGlob`: without `/` the glob names a file at any
/// depth; with one it is anchored at the `.editorconfig` folder.
pub fn matches_editor_config_glob(glob: &str, relative_path: &str) -> bool {
    let anchored = glob.contains('/');
    let pattern = if anchored {
        glob.trim_start_matches('/')
    } else {
        glob
    };
    let text: Vec<char> = relative_path.chars().collect();
    expand_braces(pattern).iter().any(|option| {
        let glob: Vec<char> = option.chars().collect();
        if anchored {
            return matches(&glob, &text);
        }
        (0..=text.len())
            .filter(|start| *start == 0 || text[start - 1] == '/')
            .any(|start| matches(&glob, &text[start..]))
    })
}

fn positive(value: Option<&str>) -> Option<usize> {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|size| (1..=16).contains(size))
}

fn from_editor_config(text: &str, relative_path: &str) -> Partial {
    let mut props: Vec<(String, String)> = Vec::new();
    for (glob, section) in parse_editor_config(text) {
        if !matches_editor_config_glob(&glob, relative_path) {
            continue;
        }
        for (key, value) in section {
            props.retain(|(existing, _)| *existing != key);
            props.push((key, value));
        }
    }
    let get = |key: &str| {
        props
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    let use_tabs = match get("indent_style") {
        Some("tab") => Some(true),
        Some("space") => Some(false),
        _ => None,
    };
    let tab_width = positive(get("tab_width"));
    let indent_size = match get("indent_size") {
        Some("tab") => tab_width,
        other => positive(other),
    };
    // A tab is drawn tab_width wide; spaces step by indent_size; each falls
    // back to the other.
    let size = if use_tabs == Some(true) {
        tab_width.or(indent_size)
    } else {
        indent_size.or(tab_width)
    };
    Partial { use_tabs, size }
}

/// `detectIndentation`: tabs or spaces, and the usual step between levels.
fn detect(content: &str) -> Partial {
    let (mut tab_lines, mut space_lines, mut previous) = (0, 0, 0usize);
    let mut steps: Vec<(usize, usize)> = Vec::new();
    for line in content.lines().take(2_000) {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('\t') {
            tab_lines += 1;
            continue;
        }
        let spaces = line.len() - line.trim_start_matches(' ').len();
        // ` * ` continues a block comment; it is not a level.
        if spaces > 0 && !line.trim_start_matches(' ').starts_with('*') {
            space_lines += 1;
        }
        let step = spaces.wrapping_sub(previous);
        if matches!(step, 2 | 4 | 8) {
            match steps.iter_mut().find(|(size, _)| *size == step) {
                Some(entry) => entry.1 += 1,
                None => steps.push((step, 1)),
            }
        }
        previous = spaces;
    }
    if tab_lines == 0 && space_lines == 0 {
        return Partial::default();
    }
    if tab_lines > space_lines {
        return Partial {
            use_tabs: Some(true),
            size: None,
        };
    }
    let mut best: Option<(usize, usize)> = None;
    for (step, count) in steps {
        if best.is_none_or(|(_, most)| count > most) {
            best = Some((step, count));
        }
    }
    Partial {
        use_tabs: Some(false),
        size: best.map(|(step, _)| step),
    }
}

/// `resolveFileIndentation`
pub fn resolve_file_indentation(
    editor_config: Option<&str>,
    relative_path: &str,
    content: &str,
) -> FileIndentation {
    let configured = editor_config
        .map(|text| from_editor_config(text, relative_path))
        .unwrap_or_default();
    let detected = detect(content);
    let use_tabs = configured
        .use_tabs
        .or(detected.use_tabs)
        .unwrap_or(DEFAULT_INDENTATION.use_tabs);
    let size = configured
        .size
        .or(if use_tabs { Some(4) } else { detected.size })
        .unwrap_or(DEFAULT_INDENTATION.size);
    FileIndentation { use_tabs, size }
}

/// `indentUnit`: what one level is.
pub fn indent_unit(indentation: FileIndentation) -> String {
    if indentation.use_tabs {
        "\t".to_string()
    } else {
        " ".repeat(indentation.size)
    }
}

/// What Enter inserts, and where the caret lands (lines down, byte in it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineBreak {
    pub text: String,
    pub caret_line_offset: usize,
    pub caret_column: usize,
}

/// `lineBreakInsertion`: the line's own indentation, one level more after
/// an opening bracket, and a closing bracket right after the caret pushed
/// to its own line. `column` is a byte offset into `line`.
pub fn line_break_insertion(line: &str, column: usize, unit: &str, eol: &str) -> LineBreak {
    let column = column.min(line.len());
    let (before, after) = line.split_at(column);
    let base: String = before
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let closer = match before.trim_end().chars().last() {
        Some('{') => Some('}'),
        Some('(') => Some(')'),
        Some('[') => Some(']'),
        _ => None,
    };
    let Some(closer) = closer else {
        return LineBreak {
            text: format!("{eol}{base}"),
            caret_line_offset: 1,
            caret_column: base.len(),
        };
    };
    let inner = format!("{base}{unit}");
    let text = if after.trim_start().starts_with(closer) {
        format!("{eol}{inner}{eol}{base}")
    } else {
        format!("{eol}{inner}")
    };
    LineBreak {
        text,
        caret_line_offset: 1,
        caret_column: inner.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "
root = true

[*]
indent_style = space
indent_size = 2

[*.{rs,py}]
indent_size = 4

[Makefile]
indent_style = tab

[apps/legacy/**]
indent_style = tab
tab_width = 8
";

    fn indentation(use_tabs: bool, size: usize) -> FileIndentation {
        FileIndentation { use_tabs, size }
    }

    #[test]
    fn matches_names_at_any_depth_and_anchors_globs_with_a_slash() {
        assert!(matches_editor_config_glob(
            "*.{rs,py}",
            "crates/core/src/lib.rs"
        ));
        assert!(!matches_editor_config_glob("*.{rs,py}", "src/main.ts"));
        assert!(matches_editor_config_glob("Makefile", "tools/Makefile"));
        assert!(matches_editor_config_glob(
            "apps/legacy/**",
            "apps/legacy/a/b.js"
        ));
        assert!(!matches_editor_config_glob(
            "apps/legacy/**",
            "x/apps/legacy/b.js"
        ));
        assert!(matches_editor_config_glob("[Mm]akefile", "makefile"));
    }

    #[test]
    fn follows_the_last_matching_editorconfig_section() {
        let resolve = |path| resolve_file_indentation(Some(CONFIG), path, "");
        assert_eq!(resolve("src/App.tsx"), indentation(false, 2));
        assert_eq!(resolve("crates/lib.rs"), indentation(false, 4));
        assert_eq!(resolve("Makefile"), indentation(true, 2));
        assert_eq!(resolve("apps/legacy/old.js"), indentation(true, 8));
    }

    #[test]
    fn falls_back_to_what_the_file_does() {
        assert_eq!(
            resolve_file_indentation(None, "a.ts", "if (a) {\n\treturn 1;\n}\n"),
            indentation(true, 4)
        );
        assert_eq!(
            resolve_file_indentation(None, "a.py", "def f():\n    if x:\n        return 1\n"),
            indentation(false, 4)
        );
        assert_eq!(
            resolve_file_indentation(None, "a.txt", "plain\n"),
            indentation(false, 2)
        );
    }

    #[test]
    fn does_not_count_block_comment_stars_as_indentation() {
        let found = detect("/**\n * doc\n */\nconst a = 1;\n");
        assert_eq!((found.use_tabs, found.size), (None, None));
    }

    #[test]
    fn enter_keeps_the_lines_indentation() {
        assert_eq!(
            line_break_insertion("\t\tconst a = 1;", 15, "\t", "\n"),
            LineBreak {
                text: "\n\t\t".into(),
                caret_line_offset: 1,
                caret_column: 2,
            }
        );
    }

    #[test]
    fn enter_goes_one_level_in_after_an_opening_bracket() {
        assert_eq!(
            line_break_insertion("  if (a) {", 10, "  ", "\n"),
            LineBreak {
                text: "\n    ".into(),
                caret_line_offset: 1,
                caret_column: 4,
            }
        );
    }

    #[test]
    fn enter_moves_a_closing_bracket_right_after_the_caret_to_its_own_line() {
        assert_eq!(
            line_break_insertion("  call({})", 8, "  ", "\n"),
            LineBreak {
                text: "\n    \n  ".into(),
                caret_line_offset: 1,
                caret_column: 4,
            }
        );
    }
}
