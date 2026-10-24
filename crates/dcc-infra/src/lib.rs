pub mod ai_memory;
pub mod connector_icon;
pub mod credential_store;
pub mod db;
pub mod delegation_apply;
pub mod decision_context;
pub mod decision_provider;
pub mod events;
pub mod fs;
pub mod forge;
pub mod git;
pub mod workspace_actions;
pub mod workspace_cleanup;
pub mod workspace_deletion;
pub mod workspace_git;
mod git_command;
mod git_parsing;
#[cfg(feature = "guarded-undo-capture-v2")]
pub mod guarded_undo;
pub mod mcp_db;
pub mod mcp_gateway;
pub mod mcp_oauth;
pub mod mcp_probe;
pub mod mission_specs;
pub mod native_mcp;
pub mod notes;
pub mod process;
pub mod project_actions;
pub mod prompt_attachments;
pub mod runtime_owner;
pub mod skills_context;
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
