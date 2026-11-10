//! Markdown to a small block model (GitHub flavour, like Streamdown):
//! paragraphs, headings, lists, code, quotes, rules and tables, with inline
//! bold/italic/strike/code/links as styled ranges of plain text. HTML in it
//! keeps its text and links (`super::html`).

use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    /// An address written as itself (`https://…`, `www.…`), drawn like the
    /// composer draws it: blue on a faint fill, the site's icon before it.
    pub site: bool,
    /// An `@Agent` of the team, in its mascot's `0xRRGGBB` (`super::mentions`).
    pub mention: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: SpanStyle,
    pub link: Option<String>,
}

/// Text with styled ranges (byte offsets into `text`) and the images that
/// sit in it (`super::image`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inline {
    pub text: String,
    pub spans: Vec<Span>,
    pub images: Vec<super::image::InlineImage>,
}

impl Inline {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.images.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Paragraph(Inline),
    Heading {
        level: u8,
        text: Inline,
    },
    List {
        /// The first number of an ordered list.
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Code {
        language: Option<String>,
        code: String,
    },
    Quote(Vec<Block>),
    Rule,
    Table {
        alignments: Vec<Alignment>,
        header: Vec<Inline>,
        rows: Vec<Vec<Inline>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    /// `Some` for a GFM task item, with whether it is done.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Default)]
pub(super) struct InlineBuilder {
    pub(super) inline: Inline,
    pub(super) style: SpanStyle,
    pub(super) link: Option<String>,
    /// A markdown image being read: its url, its alt so far, and the link
    /// around it.
    image: Option<(String, String, Option<String>)>,
}

impl InlineBuilder {
    pub(super) fn push(&mut self, text: &str, code: bool) {
        if text.is_empty() {
            return;
        }
        if let Some((_, alt, _)) = self.image.as_mut() {
            alt.push_str(text);
            return;
        }
        let start = self.inline.text.len();
        self.inline.text.push_str(text);
        let style = SpanStyle { code, ..self.style };
        if style != SpanStyle::default() || self.link.is_some() {
            self.inline.spans.push(Span {
                range: start..self.inline.text.len(),
                style,
                link: self.link.clone(),
            });
        }
    }

    /// An image at the end of the text so far.
    pub(super) fn push_image(
        &mut self,
        url: String,
        alt: String,
        height: Option<f32>,
        link: Option<String>,
    ) {
        let at = self.inline.text.len();
        self.inline.images.push(super::image::InlineImage {
            at,
            url,
            alt,
            height,
            link,
        });
    }

    fn start_image(&mut self, url: &str) {
        if super::image::loads(url) {
            self.image = Some((url.to_string(), String::new(), self.link.clone()));
        } else {
            // Shown as its alt, linking to it.
            self.link = Some(url.to_string());
        }
    }

    fn end_image(&mut self) {
        match self.image.take() {
            Some((url, alt, link)) => {
                self.push_image(url, alt.trim().to_string(), None, link.clone());
                self.link = link;
            }
            None => self.link = None,
        }
    }

    fn take(&mut self) -> Inline {
        let mut inline = std::mem::take(&mut self.inline);
        linkify(&mut inline);
        inline
    }
}

/// Bare addresses in plain text become links (GitHub's autolinks); text
/// that is code, a link already or styled stays as it is.
fn linkify(inline: &mut Inline) {
    let found = crate::ui::text_area::links::find(&inline.text);
    if found.is_empty() {
        return;
    }
    for link in found {
        let taken = inline
            .spans
            .iter()
            .any(|span| span.range.start < link.range.end && link.range.start < span.range.end);
        if taken {
            continue;
        }
        inline.spans.push(Span {
            range: link.range,
            style: SpanStyle {
                site: true,
                ..SpanStyle::default()
            },
            link: Some(link.url),
        });
    }
    inline.spans.sort_by_key(|span| span.range.start);
}

/// A container being filled while the parser walks the events.
enum Frame {
    Root(Vec<Block>),
    Quote(Vec<Block>),
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Item {
        task: Option<bool>,
        blocks: Vec<Block>,
    },
}

impl Frame {
    fn blocks(&mut self) -> Option<&mut Vec<Block>> {
        match self {
            Frame::Root(blocks) | Frame::Quote(blocks) => Some(blocks),
            Frame::Item { blocks, .. } => Some(blocks),
            Frame::List { .. } => None,
        }
    }
}

/// A table being read: alignments, header, body rows, the row being read and
/// whether the header is still open.
type TableFrame = (
    Vec<Alignment>,
    Vec<Inline>,
    Vec<Vec<Inline>>,
    Vec<Inline>,
    bool,
);

struct Builder {
    frames: Vec<Frame>,
    inline: InlineBuilder,
    code: Option<(Option<String>, String)>,
    heading: Option<u8>,
    table: Option<TableFrame>,
    html: super::html::Html,
}

impl Builder {
    fn push_block(&mut self, block: Block) {
        for frame in self.frames.iter_mut().rev() {
            if let Some(blocks) = frame.blocks() {
                blocks.push(block);
                return;
            }
        }
    }

    /// Text outside a paragraph (a tight list item) becomes one when the item closes.
    fn flush_loose_text(&mut self) {
        if !self.inline.inline.text.is_empty() {
            let inline = self.inline.take();
            self.push_block(Block::Paragraph(inline));
        }
    }
}

pub fn parse(markdown: &str) -> Vec<Block> {
    parse_with(markdown, false)
}

/// A message the person wrote: each line they broke stays a line, as chat
/// apps and GitHub comments show it (`remark-breaks`), not joined with the
/// next as CommonMark would.
pub fn parse_prompt(markdown: &str) -> Vec<Block> {
    parse_with(markdown, true)
}

fn parse_with(markdown: &str, keep_lines: bool) -> Vec<Block> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut builder = Builder {
        frames: vec![Frame::Root(Vec::new())],
        inline: InlineBuilder::default(),
        code: None,
        heading: None,
        table: None,
        html: Default::default(),
    };
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph | Tag::HtmlBlock => builder.flush_loose_text(),
                Tag::Heading { level, .. } => {
                    builder.flush_loose_text();
                    builder.heading = Some(match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    });
                }
                Tag::BlockQuote(_) => {
                    builder.flush_loose_text();
                    builder.frames.push(Frame::Quote(Vec::new()));
                }
                Tag::CodeBlock(kind) => {
                    builder.flush_loose_text();
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => info
                            .split_whitespace()
                            .next()
                            .filter(|lang| !lang.is_empty())
                            .map(str::to_string),
                        CodeBlockKind::Indented => None,
                    };
                    builder.code = Some((language, String::new()));
                }
                Tag::List(start) => {
                    builder.flush_loose_text();
                    builder.frames.push(Frame::List {
                        start,
                        items: Vec::new(),
                    });
                }
                Tag::Item => builder.frames.push(Frame::Item {
                    task: None,
                    blocks: Vec::new(),
                }),
                Tag::Table(alignments) => {
                    builder.table = Some((alignments, Vec::new(), Vec::new(), Vec::new(), false))
                }
                Tag::TableHead => {
                    if let Some(table) = builder.table.as_mut() {
                        table.4 = true;
                    }
                }
                Tag::TableRow | Tag::TableCell => {}
                Tag::Emphasis => builder.inline.style.italic = true,
                Tag::Strong => builder.inline.style.bold = true,
                Tag::Strikethrough => builder.inline.style.strike = true,
                Tag::Link { dest_url, .. } => builder.inline.link = Some(dest_url.to_string()),
                Tag::Image { dest_url, .. } => builder.inline.start_image(&dest_url),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::HtmlBlock => {
                    builder.flush_loose_text();
                    builder.html.end_block(&mut builder.inline);
                }
                TagEnd::Paragraph => {
                    let inline = builder.inline.take();
                    builder.push_block(Block::Paragraph(inline));
                }
                TagEnd::Heading(_) => {
                    let text = builder.inline.take();
                    let level = builder.heading.take().unwrap_or(1);
                    builder.push_block(Block::Heading { level, text });
                }
                TagEnd::BlockQuote(_) => {
                    builder.flush_loose_text();
                    if let Some(Frame::Quote(blocks)) = builder.frames.pop() {
                        builder.push_block(Block::Quote(blocks));
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some((language, mut code)) = builder.code.take() {
                        if code.ends_with('\n') {
                            code.pop();
                        }
                        builder.push_block(Block::Code { language, code });
                    }
                }
                TagEnd::Item => {
                    builder.flush_loose_text();
                    if let Some(Frame::Item { task, blocks }) = builder.frames.pop()
                        && let Some(Frame::List { items, .. }) = builder.frames.last_mut()
                    {
                        items.push(ListItem { task, blocks });
                    }
                }
                TagEnd::List(_) => {
                    if let Some(Frame::List { start, items }) = builder.frames.pop() {
                        builder.push_block(Block::List { start, items });
                    }
                }
                TagEnd::TableCell => {
                    let cell = builder.inline.take();
                    if let Some(table) = builder.table.as_mut() {
                        table.3.push(cell);
                    }
                }
                TagEnd::TableHead => {
                    if let Some(table) = builder.table.as_mut() {
                        table.1 = std::mem::take(&mut table.3);
                        table.4 = false;
                    }
                }
                TagEnd::TableRow => {
                    if let Some(table) = builder.table.as_mut() {
                        let row = std::mem::take(&mut table.3);
                        table.2.push(row);
                    }
                }
                TagEnd::Table => {
                    if let Some((alignments, header, rows, _, _)) = builder.table.take() {
                        builder.push_block(Block::Table {
                            alignments,
                            header,
                            rows,
                        });
                    }
                }
                TagEnd::Emphasis => builder.inline.style.italic = false,
                TagEnd::Strong => builder.inline.style.bold = false,
                TagEnd::Strikethrough => builder.inline.style.strike = false,
                TagEnd::Link => builder.inline.link = None,
                TagEnd::Image => builder.inline.end_image(),
                _ => {}
            },
            Event::Text(text) => match builder.code.as_mut() {
                Some((_, code)) => code.push_str(&text),
                None => builder.inline.push(&text, false),
            },
            Event::Code(text) => builder.inline.push(&text, true),
            Event::SoftBreak => builder
                .inline
                .push(if keep_lines { "\n" } else { " " }, false),
            Event::HardBreak => builder.inline.push("\n", false),
            Event::Rule => {
                builder.flush_loose_text();
                builder.push_block(Block::Rule);
            }
            Event::TaskListMarker(done) => {
                if let Some(Frame::Item { task, .. }) = builder.frames.last_mut() {
                    *task = Some(done);
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                super::html::push(&html, &mut builder.html, &mut builder.inline)
            }
            _ => {}
        }
    }
    builder.flush_loose_text();
    match builder.frames.into_iter().next() {
        Some(Frame::Root(blocks)) => blocks,
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_and_inline_styles() {
        let blocks = parse(
            "# Título\n\nUm **forte** e `code` e [link](https://x.y).\n\n- a\n- [x] b\n\n```rust\nfn main() {}\n```\n\n> nota\n\n---\n\n| A | B |\n|---|:-:|\n| 1 | 2 |\n",
        );
        assert!(matches!(&blocks[0], Block::Heading { level: 1, text } if text.text == "Título"));
        let Block::Paragraph(paragraph) = &blocks[1] else {
            panic!()
        };
        assert_eq!(paragraph.text, "Um forte e code e link.");
        assert!(
            paragraph
                .spans
                .iter()
                .any(|span| span.style.bold && &paragraph.text[span.range.clone()] == "forte")
        );
        assert!(
            paragraph
                .spans
                .iter()
                .any(|span| span.style.code && &paragraph.text[span.range.clone()] == "code")
        );
        assert!(
            paragraph
                .spans
                .iter()
                .any(|span| span.link.as_deref() == Some("https://x.y"))
        );
        let Block::List { start: None, items } = &blocks[2] else {
            panic!()
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].task, Some(true));
        assert!(matches!(&items[0].blocks[0], Block::Paragraph(text) if text.text == "a"));
        assert!(
            matches!(&blocks[3], Block::Code { language: Some(lang), code } if lang == "rust" && code == "fn main() {}")
        );
        assert!(matches!(&blocks[4], Block::Quote(inner) if inner.len() == 1));
        assert_eq!(blocks[5], Block::Rule);
        let Block::Table {
            header,
            rows,
            alignments,
        } = &blocks[6]
        else {
            panic!()
        };
        assert_eq!(header[1].text, "B");
        assert_eq!(rows[0][0].text, "1");
        assert_eq!(alignments[1], Alignment::Center);
    }

    #[test]
    fn ordered_and_nested_lists() {
        let blocks = parse("3. um\n4. dois\n   - dentro\n");
        let Block::List {
            start: Some(3),
            items,
        } = &blocks[0]
        else {
            panic!("{blocks:?}")
        };
        assert_eq!(items.len(), 2);
        assert!(
            items[1]
                .blocks
                .iter()
                .any(|block| matches!(block, Block::List { .. }))
        );
    }

    #[test]
    fn a_prompt_keeps_its_lines_and_a_reply_joins_them() {
        let text = "[12:10] Giovanni: sem TID?\n[12:11] Giovanni: e o loop?\n\n- um\n- dois";
        let Block::Paragraph(prompt) = &parse_prompt(text)[0] else {
            panic!("paragraph");
        };
        assert_eq!(
            prompt.text,
            "[12:10] Giovanni: sem TID?\n[12:11] Giovanni: e o loop?"
        );
        let Block::Paragraph(reply) = &parse(text)[0] else {
            panic!("paragraph");
        };
        assert_eq!(
            reply.text,
            "[12:10] Giovanni: sem TID? [12:11] Giovanni: e o loop?"
        );
        assert!(
            matches!(&parse_prompt(text)[1], Block::List { .. }),
            "markdown still works"
        );
    }
}
