//! ⌘/ and ⇧⌥A (the Pierre editor's `toggleComment` / `toggleBlockComment`):
//! each language's comment tokens by the file's name (`languages.js`, with
//! `//` and `/* */` for the rest), a line comment aligned to the lines'
//! least indentation, and a block comment around the selection, both
//! taken off again when already there. A language without line comments
//! wraps the whole lines in its block comment.

use std::ops::Range;

use super::lines::{Change, Doc, Edit, indent_of, remap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommentTokens {
    pub line: Option<&'static str>,
    pub block: (&'static str, &'static str),
}

const HTML: (&str, &str) = ("<!--", "-->");

/// The language for a file name (`getFiletypeFromFileName`), only for the
/// languages with tokens of their own.
fn language(path: &str) -> Option<&'static str> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let by_name = |key: &str| -> Option<&'static str> {
        Some(match key {
            "sql" => "sql",
            "rb" | "rbw" | "rake" | "gemspec" | "jbuilder" | "builder" | "rabl" | "arb" | "ru"
            | "podspec" | "Gemfile" | "Rakefile" | "Guardfile" | "Capfile" | "Berksfile"
            | "Brewfile" | "Vagrantfile" | "Thorfile" | "Appraisals" | "Dangerfile" => "ruby",
            "rst" => "rst",
            "coffee" => "coffeescript",
            "bat" | "cmd" => "cmd",
            "jl" => "julia",
            "yaml" | "yml" => "yaml",
            "md" | "markdown" => "markdown",
            "sh" | "bash" | "zsh" => "zsh",
            "Makefile" | "makefile" | "mk" => "makefile",
            "hbs" | "handlebars" => "handlebars",
            "ini" | "cfg" => "ini",
            "ps1" | "psm1" | "psd1" => "powershell",
            "vb" | "vbs" | "bas" => "vb",
            "xml" => "xml",
            "lua" => "lua",
            "html" | "htm" => "html",
            "diff" | "patch" => "diff",
            "r" | "R" => "r",
            "fs" | "fsi" | "fsx" => "fsharp",
            "jade" | "pug" => "pug",
            "pm" | "t" => "perl",
            "cls" | "tex" | "ltx" | "sty" => "tex",
            "clj" | "cljs" | "cljc" => "clojure",
            "css" => "css",
            "py" | "pyw" | "pyi" | "BUCK" | "BUILD" | "PACKAGE" | "WORKSPACE" | "Tiltfile"
            | "bazel" | "bxl" | "bzl" | "bzlmod" | "star" | "sky" => "python",
            "env" => "dotenv",
            "Dockerfile" | "dockerfile" => "dockerfile",
            "razor" | "cshtml" => "razor",
            _ => return None,
        })
    };
    by_name(name).or_else(|| {
        let (_, extension) = name.rsplit_once('.')?;
        by_name(extension)
    })
}

/// `resolveCommentConfig`
pub fn comment_tokens(path: &str) -> CommentTokens {
    let (line, block) = match language(path) {
        Some("sql") => (Some("--"), None),
        Some("ruby") => (Some("#"), Some(("=begin", "=end"))),
        Some("rst") => (Some(".."), None),
        Some("coffeescript") => (Some("#"), Some(("###", "###"))),
        Some("cmd") => (Some("@REM"), None),
        Some("julia") => (Some("#"), Some(("#=", "=#"))),
        Some("markdown" | "xml" | "html" | "razor") => (None, Some(HTML)),
        Some("handlebars") => (None, Some(("{{!--", "--}}"))),
        Some("ini") => (Some(";"), Some((";", " "))),
        Some("powershell") => (Some("#"), Some(("<#", "#>"))),
        Some("vb") => (Some("'"), None),
        Some("lua") => (Some("--"), Some(("--[[", "]]"))),
        Some("diff") => (Some("#"), Some(("#", " "))),
        Some("fsharp") => (Some("//"), Some(("(*", "*)"))),
        Some("pug") => (Some("//-"), None),
        Some("tex") => (Some("%"), None),
        Some("clojure") => (Some(";;"), None),
        Some("css") => (None, None),
        Some("python") => (Some("#"), Some(("\"\"\"", "\"\"\""))),
        Some("yaml" | "zsh" | "makefile" | "r" | "perl" | "dotenv" | "dockerfile") => {
            (Some("#"), None)
        }
        _ => (Some("//"), None),
    };
    CommentTokens {
        line,
        block: block.unwrap_or(("/*", "*/")),
    }
}

struct Line {
    line: usize,
    /// Where the token already is, if it starts the text.
    comment: Option<usize>,
    empty: bool,
    indent: usize,
}

/// `resolveLineCommentEdits`: the token at the lines' least indentation on
/// every line (blank lines skipped, unless the only one), or off every
/// line when all of them have it.
fn line_comment(doc: &Doc, selection: &Range<usize>, reversed: bool, token: &str) -> Change {
    let (first, last) = doc.block(selection);
    let mut lines: Vec<Line> = (first..=last)
        .map(|line| {
            let text = doc.line(line);
            let indent = indent_of(text);
            Line {
                line,
                comment: text[indent..].starts_with(token).then_some(indent),
                empty: indent == text.len(),
                indent,
            }
        })
        .collect();
    if let Some(least) = lines
        .iter()
        .filter(|line| !line.empty)
        .map(|line| line.indent)
        .min()
    {
        for line in lines.iter_mut().filter(|line| !line.empty) {
            line.indent = least;
        }
    }
    let single = lines.len() == 1;
    let comment = lines
        .iter()
        .any(|line| line.comment.is_none() && (!line.empty || single));
    let mut edits = Vec::new();
    for line in &lines {
        let start = doc.start(line.line);
        if comment {
            if line.empty && !single {
                continue;
            }
            let at = start + line.indent;
            edits.push(Edit {
                range: at..at,
                text: format!("{token} "),
            });
        } else if let Some(column) = line.comment {
            let at = start + column;
            let space = doc.line(line.line)[column + token.len()..].starts_with(' ');
            edits.push(Edit {
                range: at..at + token.len() + usize::from(space),
                text: String::new(),
            });
        }
    }
    let selection = remap(selection.start, &edits)..remap(selection.end, &edits);
    Change {
        edits,
        selection,
        reversed,
    }
}

const MARGIN: usize = 50;

fn boundary_back(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn boundary_forward(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

fn whitespace_len(character: Option<char>) -> usize {
    character
        .filter(|c| c.is_whitespace())
        .map_or(0, char::len_utf8)
}

/// A block comment around `from..to` or just inside it
/// (`findBlockComment`): what deleting it takes, and the text it holds.
struct Found {
    open: Range<usize>,
    close: Range<usize>,
    content: Range<usize>,
}

fn find_block(text: &str, (open, close): (&str, &str), from: usize, to: usize) -> Option<Found> {
    let before = &text[boundary_forward(text, from.saturating_sub(MARGIN))..from];
    let after = &text[to..boundary_back(text, to + MARGIN)];
    let space_before = before.len() - before.trim_end().len();
    let space_after = after.len() - after.trim_start().len();
    let kept = &before[..before.len() - space_before];
    if kept.ends_with(open) && after[space_after..].starts_with(close) {
        let one_before = whitespace_len(before[kept.len()..].chars().next());
        let one_after = whitespace_len(after[..space_after].chars().next_back());
        let open_start = from - space_before - open.len();
        let close_end = to + space_after + close.len();
        return Some(Found {
            open: open_start..from - space_before + one_before,
            close: to + space_after - one_after..close_end,
            content: from..to,
        });
    }
    let inside = &text[from..to];
    let start_text = if to - from <= 100 {
        inside
    } else {
        &text[from..boundary_back(text, from + MARGIN)]
    };
    let end_text = if to - from <= 100 {
        inside
    } else {
        &text[boundary_forward(text, to - MARGIN)..to]
    };
    let start_space = start_text.len() - start_text.trim_start().len();
    let close_start =
        (to - (end_text.len() - end_text.trim_end().len())).checked_sub(close.len())?;
    if !start_text[start_space..].starts_with(open)
        || text.get(close_start..close_start + close.len()) != Some(close)
    {
        return None;
    }
    let open_start = from + start_space;
    let open_end =
        open_start + open.len() + whitespace_len(text[open_start + open.len()..].chars().next());
    if close_start < open_start + open.len() {
        return None;
    }
    let close_delete = (close_start - whitespace_len(text[..close_start].chars().next_back()))
        .max(open_end.min(close_start));
    Some(Found {
        open: open_start..open_end,
        close: close_delete..close_start + close.len(),
        content: open_end..close_delete,
    })
}

/// `mapOffset`: an offset carried through the edits, an insertion right on
/// it put before (`association < 0`) or after it.
fn map_offset(offset: usize, edits: &[Edit], association: i8) -> usize {
    let mut delta: isize = 0;
    for edit in edits {
        let length = edit.text.len() as isize;
        if edit.range.end < offset {
            delta += length - edit.range.len() as isize;
            continue;
        }
        if offset < edit.range.start {
            break;
        }
        if edit.range.is_empty() && offset == edit.range.start {
            if association < 0 {
                break;
            }
            delta += length;
            continue;
        }
        let start = edit.range.start as isize + delta;
        if offset == edit.range.end {
            return (start + length) as usize;
        }
        return (start + if association > 0 { length } else { 0 }) as usize;
    }
    (offset as isize + delta) as usize
}

/// `resolveBlockCommentEdits` for one selection; `linewise` takes its whole
/// lines from the first one's indentation.
fn block_comment(
    doc: &Doc,
    selection: &Range<usize>,
    reversed: bool,
    tokens: (&str, &str),
    linewise: bool,
) -> Option<Change> {
    let (open, close) = tokens;
    let (from, to) = if linewise {
        let (first, last) = doc.block(selection);
        (doc.start(first) + indent_of(doc.line(first)), doc.end(last))
    } else {
        (selection.start, selection.end)
    };
    let found = find_block(doc.text, tokens, from, to);
    let edits = match &found {
        Some(found) => vec![
            Edit {
                range: found.open.clone(),
                text: String::new(),
            },
            Edit {
                range: found.close.clone(),
                text: String::new(),
            },
        ],
        None if from == to => vec![Edit {
            range: from..to,
            text: format!("{open}  {close}"),
        }],
        None => vec![
            Edit {
                range: from..from,
                text: format!("{open} "),
            },
            Edit {
                range: to..to,
                text: format!(" {close}"),
            },
        ],
    };
    let selection = if linewise {
        remap(selection.start, &edits)..remap(selection.end, &edits)
    } else {
        match &found {
            Some(found) => {
                map_offset(found.content.start, &edits, 1)
                    ..map_offset(found.content.end, &edits, -1)
            }
            None if from == to => {
                let at = map_offset(from, &edits, -1) + open.len() + 1;
                at..at
            }
            None => map_offset(from, &edits, 1)..map_offset(to, &edits, -1),
        }
    };
    Some(Change {
        edits,
        selection,
        reversed,
    })
}

/// ⌘/: line comments, or the block comment over whole lines without them.
pub fn toggle_comment(
    doc: &Doc,
    selection: &Range<usize>,
    reversed: bool,
    tokens: CommentTokens,
) -> Option<Change> {
    match tokens.line {
        Some(token) => Some(line_comment(doc, selection, reversed, token)),
        None => block_comment(doc, selection, reversed, tokens.block, true),
    }
}

/// ⇧⌥A: the block comment around the selection.
pub fn toggle_block_comment(
    doc: &Doc,
    selection: &Range<usize>,
    reversed: bool,
    tokens: CommentTokens,
) -> Option<Change> {
    block_comment(doc, selection, reversed, tokens.block, false)
}

#[cfg(test)]
#[path = "comments_tests.rs"]
mod tests;
