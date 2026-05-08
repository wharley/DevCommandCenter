pub mod db;
pub mod events;
pub mod fs;
pub mod git;
mod git_command;
mod git_parsing;
pub mod process;
mod repo_config;

pub const PHASE_0A_MODULES: [&str; 6] = ["db", "events", "fs", "git", "process", "repo_config"];
