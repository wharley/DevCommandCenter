//! Markdown for messages: parsed to a block model (`parse`), drawn with the
//! desktop's `.assistant-markdown-scale` measures (`render`).

mod parse;
mod render;

pub use parse::{Block, Inline, ListItem, Span, SpanStyle, parse};
pub use render::{MarkdownStyle, render_markdown};
