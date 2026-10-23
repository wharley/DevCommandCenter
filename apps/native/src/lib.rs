//! Dev Command Center, native GPUI frontend.
//!
//! - `theme`: colors, radii and fonts from the desktop CSS tokens
//! - `ui`: reusable components that know nothing about the product
//! - `screens`: one folder per screen
//! - `prefs`: the native app's own UI preferences

mod app;
pub mod prefs;
pub mod screens;
pub mod theme;
pub mod ui;

pub use app::run;
