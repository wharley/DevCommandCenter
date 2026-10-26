//! Screen 8: the Browser beside (or over) the conversation. The page is the
//! system's engine as a child view of the GPUI window (`wry`); the bar
//! around it is GPUI.

pub mod address;
pub mod approval;
mod audit;
mod bridge;
mod context;
mod control;
mod cookies;
mod download;
mod evidence;
mod input;
pub mod layout;
pub mod page;
mod popup;
pub mod sessions;
mod sessions_view;
pub mod surface;
mod surface_view;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_approval;
#[cfg(test)]
mod tests_evidence;

pub use surface::{BrowserEvent, BrowserRequest, BrowserSurface};
