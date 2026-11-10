//! `@Agent` in parsed markdown: a pass over the blocks that marks each
//! mention's bytes with the agent's color (`SpanStyle::mention`), which the
//! renderer draws as a pill. It runs once, when the block model is made
//! (`room_markdown` keeps it), so a frame only draws. Code, and text that is
//! already a link, are left as they are.

use std::ops::Range;

use super::parse::{Block, Inline, Span, SpanStyle};
use crate::data::agent_mentions::MentionNames;

/// Marks the mentions in every text of `blocks`.
pub fn mark(blocks: &mut [Block], names: &MentionNames) {
    if names.is_empty() {
        return;
    }
    for block in blocks {
        match block {
            Block::Paragraph(inline) | Block::Heading { text: inline, .. } => {
                mark_inline(inline, names)
            }
            Block::List { items, .. } => {
                for item in items {
                    mark(&mut item.blocks, names);
                }
            }
            Block::Quote(inner) => mark(inner, names),
            Block::Table { header, rows, .. } => {
                for cell in header.iter_mut().chain(rows.iter_mut().flatten()) {
                    mark_inline(cell, names);
                }
            }
            Block::Code { .. } | Block::Rule => {}
        }
    }
}

fn mark_inline(inline: &mut Inline, names: &MentionNames) {
    let found: Vec<(Range<usize>, u32)> = names
        .find(&inline.text)
        .into_iter()
        .filter(|(range, _)| {
            !inline.spans.iter().any(|span| {
                (span.style.code || span.link.is_some())
                    && span.range.start < range.end
                    && range.start < span.range.end
            })
        })
        .collect();
    for (range, rgb) in found {
        paint(&mut inline.spans, range, rgb);
    }
}

/// Gives `range` the mention color, cutting the spans it crosses.
fn paint(spans: &mut Vec<Span>, range: Range<usize>, rgb: u32) {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len() + 2);
    let mut cursor = range.start;
    for span in spans.drain(..) {
        if span.range.end <= range.start || span.range.start >= range.end {
            out.push(span);
            continue;
        }
        if span.range.start < range.start {
            out.push(Span {
                range: span.range.start..range.start,
                ..span.clone()
            });
        }
        let from = span.range.start.max(range.start);
        let to = span.range.end.min(range.end);
        if from > cursor {
            out.push(pill(cursor..from, rgb));
        }
        out.push(Span {
            range: from..to,
            style: SpanStyle {
                mention: Some(rgb),
                ..span.style
            },
            link: span.link.clone(),
        });
        cursor = to;
        if span.range.end > range.end {
            out.push(Span {
                range: range.end..span.range.end,
                ..span
            });
        }
    }
    if cursor < range.end {
        out.push(pill(cursor..range.end, rgb));
    }
    out.sort_by_key(|span| span.range.start);
    *spans = out;
}

fn pill(range: Range<usize>, rgb: u32) -> Span {
    Span {
        range,
        style: SpanStyle {
            mention: Some(rgb),
            ..SpanStyle::default()
        },
        link: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse;

    fn names() -> MentionNames {
        MentionNames::new([("Revisor", "violet"), ("QA Mobile", "green")], |c| {
            c.len() as u32
        })
    }

    fn pills(markdown: &str) -> Vec<String> {
        let mut blocks = parse(markdown);
        mark(&mut blocks, &names());
        let Some(Block::Paragraph(inline)) = blocks.first() else {
            panic!("a paragraph");
        };
        inline
            .spans
            .iter()
            .filter(|span| span.style.mention.is_some())
            .map(|span| inline.text[span.range.clone()].to_string())
            .collect()
    }

    #[test]
    fn a_mention_in_prose_becomes_a_pill() {
        assert_eq!(pills("Passo para @Revisor agora"), ["@Revisor"]);
        assert_eq!(pills("@QA Mobile e @Revisor"), ["@QA Mobile", "@Revisor"]);
    }

    #[test]
    fn a_mention_inside_bold_keeps_the_bold() {
        let mut blocks = parse("veja **@Revisor ok** aqui");
        mark(&mut blocks, &names());
        let Some(Block::Paragraph(inline)) = blocks.first() else {
            panic!("a paragraph");
        };
        let pill = inline
            .spans
            .iter()
            .find(|span| span.style.mention.is_some())
            .expect("a pill");
        assert!(pill.style.bold);
        assert_eq!(&inline.text[pill.range.clone()], "@Revisor");
        // The bold part after it is still there, still bold.
        assert!(
            inline
                .spans
                .iter()
                .any(|span| span.style.bold && span.style.mention.is_none())
        );
        let starts: Vec<usize> = inline.spans.iter().map(|span| span.range.start).collect();
        assert!(starts.is_sorted());
    }

    #[test]
    fn code_unknown_names_and_emails_stay_text() {
        assert!(pills("use `@Revisor` assim").is_empty());
        assert!(pills("fale com @Ninguem").is_empty());
        assert!(pills("escreva a dev@Revisor.com").is_empty());
        assert!(pills("[@Revisor](https://x.dev)").is_empty());
    }
}
