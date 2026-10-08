use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The key every install shipped with before keys were generated per machine.
/// Anyone who read the source knows it, so the server refuses to start with it.
pub const LEGACY_DEFAULT_API_KEY: &str = "dcc-dev-key-change-me";

const DEFAULT_BEARER_TOKEN_TTL_SECONDS: i64 = 60 * 60 * 24;
const DEFAULT_BEARER_TOKEN_GRACE_SECONDS: i64 = 60 * 5;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HttpAuthMode {
    Local,
    Remote,
    Mixed,
}

impl Default for HttpAuthMode {
    fn default() -> Self {
        Self::Local
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BearerTokenRotation {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub api_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<HttpAuthMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_token_previous: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_token_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_token_previous_expires_at: Option<DateTime<Utc>>,
    pub db_path: PathBuf,
    pub cors_origins: Vec<String>,
    /// Explicit opt-in (`DCC_HTTP_LAN=1`) to also listen on every interface so
    /// a phone on the same network can pair. Env-only on purpose: it is never
    /// written to http-config.json, so one pairing session does not leave the
    /// next run exposed.
    #[serde(skip)]
    pub lan: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 9876,
            // Empty means "not configured yet"; `load` generates one.
            api_key: String::new(),
            auth_mode: None,
            bearer_token: None,
            bearer_token_previous: None,
            bearer_token_expires_at: None,
            bearer_token_previous_expires_at: None,
            db_path: PathBuf::from("dcc.db"),
            cors_origins: vec![
                "http://localhost:3000".to_string(),
                "http://localhost:5173".to_string(),
            ],
            lan: false,
        }
    }
}

impl HttpConfig {
    pub fn default_config_path() -> Option<PathBuf> {
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".dcc").join("http-config.json"))
    }

    /// Load configuration from environment variables and optional JSON file
    pub fn load() -> Result<Self, String> {
        Self::load_from(Self::default_config_path().as_deref())
    }

    pub fn load_from(config_path: Option<&Path>) -> Result<Self, String> {
        // JSON file first (lowest priority)
        let mut config = match config_path {
            Some(path) => Self::load_file_layer(path)?,
            None => {
                let mut config = Self::default();
                config.api_key = generate_api_key();
                eprintln!(
                    "[DCC HTTP] WARNING: no HOME to persist http-config.json; using a one-off API key."
                );
                config
            }
        };

        // Override with environment variables (highest priority)
        if let Ok(val) = std::env::var("DCC_HTTP_ENABLED") {
            config.enabled = val.to_lowercase() == "true" || val == "1";
        }

        if let Ok(val) = std::env::var("DCC_HTTP_HOST") {
            config.host = val;
        }

        if let Ok(val) = std::env::var("DCC_HTTP_PORT") {
            config.port = val.parse().map_err(|e| format!("Invalid port: {}", e))?;
        }

        if let Ok(val) = std::env::var("DCC_HTTP_API_KEY") {
            config.api_key = val;
        }

        if let Ok(val) = std::env::var("DCC_HTTP_AUTH_MODE") {
            config.auth_mode = match val.to_lowercase().as_str() {
                "local" => Some(HttpAuthMode::Local),
                "remote" => Some(HttpAuthMode::Remote),
                "mixed" | "both" => Some(HttpAuthMode::Mixed),
                other => return Err(format!("Invalid auth mode: {}", other)),
            };
        }

        if let Ok(val) = std::env::var("DCC_HTTP_BEARER_TOKEN") {
            config.bearer_token = Some(val);
        }

        if let Ok(val) = std::env::var("DCC_HTTP_BEARER_TOKEN_PREVIOUS") {
            config.bearer_token_previous = Some(val);
        }

        if let Ok(val) = std::env::var("DCC_HTTP_BEARER_TOKEN_EXPIRES_AT") {
            config.bearer_token_expires_at = Some(parse_rfc3339_timestamp(&val)?);
        }

        if let Ok(val) = std::env::var("DCC_HTTP_BEARER_TOKEN_PREVIOUS_EXPIRES_AT") {
            config.bearer_token_previous_expires_at = Some(parse_rfc3339_timestamp(&val)?);
        }

        if let Ok(val) = std::env::var("DCC_HTTP_DB_PATH") {
            config.db_path = PathBuf::from(val);
        }

        if let Ok(val) = std::env::var("DCC_HTTP_CORS_ORIGINS") {
            config.cors_origins = val
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        if let Ok(val) = std::env::var("DCC_HTTP_LAN") {
            config.lan = matches!(val.to_lowercase().as_str(), "1" | "true");
        }

        config.normalize_auth_mode();
        config.normalize_bearer_token_state()?;

        // Validation
        validate_api_key_strength(&config.api_key)?;

        if !config.db_path.exists() {
            eprintln!(
                "[DCC HTTP] WARNING: Database path does not exist: {:?}",
                config.db_path
            );
        }

        // An operator who asked for remote/mixed auth must provide the shared
        // bearer. The LAN opt-in only forces remote so that paired devices are
        // the sole credential accepted from the network; it needs no bearer.
        let remote_requested = matches!(
            config.auth_mode,
            Some(HttpAuthMode::Remote) | Some(HttpAuthMode::Mixed)
        );
        if remote_requested && !config.lan && config.bearer_token.is_none() {
            return Err(
                "Remote or mixed HTTP auth requires DCC_HTTP_BEARER_TOKEN or bearerToken in http-config.json"
                    .to_string(),
            );
        }

        Ok(config)
    }

    /// Reads the JSON file, giving it a random API key (persisted, 0600) when it
    /// has none or still carries the legacy default. A file that exists but does
    /// not parse is an error: overwriting it would drop whatever it configured.
    fn load_file_layer(path: &Path) -> Result<Self, String> {
        let mut config = if path.exists() {
            let content = fs::read_to_string(path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
            serde_json::from_str::<HttpConfig>(&content)
                .map_err(|e| format!("Invalid {}: {}", path.display(), e))?
        } else {
            Self::default()
        };

        if config.api_key.is_empty() || config.api_key == LEGACY_DEFAULT_API_KEY {
            config.api_key = generate_api_key();
            match config.save(path) {
                Ok(()) => println!("[DCC HTTP] Generated a new API key in {}", path.display()),
                Err(error) => eprintln!(
                    "[DCC HTTP] WARNING: could not persist the generated API key ({error}); using it for this run only."
                ),
            }
        }

        Ok(config)
    }

    /// True when some listener accepts connections from other machines.
    pub fn is_network_exposed(&self) -> bool {
        self.lan || !is_loopback_host(&self.host)
    }

    pub fn effective_auth_mode(&self) -> HttpAuthMode {
        let configured = self.auth_mode.unwrap_or_else(|| {
            if is_loopback_host(&self.host) {
                HttpAuthMode::Local
            } else {
                HttpAuthMode::Remote
            }
        });
        // The API key alone never guards a listener reachable from the network.
        if configured == HttpAuthMode::Local && self.is_network_exposed() {
            HttpAuthMode::Remote
        } else {
            configured
        }
    }

    pub fn bearer_token_is_active(&self) -> bool {
        self.bearer_token
            .as_ref()
            .zip(self.bearer_token_expires_at.as_ref())
            .map(|(token, expires_at)| !token.is_empty() && Utc::now() <= *expires_at)
            .unwrap_or(false)
    }

    pub fn bearer_token_previous_is_active(&self) -> bool {
        self.bearer_token_previous
            .as_ref()
            .zip(self.bearer_token_previous_expires_at.as_ref())
            .map(|(token, expires_at)| !token.is_empty() && Utc::now() <= *expires_at)
            .unwrap_or(false)
    }

    pub fn bearer_token_summary(&self) -> Option<BearerTokenRotation> {
        let token = self.bearer_token.clone()?;
        let expires_at = self.bearer_token_expires_at?;
        Some(BearerTokenRotation {
            token,
            expires_at,
            previous_expires_at: self.bearer_token_previous_expires_at,
        })
    }

    pub fn rotate_bearer_token(
        &mut self,
        ttl_seconds: Option<i64>,
        grace_seconds: Option<i64>,
    ) -> BearerTokenRotation {
        let ttl_seconds = ttl_seconds.unwrap_or(DEFAULT_BEARER_TOKEN_TTL_SECONDS);
        let grace_seconds = grace_seconds.unwrap_or(DEFAULT_BEARER_TOKEN_GRACE_SECONDS);
        let now = Utc::now();
        let previous_token = self.bearer_token.take();

        self.bearer_token_previous = previous_token;
        self.bearer_token_previous_expires_at = self
            .bearer_token_previous
            .as_ref()
            .map(|_| now + ChronoDuration::seconds(grace_seconds));

        let token = Uuid::new_v4().simple().to_string();
        let expires_at = now + ChronoDuration::seconds(ttl_seconds);
        self.bearer_token = Some(token.clone());
        self.bearer_token_expires_at = Some(expires_at);
        self.auth_mode = Some(match self.effective_auth_mode() {
            HttpAuthMode::Mixed => HttpAuthMode::Mixed,
            HttpAuthMode::Remote => HttpAuthMode::Remote,
            HttpAuthMode::Local => HttpAuthMode::Mixed,
        });

        BearerTokenRotation {
            token,
            expires_at,
            previous_expires_at: self.bearer_token_previous_expires_at,
        }
    }

    /// Save configuration to JSON file
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        write_private_file(path, json.as_bytes())
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        Ok(())
    }

    fn normalize_auth_mode(&mut self) {
        if self.auth_mode.is_none() {
            self.auth_mode = Some(if is_loopback_host(&self.host) {
                HttpAuthMode::Local
            } else {
                HttpAuthMode::Remote
            });
        }
    }

    fn normalize_bearer_token_state(&mut self) -> Result<(), String> {
        if self.bearer_token.is_some() && self.bearer_token_expires_at.is_none() {
            let ttl_seconds = std::env::var("DCC_HTTP_BEARER_TOKEN_TTL_SECONDS")
                .ok()
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(DEFAULT_BEARER_TOKEN_TTL_SECONDS);
            self.bearer_token_expires_at = Some(Utc::now() + ChronoDuration::seconds(ttl_seconds));
        }

        if self.bearer_token_previous.is_some() && self.bearer_token_previous_expires_at.is_none() {
            self.bearer_token_previous_expires_at =
                Some(Utc::now() + ChronoDuration::seconds(DEFAULT_BEARER_TOKEN_GRACE_SECONDS));
        }

        if let Some(expires_at) = self.bearer_token_expires_at {
            if expires_at <= Utc::now() {
                return Err("Bearer token expiration must be in the future".to_string());
            }
        }

        if let Some(expires_at) = self.bearer_token_previous_expires_at {
            if expires_at <= Utc::now() {
                return Err("Previous bearer token expiration must be in the future".to_string());
            }
        }

        Ok(())
    }
}

fn parse_rfc3339_timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| format!("Invalid RFC3339 timestamp '{}': {}", value, error))
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn generate_api_key() -> String {
    use rand::RngCore;
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn validate_api_key_strength(api_key: &str) -> Result<(), String> {
    if api_key == LEGACY_DEFAULT_API_KEY {
        return Err(format!(
            "Refusing to start with the public default API key '{LEGACY_DEFAULT_API_KEY}'. Unset DCC_HTTP_API_KEY to get a generated one, or set your own."
        ));
    }
    if api_key.trim().is_empty() {
        return Err("API key must not be empty".to_string());
    }
    Ok(())
}

/// The config holds the API key and bearer tokens: owner read/write only.
fn write_private_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        // `mode` only applies on create; tighten a file that already existed.
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(contents)
    }
    #[cfg(not(unix))]
    {
        fs::write(path, contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HttpConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 9876);
        assert!(config.enabled);
        assert_eq!(config.effective_auth_mode(), HttpAuthMode::Local);
        assert!(!config.is_network_exposed());
    }

    #[test]
    fn first_load_generates_and_persists_a_private_api_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".dcc").join("http-config.json");

        let first = HttpConfig::load_file_layer(&path).unwrap();
        assert_eq!(first.api_key.len(), 64);
        assert_ne!(first.api_key, LEGACY_DEFAULT_API_KEY);
        assert!(path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        let second = HttpConfig::load_file_layer(&path).unwrap();
        assert_eq!(second.api_key, first.api_key);
    }

    #[test]
    fn legacy_default_key_in_file_is_replaced_and_other_fields_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("http-config.json");
        fs::write(
            &path,
            r#"{"enabled":true,"host":"127.0.0.1","port":9999,"api_key":"dcc-dev-key-change-me"}"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }

        let config = HttpConfig::load_file_layer(&path).unwrap();
        assert_ne!(config.api_key, LEGACY_DEFAULT_API_KEY);
        assert_eq!(config.port, 9999);

        let saved: HttpConfig = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.api_key, config.api_key);
        assert_eq!(saved.port, 9999);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn unparsable_config_file_is_an_error_and_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("http-config.json");
        fs::write(&path, "{not json").unwrap();

        assert!(HttpConfig::load_file_layer(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{not json");
    }

    #[test]
    fn legacy_or_empty_api_key_is_refused() {
        assert!(validate_api_key_strength(LEGACY_DEFAULT_API_KEY).is_err());
        assert!(validate_api_key_strength("  ").is_err());
        assert!(validate_api_key_strength("a-real-key").is_ok());
    }

    #[test]
    fn lan_opt_in_forces_remote_auth() {
        let config = HttpConfig {
            auth_mode: Some(HttpAuthMode::Local),
            lan: true,
            ..HttpConfig::default()
        };
        assert!(config.is_network_exposed());
        assert_eq!(config.effective_auth_mode(), HttpAuthMode::Remote);
    }

    #[test]
    fn non_loopback_host_never_runs_in_local_mode() {
        let config = HttpConfig {
            host: "0.0.0.0".to_string(),
            auth_mode: Some(HttpAuthMode::Local),
            ..HttpConfig::default()
        };
        assert_eq!(config.effective_auth_mode(), HttpAuthMode::Remote);

        let mixed = HttpConfig {
            host: "0.0.0.0".to_string(),
            auth_mode: Some(HttpAuthMode::Mixed),
            ..HttpConfig::default()
        };
        assert_eq!(mixed.effective_auth_mode(), HttpAuthMode::Mixed);
    }

    #[test]
    fn lan_flag_is_never_persisted() {
        let config = HttpConfig {
            lan: true,
            api_key: "k".to_string(),
            ..HttpConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("\"lan\""));
    }

    #[test]
    fn test_load_from_env() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("DCC_HTTP_PORT", "8080");
        std::env::set_var("DCC_HTTP_API_KEY", "test-key");

        let config = HttpConfig::load_from(Some(&dir.path().join("http-config.json"))).unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(config.api_key, "test-key");

        std::env::remove_var("DCC_HTTP_PORT");
        std::env::remove_var("DCC_HTTP_API_KEY");
    }

    #[test]
    fn test_rotate_bearer_token() {
        let mut config = HttpConfig::default();
        config.auth_mode = Some(HttpAuthMode::Mixed);
        config.bearer_token = Some("old-token".to_string());
        config.bearer_token_expires_at = Some(Utc::now() + ChronoDuration::seconds(3600));

        let rotation = config.rotate_bearer_token(Some(7200), Some(600));

        assert_ne!(rotation.token, "old-token");
        assert_eq!(
            config.bearer_token.as_deref(),
            Some(rotation.token.as_str())
        );
        assert!(config.bearer_token_previous_is_active());
        assert_eq!(config.effective_auth_mode(), HttpAuthMode::Mixed);
        assert!(rotation.expires_at > Utc::now());
    }
}
