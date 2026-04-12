use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

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
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 9876,
            api_key: "dcc-dev-key-change-me".to_string(),
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
        let mut config = Self::default();

        // Try to load from JSON file first (lowest priority)
        if let Ok(home) = std::env::var("HOME") {
            let config_path = PathBuf::from(home).join(".dcc").join("http-config.json");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(file_config) = serde_json::from_str::<HttpConfig>(&content) {
                        config = file_config;
                    }
                }
            }
        }

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

        config.normalize_auth_mode();
        config.normalize_bearer_token_state()?;

        // Validation
        if config.api_key == "dcc-dev-key-change-me" {
            eprintln!(
                "[DCC HTTP] WARNING: Using default API key! Set DCC_HTTP_API_KEY for security."
            );
        }

        if !config.db_path.exists() {
            eprintln!(
                "[DCC HTTP] WARNING: Database path does not exist: {:?}",
                config.db_path
            );
        }

        if config.effective_auth_mode() != HttpAuthMode::Local && config.bearer_token.is_none() {
            return Err(
                "Remote or mixed HTTP auth requires DCC_HTTP_BEARER_TOKEN or bearerToken in http-config.json"
                    .to_string(),
            );
        }

        Ok(config)
    }

    pub fn effective_auth_mode(&self) -> HttpAuthMode {
        self.auth_mode.unwrap_or_else(|| {
            if is_loopback_host(&self.host) {
                HttpAuthMode::Local
            } else {
                HttpAuthMode::Remote
            }
        })
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
    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        fs::write(path, json).map_err(|e| format!("Failed to write config file: {}", e))?;

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
    }

    #[test]
    fn test_load_from_env() {
        std::env::set_var("DCC_HTTP_PORT", "8080");
        std::env::set_var("DCC_HTTP_API_KEY", "test-key");

        let config = HttpConfig::load().unwrap();
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
