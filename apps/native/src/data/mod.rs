//! The sidebar's data: read from the desktop's database, shaped the way the
//! desktop's rail shapes it. No GPUI types here; everything is testable.

pub mod actions;
pub mod activity;
pub mod agents;
pub mod avatar;
pub mod child_tasks;
pub mod code_tree;
pub mod composer_settings;
pub mod conflict_api;
pub mod conversation;
pub mod daily_recap;
pub mod deletion;
pub mod desktop_api;
pub mod diff;
pub mod disk;
pub mod forge;
pub mod git_watch;
pub mod goal;
pub mod highlight;
pub mod loader;
pub mod model_picker;
pub mod open_project;
pub mod plan_checklist;
pub mod project_logo;
pub mod prompt;
pub mod provider_catalog;
pub mod provider_selection;
pub mod rail;
pub mod recap;
pub mod seen;
pub mod session_list;
pub mod setup_report;
pub mod snooze;
pub mod tasks;
pub mod terminal_api;
pub mod time;
