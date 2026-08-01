use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::repo_config::read_workspace_setup_command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSetupSuggestion {
    pub label: String,
    pub command: String,
    pub source_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspaceSetupKind {
    JavaScriptDeps,
    RustBuild,
}

struct DetectedSetupStep {
    kind: WorkspaceSetupKind,
    suggestion: WorkspaceSetupSuggestion,
}

pub fn detect_workspace_setup_suggestions(workspace_root: &str) -> Vec<WorkspaceSetupSuggestion> {
    let workspace_root = Path::new(workspace_root);
    if !workspace_root.exists() {
        return Vec::new();
    }

    let heuristic_steps = detect_heuristic_setup_steps(workspace_root);
    let Some(explicit_setup) = read_workspace_setup_command(workspace_root) else {
        return heuristic_steps
            .into_iter()
            .map(|step| step.suggestion)
            .collect();
    };

    let explicit_command = if workspace_root.join("package.json").is_file()
        && !explicit_script_selects_runtime(&explicit_setup.command)
    {
        terminal_setup_command(workspace_root, &explicit_setup.command)
    } else {
        explicit_setup.command.clone()
    };
    let mut suggestions = vec![WorkspaceSetupSuggestion {
        label: "Run repository setup script".to_string(),
        command: explicit_command,
        source_path: explicit_setup.source_path,
    }];

    if explicit_script_covers_javascript(&explicit_setup.command) {
        suggestions.extend(
            heuristic_steps
                .into_iter()
                .filter(|step| step.kind == WorkspaceSetupKind::RustBuild)
                .map(|step| step.suggestion),
        );
    } else if explicit_script_covers_rust(&explicit_setup.command) {
        suggestions.extend(
            heuristic_steps
                .into_iter()
                .filter(|step| step.kind == WorkspaceSetupKind::JavaScriptDeps)
                .map(|step| step.suggestion),
        );
    }

    suggestions
}

fn detect_heuristic_setup_steps(workspace_root: &Path) -> Vec<DetectedSetupStep> {
    let mut suggestions = Vec::new();
    let package_json = workspace_root.join("package.json");
    if package_json.is_file() {
        suggestions.push(DetectedSetupStep {
            kind: WorkspaceSetupKind::JavaScriptDeps,
            suggestion: WorkspaceSetupSuggestion {
                label: "Install JavaScript dependencies".to_string(),
                command: terminal_setup_command(
                    workspace_root,
                    &detect_package_manager_install_command(workspace_root),
                ),
                source_path: normalize_source_path(package_json),
            },
        });
    }

    let cargo_toml = workspace_root.join("Cargo.toml");
    if cargo_toml.is_file() {
        suggestions.push(DetectedSetupStep {
            kind: WorkspaceSetupKind::RustBuild,
            suggestion: WorkspaceSetupSuggestion {
                label: "Build Rust workspace".to_string(),
                command: "cargo build".to_string(),
                source_path: normalize_source_path(cargo_toml),
            },
        });
    }

    suggestions
}

fn detect_package_manager_install_command(workspace_root: &Path) -> String {
    if let Some(package_manager) = read_package_manager(workspace_root) {
        return match package_manager.as_str() {
            "pnpm" => "corepack pnpm install".to_string(),
            "yarn" => "corepack yarn install".to_string(),
            "npm" => "npm install".to_string(),
            "bun" => "bun install".to_string(),
            _ => "npm install".to_string(),
        };
    }
    if workspace_root.join("pnpm-lock.yaml").is_file() {
        "pnpm install".to_string()
    } else if workspace_root.join("yarn.lock").is_file() {
        "yarn install".to_string()
    } else if workspace_root.join("bun.lock").is_file()
        || workspace_root.join("bun.lockb").is_file()
    {
        "bun install".to_string()
    } else {
        "npm install".to_string()
    }
}

fn read_package_manager(workspace_root: &Path) -> Option<String> {
    let raw = fs::read_to_string(workspace_root.join("package.json")).ok()?;
    let parsed = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    parsed
        .get("packageManager")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.split('@').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

fn terminal_setup_command(workspace_root: &Path, install_command: &str) -> String {
    if cfg!(windows) {
        return install_command.to_string();
    }

    let has_nvmrc = workspace_root.join(".nvmrc").is_file();
    let has_node_version = workspace_root.join(".node-version").is_file();
    let has_tool_version_file = workspace_root.join(".tool-versions").is_file()
        || workspace_root.join("mise.toml").is_file()
        || workspace_root.join("mise.local.toml").is_file();

    if has_tool_version_file {
        return format!(
            "if command -v mise >/dev/null 2>&1; then mise exec -- {install_command}; elif command -v asdf >/dev/null 2>&1; then asdf exec {install_command}; else echo 'DCC: configure mise/asdf for this project before setup' >&2; false; fi"
        );
    }
    if has_nvmrc {
        return format!(
            "if command -v nvm >/dev/null 2>&1; then nvm use && {install_command}; elif command -v fnm >/dev/null 2>&1; then fnm use && {install_command}; elif command -v mise >/dev/null 2>&1; then mise exec -- {install_command}; elif command -v volta >/dev/null 2>&1; then {install_command}; else echo 'DCC: no compatible Node version manager is available' >&2; false; fi"
        );
    }
    if has_node_version {
        return format!(
            "if command -v fnm >/dev/null 2>&1; then fnm use && {install_command}; elif command -v mise >/dev/null 2>&1; then mise exec -- {install_command}; elif command -v asdf >/dev/null 2>&1; then asdf exec {install_command}; elif command -v nvm >/dev/null 2>&1; then nvm use \"$(cat .node-version)\" && {install_command}; elif command -v volta >/dev/null 2>&1; then {install_command}; else echo 'DCC: no compatible Node version manager is available' >&2; false; fi"
        );
    }
    install_command.to_string()
}

fn explicit_script_covers_javascript(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    ["npm", "pnpm", "yarn", "bun"]
        .iter()
        .any(|tool| command.contains(tool))
}

fn explicit_script_covers_rust(command: &str) -> bool {
    command.to_ascii_lowercase().contains("cargo")
}

fn explicit_script_selects_runtime(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    ["nvm use", "fnm use", "mise exec", "asdf exec", "volta run"]
        .iter()
        .any(|selector| command.contains(selector))
}

fn normalize_source_path(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use uuid::Uuid;

    use super::detect_workspace_setup_suggestions;

    #[test]
    fn prefers_lockfile_package_manager() {
        let root = temp_test_dir("setup-node");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(root.join("package.json"), "{}").expect("write package.json");
        fs::write(root.join("yarn.lock"), "").expect("write yarn.lock");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].command, "yarn install");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepares_node_version_manager_activation_for_terminal_setup() {
        let root = temp_test_dir("setup-node-version");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(root.join("package.json"), "{}").expect("write package.json");
        fs::write(root.join("pnpm-lock.yaml"), "").expect("write lockfile");
        fs::write(root.join(".nvmrc"), "22\n").expect("write nvmrc");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].command.contains("nvm use"));
        assert!(suggestions[0].command.contains("fnm use"));
        assert!(suggestions[0].command.contains("pnpm install"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn passes_node_version_file_value_to_nvm() {
        let root = temp_test_dir("setup-node-version-file");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(root.join("package.json"), "{}").expect("write package.json");
        fs::write(root.join(".node-version"), "22.18.0\n").expect("write node version");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0]
            .command
            .contains("nvm use \"$(cat .node-version)\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn activates_project_node_version_before_explicit_setup_script() {
        let root = temp_test_dir("setup-explicit-node-version");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(root.join("package.json"), "{}").expect("write package.json");
        fs::write(root.join(".nvmrc"), "22\n").expect("write nvmrc");
        fs::write(
            root.join(".dcc.toml"),
            "[scripts]\nsetup = \"pnpm bootstrap\"\n",
        )
        .expect("write repo config");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].command.contains("nvm use"));
        assert!(suggestions[0].command.contains("pnpm bootstrap"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn respects_package_manager_field_via_corepack() {
        let root = temp_test_dir("setup-package-manager");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join("package.json"),
            r#"{"packageManager":"pnpm@10.15.0"}"#,
        )
        .expect("write package.json");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].command, "corepack pnpm install");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn includes_rust_build() {
        let root = temp_test_dir("setup-rust");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("write cargo");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].command, "cargo build");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_repo_script_and_adds_complementary_rust_build() {
        let root = temp_test_dir("setup-repo-config-js");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "[scripts]\nsetup = \"pnpm bootstrap\"\n",
        )
        .expect("write repo config");
        fs::write(root.join("package.json"), "{}").expect("write package.json");
        fs::write(root.join("pnpm-lock.yaml"), "").expect("write lockfile");
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("write cargo");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].command, "pnpm bootstrap");
        assert_eq!(suggestions[1].command, "cargo build");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_repo_script_and_adds_complementary_js_install() {
        let root = temp_test_dir("setup-repo-config-rust");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(root.join(".dcc.toml"), "setup_command = \"cargo check\"\n")
            .expect("write repo config");
        fs::write(root.join("package.json"), "{}").expect("write package.json");
        fs::write(root.join("pnpm-lock.yaml"), "").expect("write lockfile");
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("write cargo");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].command, "cargo check");
        assert_eq!(suggestions[1].command, "pnpm install");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_only_explicit_script_when_coverage_is_unknown() {
        let root = temp_test_dir("setup-repo-config-unknown");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "[scripts]\nsetup = \"just setup\"\n",
        )
        .expect("write repo config");
        fs::write(root.join("package.json"), "{}").expect("write package.json");
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("write cargo");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].command, "just setup");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn falls_back_when_repo_config_is_invalid() {
        let root = temp_test_dir("setup-invalid-repo-config");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "[scripts\nsetup = \"pnpm bootstrap\"\n",
        )
        .expect("write invalid repo config");
        fs::write(root.join("package.json"), "{}").expect("write package.json");

        let suggestions =
            detect_workspace_setup_suggestions(root.to_str().expect("temp path should be utf-8"));

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].command, "npm install");

        let _ = fs::remove_dir_all(root);
    }

    fn temp_test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dcc-workspace-setup-tests-{label}-{}",
            Uuid::new_v4()
        ))
    }
}
