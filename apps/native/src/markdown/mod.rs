//! Markdown for messages: parsed to a block model (`parse`), drawn with the
//! desktop's `.assistant-markdown-scale` measures (`render`).

mod html;
mod image;
pub mod mentions;
mod parse;
mod render;
pub mod selection;
mod site_links;

pub use image::InlineImage;
pub use parse::{Block, Inline, ListItem, Span, SpanStyle, parse, parse_prompt};
pub use render::{MarkdownStyle, render_markdown};

/// What the app does with a clicked link (a task's file opens on the
/// stage); without one, web links open in the browser.
pub type OpenLink = dyn Fn(&str, &mut gpui::Window, &mut gpui::App);

#[derive(Clone)]
pub struct LinkHandler(pub std::rc::Rc<OpenLink>);

impl gpui::Global for LinkHandler {}
