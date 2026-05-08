pub mod db;
pub mod events;
pub mod fs;
pub mod git;
mod git_command;
mod git_parsing;
pub mod process;

pub const PHASE_0A_MODULES: [&str; 5] = ["db", "events", "fs", "git", "process"];
