//! File paths in parsed markdown as chips the view can open: a pass over
//! the blocks that marks each known path's bytes (`SpanStyle::file`) with a
//! link the renderer hands back as a `FileRequest`, for the view that owns
//! those files to open (a pull request's changed files, in its code tab).
//! A path already inside a link is left as it is; one written as code
//! becomes the chip.

use std::ops::Range;

use super::parse::{Block, Inline, Span, SpanStyle};

/// The link a file chip carries, before its path.
pub const LINK_PREFIX: &str = "dcc-file:";

/// The file a chip asked to open, until the view that owns it takes it.
#[derive(Default)]
pub struct FileRequest(pub Option<String>);

impl gpui::Global for FileRequest {}

/// Marks every occurrence of `paths` in the text of `blocks`.
pub fn mark(blocks: &mut [Block], paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    // The longest first, so `src/a.test.ts` is not cut at `src/a.ts`'s end.
    let mut paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    paths.sort_by_key(|path| std::cmp::Reverse(path.len()));
    mark_blocks(blocks, &paths);
}

fn mark_blocks(blocks: &mut [Block], paths: &[&str]) {
    for block in blocks {
        match block {
            Block::Paragraph(inline) | Block::Heading { text: inline, .. } => {
                mark_inline(inline, paths)
            }
            Block::List { items, .. } => {
                for item in items {
                    mark_blocks(&mut item.blocks, paths);
                }
            }
            Block::Quote(inner) => mark_blocks(inner, paths),
            Block::Table { header, rows, .. } => {
                for cell in header.iter_mut().chain(rows.iter_mut().flatten()) {
                    mark_inline(cell, paths);
                }
            }
            Block::Code { .. } | Block::Rule => {}
        }
    }
}

/// A path ends where a name could not go on.
fn boundary(text: &str, at: usize, before: bool) -> bool {
    let near = if before {
        text[..at].chars().next_back()
    } else {
        text[at..].chars().next()
    };
    near.is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '/')))
}

pub(crate) fn find(text: &str, paths: &[&str]) -> Vec<(Range<usize>, String)> {
    let mut found: Vec<(Range<usize>, String)> = Vec::new();
    for path in paths {
        let mut from = 0;
        while let Some(at) = text[from..].find(path) {
            let range = from + at..from + at + path.len();
            from = range.end;
            let free = !found
                .iter()
                .any(|(taken, _)| taken.start < range.end && range.start < taken.end);
            if free && boundary(text, range.start, true) && boundary(text, range.end, false) {
                found.push((range, path.to_string()));
            }
        }
    }
    found.sort_by_key(|(range, _)| range.start);
    found
}

fn mark_inline(inline: &mut Inline, paths: &[&str]) {
    for (range, path) in find(&inline.text, paths) {
        let linked = inline.spans.iter().any(|span| {
            span.link.is_some() && span.range.start < range.end && range.start < span.range.end
        });
        if linked {
            continue;
        }
        // The chip replaces what styled those bytes (a code span, mostly).
        let mut spans: Vec<Span> = Vec::with_capacity(inline.spans.len() + 2);
        for span in inline.spans.drain(..) {
            if span.range.end <= range.start || span.range.start >= range.end {
                spans.push(span);
                continue;
            }
            if span.range.start < range.start {
                spans.push(Span {
                    range: span.range.start..range.start,
                    ..span.clone()
                });
            }
            if span.range.end > range.end {
                spans.push(Span {
                    range: range.end..span.range.end,
                    ..span
                });
            }
        }
        spans.push(Span {
            range,
            style: SpanStyle {
                file: true,
                ..SpanStyle::default()
            },
            link: Some(format!("{LINK_PREFIX}{path}")),
        });
        spans.sort_by_key(|span| span.range.start);
        inline.spans = spans;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse;

    fn chips(text: &str, paths: &[&str]) -> Vec<String> {
        let mut blocks = parse(text);
        let paths: Vec<String> = paths.iter().map(|path| path.to_string()).collect();
        mark(&mut blocks, &paths);
        let Some(Block::Paragraph(inline)) = blocks.first() else {
            return Vec::new();
        };
        inline
            .spans
            .iter()
            .filter(|span| span.style.file)
            .map(|span| inline.text[span.range.clone()].to_string())
            .collect()
    }

    #[test]
    fn known_paths_become_chips_whole_and_only_as_whole_names() {
        let paths = ["src/lib/pay.ts", "src/lib/pay.test.ts"];
        assert_eq!(
            chips("Não vi: src/lib/pay.ts, src/lib/pay.test.ts.", &paths),
            ["src/lib/pay.ts", "src/lib/pay.test.ts"]
        );
        assert_eq!(
            chips("Ver `src/lib/pay.ts` de novo", &paths),
            ["src/lib/pay.ts"]
        );
        // Part of a longer name, or already a link: left alone.
        assert!(chips("em vendor/src/lib/pay.ts", &paths).is_empty());
        assert!(chips("[src/lib/pay.ts](https://example.com)", &paths).is_empty());
    }
}
