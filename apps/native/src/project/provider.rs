//! `ProviderIcon`: each agent provider's brand mark. Claude and Gemini keep
//! their colors; Codex, Cursor and Droid take the text color; Grok and
//! Antigravity are Lucide marks; anything else gets a muted Sparkles.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::{AnyElement, App, Hsla, Image, ImageFormat, Window, img, prelude::*, px, svg};

use crate::assets::provider_mark;
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// The provider's name as its `dcc-providers` descriptor labels it.
pub fn provider_label(provider_id: &str) -> Option<&'static str> {
    Some(match provider_id {
        "claude_code" => "Claude Code",
        "codex" => "Codex",
        "gemini" => "Gemini CLI (legacy)",
        "cursor" => "Cursor",
        "droid" => "Droid",
        "grok" => "Grok Build",
        "antigravity" => "Antigravity",
        _ => return None,
    })
}

/// A model's label from the static catalogs, else its id.
pub fn model_label(model: &str) -> String {
    dcc_core::domain::model_registry::label_for(model)
        .map_or_else(|| model.trim().to_string(), str::to_string)
}

/// `agentBrandKey` in `@dcc/mascots`.
pub fn brand_key(provider: &str) -> Option<&'static str> {
    let value = provider.to_lowercase();
    let has = |needle: &str| value.contains(needle);
    Some(if has("claude") || has("anthropic") {
        "claude"
    } else if has("codex") || has("openai") {
        "codex"
    } else if has("gemini") || has("google") || has("antigravity") {
        "gemini"
    } else if has("grok") || has("xai") {
        "grok"
    } else if has("cursor") {
        "cursor"
    } else if has("droid") || has("factory") {
        "droid"
    } else {
        return None;
    })
}

#[derive(IntoElement)]
pub struct ProviderIcon {
    provider: String,
    size: f32,
    opacity: f32,
}

impl ProviderIcon {
    pub fn new(provider: &str) -> Self {
        Self {
            provider: provider.to_string(),
            size: 16.0,
            opacity: 1.0,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

impl RenderOnce for ProviderIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let current_color = window.text_style().color;
        let size = px(self.size);
        let element: AnyElement = if self.provider.to_lowercase().contains("antigravity") {
            Icon::new("rocket")
                .size(self.size)
                .stroke(1.7)
                .color(gpui::rgb(0x4285f4).into())
                .into_any_element()
        } else {
            match brand_key(&self.provider) {
                Some(name @ ("claude" | "gemini")) => match colored_mark(name) {
                    Some(image) => img(image).size(size).flex_shrink_0().into_any_element(),
                    None => gpui::Empty.into_any_element(),
                },
                Some("grok") => Icon::new("orbit")
                    .size(self.size)
                    .stroke(1.7)
                    .into_any_element(),
                Some(name) => svg()
                    .path(format!("providers/{name}.svg"))
                    .size(size)
                    .flex_shrink_0()
                    .text_color(current_color)
                    .into_any_element(),
                None => {
                    let muted: Hsla = cx.theme().colors.muted_foreground;
                    Icon::new("sparkles")
                        .size(self.size)
                        .stroke(1.6)
                        .color(muted)
                        .into_any_element()
                }
            }
        };
        gpui::div()
            .flex_shrink_0()
            .opacity(self.opacity)
            .child(element)
    }
}

/// Marks with their own colors are drawn as images, not tinted masks.
fn colored_mark(name: &'static str) -> Option<Arc<Image>> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Arc<Image>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().ok()?;
    if let Some(image) = cache.get(name) {
        return Some(image.clone());
    }
    let svg = provider_mark(&format!("providers/{name}.svg"))?;
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg.as_bytes().to_vec()));
    cache.insert(name, image.clone());
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_map_to_their_brand() {
        assert_eq!(brand_key("claude_code"), Some("claude"));
        assert_eq!(brand_key("Codex"), Some("codex"));
        assert_eq!(brand_key("antigravity"), Some("gemini"));
        assert_eq!(brand_key("opencode"), None);
    }
}
