//! The global usage-limit preference (`usage-limit-settings.json` in the app
//! data folder), read and written by the desktop and read by the native app.

use std::path::Path;

use dcc_core::domain::usage_limit::UsageLimitSettings;

pub const SETTINGS_FILE_NAME: &str = "usage-limit-settings.json";

pub fn load_settings(app_data_dir: &Path) -> UsageLimitSettings {
    std::fs::read(app_data_dir.join(SETTINGS_FILE_NAME))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UsageLimitSettings>(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(app_data_dir: &Path, settings: UsageLimitSettings) -> Result<(), String> {
    std::fs::create_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    let path = app_data_dir.join(SETTINGS_FILE_NAME);
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults_off() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!load_settings(dir.path()).auto_resume);
        save_settings(dir.path(), UsageLimitSettings { auto_resume: true }).unwrap();
        assert!(load_settings(dir.path()).auto_resume);
    }
}
