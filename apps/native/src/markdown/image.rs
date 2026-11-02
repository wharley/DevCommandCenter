//! Images in markdown (`![alt](url)` and HTML `<img>`), as GitHub shows a
//! bot's comment or a PR body: drawn where they sit in the line — a status
//! badge in a table cell, a "Request Review" button inside a link, a
//! screenshot on its own. Only `https` images load (`app::image_http`);
//! any other keeps its alt text, as before.

use gpui::{AnyElement, App, ObjectFit, SharedString, div, img, prelude::*, px};

use super::parse::{Inline, Span};
use crate::theme::ActiveTheme;

/// The tallest an image without a height of its own is drawn.
const MAX_HEIGHT: f32 = 320.0;

#[derive(Clone, Debug, PartialEq)]
pub struct InlineImage {
    /// Where it sits: a byte offset into the line's text.
    pub at: usize,
    pub url: String,
    pub alt: String,
    /// `<img height>`, in pixels.
    pub height: Option<f32>,
    /// The link around it.
    pub link: Option<String>,
}

/// Whether an image from `url` is drawn (otherwise its alt is text).
pub fn loads(url: &str) -> bool {
    url.starts_with("https://")
}

/// The part of `inline` between two offsets, its spans cut to fit.
fn slice(inline: &Inline, range: std::ops::Range<usize>) -> Inline {
    let spans = inline
        .spans
        .iter()
        .filter_map(|span| {
            let start = span.range.start.max(range.start);
            let end = span.range.end.min(range.end);
            (start < end).then(|| Span {
                range: start - range.start..end - range.start,
                ..span.clone()
            })
        })
        .collect();
    Inline {
        text: inline.text[range].to_string(),
        spans,
        images: Vec::new(),
    }
}

/// The line cut at its images: text, image, text…
pub(super) fn pieces(inline: &Inline) -> Vec<Result<Inline, &InlineImage>> {
    let mut pieces = Vec::new();
    let mut from = 0;
    for image in &inline.images {
        let at = image.at.clamp(from, inline.text.len());
        if at > from {
            pieces.push(Ok(slice(inline, from..at)));
        }
        pieces.push(Err(image));
        from = at;
    }
    if from < inline.text.len() {
        pieces.push(Ok(slice(inline, from..inline.text.len())));
    }
    pieces
}

/// One image: its own height (or the line's at most `MAX_HEIGHT`), the alt
/// while it fails, a click on its link.
pub(super) fn image(image: &InlineImage, id: SharedString, cx: &App) -> AnyElement {
    let muted = cx.theme().colors.muted_foreground;
    let alt = SharedString::from(image.alt.clone());
    let picture = img(SharedString::from(image.url.clone()))
        .object_fit(ObjectFit::Contain)
        .max_w_full()
        .map(|picture| match image.height {
            Some(height) => picture.h(px(height)),
            None => picture.max_h(px(MAX_HEIGHT)),
        })
        .with_fallback(move || {
            div()
                .text_color(muted)
                .child(alt.clone())
                .into_any_element()
        });
    let Some(link) = image.link.clone() else {
        return picture.into_any_element();
    };
    div()
        .id(id)
        .cursor_pointer()
        .on_click(move |_, window, cx| {
            if let Some(handler) = cx
                .try_global::<super::LinkHandler>()
                .map(|global| global.0.clone())
            {
                handler(&link, window, cx);
            } else if link.starts_with("http://") || link.starts_with("https://") {
                cx.open_url(&link);
            }
        })
        .child(picture)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{Block, parse};

    fn first(markdown: &str) -> Inline {
        match &parse(markdown)[0] {
            Block::Paragraph(inline) => inline.clone(),
            Block::Table { rows, .. } => rows[0][1].clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_bots_badge_and_button_are_images_where_they_sit() {
        let cell = first(
            "| Name | Status |\n|---|---|\n| app | ![Ready](https://vercel.com/static/status/ready.svg) [Preview](https://p.dev) |\n",
        );
        assert_eq!(cell.images.len(), 1);
        assert_eq!(
            cell.images[0].url,
            "https://vercel.com/static/status/ready.svg"
        );
        assert_eq!(cell.images[0].alt, "Ready");
        assert!(!cell.text.contains("Ready"), "the alt is not text too");

        let button = first(
            "<a href=\"https://vade.dev/r\"><img src=\"https://agents.vercel.sh/request-review-light.svg\" alt=\"Request Review\" height=\"28\"></a>",
        );
        assert_eq!(button.images[0].link.as_deref(), Some("https://vade.dev/r"));
        assert_eq!(button.images[0].height, Some(28.0));
        assert!(!button.is_empty(), "a line of images still shows");
    }

    #[test]
    fn an_image_that_does_not_load_stays_its_alt_and_lines_cut_at_images() {
        let local = first("antes ![logo](logo.png) depois");
        assert!(local.images.is_empty());
        assert!(local.text.contains("logo"));

        let line = first("**a** ![x](https://i.dev/x.png) b");
        let pieces = pieces(&line);
        assert_eq!(pieces.len(), 3);
        let Ok(head) = &pieces[0] else { panic!() };
        assert_eq!(head.text, "a ");
        assert_eq!(head.spans[0].range, 0..1, "bold kept on its own piece");
        assert!(pieces[1].is_err());
    }
}
