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

fn normalize_source_path(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use uuid::Uuid;

    use super::read_workspace_setup_command;

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

    fn temp_test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dcc-repo-config-tests-{label}-{}", Uuid::new_v4()))
    }
}
