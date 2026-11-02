//! An address written as itself in markdown, drawn the way the composer
//! draws it (`ui::text_area::links`): blue on a faint fill with the site's
//! favicon before it, a globe while it loads or for a local host. The icon
//! sits in room made in the line (`SPACER`), which a copy leaves out.

use gpui::{AnyElement, App, Hsla, SharedString, TextLayout, canvas, div, img, prelude::*, px};

use super::parse::{Inline, Span};
use crate::ui::Icon;
use crate::ui::text_area::links;

/// The room before an address for its icon (an em and a thin space).
pub(super) const SPACER: &str = "\u{2003}\u{2009}";

/// `inline` with room before each address, and where each one's icon goes.
pub(super) fn with_room(inline: &Inline) -> (Inline, Vec<(usize, String)>) {
    let starts: Vec<(usize, String)> = inline
        .spans
        .iter()
        .filter(|span| span.style.site)
        .filter_map(|span| Some((span.range.start, span.link.clone()?)))
        .collect();
    if starts.is_empty() {
        return (inline.clone(), Vec::new());
    }
    // Where an offset lands once the room before it is made; an address's own
    // start stays before its room, so its fill takes the icon in.
    let shift = |offset: usize, own_start: bool| {
        let before = starts
            .iter()
            .filter(|(start, _)| *start < offset || (*start == offset && !own_start))
            .count();
        offset + before * SPACER.len()
    };
    let mut text = String::with_capacity(inline.text.len() + starts.len() * SPACER.len());
    let mut from = 0;
    for (start, _) in &starts {
        text.push_str(&inline.text[from..*start]);
        text.push_str(SPACER);
        from = *start;
    }
    text.push_str(&inline.text[from..]);
    let spans = inline
        .spans
        .iter()
        .map(|span| Span {
            range: shift(span.range.start, span.style.site)..shift(span.range.end, false),
            ..span.clone()
        })
        .collect();
    let images = inline
        .images
        .iter()
        .map(|image| super::image::InlineImage {
            at: shift(image.at, false),
            ..image.clone()
        })
        .collect();
    let sites = starts
        .iter()
        .map(|(start, url)| (shift(*start, true), url.clone()))
        .collect();
    (
        Inline {
            text,
            spans,
            images,
        },
        sites,
    )
}

fn icon(url: &str, color: Hsla) -> AnyElement {
    let globe = move || {
        Icon::new("globe")
            .size(links::ICON)
            .color(color)
            .into_any_element()
    };
    match links::favicon_source(url) {
        Some(source) => img(SharedString::from(source))
            .size(px(links::ICON))
            .rounded(px(2.0))
            .with_fallback(globe)
            .into_any_element(),
        None => globe(),
    }
}

/// `text` with each address's icon drawn in its room.
pub(super) fn with_icons(
    text: AnyElement,
    layout: TextLayout,
    sites: Vec<(usize, String)>,
    cx: &App,
) -> AnyElement {
    if sites.is_empty() {
        return text;
    }
    let color = links::color(cx);
    div()
        .relative()
        .min_w_0()
        .child(text)
        .child(
            canvas(
                move |_, window, cx| {
                    let line_height = layout.line_height();
                    sites
                        .iter()
                        .filter_map(|(at, url)| {
                            let position = layout.position_for_index(*at)?;
                            let mut icon = icon(url, color);
                            links::prepaint_icon(&mut icon, position, line_height, window, cx);
                            Some(icon)
                        })
                        .collect::<Vec<_>>()
                },
                |_, mut icons, window, cx| {
                    for icon in &mut icons {
                        icon.paint(window, cx);
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{Block, parse};

    fn paragraph(markdown: &str) -> Inline {
        match &parse(markdown)[0] {
            Block::Paragraph(inline) => inline.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn only_a_bare_address_becomes_a_site_link() {
        let inline = paragraph(
            "veja https://x.com/thread_1, `https://code.dev` e [aqui](https://y.dev) ou www.bloop.email/.",
        );
        let sites: Vec<(&str, &str)> = inline
            .spans
            .iter()
            .filter(|span| span.style.site)
            .map(|span| {
                (
                    &inline.text[span.range.clone()],
                    span.link.as_deref().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            sites,
            vec![
                ("https://x.com/thread_1", "https://x.com/thread_1"),
                ("www.bloop.email/", "https://www.bloop.email/"),
            ]
        );
    }

    #[test]
    fn room_before_each_address_keeps_the_other_styles_in_place() {
        let inline = paragraph("ok https://a.dev e **forte**");
        let (shown, sites) = with_room(&inline);
        assert_eq!(shown.text, format!("ok {SPACER}https://a.dev e forte"));
        assert_eq!(sites, vec![(3, "https://a.dev/".to_string())]);
        let site = shown.spans.iter().find(|span| span.style.site).unwrap();
        assert_eq!(
            &shown.text[site.range.clone()],
            format!("{SPACER}https://a.dev"),
            "the fill takes the icon in"
        );
        let bold = shown.spans.iter().find(|span| span.style.bold).unwrap();
        assert_eq!(&shown.text[bold.range.clone()], "forte");
    }
}
