//! A custom connector (`AddCustomConnectorDialog`, `connector-model.ts`):
//! its address, the name it gets, the scopes and the logo the person
//! picks; and the import offer (`import-offer.ts`): the remote servers
//! Claude Code, Codex or Cursor already have, offered once each.

use std::path::Path;

use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};

use super::connectors::{Integration, integration_for_url, preset_for_url, same_url};
use super::desktop_api::{ApiError, DesktopApi};

/// `MAX_LOGO_BYTES`
pub const MAX_LOGO_BYTES: usize = 256 * 1024;

/// `parseCustomConnectorUrl`: https (or loopback http), no credentials, a
/// host with a dot; the fragment dropped.
pub fn parse_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() > 2048 {
        return None;
    }
    let (scheme, rest) = value.split_once("://")?;
    let scheme = scheme.to_lowercase();
    let rest = rest.split('#').next().unwrap_or_default();
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains('@') || authority.is_empty() {
        return None;
    }
    let host = if authority.starts_with('[') {
        authority.split_inclusive(']').next().unwrap_or(authority)
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, port)| {
                if port.chars().all(|c| c.is_ascii_digit()) {
                    host
                } else {
                    authority
                }
            })
    }
    .to_lowercase();
    let loopback = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]");
    let allowed = scheme == "https" || (scheme == "http" && loopback);
    if !allowed || (!loopback && !host.contains('.')) || host.contains(char::is_whitespace) {
        return None;
    }
    let path = &rest[end..];
    // `new URL(...).toString()` adds the root path.
    let path = if path.is_empty() || path.starts_with('?') {
        format!("/{path}")
    } else {
        path.to_string()
    };
    Some(format!("{scheme}://{}{path}", authority.to_lowercase()))
}

/// Hosting domains whose own name says nothing about the service.
const HOSTING: [&str; 9] = [
    "vercel.app",
    "netlify.app",
    "pages.dev",
    "workers.dev",
    "fly.dev",
    "onrender.com",
    "railway.app",
    "herokuapp.com",
    "up.railway.app",
];

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// `defaultConnectorName`: "mcp.example.com" → "Example",
/// "app.zedy.com.br" → "Zedy", "zedy-git-x.vercel.app" → "Zedy".
pub fn default_name(url: &str) -> String {
    let host = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split(['/', '?', ':'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    if let Some(hosting) = HOSTING
        .iter()
        .find(|suffix| host.ends_with(&format!(".{suffix}")))
    {
        let label = host[..host.len() - hosting.len() - 1]
            .split('.')
            .next()
            .unwrap_or(&host);
        let word = label
            .split('-')
            .next()
            .filter(|word| !word.is_empty())
            .unwrap_or(label);
        return capitalized(word);
    }
    let parts: Vec<&str> = host
        .split('.')
        .filter(|part| *part != "www" && *part != "mcp")
        .collect();
    if parts.is_empty() {
        return capitalized(&host);
    }
    let mut end = parts.len() - 1;
    // ".com.br", ".co.uk"
    if end >= 2
        && parts[end].len() == 2
        && ["com", "co", "org", "net", "gov", "edu", "ac"].contains(&parts[end - 1])
    {
        end -= 1;
    }
    capitalized(if end >= 1 { parts[end - 1] } else { parts[0] })
}

/// `isValidScopeList`: RFC 6749 tokens separated by spaces.
pub fn valid_scopes(value: &str) -> bool {
    let scopes: Vec<&str> = value.split_whitespace().collect();
    !scopes.is_empty()
        && scopes.join(" ").len() <= 1024
        && scopes.iter().all(|scope| {
            scope.bytes().all(|byte| {
                byte == 0x21 || (0x23..=0x5B).contains(&byte) || (0x5D..=0x7E).contains(&byte)
            })
        })
}

/// `readLogoFile`: an image the person picked as a `data:` URI (PNG, JPEG,
/// GIF, WebP or SVG, 256 KB at most).
pub fn read_logo(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > MAX_LOGO_BYTES {
        return None;
    }
    let extension = path.extension()?.to_str()?.to_lowercase();
    let kind = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    Some(format!(
        "data:{kind};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// `NativeMcpServerSummary`
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeServer {
    /// `claudeCode` | `codex` | `cursor`
    pub app: String,
    pub name: String,
    pub url: String,
}

/// `NATIVE_APP_NAMES`
pub fn app_name(app: &str) -> &str {
    match app {
        "claudeCode" => "Claude Code",
        "codex" => "Codex",
        "cursor" => "Cursor",
        other => other,
    }
}

/// `importDisplayName`: the catalog's name, else the server's own, spaced.
pub fn import_name(server: &NativeServer) -> String {
    if let Some(preset) = preset_for_url(&server.url) {
        return preset.name.into();
    }
    let mut spaced = String::new();
    let mut previous: Option<char> = None;
    for c in server.name.chars() {
        if c.is_ascii_uppercase()
            && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
        {
            spaced.push(' ');
        }
        spaced.push(if c == '-' || c == '_' { ' ' } else { c });
        previous = Some(c);
    }
    let spaced = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if spaced.is_empty() {
        server.url.clone()
    } else {
        capitalized(&spaced)
    }
}

/// `importCandidates`: not in DCC yet, not offered before, one per URL.
pub fn candidates(
    found: &[NativeServer],
    integrations: &[Integration],
    offered: &[String],
) -> Vec<(NativeServer, String)> {
    let mut picked: Vec<(NativeServer, String)> = Vec::new();
    for server in found {
        if integration_for_url(integrations, &server.url).is_some()
            || offered.iter().any(|url| same_url(url, &server.url))
            || picked
                .iter()
                .any(|(other, _)| same_url(&other.url, &server.url))
        {
            continue;
        }
        picked.push((server.clone(), import_name(server)));
    }
    picked
}

/// Servers offered, imported or declined (`dcc.connectors.importOffered`).
pub fn offered_path() -> std::path::PathBuf {
    crate::prefs::native_file("connector-import-offered.json")
}

pub fn read_offered(path: &Path) -> Vec<String> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// `rememberOffered`: the last 500.
pub fn remember_offered(path: &Path, urls: &[String]) {
    let mut all = read_offered(path);
    for url in urls {
        if !all.contains(url) {
            all.push(url.clone());
        }
    }
    let keep = all.len().saturating_sub(500);
    all.drain(..keep);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_vec(&all).unwrap_or_default());
}

impl DesktopApi {
    /// `setMcpConnectorLogo` (`None` removes it).
    pub fn set_connector_logo(&self, id: &str, logo: Option<&str>) -> Result<(), ApiError> {
        self.post(
            "/api/v1/mcp/logo",
            &json!({ "definitionId": id, "logo": logo }),
        )
        .map(|_| ())
    }

    /// `fetchMcpConnectorLogo`: the site's icon, if it has one.
    pub fn fetch_connector_logo(&self, id: &str) -> Result<Option<String>, ApiError> {
        let value = self.post("/api/v1/mcp/logo/fetch", &json!({ "definitionId": id }))?;
        Ok(value
            .get("logo")
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    /// `setMcpProjectEnabled`
    pub fn set_connector_project(
        &self,
        id: &str,
        project_id: &str,
        enabled: bool,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/mcp/project",
            &json!({ "definitionId": id, "projectId": project_id, "enabled": enabled }),
        )
        .map(|_| ())
    }

    /// `setMcpToolPolicy`: `ask`, `allow` or `deny`.
    pub fn set_tool_policy(&self, id: &str, tool: &str, decision: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/mcp/tool-policy",
            &json!({ "definitionId": id, "toolName": tool, "decision": decision }),
        )
        .map(|_| ())
    }

    /// `listNativeMcpServers`
    pub fn native_servers(&self) -> Result<Vec<NativeServer>, ApiError> {
        let value = self.get("/api/v1/mcp/native-servers")?;
        serde_json::from_value(
            value
                .get("servers")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
        .map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// `importNativeMcpServers`: the integrations made.
    pub fn import_native(
        &self,
        servers: &[(NativeServer, String)],
    ) -> Result<Vec<Integration>, ApiError> {
        let servers: Vec<Value> = servers
            .iter()
            .map(|(server, display)| json!({ "app": server.app, "name": server.name, "displayName": display }))
            .collect();
        let value = self.post("/api/v1/mcp/native-servers", &json!({ "servers": servers }))?;
        serde_json::from_value(
            value
                .get("integrations")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
        .map_err(|error| ApiError::Failed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_names_and_scopes_read_like_the_desktop() {
        assert_eq!(
            parse_url(" https://MCP.Example.com/mcp#x ").as_deref(),
            Some("https://mcp.example.com/mcp")
        );
        assert_eq!(
            parse_url("https://example.com").as_deref(),
            Some("https://example.com/")
        );
        assert_eq!(
            parse_url("http://localhost:8080/mcp").as_deref(),
            Some("http://localhost:8080/mcp")
        );
        assert_eq!(parse_url("http://example.com/mcp"), None);
        assert_eq!(parse_url("https://user:pass@example.com"), None);
        assert_eq!(parse_url("https://intranet/mcp"), None);
        assert_eq!(parse_url("nope"), None);
        assert_eq!(default_name("https://mcp.example.com/mcp"), "Example");
        assert_eq!(default_name("https://app.zedy.com.br/mcp"), "Zedy");
        assert_eq!(
            default_name("https://zedy-git-feature.vercel.app/mcp"),
            "Zedy"
        );
        assert_eq!(default_name("https://localhost/mcp"), "Localhost");
        assert!(valid_scopes("support:read  reports:read"));
        assert!(!valid_scopes("bad\\scope"));
        assert!(!valid_scopes("   "));
    }

    #[test]
    fn the_import_offer_skips_what_is_known_and_offered() {
        let server = |app: &str, name: &str, url: &str| NativeServer {
            app: app.into(),
            name: name.into(),
            url: url.into(),
        };
        let found = [
            server("claudeCode", "linear", "https://mcp.linear.app/mcp"),
            server("cursor", "linearAgain", "https://mcp.linear.app/mcp/"),
            server("codex", "myInternal_tools", "https://tools.acme.dev/mcp"),
            server("codex", "old", "https://old.acme.dev/mcp"),
        ];
        let picked = candidates(&found, &[], &["https://old.acme.dev/mcp".into()]);
        let names: Vec<&str> = picked.iter().map(|(_, name)| name.as_str()).collect();
        assert_eq!(names, ["Linear", "My Internal tools"]);
        assert_eq!(app_name("claudeCode"), "Claude Code");
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("offered.json");
        remember_offered(&file, &["a".into(), "b".into()]);
        remember_offered(&file, &["b".into(), "c".into()]);
        assert_eq!(read_offered(&file), ["a", "b", "c"]);
        let logo = dir.path().join("logo.png");
        std::fs::write(&logo, b"\\x89PNG").unwrap();
        assert!(
            read_logo(&logo)
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        std::fs::write(dir.path().join("logo.bmp"), b"x").unwrap();
        assert_eq!(read_logo(&dir.path().join("logo.bmp")), None);
    }
}
