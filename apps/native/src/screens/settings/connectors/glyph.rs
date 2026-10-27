//! `ConnectorGlyph`: a connector's mark in a neutral tile — its uploaded or
//! site logo, the catalog's brand mark, or its initial.

use std::sync::Arc;

use base64::Engine as _;
use gpui::{AnyElement, FontWeight, Image, ImageFormat, div, img, prelude::*, px, svg};

use crate::data::connector_marks;
use crate::theme::ActiveTheme;
use crate::theme::color::hex;

/// A `data:image/…;base64,` logo as an image (drawn, so an SVG never runs).
fn decoded(uri: &str) -> Option<Arc<Image>> {
    let rest = uri.strip_prefix("data:")?;
    let (kind, data) = rest.split_once(";base64,")?;
    let format = match kind {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        "image/gif" => ImageFormat::Gif,
        "image/webp" => ImageFormat::Webp,
        "image/svg+xml" => ImageFormat::Svg,
        _ => return None,
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

/// `size`: the tile (36, or 48 in the connect dialog).
pub(super) fn glyph(
    logo: Option<&str>,
    logo_uri: Option<&str>,
    name: &str,
    size: f32,
    cx: &gpui::App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    let big = size > 40.0;
    let tile = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded(px(if big { 14.0 } else { 10.0 }))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.muted.opacity(0.4))
        .text_color(colors.foreground);
    if let Some(image) = logo_uri.and_then(decoded) {
        return tile
            .child(img(image).size(px(22.0)).rounded(px(5.0)))
            .into_any_element();
    }
    if let Some(id) =
        logo.filter(|id| connector_marks::svg(&format!("connectors/{id}.svg")).is_some())
    {
        let color = connector_marks::brand_color(id)
            .map(hex)
            .unwrap_or(colors.foreground);
        return tile
            .child(
                svg()
                    .path(format!("connectors/{id}.svg"))
                    .size(px(if big { 24.0 } else { 18.0 }))
                    .text_color(color),
            )
            .into_any_element();
    }
    let initial = name
        .trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into());
    tile.text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .child(initial)
        .into_any_element()
}
