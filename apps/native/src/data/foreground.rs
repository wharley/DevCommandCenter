//! Whether the main window is in front. Background reads that shell out
//! (`git`, `gh`, `glab`) pause while it is not and run again as it comes
//! back, like the desktop's react-query (`refetchInterval` stops with the
//! window unfocused, `refetchOnWindowFocus` reads on return): each one
//! starts a process, which costs the Mac more than the read is worth while
//! nobody looks.

use gpui::{App, Global};

#[derive(Clone, Copy)]
pub struct Foreground(pub bool);

impl Global for Foreground {}

/// True until the window says otherwise (tests, the first frames).
pub fn is_foreground(cx: &App) -> bool {
    cx.try_global::<Foreground>()
        .is_none_or(|foreground| foreground.0)
}
