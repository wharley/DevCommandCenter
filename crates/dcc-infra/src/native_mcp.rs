//! Remote MCP servers the user configured in Claude Code, Codex and Cursor.
//!
//! Read-only: DCC never edits these files. It uses them to offer an import
//! into its own connectors and to avoid sending a provider a second copy of a
//! server that provider already loads from its own config. Only remote HTTP
//! servers are read; local commands keep running from the app that owns
//! them.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

/// `~/.claude.json` also holds history; anything past this is not a config.
const MAX_CONFIG_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SERVERS_PER_SOURCE: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeMcpSource {
    ClaudeCode,
    Codex,
    Cursor,
}

#[derive(Clone, PartialEq, Eq)]
pub struct NativeRemoteServer {
    pub source: NativeMcpSource,
    pub name: String,
    pub url: String,
    /// Literal header values only. Values that point at environment
    /// variables are left out: the desktop app does not see the shell's
    /// environment.
    pub headers: Vec<(String, String)>,
}

impl fmt::Debug for NativeRemoteServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeRemoteServer")
            .field("source", &self.source)
            .field("name", &self.name)
            .field("url", &self.url)
            .field(
                "headers",
                &self
                    .headers
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Where each app keeps its user-level servers.
#[derive(Clone, Debug)]
pub struct NativeMcpLocations {
    /// Claude Code: `$CLAUDE_CONFIG_DIR/.claude.json`, else `~/.claude.json`.
    pub claude_file: Option<PathBuf>,
    /// Codex: `$CODEX_HOME/config.toml`.
    pub codex_file: Option<PathBuf>,
    /// Cursor: `~/.cursor/mcp.json`.
    pub cursor_file: Option<PathBuf>,
}

impl NativeMcpLocations {
    pub fn for_home(home: &Path, claude_config_dir: Option<&Path>, codex_home: &Path) -> Self {
        Self {
            claude_file: Some(claude_config_file(home, claude_config_dir)),
            codex_file: Some(codex_home.join("config.toml")),
            cursor_file: Some(home.join(".cursor").join("mcp.json")),
        }
    }
}

pub fn claude_config_file(home: &Path, claude_config_dir: Option<&Path>) -> PathBuf {
    match claude_config_dir {
        Some(dir) => dir.join(".claude.json"),
        None => home.join(".claude.json"),
    }
}

/// Every remote server found, in source order. Unreadable files count as
/// empty.
pub fn discover(locations: &NativeMcpLocations) -> Vec<NativeRemoteServer> {
    let mut servers = Vec::new();
    if let Some(path) = &locations.claude_file {
        servers.extend(read_json_servers(path, NativeMcpSource::ClaudeCode));
    }
    if let Some(path) = &locations.codex_file {
        servers.extend(read_codex_servers(path));
    }
    if let Some(path) = &locations.cursor_file {
        servers.extend(read_json_servers(path, NativeMcpSource::Cursor));
    }
    servers
}

/// Claude Code and Cursor share the `mcpServers` object shape.
pub fn read_json_servers(path: &Path, source: NativeMcpSource) -> Vec<NativeRemoteServer> {
    let Some(text) = read_config(path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let Some(servers) = root.get("mcpServers").and_then(Value::as_object) else {
        return Vec::new();
    };
    servers
        .iter()
        .take(MAX_SERVERS_PER_SOURCE)
        .filter_map(|(name, server)| {
            // SSE-only servers are not supported by DCC's gateway.
            let kind = server.get("type").and_then(Value::as_str);
            if !matches!(
                kind,
                None | Some("http" | "streamable-http" | "streamableHttp")
            ) {
                return None;
            }
            let url = remote_url(server.get("url")?.as_str()?)?;
            let headers = server
                .get("headers")
                .and_then(Value::as_object)
                .map(|headers| {
                    headers
                        .iter()
                        .filter_map(|(name, value)| literal_header(name, value.as_str()?))
                        .collect()
                })
                .unwrap_or_default();
            Some(NativeRemoteServer {
                source,
                name: name.clone(),
                url,
                headers,
            })
        })
        .collect()
}

/// Codex keeps servers under `[mcp_servers.<name>]` in `config.toml`.
pub fn read_codex_servers(path: &Path) -> Vec<NativeRemoteServer> {
    let Some(text) = read_config(path) else {
        return Vec::new();
    };
    let Ok(root) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(servers) = root.get("mcp_servers").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    servers
        .iter()
        .take(MAX_SERVERS_PER_SOURCE)
        .filter_map(|(name, server)| {
            let server = server.as_table()?;
            if server.get("enabled").and_then(toml::Value::as_bool) == Some(false) {
                return None;
            }
            let url = remote_url(server.get("url")?.as_str()?)?;
            let headers = server
                .get("http_headers")
                .and_then(toml::Value::as_table)
                .map(|headers| {
                    headers
                        .iter()
                        .filter_map(|(name, value)| literal_header(name, value.as_str()?))
                        .collect()
                })
                .unwrap_or_default();
            Some(NativeRemoteServer {
                source: NativeMcpSource::Codex,
                name: name.clone(),
                url,
                headers,
            })
        })
        .collect()
}

/// Same server when scheme, host (any case), port, path (ignoring a
/// trailing slash) and query match. Mirrors the desktop catalog's rule.
pub fn same_server_url(left: &str, right: &str) -> bool {
    match (normalized(left), normalized(right)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn normalized(value: &str) -> Option<String> {
    let mut url = Url::parse(value.trim()).ok()?;
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/').to_string();
    Some(format!(
        "{}://{}{}{}{}",
        url.scheme(),
        url.host_str()?.to_ascii_lowercase(),
        url.port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default(),
        path,
        url.query()
            .map(|query| format!("?{query}"))
            .unwrap_or_default(),
    ))
}

fn read_config(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// HTTPS, or HTTP on a loopback host, without credentials in the URL.
fn remote_url(value: &str) -> Option<String> {
    let url = Url::parse(value.trim()).ok()?;
    let loopback = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    let allowed = url.scheme() == "https" || (url.scheme() == "http" && loopback);
    if !allowed || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    Some(value.trim().to_string())
}

fn literal_header(name: &str, value: &str) -> Option<(String, String)> {
    let value = value.trim();
    if name.trim().is_empty() || value.is_empty() || value.contains("${") || value.contains("$env:")
    {
        return None;
    }
    Some((name.trim().to_string(), value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn reads_remote_servers_from_all_three_apps() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        write(
            home,
            ".claude.json",
            r#"{
              "numStartups": 3,
              "mcpServers": {
                "clickup": { "type": "http", "url": "https://mcp.clickup.com/mcp" },
                "legacy": { "type": "sse", "url": "https://legacy.example.com/sse" },
                "local": { "type": "stdio", "command": "npx", "args": ["server"] },
                "github": {
                  "type": "http",
                  "url": "https://api.githubcopilot.com/mcp/",
                  "headers": { "Authorization": "Bearer ghp_literal", "X-Env": "${TOKEN}" }
                }
              }
            }"#,
        );
        let codex_home = home.join(".codex");
        write(
            &codex_home,
            "config.toml",
            r#"
model = "gpt-5"
[mcp_servers.linear]
url = "https://mcp.linear.app/mcp"
[mcp_servers.off]
url = "https://off.example.com/mcp"
enabled = false
[mcp_servers.fs]
command = "npx"
"#,
        );
        write(
            home,
            ".cursor/mcp.json",
            r#"{ "mcpServers": { "notion": { "url": "https://mcp.notion.com/mcp" },
                 "plain": { "url": "http://intranet.example.com/mcp" } } }"#,
        );

        let found = discover(&NativeMcpLocations::for_home(home, None, &codex_home));
        let names = found
            .iter()
            .map(|server| (server.source, server.name.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                (NativeMcpSource::ClaudeCode, "clickup"),
                (NativeMcpSource::ClaudeCode, "github"),
                (NativeMcpSource::Codex, "linear"),
                (NativeMcpSource::Cursor, "notion"),
            ]
        );
        let github = &found[1];
        assert_eq!(
            github.headers,
            vec![(
                "Authorization".to_string(),
                "Bearer ghp_literal".to_string()
            )]
        );
        assert!(!format!("{github:?}").contains("ghp_literal"));
    }

    #[test]
    fn an_account_config_dir_has_its_own_claude_file() {
        let home = Path::new("/home/me");
        assert_eq!(
            claude_config_file(home, Some(Path::new("/accounts/work"))),
            PathBuf::from("/accounts/work/.claude.json")
        );
        assert_eq!(
            claude_config_file(home, None),
            PathBuf::from("/home/me/.claude.json")
        );
    }

    #[test]
    fn missing_or_broken_files_count_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let broken = write(dir.path(), "broken.json", "{ not json");
        assert!(read_json_servers(&broken, NativeMcpSource::Cursor).is_empty());
        assert!(read_codex_servers(&dir.path().join("missing.toml")).is_empty());
    }

    #[test]
    fn compares_urls_like_the_catalog() {
        assert!(same_server_url(
            "https://MCP.Linear.app/mcp/",
            "https://mcp.linear.app/mcp"
        ));
        assert!(same_server_url(
            "https://mcp.linear.app/mcp#x",
            "https://mcp.linear.app/mcp"
        ));
        assert!(!same_server_url(
            "https://mcp.linear.app/mcp",
            "https://mcp.linear.app/sse"
        ));
        assert!(!same_server_url("not a url", "not a url"));
    }
}
