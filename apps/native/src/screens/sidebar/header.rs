//! Top bar: the DCC mark, the code column toggle and the collapse button
//! (`data-slot="window-safe-top"`).

use std::sync::{Arc, OnceLock};

use gpui::{Context, Image, ImageFormat, div, img, prelude::*, px};

use super::{Sidebar, SidebarEvent};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::tooltip::Side;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, Tooltip, Tracked, command_key, shortcut_keys,
};

/// `CodeColumnToggle`: the label with its shortcut (⌘⇧E). The column itself
/// comes with the code screen, so it always reads "show".
pub fn code_column_toggle(
    id: &'static str,
    side: Side,
    cx: &mut Context<Sidebar>,
) -> gpui::AnyElement {
    let button = Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .muted()
        .child(Icon::new("folder-tree").stroke(1.8))
        .on_click(cx.listener(|_, _, _, cx| cx.emit(SidebarEvent::ToggleCodeColumn)));
    Tooltip::new(format!("tip-{id}"), button, |_, cx| {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(cx.t("codeColumn.open"))
            .child(shortcut_keys(&[command_key(), "Shift", "E"], cx))
            .into_any_element()
    })
    .side(side)
    .into_any_element()
}

/// The desktop's own `public/dcc-glyph.svg` (gradients, so drawn as an image).
fn glyph() -> Arc<Image> {
    static GLYPH: OnceLock<Arc<Image>> = OnceLock::new();
    GLYPH
        .get_or_init(|| {
            let svg = include_bytes!("../../../../../public/dcc-glyph.svg");
            Arc::new(Image::from_bytes(ImageFormat::Svg, svg.to_vec()))
        })
        .clone()
}

pub fn render(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let colors = &cx.theme().colors;
    let has_selection = sidebar.state.selected_task_id.is_some();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(36.0))
        .px(px(12.0))
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(6.0))
                .child(img(glyph()).size(px(18.0)).flex_shrink_0())
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(colors.foreground.opacity(0.8))
                        .child(Tracked::new("DCC", 11.0, 0.14)),
                ),
        )
        .children(has_selection.then(|| code_column_toggle("code-column-toggle", Side::Bottom, cx)))
        .child(
            Tooltip::text(
                "tip-collapse-sidebar",
                Button::new("collapse-sidebar")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .muted()
                    .child(Icon::new("panel-left").stroke(1.8))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SidebarEvent::ToggleCollapsed))),
                cx.t("sidebar.collapseSidebar"),
            )
            .side(Side::Bottom),
        )
}
