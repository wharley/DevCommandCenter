use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use dcc_core::domain::provider::ProviderModelDescriptor;
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u8 = 1;
const MAX_STATE_BYTES: u64 = 1024 * 1024;
const MAX_MODELS: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AntigravityAccountState {
    schema_version: u8,
    pub(crate) profile_path: PathBuf,
    pub(crate) verified_at: String,
    pub(crate) models: Vec<ProviderModelDescriptor>,
}

pub(crate) fn save(
    app_data_dir: &Path,
    profile_path: &Path,
    models: Vec<ProviderModelDescriptor>,
) -> Result<AntigravityAccountState, String> {
    let profile_path = absolute_profile_path(profile_path)?;
    let state = AntigravityAccountState {
        schema_version: SCHEMA_VERSION,
        profile_path,
        verified_at: chrono::Utc::now().to_rfc3339(),
        models: sanitize_models(models),
    };
    let bytes = serde_json::to_vec_pretty(&state)
        .map_err(|error| format!("could not serialize Antigravity account state: {error}"))?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err("Antigravity account state is unexpectedly large".into());
    }

    let directory = state_directory(app_data_dir);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("could not prepare Antigravity account state: {error}"))?;
    set_private_directory(&directory)?;

    let target = state_path(app_data_dir);
    let temporary = directory.join(format!("account-state.{}.tmp", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("could not create Antigravity account state: {error}"))?;
    set_private_file(&file)?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "could not persist Antigravity account state: {error}"
        ));
    }
    drop(file);
    if let Err(error) = atomic_replace(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "could not activate Antigravity account state: {error}"
        ));
    }
    Ok(state)
}

pub(crate) fn load(app_data_dir: &Path) -> Option<AntigravityAccountState> {
    let path = state_path(app_data_dir);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_STATE_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .ok()?
        .take(MAX_STATE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return None;
    }
    let mut state: AntigravityAccountState = serde_json::from_slice(&bytes).ok()?;
    if state.schema_version != SCHEMA_VERSION || !state.profile_path.is_absolute() {
        return None;
    }
    state.models = sanitize_models(state.models);
    Some(state)
}

pub(crate) fn has_saved_login(state: &AntigravityAccountState, profile_path: &Path) -> bool {
    absolute_profile_path(profile_path).ok().as_deref() == Some(state.profile_path.as_path())
        && profile_has_login(&state.profile_path)
}

pub(crate) fn profile_has_login(profile_path: &Path) -> bool {
    valid_token_file(&profile_path.join("antigravity-acp/acp_token.json"))
}

fn sanitize_models(models: Vec<ProviderModelDescriptor>) -> Vec<ProviderModelDescriptor> {
    let mut seen = HashSet::new();
    models
        .into_iter()
        .filter_map(|mut model| {
            model.id = model.id.trim().to_string();
            model.label = model.label.trim().to_string();
            model.description = model.description.trim().to_string();
            if model.id.is_empty()
                || model.id.len() > 256
                || model.label.len() > 512
                || model.description.len() > 4096
                || model.id.chars().any(char::is_control)
                || !seen.insert(model.id.clone())
            {
                return None;
            }
            model.effort_levels.truncate(16);
            Some(model)
        })
        .take(MAX_MODELS)
        .collect()
}

fn valid_token_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_file() && metadata.len() > 0)
}

fn absolute_profile_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Antigravity profile path must be absolute".into());
    }
    Ok(path.to_path_buf())
}

fn state_directory(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("provider-state").join("antigravity")
}

fn state_path(app_data_dir: &Path) -> PathBuf {
    state_directory(app_data_dir).join("account-state.json")
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("could not protect Antigravity account state: {error}"))
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(file: &fs::File) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("could not protect Antigravity account state: {error}"))
}

#[cfg(not(unix))]
fn set_private_file(_file: &fs::File) -> Result<(), String> {
    Ok(())
}

fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::{iter, os::windows::ffi::OsStrExt};
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = source
            .as_os_str()
            .encode_wide()
            .chain(iter::once(0))
            .collect();
        let destination: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(iter::once(0))
            .collect();
        let result = unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> ProviderModelDescriptor {
        ProviderModelDescriptor {
            id: id.to_string(),
            label: id.to_string(),
            description: String::new(),
            recommended: false,
            effort_levels: Vec::new(),
        }
    }

    #[test]
    fn saved_models_and_login_survive_a_process_style_reload() {
        let root = tempfile::tempdir().expect("state root");
        let profile = root.path().join("profile");
        fs::create_dir_all(profile.join("antigravity-acp")).expect("profile");
        fs::write(
            profile.join("antigravity-acp/acp_token.json"),
            b"{\"token\":\"secret-owned-by-runtime\"}",
        )
        .expect("token");

        save(root.path(), &profile, vec![model("gemini-test")]).expect("save state");
        let restored = load(root.path()).expect("reload state");

        assert!(has_saved_login(&restored, &profile));
        assert_eq!(restored.models[0].id, "gemini-test");
        let persisted = fs::read_to_string(state_path(root.path())).expect("persisted state");
        assert!(!persisted.contains("secret-owned-by-runtime"));
    }

    #[test]
    fn login_is_not_claimed_without_the_runtime_token() {
        let root = tempfile::tempdir().expect("state root");
        let profile = root.path().join("profile");
        fs::create_dir_all(&profile).expect("profile");
        let saved = save(root.path(), &profile, vec![model("gemini-test")]).expect("save state");

        assert!(!has_saved_login(&saved, &profile));
    }
}
