//! GitHub's diffstat bar: five squares, green for the added share, red for
//! the removed, neutral for what is left when few lines changed.

use gpui::{AnyElement, App, Hsla, IntoElement, div, prelude::*, px};

use crate::theme::ActiveTheme;

pub const SQUARES: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Square {
    Added,
    Removed,
    Neutral,
}

/// One square per changed line up to five; the colored ones split by
/// share, each side that changed anything keeping at least one.
pub fn squares(additions: u64, deletions: u64) -> [Square; SQUARES] {
    let total = additions + deletions;
    let colored = total.min(SQUARES as u64) as usize;
    let mut added = if total == 0 {
        0
    } else {
        ((additions as f64 / total as f64) * colored as f64).round() as usize
    };
    if additions > 0 {
        added = added.max(1);
    }
    if deletions > 0 && added >= colored {
        added = colored.saturating_sub(1);
    }
    let mut out = [Square::Neutral; SQUARES];
    for (ix, square) in out.iter_mut().enumerate().take(colored) {
        *square = if ix < added {
            Square::Added
        } else {
            Square::Removed
        };
    }
    out
}

/// The bar, `size` px a square.
pub fn diff_squares(
    additions: u64,
    deletions: u64,
    size: f32,
    (added, removed): (Hsla, Hsla),
    cx: &App,
) -> AnyElement {
    let neutral = cx.theme().colors.muted_foreground.opacity(0.25);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(1.5))
        .children(squares(additions, deletions).into_iter().map(|square| {
            div().size(px(size)).rounded(px(1.5)).bg(match square {
                Square::Added => added,
                Square::Removed => removed,
                Square::Neutral => neutral,
            })
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::Square::{Added as A, Neutral as N, Removed as R};
    use super::squares;

    #[test]
    fn five_squares_split_by_share() {
        assert_eq!(squares(0, 0), [N; 5]);
        assert_eq!(squares(10, 0), [A; 5]);
        assert_eq!(squares(0, 3), [R, R, R, N, N]);
        assert_eq!(squares(482, 37), [A, A, A, A, R]);
        assert_eq!(squares(30, 4), [A, A, A, A, R]);
        assert_eq!(squares(1, 1), [A, R, N, N, N]);
        assert_eq!(squares(2, 0), [A, A, N, N, N]);
        assert_eq!(squares(1, 50), [A, R, R, R, R]);
        assert_eq!(squares(50, 50), [A, A, A, R, R]);
    }
}
