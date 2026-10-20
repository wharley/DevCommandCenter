//! The user's own MCP config in Claude Code, Codex and Cursor, as DCC's
//! connectors see it: what can be imported, and which DCC connector a
//! provider already loads natively (D11 — that provider keeps its own copy
//! and does not get DCC's as well).

use std::path::PathBuf;

use dcc_core::ports::{ProviderMcpServerConfig, ProviderMcpTransport, ProviderRuntimeConfig};
use dcc_infra::native_mcp::{
    claude_config_file, discover, read_codex_servers, read_json_servers, same_server_url,
    NativeMcpLocations, NativeMcpSource, NativeRemoteServer,
};
use dcc_providers::common::{codex_system_home, resolve_runtime_home_path};

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

fn inherited_claude_config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

/// User-level servers of the system logins, for the import offer.
pub fn discover_importable() -> Vec<NativeRemoteServer> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    discover(&NativeMcpLocations::for_home(
        &home,
        inherited_claude_config_dir().as_deref(),
        &codex_system_home(),
    ))
}

/// Remote servers the provider process of this session loads by itself:
/// Claude Code from the account's `.claude.json`, Codex from its home's
/// `config.toml`. Other providers load none DCC knows of.
fn native_servers_for(
    provider_id: &str,
    runtime: &ProviderRuntimeConfig,
) -> Vec<NativeRemoteServer> {
    match provider_id {
        "claude_code" => {
            let Some(home) = home_dir() else {
                return Vec::new();
            };
            let config_dir = resolve_runtime_home_path(runtime.config_dir.as_deref())
                .or_else(inherited_claude_config_dir);
            read_json_servers(
                &claude_config_file(&home, config_dir.as_deref()),
                NativeMcpSource::ClaudeCode,
            )
        }
        "codex" => {
            let codex_home = resolve_runtime_home_path(runtime.home_path.as_deref())
                .unwrap_or_else(codex_system_home);
            read_codex_servers(&codex_home.join("config.toml"))
        }
        _ => Vec::new(),
    }
}

/// Leaves out DCC's remote servers that this provider already loads from its
/// own config, so the agent never sees the same server twice.
pub fn drop_native_duplicates(
    provider_id: &str,
    runtime: &ProviderRuntimeConfig,
    servers: Vec<ProviderMcpServerConfig>,
) -> Vec<ProviderMcpServerConfig> {
    if servers.is_empty() {
        return servers;
    }
    let native = native_servers_for(provider_id, runtime);
    without_native(servers, &native)
}

fn without_native(
    servers: Vec<ProviderMcpServerConfig>,
    native: &[NativeRemoteServer],
) -> Vec<ProviderMcpServerConfig> {
    if native.is_empty() {
        return servers;
    }
    servers
        .into_iter()
        .filter(|server| match &server.transport {
            ProviderMcpTransport::Http { url, .. } => !native
                .iter()
                .any(|native| same_server_url(&native.url, url)),
            _ => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::mcp::McpDefinitionId;

    fn http(id: &str, url: &str) -> ProviderMcpServerConfig {
        ProviderMcpServerConfig {
            definition_id: McpDefinitionId(id.to_string()),
            server_name: id.to_string(),
            transport: ProviderMcpTransport::Http {
                url: url.to_string(),
                headers: Vec::new(),
            },
            tool_policies: Vec::new(),
        }
    }

    #[test]
    fn a_server_the_provider_already_loads_is_not_sent_twice() {
        let native = vec![NativeRemoteServer {
            source: NativeMcpSource::ClaudeCode,
            name: "clickup".to_string(),
            url: "https://mcp.clickup.com/mcp/".to_string(),
            headers: Vec::new(),
        }];
        let kept = without_native(
            vec![
                http("clickup", "https://mcp.clickup.com/mcp"),
                http("linear", "https://mcp.linear.app/mcp"),
            ],
            &native,
        );
        let ids = kept
            .iter()
            .map(|server| server.definition_id.0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["linear"]);
    }

    #[test]
    fn providers_without_a_known_config_keep_everything() {
        let servers = vec![http("clickup", "https://mcp.clickup.com/mcp")];
        assert_eq!(
            drop_native_duplicates("cursor", &ProviderRuntimeConfig::default(), servers).len(),
            1
        );
    }
}
