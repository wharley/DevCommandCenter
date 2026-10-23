//! Shell geometry, one to one with `apps/desktop/src/shell/layout.ts`
//! and the sidebar resize in `use-panels.ts`. Pure: no window needed.

pub const DEFAULT_SIDEBAR_WIDTH: f32 = 336.0;
pub const MIN_SIDEBAR_WIDTH: f32 = 220.0;
pub const MAX_SIDEBAR_WIDTH: f32 = 520.0;
/// Width of the sidebar rail while collapsed (`App.tsx`).
pub const COLLAPSED_SIDEBAR_WIDTH: f32 = 76.0;
/// Hit area of the resize separator; the visible line sits in its middle.
pub const RESIZE_HIT_AREA: f32 = 20.0;

pub fn clamp_sidebar_width(width: f32) -> f32 {
    if width.is_finite() {
        width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH)
    } else {
        DEFAULT_SIDEBAR_WIDTH
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellLayout {
    /// The width the sidebar opens at; kept while collapsed.
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    /// Pointer x and width when a resize drag started.
    drag_origin: Option<(f32, f32)>,
}

impl ShellLayout {
    pub fn new(sidebar_width: f32, sidebar_collapsed: bool) -> Self {
        Self {
            sidebar_width: clamp_sidebar_width(sidebar_width),
            sidebar_collapsed,
            drag_origin: None,
        }
    }

    /// What the sidebar actually occupies on screen.
    pub fn sidebar_rail_width(&self) -> f32 {
        if self.sidebar_collapsed {
            COLLAPSED_SIDEBAR_WIDTH
        } else {
            self.sidebar_width
        }
    }

    /// Left edge of the separator's hit area, centered on the sidebar edge.
    pub fn separator_left(&self) -> f32 {
        (self.sidebar_rail_width() - RESIZE_HIT_AREA / 2.0).max(0.0)
    }

    pub fn is_resizing(&self) -> bool {
        self.drag_origin.is_some()
    }

    pub fn start_resize(&mut self, pointer_x: f32) {
        self.drag_origin = Some((pointer_x, self.sidebar_width));
    }

    /// Applies a pointer move; returns whether the width changed.
    pub fn resize_to(&mut self, pointer_x: f32) -> bool {
        let Some((origin_x, origin_width)) = self.drag_origin else {
            return false;
        };
        let next = clamp_sidebar_width(origin_width + pointer_x - origin_x);
        if next == self.sidebar_width {
            return false;
        }
        self.sidebar_width = next;
        true
    }

    /// Ends the drag; returns whether a drag was in progress.
    pub fn finish_resize(&mut self) -> bool {
        self.drag_origin.take().is_some()
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_is_clamped_like_the_desktop() {
        assert_eq!(clamp_sidebar_width(100.0), 220.0);
        assert_eq!(clamp_sidebar_width(1000.0), 520.0);
        assert_eq!(clamp_sidebar_width(f32::NAN), 336.0);
        assert_eq!(ShellLayout::new(10.0, false).sidebar_width, 220.0);
    }

    #[test]
    fn collapsed_rail_keeps_the_open_width() {
        let mut layout = ShellLayout::new(400.0, false);
        layout.toggle_sidebar();
        assert_eq!(layout.sidebar_rail_width(), 76.0);
        layout.toggle_sidebar();
        assert_eq!(layout.sidebar_rail_width(), 400.0);
    }

    #[test]
    fn separator_is_centered_on_the_edge() {
        let layout = ShellLayout::new(336.0, false);
        assert_eq!(layout.separator_left(), 326.0);
    }

    #[test]
    fn drag_follows_the_pointer_from_where_it_started() {
        let mut layout = ShellLayout::new(336.0, false);
        assert!(!layout.resize_to(500.0), "no drag yet");
        layout.start_resize(336.0);
        assert!(layout.is_resizing());
        assert!(layout.resize_to(386.0));
        assert_eq!(layout.sidebar_width, 386.0);
        assert!(layout.resize_to(10_000.0));
        assert_eq!(layout.sidebar_width, 520.0);
        assert!(!layout.resize_to(10_100.0), "already at the max");
        assert!(layout.finish_resize());
        assert!(!layout.finish_resize());
    }
}
