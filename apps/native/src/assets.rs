//! Embedded assets GPUI loads by path. Icons are asked for as
//! `icons/<lucide-name>@<stroke>.svg`: the desktop sets a stroke width per
//! icon (1.8, 1.9, 2.2…), so the width is filled in when the icon loads.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(svg) = provider_mark(path).or_else(|| editor_mark(path)) {
            return Ok(Some(Cow::Borrowed(svg.as_bytes())));
        }
        Ok(icon_svg(path)
            .or_else(|| crate::data::connector_marks::svg(path))
            .map(|svg| Cow::Owned(svg.into_bytes())))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(generated::ICONS
            .iter()
            .map(|(name, _)| SharedString::from(format!("icons/{name}.svg")))
            .filter(|asset| asset.starts_with(path))
            .collect())
    }
}

/// The asset path for an icon drawn with `stroke` width (Lucide units).
pub fn icon_path(name: &str, stroke: f32) -> SharedString {
    format!("icons/{name}@{stroke}.svg").into()
}

fn icon_svg(path: &str) -> Option<String> {
    let file = path.strip_prefix("icons/")?.strip_suffix(".svg")?;
    let (name, stroke) = file.split_once('@').unwrap_or((file, "2"));
    let stroke: f32 = stroke.parse().ok()?;
    let template = generated::ICONS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, svg)| *svg)?;
    Some(template.replace("{{stroke}}", &stroke.to_string()))
}

/// A provider's brand mark (`providers/<name>.svg`), as the desktop draws it.
pub fn provider_mark(path: &str) -> Option<&'static str> {
    let name = path.strip_prefix("providers/")?.strip_suffix(".svg")?;
    generated::PROVIDER_MARKS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, svg)| *svg)
}

/// An editor's brand mark (`editors/<id>.svg`, `editor-brand-icons.tsx`).
pub fn editor_mark(path: &str) -> Option<&'static str> {
    let name = path.strip_prefix("editors/")?.strip_suffix(".svg")?;
    generated::EDITOR_MARKS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, svg)| *svg)
}

pub fn has_icon(name: &str) -> bool {
    generated::ICONS
        .iter()
        .any(|(candidate, _)| *candidate == name)
}

/// The desktop's own `public/dcc-glyph.svg` (gradients, so drawn as an image).
pub fn dcc_glyph() -> std::sync::Arc<gpui::Image> {
    static GLYPH: std::sync::OnceLock<std::sync::Arc<gpui::Image>> = std::sync::OnceLock::new();
    GLYPH
        .get_or_init(|| {
            let svg = include_bytes!("../../../public/dcc-glyph.svg");
            std::sync::Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
                svg.to_vec(),
            ))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_take_the_stroke_from_the_path() {
        let svg = icon_svg(&icon_path("panel-left", 1.8)).unwrap();
        assert!(svg.contains(r#"stroke-width="1.8""#));
        assert!(
            icon_svg("icons/panel-left.svg")
                .unwrap()
                .contains(r#"stroke-width="2""#)
        );
        assert!(icon_svg("icons/nope@2.svg").is_none());
    }
}
