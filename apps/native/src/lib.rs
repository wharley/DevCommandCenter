//! Dev Command Center, native GPUI frontend.
//!
//! - `theme`: colors, radii and fonts from the desktop CSS tokens
//! - `ui`: reusable components that know nothing about the product
//! - `screens`: one folder per screen
//! - `prefs`: the native app's own UI preferences

pub mod agent;
mod app;
pub mod assets;
pub mod data;
pub mod i18n;
pub mod prefs;
pub mod project;
pub mod screens;
pub mod theme;
pub mod ui;

pub use app::run;
