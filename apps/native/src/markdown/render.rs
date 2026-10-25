//! Draws the block model with `.assistant-markdown-scale` and Streamdown's
//! classes: paragraphs at 1.82 line height, 0.75rem between blocks, headings
//! at 1.35/1.2/1.1em, lists inset 1.35rem, code in a bordered card with its
//! language on top, tables in a framed sidebar-tinted box.

use gpui::{
    AnyElement, App, FontStyle, FontWeight, HighlightStyle, InteractiveText, SharedString,
    StrikethroughStyle, StyledText, UnderlineStyle, Window, div, prelude::*, px,
};

use super::parse::{Block, Inline, ListItem};
use crate::theme::ActiveTheme;

/// `0.75rem` between blocks.
const BLOCK_GAP: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkdownStyle {
    /// The prose size the text inherits (16 in a reply, 13 in a prompt).
    pub size: f32,
    /// Prompt code blocks taller than this start collapsed (16 lines).
    pub collapsed_lines: Option<usize>,
}

impl MarkdownStyle {
    pub fn reply() -> Self {
        Self {
            size: 16.0,
            collapsed_lines: None,
        }
    }

    pub fn prompt() -> Self {
        Self {
            size: 13.0,
            collapsed_lines: Some(16),
        }
    }
}

fn inline_text(inline: &Inline, id: SharedString, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    let mono = cx.theme().fonts.mono.clone();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut links: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for span in &inline.spans {
        let mut style = HighlightStyle::default();
        if span.style.bold {
            style.font_weight = Some(FontWeight::SEMIBOLD);
        }
        if span.style.italic {
            style.font_style = Some(FontStyle::Italic);
        }
        if span.style.strike {
            style.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.0),
                color: None,
            });
        }
        if span.style.code {
            // `rounded bg-muted px-1.5 font-mono`; GPUI runs keep one size.
            style.background_color = Some(colors.muted);
            let _ = &mono;
        }
        if let Some(link) = &span.link {
            style.color = Some(colors.primary);
            style.font_weight = Some(FontWeight::MEDIUM);
            style.underline = Some(UnderlineStyle {
                thickness: px(1.0),
                color: Some(colors.primary),
                wavy: false,
            });
            links.push((span.range.clone(), link.clone()));
        }
        highlights.push((span.range.clone(), style));
    }
    let text = StyledText::new(SharedString::from(inline.text.clone())).with_highlights(highlights);
    if links.is_empty() {
        return text.into_any_element();
    }
    let ranges: Vec<std::ops::Range<usize>> =
        links.iter().map(|(range, _)| range.clone()).collect();
    let targets: Vec<String> = links.into_iter().map(|(_, url)| url).collect();
    InteractiveText::new(id, text)
        .on_click(ranges, move |index, window, cx| {
            let target = targets[index].clone();
            // The app opens what it can (a task's file on the stage).
            if let Some(handler) = cx
                .try_global::<super::LinkHandler>()
                .map(|global| global.0.clone())
            {
                handler(&target, window, cx);
                return;
            }
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                cx.open_url(&target);
            }
        })
        .into_any_element()
}

/// `.assistant-markdown-scale :where(p, …):not(:first-child) { margin-top: .75rem }`;
/// a block right after a heading sits on the heading's own bottom margin.
fn gap_before(previous: Option<&Block>, block: &Block, size: f32) -> f32 {
    let Some(previous) = previous else { return 0.0 };
    match block {
        Block::Heading { level, .. } => heading_measures(*level, size).2,
        _ if matches!(previous, Block::Heading { .. }) => 0.0,
        Block::Rule => 24.0,
        _ => BLOCK_GAP,
    }
}

/// Size multiplier, weight, top margin and bottom margin (`h1`…`h6`).
fn heading_measures(level: u8, _size: f32) -> (f32, f32, f32, f32, f32) {
    match level {
        1 => (1.35, 650.0, 26.4, 11.2, 1.3),
        2 => (1.2, 650.0, 23.2, 8.8, 1.32),
        3 => (1.1, 620.0, 18.4, 7.2, 1.36),
        _ => (1.0, 620.0, 15.2, 5.6, 1.4),
    }
}

fn blocks(
    blocks: &[Block],
    style: MarkdownStyle,
    path: &str,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let mut out = Vec::with_capacity(blocks.len());
    let mut previous: Option<&Block> = None;
    for (index, block) in blocks.iter().enumerate() {
        let gap = gap_before(previous, block, style.size);
        let element = block_element(block, style, &format!("{path}-{index}"), window, cx);
        out.push(div().pt(px(gap)).child(element).into_any_element());
        previous = Some(block);
    }
    out
}

fn list_marker(start: Option<u64>, index: usize, task: Option<bool>) -> String {
    match (task, start) {
        (Some(true), _) => "☑".into(),
        (Some(false), _) => "☐".into(),
        (None, Some(first)) => format!("{}.", first + index as u64),
        (None, None) => "•".into(),
    }
}

fn list(
    start: Option<u64>,
    items: &[ListItem],
    style: MarkdownStyle,
    path: &str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let size = style.size;
    let mut rows = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let content = blocks(
            &item.blocks,
            style,
            &format!("{path}-item{index}"),
            window,
            cx,
        );
        rows.push(
            div()
                .flex()
                .min_w_0()
                .gap(px(6.0))
                .when(index > 0, |row| row.mt(px(size * 0.28)))
                .line_height(gpui::relative(1.7))
                .child(
                    div()
                        .flex_shrink_0()
                        .min_w(px(size * 0.9))
                        .text_color(cx.theme().colors.muted_foreground)
                        .child(SharedString::from(list_marker(start, index, item.task))),
                )
                .child(div().flex_1().min_w_0().children(content)),
        );
    }
    div()
        .pl(px(size * 1.35 - size * 0.9))
        .flex()
        .flex_col()
        .children(rows)
        .into_any_element()
}

fn code_block(
    language: Option<&str>,
    code: &str,
    style: MarkdownStyle,
    path: &str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let mono = theme.fonts.mono.clone();
    let radius = theme.radius;
    let lines: Vec<&str> = code.split('\n').collect();
    // Long prompt payloads start folded (`collapsedLines` + 4 lines of slack).
    let expanded_state = window.use_keyed_state(
        SharedString::from(format!("{path}-expanded")),
        cx,
        |_, _| false,
    );
    let expanded = *expanded_state.read(cx);
    let collapsible = style
        .collapsed_lines
        .is_some_and(|limit| lines.len() > limit + 4);
    let visible = match (collapsible, expanded, style.collapsed_lines) {
        (true, false, Some(limit)) => lines[..limit].join("\n"),
        _ => code.to_string(),
    };
    let header = language.map(|language| {
        div()
            .flex()
            .items_center()
            .h(px(32.0))
            .pl(px(12.0))
            .pr(px(6.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.5))
            .child(
                div()
                    .truncate()
                    .font_family(mono.clone())
                    .text_size(px(11.0))
                    .line_height(px(11.0))
                    .text_color(colors.muted_foreground.opacity(0.7))
                    .child(SharedString::from(language.to_string())),
            )
    });
    let toggle = collapsible.then(|| {
        let state = expanded_state.clone();
        let label = if expanded {
            crate::i18n::Translate::t(cx, "conversation.message.codeCollapse")
        } else {
            crate::i18n::Translate::t_with(
                cx,
                "conversation.message.codeShowAll",
                &crate::i18n::Args::default().with("lines", lines.len().to_string()),
            )
        };
        let hover = colors.accent.opacity(0.5);
        let hover_text = colors.foreground;
        div()
            .id(SharedString::from(format!("{path}-toggle")))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .h(px(28.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.5))
            .text_size(px(11.0))
            .line_height(px(11.0))
            .text_color(colors.muted_foreground)
            .cursor_pointer()
            .hover(move |style| style.bg(hover).text_color(hover_text))
            .on_click(move |_, _, cx| {
                state.update(cx, |expanded, cx| {
                    *expanded = !*expanded;
                    cx.notify();
                })
            })
            .child(
                crate::ui::Icon::new(if expanded {
                    "chevron-up"
                } else {
                    "chevron-down"
                })
                .size(12.0),
            )
            .child(label)
    });
    div()
        .my(px(12.0))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(radius.lg)
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background)
        .children(header)
        .child(
            div()
                .id(SharedString::from(format!("{path}-code")))
                .overflow_x_scroll()
                .px(px(12.0))
                .py(px(10.0))
                .font_family(mono)
                .text_size(px(12.5))
                .line_height(gpui::relative(1.6))
                .text_color(colors.foreground)
                .child(div().whitespace_nowrap().child(SharedString::from(visible))),
        )
        .children(toggle)
        .into_any_element()
}

fn table(header: &[Inline], rows: &[Vec<Inline>], path: &str, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    let radius = cx.theme().radius;
    let columns = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1);
    let cell = |inline: Option<&Inline>, bold: bool, id: String| {
        div()
            .px(px(16.0))
            .py(px(8.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .when(bold, |cell| cell.font_weight(FontWeight::SEMIBOLD))
            .border_1()
            .border_color(colors.border)
            .children(inline.map(|inline| inline_text(inline, SharedString::from(id), cx)))
    };
    let mut grid = Vec::new();
    for column in 0..columns {
        grid.push(
            cell(header.get(column), true, format!("{path}-h{column}"))
                .bg(colors.muted.opacity(0.8)),
        );
    }
    for (row_index, row) in rows.iter().enumerate() {
        for column in 0..columns {
            grid.push(cell(
                row.get(column),
                false,
                format!("{path}-r{row_index}c{column}"),
            ));
        }
    }
    div()
        .my(px(16.0))
        .p(px(8.0))
        .rounded(radius.lg)
        .border_1()
        .border_color(colors.border)
        .bg(colors.sidebar)
        .child(div().grid().grid_cols(columns as u16).children(grid))
        .into_any_element()
}

fn block_element(
    block: &Block,
    style: MarkdownStyle,
    path: &str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let size = style.size;
    let colors = cx.theme().colors.clone();
    match block {
        Block::Paragraph(inline) => div()
            .line_height(gpui::relative(1.82))
            .child(inline_text(
                inline,
                SharedString::from(format!("{path}-p")),
                cx,
            ))
            .into_any_element(),
        Block::Heading { level, text } => {
            let (scale, weight, _, bottom, line) = heading_measures(*level, size);
            div()
                .pb(px(bottom))
                .text_size(px(size * scale))
                .line_height(gpui::relative(line))
                .font_weight(FontWeight(weight))
                .text_color(colors.foreground)
                .child(inline_text(
                    text,
                    SharedString::from(format!("{path}-h")),
                    cx,
                ))
                .into_any_element()
        }
        Block::List { start, items } => list(*start, items, style, path, window, cx),
        Block::Code { language, code } => {
            code_block(language.as_deref(), code, style, path, window, cx)
        }
        Block::Quote(inner) => div()
            .my(px(16.0))
            .pl(px(16.0))
            .border_l_4()
            .border_color(colors.muted_foreground.opacity(0.3))
            .text_color(colors.muted_foreground)
            .italic()
            .line_height(gpui::relative(1.45))
            .children(blocks(inner, style, &format!("{path}-q"), window, cx))
            .into_any_element(),
        Block::Rule => div()
            .my(px(24.0))
            .h(px(1.0))
            .bg(colors.border)
            .into_any_element(),
        Block::Table { header, rows, .. } => table(header, rows, path, cx),
    }
}

/// The whole message: its blocks stacked with the scale's spacing.
pub fn render_markdown(
    markdown: &[Block],
    style: MarkdownStyle,
    id: &str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .text_size(px(style.size))
        .children(blocks(markdown, style, id, window, cx))
        .into_any_element()
}
