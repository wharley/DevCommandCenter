//! The theme tiles' little window (`.dcc-settings-theme-preview`): a
//! sidebar with three bars and a conversation with three lines and the
//! composer, in fixed light or dark colors; "Sistema" lays the dark one
//! over the lower-right half (`clip-path: polygon(100% 0, 100% 100%, 0 100%)`).

use gpui::{Bounds, Hsla, Path, Pixels, Point, Window, canvas, fill, point, prelude::*, px, size};

use crate::theme::color::hex;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Light,
    Dark,
    /// Light, the dark one over the lower-right triangle.
    System,
}

struct Colors {
    background: u32,
    sidebar: u32,
    bar: u32,
    line: u32,
    input: u32,
    input_border: u32,
}

const LIGHT: Colors = Colors {
    background: 0xfafafa,
    sidebar: 0xeeeeed,
    bar: 0xbcbfbd,
    line: 0xd7dad8,
    input: 0xf0f1ef,
    input_border: 0xdfe1df,
};

const DARK: Colors = Colors {
    background: 0x111312,
    sidebar: 0x202321,
    bar: 0x555c58,
    line: 0x3e4541,
    input: 0x222623,
    input_border: 0x363d38,
};

/// The calm bar in both.
const SELECTED_BAR: u32 = 0x78b8a6;

/// One painted box: where, its color and corner radius.
struct Piece {
    bounds: Bounds<Pixels>,
    color: Hsla,
    radius: f32,
}

/// The pieces of one look inside `area` (the preview, inside its border).
fn pieces(area: Bounds<Pixels>, colors: &Colors) -> Vec<Piece> {
    let (x, y) = (f32::from(area.origin.x), f32::from(area.origin.y));
    let (width, height) = (f32::from(area.size.width), f32::from(area.size.height));
    let rect = |left: f32, top: f32, w: f32, h: f32| {
        Bounds::new(point(px(x + left), px(y + top)), size(px(w), px(h)))
    };
    let mut out = vec![Piece {
        bounds: area,
        color: hex(colors.background),
        radius: 0.0,
    }];
    // `.dcc-settings-preview-sidebar`: 27% wide, 14/8 padding, a 1px edge.
    let sidebar = width * 0.27;
    out.push(Piece {
        bounds: rect(0.0, 0.0, sidebar, height),
        color: hex(colors.sidebar),
        radius: 0.0,
    });
    out.push(Piece {
        bounds: rect(sidebar - 1.0, 0.0, 1.0, height),
        color: Hsla::from(gpui::rgba(0x8080801f)),
        radius: 0.0,
    });
    let bar_width = sidebar - 1.0 - 16.0;
    for (index, (share, color)) in [(0.75, colors.bar), (1.0, SELECTED_BAR), (0.75, colors.bar)]
        .into_iter()
        .enumerate()
    {
        out.push(Piece {
            bounds: rect(8.0, 14.0 + index as f32 * 13.0, bar_width * share, 4.0),
            color: hex(color),
            radius: 3.0,
        });
    }
    // `.dcc-settings-preview-conversation`: 17/12/10 padding.
    let left = sidebar + 12.0;
    let room = width - sidebar - 24.0;
    for (index, share) in [0.8, 0.95, 0.55].into_iter().enumerate() {
        out.push(Piece {
            bounds: rect(left, 17.0 + index as f32 * 9.0, room * share, 3.0),
            color: hex(colors.line),
            radius: 3.0,
        });
    }
    let composer = rect(left, height - 10.0 - 19.0, room, 19.0);
    out.push(Piece {
        bounds: composer,
        color: hex(colors.input_border),
        radius: 5.0,
    });
    out.push(Piece {
        bounds: Bounds::new(
            point(composer.origin.x + px(1.0), composer.origin.y + px(1.0)),
            size(
                composer.size.width - px(2.0),
                composer.size.height - px(2.0),
            ),
        ),
        color: hex(colors.input),
        radius: 4.0,
    });
    out
}

/// The part of `bounds` on the lower-right of the diagonal from the area's
/// top-right corner to its bottom-left one (x/w + y/h ≥ 1).
fn lower_right(bounds: Bounds<Pixels>, area: Bounds<Pixels>) -> Vec<Point<Pixels>> {
    let (x0, y0) = (f32::from(area.origin.x), f32::from(area.origin.y));
    let (w, h) = (f32::from(area.size.width), f32::from(area.size.height));
    let side = |p: (f32, f32)| (p.0 - x0) / w + (p.1 - y0) / h - 1.0;
    let (l, t) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let (r, b) = (
        l + f32::from(bounds.size.width),
        t + f32::from(bounds.size.height),
    );
    let corners = [(l, t), (r, t), (r, b), (l, b)];
    let mut out = Vec::new();
    for index in 0..4 {
        let (a, c) = (corners[index], corners[(index + 1) % 4]);
        let (sa, sc) = (side(a), side(c));
        if sa >= 0.0 {
            out.push(point(px(a.0), px(a.1)));
        }
        // A real crossing (a corner on the line is already kept).
        if (sa > 0.0 && sc < 0.0) || (sa < 0.0 && sc > 0.0) {
            let t = sa / (sa - sc);
            out.push(point(px(a.0 + (c.0 - a.0) * t), px(a.1 + (c.1 - a.1) * t)));
        }
    }
    out
}

fn paint(pieces: Vec<Piece>, window: &mut Window) {
    for piece in pieces {
        window.paint_quad(fill(piece.bounds, piece.color).corner_radii(px(piece.radius)));
    }
}

/// The preview, 93px tall, filling its tile's width.
pub fn preview(look: Look) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| match look {
            Look::Light => paint(pieces(bounds, &LIGHT), window),
            Look::Dark => paint(pieces(bounds, &DARK), window),
            Look::System => {
                paint(pieces(bounds, &LIGHT), window);
                for piece in pieces(bounds, &DARK) {
                    let polygon = lower_right(piece.bounds, bounds);
                    let Some((first, rest)) = polygon.split_first() else {
                        continue;
                    };
                    let mut path = Path::new(*first);
                    for corner in rest {
                        path.line_to(*corner);
                    }
                    path.line_to(*first);
                    window.paint_path(path, piece.color);
                }
            }
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dark_half_is_the_lower_right_triangle() {
        let area = Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(100.0)));
        // The whole area keeps three corners: top-right, bottom-right, bottom-left.
        assert_eq!(lower_right(area, area).len(), 3);
        // A box in the upper-left corner is not in it.
        let corner = Bounds::new(point(px(0.0), px(0.0)), size(px(10.0), px(10.0)));
        assert!(lower_right(corner, area).is_empty());
        // One in the lower-right is whole.
        let inside = Bounds::new(point(px(80.0), px(80.0)), size(px(10.0), px(10.0)));
        assert_eq!(lower_right(inside, area).len(), 4);
    }
}
