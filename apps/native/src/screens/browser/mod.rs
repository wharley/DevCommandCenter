//! Screen 8: the Browser beside (or over) the conversation. The page is the
//! system's engine as a child view of the GPUI window (`wry`); the bar
//! around it is GPUI.

pub mod address;
mod context;
mod download;
pub mod layout;
pub mod page;
mod popup;
pub mod surface;
mod surface_view;
#[cfg(test)]
mod tests;

pub use surface::{BrowserEvent, BrowserRequest, BrowserSurface};
