//! The sidebar's data: read from the desktop's database, shaped the way the
//! desktop's rail shapes it. No GPUI types here; everything is testable.

pub mod actions;
pub mod activity;
pub mod agents;
pub mod child_tasks;
pub mod composer_settings;
pub mod conversation;
pub mod daily_recap;
pub mod deletion;
pub mod desktop_api;
pub mod disk;
pub mod git_watch;
pub mod goal;
pub mod loader;
pub mod model_picker;
pub mod plan_checklist;
pub mod project_logo;
pub mod prompt;
pub mod provider_catalog;
pub mod rail;
pub mod recap;
pub mod seen;
pub mod setup_report;
pub mod snooze;
pub mod tasks;
pub mod time;
