//! `editor-brand-icons.tsx`: Cursor and Zed take the text color; VS Code,
//! VS Code Insiders and Trae keep their brand colors.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::{App, Image, ImageFormat, Window, img, prelude::*, px, svg};

use crate::assets::editor_mark;

#[derive(IntoElement)]
pub struct EditorIcon {
    editor: &'static str,
    size: f32,
}

impl EditorIcon {
    pub fn new(editor: &'static str) -> Self {
        Self { editor, size: 14.0 }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

fn colored(editor: &'static str) -> Option<Arc<Image>> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Arc<Image>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if let Some(image) = cache.get(editor) {
        return Some(image.clone());
    }
    let svg = editor_mark(&format!("editors/{editor}.svg"))?;
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg.as_bytes().to_vec()));
    cache.insert(editor, image.clone());
    Some(image)
}

impl RenderOnce for EditorIcon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let size = px(self.size);
        let color = window.text_style().color;
        let mark = match self.editor {
            "cursor" | "zed" => svg()
                .path(format!("editors/{}.svg", self.editor))
                .size(size)
                .text_color(color)
                .into_any_element(),
            editor => match colored(editor) {
                Some(image) => img(image).size(size).into_any_element(),
                None => gpui::Empty.into_any_element(),
            },
        };
        gpui::div().flex_shrink_0().child(mark)
    }
}
