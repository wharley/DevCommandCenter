use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub api_key: String,
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
            db_path: PathBuf::from("dcc.db"),
            cors_origins: vec![
                "http://localhost:3000".to_string(),
                "http://localhost:5173".to_string(),
            ],
        }
    }
}

impl HttpConfig {
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

        // Validation
        if config.api_key == "dcc-dev-key-change-me" {
            eprintln!("[DCC HTTP] WARNING: Using default API key! Set DCC_HTTP_API_KEY for security.");
        }

        if !config.db_path.exists() {
            eprintln!("[DCC HTTP] WARNING: Database path does not exist: {:?}", config.db_path);
        }

        Ok(config)
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

        fs::write(path, json)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        Ok(())
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
}
