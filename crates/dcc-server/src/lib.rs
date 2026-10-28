//! The DCC backend without a UI: the HTTP API and its routes, the relay and
//! pairing, the daemon and its client, the ai-memory sidecar and keep-awake.
//! The desktop hosts it today; the native app will host the same one.

// The HTTP API's endpoint index is one `json!` literal past the default depth.
#![recursion_limit = "256"]

pub mod agent_requests;
pub mod ai_memory_sidecar;
pub mod browser_agent_requests;
pub mod browser_commands;
pub mod browser_input;
pub mod browser_mcp_bridge;
pub mod command_api;
pub mod computer_use_commands;
pub mod database;
pub mod daemon_client;
pub mod daemon_runtime;
pub mod decision_provider_settings;
pub mod delta_compaction_worker;
pub mod desktop_agent_requests;
pub mod embedded_http;
pub mod host;
pub mod http_api;
pub mod http_auth;
pub mod http_config;
pub mod http_rpc_handler;
pub mod keep_awake;
pub mod menu_bar_snapshot;
pub mod named_events;
pub mod pairing;
pub mod prompt_attachments;
pub mod quick_composer_journal;
pub mod remote_connector;
pub mod remote_push;
pub mod runtime;
pub mod shell_env;
pub mod terminals;
