//! The tab's surface as the dock draws xterm.js (`TerminalOutput`,
//! `variant="drawer"`): Geist Mono 12px at 1.3, the `.dcc-terminal__*`
//! frames around it, the rows with their colors and the bar caret. It fits
//! its columns and rows to the room it gets (`FitAddon`); keys go to the
//! shell, text comes through the platform's input (accents, IME).

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use alacritty_terminal::term::TermMode;
use gpui::{
    Bounds, BoxShadow, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, FontStyle,
    FontWeight, HighlightStyle, KeyDownEvent, Pixels, Render, ScrollWheelEvent, SharedString,
    StrikethroughStyle, StyledText, UTF16Selection, UnderlineStyle, Window, canvas, div, point,
    prelude::*, px,
};

use super::emulator::{Row, Screen};
use super::keys::{key_bytes, paste_bytes};
use super::palette::TerminalPalette;
use super::session::TerminalSession;
use crate::theme::color::mix_oklab;
use crate::theme::{ActiveTheme, Appearance, fonts};

/// `fontSize: 12`, `lineHeight: 1.3`: xterm's cell is the font's height
/// (1.3em in Geist Mono) times the line height, rounded down.
pub const FONT_SIZE: f32 = 12.0;
pub const ROW_HEIGHT: f32 = 20.0;
/// `padding: "12px 2px 12px 12px"`
const PAD_TOP: f32 = 12.0;
const PAD_RIGHT: f32 = 2.0;
const PAD_LEFT: f32 = 12.0;
/// `.dcc-terminal__viewport { padding: 1rem }` and the two 1px borders.
const VIEWPORT_PAD: f32 = 16.0;

/// Columns and rows that fit `width` × `height` of text area.
pub fn fit(width: f32, height: f32, cell: f32) -> (u16, u16) {
    let cols = ((width - PAD_LEFT - PAD_RIGHT) / cell.max(1.0)).floor();
    let rows = ((height - PAD_TOP * 2.0) / ROW_HEIGHT).floor();
    (
        cols.clamp(2.0, 1000.0) as u16,
        rows.clamp(1.0, 500.0) as u16,
    )
}

fn row_runs(row: &Row) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut start = 0;
    row.runs
        .iter()
        .map(|run| {
            let range = start..start + run.len;
            start += run.len;
            let style = HighlightStyle {
                color: Some(run.foreground),
                background_color: run.background,
                font_weight: run.bold.then_some(FontWeight::BOLD),
                font_style: run.italic.then_some(FontStyle::Italic),
                underline: run.underline.then(|| UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(run.foreground),
                    wavy: false,
                }),
                strikethrough: run.strikethrough.then(|| StrikethroughStyle {
                    thickness: px(1.0),
                    color: Some(run.foreground),
                }),
                ..HighlightStyle::default()
            };
            (range, style)
        })
        .collect()
}

impl TerminalSession {
    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;
        if modifiers.platform {
            match keystroke.key.as_str() {
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        let bracketed = self.emulator.mode().contains(TermMode::BRACKETED_PASTE);
                        self.input(paste_bytes(&text, bracketed), cx);
                    }
                    cx.stop_propagation();
                }
                "c" if self.selection_text().is_some() => {
                    self.copy_selection(cx);
                    cx.stop_propagation();
                }
                _ => {}
            }
            return;
        }
        // Plain characters arrive through the platform's input (dead keys, IME).
        let plain = !modifiers.control
            && !modifiers.alt
            && keystroke.key_char.is_some()
            && keystroke.key != "enter"
            && keystroke.key != "tab"
            && keystroke.key != "space";
        if plain {
            return;
        }
        let app_cursor = self.emulator.mode().contains(TermMode::APP_CURSOR);
        if let Some(bytes) = key_bytes(keystroke, app_cursor) {
            // `\f` clears the screen and the scrollback, like the desktop.
            if bytes == "\x0c" {
                self.clear(cx);
            } else {
                self.input(bytes, cx);
            }
            // xterm cancels every key it sends: Esc stays with the shell.
            cx.stop_propagation();
        }
    }

    fn on_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = f32::from(event.delta.pixel_delta(px(ROW_HEIGHT)).y);
        self.wheel += delta / ROW_HEIGHT;
        let lines = self.wheel.trunc() as i32;
        if lines == 0 {
            return;
        }
        self.wheel -= lines as f32;
        let mode = self.emulator.mode();
        // Full-screen programs scroll with the arrows (`ALTERNATE_SCROLL`).
        if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            let app_cursor = mode.contains(TermMode::APP_CURSOR);
            let arrow = match (lines > 0, app_cursor) {
                (true, true) => "\x1bOA",
                (true, false) => "\x1b[A",
                (false, true) => "\x1bOB",
                (false, false) => "\x1b[B",
            };
            self.input(arrow.repeat(lines.unsigned_abs() as usize), cx);
        } else {
            self.emulator.scroll(lines);
            cx.notify();
        }
    }

    /// The caret's cell in window coordinates, for the IME's popup.
    fn caret_bounds(&self) -> Option<Bounds<Pixels>> {
        let origin = self.text_origin.get()?;
        let (line, column) = self.caret?;
        Some(Bounds::new(
            point(
                origin.x + px(column as f32 * self.cell.0),
                origin.y + px(line as f32 * ROW_HEIGHT),
            ),
            gpui::size(px(self.cell.0), px(ROW_HEIGHT)),
        ))
    }

    pub fn copy_selection(&self, cx: &mut Context<Self>) {
        if let Some(text) = self.selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
}

impl EntityInputHandler for TerminalSession {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let marked = self.marked.clone().unwrap_or_default();
        actual_range.replace(0..marked.encode_utf16().count());
        Some(marked)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self
            .marked
            .as_ref()
            .map_or(0, |marked| marked.encode_utf16().count());
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|marked| 0..marked.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = None;
        self.input(text.to_string(), cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = (!text.is_empty()).then(|| text.to_string());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        self.caret_bounds()
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Render for TerminalSession {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let dark = theme.appearance == Appearance::Dark;
        let colors = theme.colors.clone();
        let palette = TerminalPalette::new(dark, &colors);
        self.palette = Some(palette.clone());
        let mono = gpui::font(fonts::MONO);
        let font_id = window.text_system().resolve_font(&mono);
        let cell = window
            .text_system()
            .advance(font_id, px(FONT_SIZE), '0')
            .map(|size| f32::from(size.width))
            .unwrap_or(7.2);
        self.cell = (cell, ROW_HEIGHT);
        // `FitAddon.fit()` on the room measured last frame.
        if let Some(size) = self.text_size.get() {
            let (cols, rows) = fit(f32::from(size.width), f32::from(size.height), cell);
            self.fit(cols, rows, cx);
        }
        let screen: Screen = self.emulator.screen(&palette);
        self.caret = screen.cursor;
        let focused = self.focus.is_focused(window);
        let caret = screen
            .cursor
            .filter(|_| focused && self.marked.is_none())
            .map(|(line, column)| {
                div()
                    .absolute()
                    .top(px(PAD_TOP + line as f32 * ROW_HEIGHT))
                    .left(px(PAD_LEFT + column as f32 * cell))
                    .w(px(1.0))
                    .h(px(ROW_HEIGHT))
                    .bg(palette.cursor)
            });
        let link = self.hovered_link.clone().map(|link| {
            div()
                .absolute()
                .top(px(
                    PAD_TOP + link.line as f32 * ROW_HEIGHT + ROW_HEIGHT - 3.0
                ))
                .left(px(PAD_LEFT + link.columns.start as f32 * cell))
                .w(px(link.columns.len() as f32 * cell))
                .h(px(1.0))
                .bg(palette.foreground)
        });
        let pointer = self.hovered_link.is_some();
        let selection = self.selection_rects().into_iter().map(|rect| {
            div()
                .absolute()
                .top(px(PAD_TOP + rect.line as f32 * ROW_HEIGHT))
                .left(px(PAD_LEFT + rect.start as f32 * cell))
                .w(px((rect.end - rect.start) as f32 * cell))
                .h(px(ROW_HEIGHT))
                .bg(palette.selection)
        });
        let rows = screen.rows.iter().enumerate().map(|(index, row)| {
            let text = if row.text.is_empty() {
                " ".to_string()
            } else {
                row.text.clone()
            };
            div()
                .id(("terminal-row", index))
                .h(px(ROW_HEIGHT))
                .whitespace_nowrap()
                .child(StyledText::new(SharedString::from(text)).with_highlights(row_runs(row)))
        });
        let measured = self.text_size.clone();
        let origin = self.text_origin.clone();
        let input = (self.focus.clone(), cx.entity());
        let surface_border = colors.border.opacity(0.9);
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex_col()
            .overflow_hidden()
            // `.dcc-terminal__viewport`
            .p(px(VIEWPORT_PAD))
            .rounded(px(10.0))
            .border_1()
            .border_dashed()
            .border_color(colors.border)
            .bg(mix_oklab(
                mix_oklab(colors.muted, 0.86, colors.foreground),
                0.72,
                gpui::transparent_black(),
            ))
            .child(
                // `.dcc-terminal__surface`
                div()
                    .id("terminal-surface")
                    .debug_selector(|| "terminal-surface".into())
                    .track_focus(&self.focus)
                    .key_context("Terminal")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(surface_border)
                    .bg(palette.background)
                    .shadow(vec![BoxShadow {
                        color: gpui::white().opacity(0.06),
                        offset: point(px(0.0), px(1.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(0.0),
                        inset: true,
                    }])
                    .map(|surface| {
                        if pointer {
                            surface.cursor_pointer()
                        } else {
                            surface.cursor_text()
                        }
                    })
                    .text_color(palette.foreground)
                    .font_family(fonts::MONO)
                    .text_size(px(FONT_SIZE))
                    .line_height(px(ROW_HEIGHT))
                    .on_key_down(cx.listener(Self::on_key))
                    .on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_wheel(event, cx)))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event, window, cx| this.mouse_down(event, window, cx)),
                    )
                    .on_mouse_move(cx.listener(|this, event, _, cx| this.mouse_move(event, cx)))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event, _, cx| this.mouse_up(event, cx)),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                measured.set(Some(bounds.size));
                                origin.set(Some(point(
                                    bounds.origin.x + px(PAD_LEFT),
                                    bounds.origin.y + px(PAD_TOP),
                                )));
                            },
                            move |bounds, _, window, cx| {
                                let (focus, entity) = input;
                                if focus.is_focused(window) {
                                    window.handle_input(
                                        &focus,
                                        ElementInputHandler::new(bounds, entity),
                                        cx,
                                    );
                                }
                            },
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .children(selection)
                    .child(
                        div()
                            .absolute()
                            .top(px(PAD_TOP))
                            .left(px(PAD_LEFT))
                            .right(px(PAD_RIGHT))
                            .flex()
                            .flex_col()
                            .children(rows),
                    )
                    .children(link)
                    .children(caret),
            )
    }
}

/// Shared slots the surface fills while it paints.
pub(super) type Measured = Rc<Cell<Option<gpui::Size<Pixels>>>>;
pub(super) type Origin = Rc<Cell<Option<gpui::Point<Pixels>>>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_fits_the_room_like_fit_addon() {
        // 14px of padding across, 24px down; 7.2px cells, 20px rows.
        assert_eq!(fit(14.0 + 7.2 * 100.0, 24.0 + 20.0 * 30.0, 7.2), (100, 30));
        assert_eq!(
            fit(14.0 + 7.2 * 100.0 - 1.0, 24.0 + 20.0 * 30.0 - 1.0, 7.2),
            (99, 29)
        );
        assert_eq!(fit(0.0, 0.0, 7.2), (2, 1));
    }
}
