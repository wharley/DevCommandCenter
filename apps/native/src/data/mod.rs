//! The sidebar's data: read from the desktop's database, shaped the way the
//! desktop's rail shapes it. No GPUI types here; everything is testable.

pub mod actions;
pub mod activity;
pub mod agents;
pub mod child_tasks;
pub mod conversation;
pub mod daily_recap;
pub mod deletion;
pub mod disk;
pub mod git_watch;
pub mod loader;
pub mod plan_checklist;
pub mod project_logo;
pub mod prompt;
pub mod rail;
pub mod recap;
pub mod seen;
pub mod snooze;
pub mod tasks;
pub mod time;
