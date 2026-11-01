//! A native view (the Browser's page) sits above everything GPUI draws, so
//! the overlays that open over it say where they are (the desktop's
//! `data-dcc-browser-occluder`): a dialog, a menu, a popover. The page
//! hides while one covers it and comes back when it closes.
//!
//! Overlays paint after the page has placed itself, so a page reads the
//! overlays of the frame before; one that newly covers it asks for another
//! frame.

use gpui::{App, Bounds, Global, Pixels, Window};

#[derive(Default)]
struct Overlays {
    /// Painted in the frame being drawn.
    current: Vec<Bounds<Pixels>>,
    /// Where the page is, while there is one.
    page: Option<Bounds<Pixels>>,
    /// Whether the page shows now.
    page_visible: bool,
}

impl Global for Overlays {}

/// An overlay painted at `bounds`.
pub fn mark(bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let overlays = cx.default_global::<Overlays>();
    let Some(page) = overlays.page else {
        return;
    };
    // A page that stopped drawing reads none; keep only the latest.
    if overlays.current.len() >= 64 {
        overlays.current.remove(0);
    }
    overlays.current.push(bounds);
    if overlays.page_visible && page.intersects(&bounds) {
        window.refresh();
    }
}

/// An empty layer over its parent (positioned) that marks it as an overlay.
pub fn marker() -> impl gpui::IntoElement {
    use gpui::Styled;
    gpui::canvas(mark, |_, _, _, _| {}).absolute().inset_0()
}

/// Called by the page as it places itself: whether the overlays of the
/// frame before cover it.
pub fn page_covered(page: Bounds<Pixels>, cx: &mut App) -> bool {
    let overlays = cx.default_global::<Overlays>();
    let covered = overlays
        .current
        .drain(..)
        .any(|overlay| overlay.intersects(&page));
    overlays.page = Some(page);
    overlays.page_visible = !covered;
    covered
}

/// Whether the Browser's page is up: cached views draw fresh meanwhile, so
/// their overlays keep saying where they are.
pub fn page_open(cx: &App) -> bool {
    cx.try_global::<Overlays>()
        .is_some_and(|overlays| overlays.page.is_some())
}

/// The page is gone.
pub fn page_closed(cx: &mut App) {
    let overlays = cx.default_global::<Overlays>();
    overlays.page = None;
    overlays.current.clear();
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, point, px, size};

    use super::*;

    #[gpui::test]
    fn a_covering_overlay_hides_the_page_on_the_next_frame(cx: &mut TestAppContext) {
        let page = Bounds::new(point(px(100.0), px(100.0)), size(px(400.0), px(300.0)));
        let menu = Bounds::new(point(px(450.0), px(120.0)), size(px(200.0), px(100.0)));
        let aside = Bounds::new(point(px(0.0), px(0.0)), size(px(50.0), px(50.0)));
        let window = cx.add_empty_window();
        window.update(|window, cx| {
            assert!(!page_covered(page, cx));
            mark(aside, window, cx);
            assert!(!page_covered(page, cx), "an overlay elsewhere");
            mark(menu, window, cx);
            assert!(page_covered(page, cx), "the menu over the page");
            assert!(!page_covered(page, cx), "the menu closed");
            page_closed(cx);
            mark(menu, window, cx);
            assert!(!page_covered(page, cx), "nothing kept without a page");
        });
    }
}
