use std::{
    fs,
    path::{Path, PathBuf},
};

const REPO_CONFIG_FILENAME: &str = ".dcc.toml";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoSetupCommand {
    pub command: String,
    pub source_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoValidationConfig {
    pub commands: Vec<String>,
    pub source_path: String,
}

pub fn read_workspace_setup_command(workspace_root: &Path) -> Option<RepoSetupCommand> {
    let config_path = workspace_root.join(REPO_CONFIG_FILENAME);
    let raw = fs::read_to_string(&config_path).ok()?;
    if raw.trim().is_empty() {
        return None;
    }

    let parsed: toml::Value = toml::from_str(&raw).ok()?;
    let command = parsed
        .get("scripts")
        .and_then(|scripts| scripts.get("setup"))
        .and_then(toml::Value::as_str)
        .or_else(|| parsed.get("setup_command").and_then(toml::Value::as_str))
        .map(str::trim)
        .filter(|command| !command.is_empty())?;

    Some(RepoSetupCommand {
        command: command.to_string(),
        source_path: normalize_source_path(config_path),
    })
}

pub fn read_workspace_validation_config(
    workspace_root: &Path,
) -> Result<Option<RepoValidationConfig>, String> {
    let config_path = workspace_root.join(REPO_CONFIG_FILENAME);
    let raw = match fs::read_to_string(&config_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read .dcc.toml: {error}")),
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }

    let parsed: toml::Value =
        toml::from_str(&raw).map_err(|error| format!("invalid .dcc.toml: {error}"))?;
    let value = parsed
        .get("scripts")
        .and_then(|scripts| scripts.get("validate"))
        .or_else(|| parsed.get("validation_commands"));
    let Some(value) = value else {
        return Ok(None);
    };

    let raw_commands = if let Some(command) = value.as_str() {
        vec![command]
    } else if let Some(commands) = value.as_array() {
        let mut values = Vec::with_capacity(commands.len());
        for command in commands {
            let command = command.as_str().ok_or_else(|| {
                "`.dcc.toml` scripts.validate must contain only strings".to_string()
            })?;
            values.push(command);
        }
        values
    } else {
        return Err(
            "`.dcc.toml` scripts.validate must be a string or an array of strings".to_string(),
        );
    };

    let commands: Vec<String> = raw_commands
        .into_iter()
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .map(str::to_string)
        .collect();
    if commands.is_empty() {
        return Ok(None);
    }
    if commands.len() > 20 {
        return Err("`.dcc.toml` scripts.validate supports at most 20 commands".to_string());
    }
    if commands.iter().any(|command| command.len() > 4096) {
        return Err(
            "`.dcc.toml` scripts.validate commands must be at most 4096 characters".to_string(),
        );
    }

    Ok(Some(RepoValidationConfig {
        commands,
        source_path: normalize_source_path(config_path),
    }))
}

fn normalize_source_path(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use uuid::Uuid;

    use super::{read_workspace_setup_command, read_workspace_validation_config};

    #[test]
    fn reads_scripts_setup_from_repo_config() {
        let root = temp_test_dir("repo-config-scripts");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "[scripts]\nsetup = \"pnpm bootstrap\"\n",
        )
        .expect("write config");

        let setup = read_workspace_setup_command(&root).expect("setup command");

        assert_eq!(setup.command, "pnpm bootstrap");
        assert!(setup.source_path.ends_with(".dcc.toml"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_flat_setup_command_from_repo_config() {
        let root = temp_test_dir("repo-config-flat");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "setup_command = \"cargo xtask setup\"\n",
        )
        .expect("write config");

        let setup = read_workspace_setup_command(&root).expect("setup command");

        assert_eq!(setup.command, "cargo xtask setup");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_one_or_many_validation_commands() {
        let root = temp_test_dir("repo-config-validations");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "[scripts]\nvalidate = [\"yarn lint\", \"yarn typecheck\"]\n",
        )
        .expect("write config");

        let validation = read_workspace_validation_config(&root)
            .expect("valid config")
            .expect("validation config");
        assert_eq!(validation.commands, ["yarn lint", "yarn typecheck"]);

        fs::write(
            root.join(".dcc.toml"),
            "[scripts]\nvalidate = \"cargo test\"\n",
        )
        .expect("write config");
        let validation = read_workspace_validation_config(&root)
            .expect("valid config")
            .expect("validation config");
        assert_eq!(validation.commands, ["cargo test"]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_invalid_validation_command_types() {
        let root = temp_test_dir("repo-config-invalid-validation");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "[scripts]\nvalidate = [\"yarn lint\", 42]\n",
        )
        .expect("write config");

        let error = read_workspace_validation_config(&root).expect_err("invalid config");
        assert!(error.contains("only strings"));

        let _ = fs::remove_dir_all(root);
    }

    fn temp_test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dcc-repo-config-tests-{label}-{}", Uuid::new_v4()))
    }
}
