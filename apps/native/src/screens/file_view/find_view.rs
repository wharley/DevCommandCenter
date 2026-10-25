//! The search panel's look (`[data-search-panel]` / `[data-editor-widget]`
//! in the Pierre editor's stylesheet): a floating card 16px from the top
//! right, the find field (200px, the three toggles inside it) with the
//! count and the arrows, the replace row under it, and the round × on its
//! corner. Its labels are the Pierre editor's own (English).

use gpui::{
    AnyElement, Context, Entity, FontWeight, Hsla, InteractiveElement, SharedString, div,
    prelude::*, px,
};

use super::FileView;
use super::find::{FindMode, FindNext, FindPrevious, OpenFind, OpenReplace, PANEL_CONTEXT, Search};
use crate::data::highlight::editor_colors;
use crate::screens::diff_view::DiffPalette;
use crate::theme::color::mix_lab;
use crate::ui::{Icon, TextInput};

struct Colors {
    foreground: Hsla,
    background: Hsla,
    modified: Hsla,
}

#[allow(clippy::too_many_arguments)]
fn icon_button(
    id: &'static str,
    icon: &'static str,
    size: f32,
    pressed: bool,
    disabled: bool,
    colors: &Colors,
    on_click: impl Fn(&mut FileView, &mut gpui::Window, &mut Context<FileView>) + 'static,
    cx: &mut Context<FileView>,
) -> AnyElement {
    let rest = mix_lab(colors.foreground, 0.65, colors.background);
    let foreground = colors.foreground;
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .flex()
        .size(px(size))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .text_color(if pressed { colors.modified } else { rest })
        .map(|button| {
            if disabled {
                button.opacity(0.25)
            } else {
                button
                    .cursor_pointer()
                    .hover(move |style| style.text_color(foreground))
                    .on_click(cx.listener(move |view, _, window, cx| on_click(view, window, cx)))
            }
        })
        .child(Icon::new(icon).size(12.0))
        .into_any_element()
}

/// `[data-input-box]`: 200px, the editor's own background and a faint edge.
fn input_box(
    input: &Entity<TextInput>,
    right: f32,
    colors: &Colors,
    toggles: Option<AnyElement>,
) -> AnyElement {
    div()
        .relative()
        .flex()
        .w(px(200.0))
        .h(px(26.0))
        .items_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(mix_lab(colors.foreground, 0.12, colors.background))
        .bg(colors.background)
        .pl(px(6.0))
        .pr(px(right))
        .text_size(px(13.0))
        .line_height(px(24.0))
        .text_color(colors.foreground)
        .child(input.clone())
        .children(toggles)
        .into_any_element()
}

pub(crate) fn panel(view: &FileView, cx: &mut Context<FileView>) -> Option<AnyElement> {
    let search: &Search = view.search.as_ref()?;
    let palette = DiffPalette::new(view.dark);
    let colors = Colors {
        foreground: palette.foreground,
        background: palette.background,
        modified: {
            let rgba = editor_colors(view.dark).modified;
            gpui::Rgba {
                r: ((rgba >> 24) & 0xff) as f32 / 255.0,
                g: ((rgba >> 16) & 0xff) as f32 / 255.0,
                b: ((rgba >> 8) & 0xff) as f32 / 255.0,
                a: (rgba & 0xff) as f32 / 255.0,
            }
            .into()
        },
    };
    let none = search.matches.is_empty();
    let toggles = div()
        .absolute()
        .right(px(4.0))
        .top_0()
        .bottom_0()
        .flex()
        .items_center()
        .gap(px(1.0))
        .child(icon_button(
            "search-case",
            "diffs-editor-case",
            20.0,
            search.case_sensitive,
            false,
            &colors,
            |view, _, cx| view.toggle_search_option(|search| &mut search.case_sensitive, cx),
            cx,
        ))
        .child(icon_button(
            "search-whole-word",
            "diffs-editor-whole-word",
            20.0,
            search.whole_word,
            false,
            &colors,
            |view, _, cx| view.toggle_search_option(|search| &mut search.whole_word, cx),
            cx,
        ))
        .child(icon_button(
            "search-regex",
            "diffs-editor-regex",
            20.0,
            search.regex,
            false,
            &colors,
            |view, _, cx| view.toggle_search_option(|search| &mut search.regex, cx),
            cx,
        ))
        .into_any_element();
    let count = div()
        .debug_selector(|| "search-count".into())
        .min_w(px(50.0))
        .flex_shrink_0()
        .whitespace_nowrap()
        .text_size(px(12.0))
        .line_height(px(20.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(mix_lab(
            colors.foreground,
            if none { 0.35 } else { 0.5 },
            colors.background,
        ))
        .child(SharedString::from(search.summary()));
    let nav = div()
        .flex()
        .items_center()
        .child(icon_button(
            "search-previous",
            "diffs-editor-arrow-up",
            24.0,
            false,
            none,
            &colors,
            |view, _, cx| view.step_match(true, cx),
            cx,
        ))
        .child(icon_button(
            "search-next",
            "diffs-editor-arrow-down",
            24.0,
            false,
            none,
            &colors,
            |view, _, cx| view.step_match(false, cx),
            cx,
        ));
    let find_row = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(input_box(&search.query, 72.0, &colors, Some(toggles)))
        .child(count)
        .child(nav);
    let replace_row = (search.mode == FindMode::Replace).then(|| {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(input_box(&search.replace, 6.0, &colors, None))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(icon_button(
                        "search-replace",
                        "diffs-editor-replace",
                        24.0,
                        false,
                        false,
                        &colors,
                        |view, _, cx| view.replace_current(cx),
                        cx,
                    ))
                    .child(icon_button(
                        "search-replace-all",
                        "diffs-editor-replace-all",
                        24.0,
                        false,
                        false,
                        &colors,
                        |view, _, cx| view.replace_all(cx),
                        cx,
                    )),
            )
    });
    let widget_bg = mix_lab(colors.foreground, 0.04, colors.background);
    let close_bg = mix_lab(colors.foreground, 0.5, colors.background);
    let close_hover = colors.foreground;
    let close = div()
        .id("search-close")
        .debug_selector(|| "search-close".into())
        .absolute()
        .top(px(-10.0))
        .right(px(-10.0))
        .flex()
        .size(px(20.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(close_bg)
        .text_color(widget_bg)
        .cursor_pointer()
        .hover(move |style| style.bg(close_hover))
        .on_click(cx.listener(|view, _, window, cx| view.close_search(window, cx)))
        .child(Icon::new("diffs-editor-close").size(12.0));
    let shadow = |y: f32, blur: f32| gpui::BoxShadow {
        color: gpui::hsla(0.0, 0.0, 0.0, 0x13 as f32 / 255.0),
        offset: gpui::point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    };
    Some(
        div()
            .absolute()
            .top(px(16.0))
            .right(px(16.0))
            .key_context(PANEL_CONTEXT)
            .on_action(cx.listener(|view, _: &FindNext, _, cx| view.step_match(false, cx)))
            .on_action(cx.listener(|view, _: &FindPrevious, _, cx| view.step_match(true, cx)))
            .on_action(cx.listener(|view, _: &OpenFind, window, cx| {
                view.open_search(FindMode::Find, window, cx)
            }))
            .on_action(cx.listener(|view, _: &OpenReplace, window, cx| {
                view.open_search(FindMode::Replace, window, cx)
            }))
            .child(
                div()
                    .debug_selector(|| "search-panel".into())
                    .relative()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .min_w(px(260.0))
                    .p(px(4.0))
                    .rounded(px(9.0))
                    .border_1()
                    .border_color(colors.foreground.opacity(0.2))
                    .bg(widget_bg)
                    .shadow(vec![
                        gpui::BoxShadow {
                            color: colors.background,
                            offset: gpui::point(px(0.0), px(0.0)),
                            blur_radius: px(0.0),
                            spread_radius: px(1.0),
                            inset: true,
                        },
                        shadow(4.0, 8.0),
                        shadow(6.0, 18.0),
                    ])
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .child(find_row)
                    .children(replace_row)
                    .child(close),
            )
            .into_any_element(),
    )
}
