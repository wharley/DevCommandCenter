//! The paint order of what floats (`deferred` priorities). GPUI paints every
//! deferred draw from one list sorted by priority, nested or not, so a menu,
//! popover or tooltip opened inside a dialog must outrank the dialog, or it
//! paints under the dialog and the dialog takes its clicks.

pub const DIALOG: usize = 10;
/// Menus and popovers, over a dialog they open in.
pub const POPUP: usize = 12;
pub const SUBMENU: usize = 13;
pub const TOOLTIP: usize = 14;
pub const TOAST: usize = 20;
