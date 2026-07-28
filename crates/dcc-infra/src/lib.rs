pub mod credential_store;
pub mod db;
pub mod events;
pub mod fs;
pub mod git;
mod git_command;
mod git_parsing;
pub mod mcp_db;
pub mod process;
mod repo_config;
mod workspace_setup_plan;

pub const PHASE_0A_MODULES: [&str; 7] = [
    "db",
    "events",
    "fs",
    "git",
    "process",
    "repo_config",
    "workspace_setup_plan",
];
