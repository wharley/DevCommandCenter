//! Reusable building blocks. Nothing here knows about tasks, sessions or
//! projects: components take props and read only the theme.

mod button;
pub mod dialog;
mod feedback;
mod icon;
mod kbd;
pub mod menu;
pub mod popover;
mod resize_handle;
pub mod scrollbar;
mod text;
pub mod text_area;
pub mod text_input;
mod toast;
pub mod tooltip;

pub use button::{Button, Size as ButtonSize, Variant as ButtonVariant};
pub use dialog::{Dialog, dialog_footer, dialog_footer_split, dialog_header};
pub use feedback::{Checkbox, Spinner, ThinkingIndicator, spinner, thinking_indicator};
pub use icon::Icon;
pub use kbd::{command_key, shortcut_keys};
pub use menu::{ItemStyle, Menu, MenuAlign, MenuEntry, MenuItem};
pub use resize_handle::ResizeHandle;
pub use scrollbar::ListScrollbar;
pub use text::{Tracked, tabular_nums};
pub use text_area::{TextArea, TextAreaEvent};
pub use text_input::{TextInput, TextInputEvent};
pub use toast::{ToastKind, Toaster};
pub use tooltip::Tooltip;
