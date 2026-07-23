use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const REPO_CONFIG_FILENAME: &str = ".dcc.toml";
pub const DEFAULT_TASK_TIMEOUT_SECONDS: u64 = 600;
const MAX_TASKS: usize = 50;
const MAX_HOOK_TASKS: usize = 20;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoTaskKind {
    Check,
    Fix,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoAutomationTask {
    pub id: String,
    pub label: Option<String>,
    pub command: String,
    pub kind: RepoTaskKind,
    pub cwd: Option<String>,
    pub timeout_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoAutomationConfig {
    pub setup_command: Option<String>,
    pub tasks: Vec<RepoAutomationTask>,
    pub before_merge: Vec<String>,
    pub before_push: Vec<String>,
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
    let automation = read_workspace_automation_config(workspace_root)?;
    let Some(automation) = automation else {
        return Ok(None);
    };
    let commands = resolve_hook_commands(&automation, &automation.before_merge)?;
    if commands.is_empty() {
        return Ok(None);
    }
    Ok(Some(RepoValidationConfig {
        commands,
        source_path: automation.source_path,
    }))
}

pub fn read_workspace_automation_config(
    workspace_root: &Path,
) -> Result<Option<RepoAutomationConfig>, String> {
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
    let legacy_value = parsed
        .get("scripts")
        .and_then(|scripts| scripts.get("validate"))
        .or_else(|| parsed.get("validation_commands"));
    let raw_commands = if let Some(value) = legacy_value {
        if let Some(command) = value.as_str() {
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
        }
    } else {
        Vec::new()
    };

    let commands: Vec<String> = raw_commands
        .into_iter()
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .map(str::to_string)
        .collect();
    if commands.len() > 20 {
        return Err("`.dcc.toml` scripts.validate supports at most 20 commands".to_string());
    }
    if commands.iter().any(|command| command.len() > 4096) {
        return Err(
            "`.dcc.toml` scripts.validate commands must be at most 4096 characters".to_string(),
        );
    }

    let mut tasks = BTreeMap::<String, RepoAutomationTask>::new();
    for (index, command) in commands.into_iter().enumerate() {
        let id = format!("legacy_validate_{}", index + 1);
        tasks.insert(
            id.clone(),
            RepoAutomationTask {
                id,
                label: None,
                command,
                kind: RepoTaskKind::Check,
                cwd: None,
                timeout_seconds: DEFAULT_TASK_TIMEOUT_SECONDS,
            },
        );
    }

    if let Some(task_table) = parsed.get("tasks").and_then(toml::Value::as_table) {
        if task_table.len() + tasks.len() > MAX_TASKS {
            return Err(format!("`.dcc.toml` supports at most {MAX_TASKS} tasks"));
        }
        for (id, value) in task_table {
            validate_task_id(id)?;
            let table = value
                .as_table()
                .ok_or_else(|| format!("`.dcc.toml` task `{id}` must be a table"))?;
            let command = table
                .get("command")
                .and_then(toml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("`.dcc.toml` task `{id}` requires command"))?;
            if command.len() > 4096 {
                return Err(format!("`.dcc.toml` task `{id}` command is too long"));
            }
            let kind = match table.get("kind").and_then(toml::Value::as_str) {
                None | Some("check") => RepoTaskKind::Check,
                Some("fix") => RepoTaskKind::Fix,
                Some(other) => {
                    return Err(format!(
                        "`.dcc.toml` task `{id}` kind must be `check` or `fix`, got `{other}`"
                    ))
                }
            };
            let cwd = table
                .get("cwd")
                .and_then(toml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            if let Some(cwd) = cwd.as_deref() {
                validate_task_cwd(id, cwd)?;
            }
            let timeout_seconds = table
                .get("timeout_seconds")
                .and_then(toml::Value::as_integer)
                .map(|value| value as u64)
                .unwrap_or(DEFAULT_TASK_TIMEOUT_SECONDS);
            if !(1..=3600).contains(&timeout_seconds) {
                return Err(format!(
                    "`.dcc.toml` task `{id}` timeout_seconds must be between 1 and 3600"
                ));
            }
            let label = table
                .get("label")
                .and_then(toml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            tasks.insert(
                id.clone(),
                RepoAutomationTask {
                    id: id.clone(),
                    label,
                    command: command.to_string(),
                    kind,
                    cwd,
                    timeout_seconds,
                },
            );
        }
    }

    let hooks = parsed.get("hooks").and_then(toml::Value::as_table);
    let has_explicit_before_merge = hooks.and_then(|value| value.get("before_merge")).is_some();
    let mut before_merge = parse_hook_ids(hooks, "before_merge")?;
    let before_push = parse_hook_ids(hooks, "before_push")?;
    if !has_explicit_before_merge {
        before_merge.extend(
            tasks
                .keys()
                .filter(|id| id.starts_with("legacy_validate_"))
                .cloned(),
        );
    }
    validate_hook(&tasks, "before_merge", &before_merge)?;
    validate_hook(&tasks, "before_push", &before_push)?;

    let setup_command = parsed
        .get("scripts")
        .and_then(|scripts| scripts.get("setup"))
        .and_then(toml::Value::as_str)
        .or_else(|| parsed.get("setup_command").and_then(toml::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    Ok(Some(RepoAutomationConfig {
        setup_command,
        tasks: tasks.into_values().collect(),
        before_merge,
        before_push,
        source_path: normalize_source_path(config_path),
    }))
}

pub fn resolve_hook_commands(
    config: &RepoAutomationConfig,
    task_ids: &[String],
) -> Result<Vec<String>, String> {
    let tasks = config
        .tasks
        .iter()
        .map(|task| (task.id.as_str(), task))
        .collect::<BTreeMap<_, _>>();
    task_ids
        .iter()
        .map(|id| {
            tasks
                .get(id.as_str())
                .map(|task| task.command.clone())
                .ok_or_else(|| format!("unknown automation task `{id}`"))
        })
        .collect()
}

pub fn validate_workspace_automation_config(config: &RepoAutomationConfig) -> Result<(), String> {
    if config.tasks.len() > MAX_TASKS {
        return Err(format!("`.dcc.toml` supports at most {MAX_TASKS} tasks"));
    }
    let mut tasks = BTreeMap::new();
    for task in &config.tasks {
        validate_task_id(&task.id)?;
        if task.command.trim().is_empty() || task.command.len() > 4096 {
            return Err(format!(
                "`.dcc.toml` task `{}` requires a command of at most 4096 characters",
                task.id
            ));
        }
        validate_workspace_task_command(&task.command)
            .map_err(|error| format!("`.dcc.toml` task `{}` {error}", task.id))?;
        if !(1..=3600).contains(&task.timeout_seconds) {
            return Err(format!(
                "`.dcc.toml` task `{}` timeout_seconds must be between 1 and 3600",
                task.id
            ));
        }
        if let Some(cwd) = task.cwd.as_deref() {
            validate_task_cwd(&task.id, cwd)?;
        }
        if tasks.insert(task.id.clone(), task.clone()).is_some() {
            return Err(format!("duplicate automation task `{}`", task.id));
        }
    }
    if config.before_merge.len() > MAX_HOOK_TASKS || config.before_push.len() > MAX_HOOK_TASKS {
        return Err(format!(
            "automation hooks support at most {MAX_HOOK_TASKS} tasks"
        ));
    }
    validate_hook(&tasks, "before_merge", &config.before_merge)?;
    validate_hook(&tasks, "before_push", &config.before_push)?;
    Ok(())
}

fn parse_hook_ids(hooks: Option<&toml::value::Table>, name: &str) -> Result<Vec<String>, String> {
    let Some(value) = hooks.and_then(|hooks| hooks.get(name)) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("`.dcc.toml` hooks.{name} must be an array of task ids"))?;
    if values.len() > MAX_HOOK_TASKS {
        return Err(format!(
            "`.dcc.toml` hooks.{name} supports at most {MAX_HOOK_TASKS} tasks"
        ));
    }
    let mut unique = BTreeSet::new();
    let mut result = Vec::new();
    for value in values {
        let id = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("`.dcc.toml` hooks.{name} must contain task ids"))?;
        if unique.insert(id.to_string()) {
            result.push(id.to_string());
        }
    }
    Ok(result)
}

fn validate_hook(
    tasks: &BTreeMap<String, RepoAutomationTask>,
    name: &str,
    ids: &[String],
) -> Result<(), String> {
    for id in ids {
        let task = tasks
            .get(id)
            .ok_or_else(|| format!("`.dcc.toml` hooks.{name} references unknown task `{id}`"))?;
        if task.kind == RepoTaskKind::Fix {
            return Err(format!(
                "`.dcc.toml` hooks.{name} cannot run fix task `{id}` automatically"
            ));
        }
    }
    Ok(())
}

fn validate_task_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
    {
        return Err(format!(
            "`.dcc.toml` task id `{id}` must use only letters, numbers, `_` or `-`"
        ));
    }
    Ok(())
}

pub fn validate_workspace_task_command(command: &str) -> Result<(), String> {
    let Some(flag) = command
        .split_whitespace()
        .find(|part| part.starts_with(is_typographic_dash))
    else {
        return Ok(());
    };
    Err(format!(
        "contains a typographic dash in flag `{flag}`. Use ASCII hyphens for command flags (for example `--fix`)"
    ))
}

fn is_typographic_dash(value: char) -> bool {
    matches!(
        value,
        '\u{058A}'
            | '\u{05BE}'
            | '\u{1400}'
            | '\u{1806}'
            | '\u{2010}'
            | '\u{2011}'
            | '\u{2012}'
            | '\u{2013}'
            | '\u{2014}'
            | '\u{2015}'
            | '\u{2212}'
            | '\u{2E17}'
            | '\u{2E1A}'
            | '\u{2E3A}'
            | '\u{2E3B}'
            | '\u{2E40}'
            | '\u{301C}'
            | '\u{3030}'
            | '\u{30A0}'
            | '\u{FE31}'
            | '\u{FE32}'
            | '\u{FE58}'
            | '\u{FE63}'
            | '\u{FF0D}'
    )
}

fn validate_task_cwd(id: &str, cwd: &str) -> Result<(), String> {
    let path = Path::new(cwd);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "`.dcc.toml` task `{id}` cwd must stay inside the workspace"
        ));
    }
    Ok(())
}

fn normalize_source_path(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use uuid::Uuid;

    use super::{
        read_workspace_automation_config, read_workspace_setup_command,
        read_workspace_validation_config, validate_workspace_automation_config,
        RepoAutomationConfig, RepoAutomationTask, RepoTaskKind,
    };

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

    #[test]
    fn reads_project_tasks_hooks_and_setup() {
        let root = temp_test_dir("repo-config-automation");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            r#"
[scripts]
setup = "yarn install"

[tasks.lint]
label = "Lint"
command = "yarn lint"
kind = "check"
cwd = "apps/web"
timeout_seconds = 90

[tasks.lint_fix]
command = "yarn lint --fix"
kind = "fix"

[hooks]
before_merge = ["lint"]
before_push = ["lint"]
"#,
        )
        .expect("write config");

        let automation = read_workspace_automation_config(&root)
            .expect("valid config")
            .expect("automation config");
        assert_eq!(automation.setup_command.as_deref(), Some("yarn install"));
        assert_eq!(automation.before_merge, ["lint"]);
        assert_eq!(automation.before_push, ["lint"]);
        assert_eq!(automation.tasks.len(), 2);
        let lint = automation
            .tasks
            .iter()
            .find(|task| task.id == "lint")
            .expect("lint task");
        assert_eq!(lint.kind, RepoTaskKind::Check);
        assert_eq!(lint.cwd.as_deref(), Some("apps/web"));
        assert_eq!(lint.timeout_seconds, 90);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_legacy_validation_as_before_merge_check() {
        let root = temp_test_dir("repo-config-legacy-automation");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "[scripts]\nvalidate = [\"yarn lint\", \"yarn test\"]\n",
        )
        .expect("write config");

        let automation = read_workspace_automation_config(&root)
            .expect("valid config")
            .expect("automation config");
        assert_eq!(
            automation.before_merge,
            ["legacy_validate_1", "legacy_validate_2"]
        );
        assert!(automation.before_push.is_empty());
        assert!(automation
            .tasks
            .iter()
            .all(|task| task.kind == RepoTaskKind::Check));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_fix_tasks_in_automatic_hooks() {
        let root = temp_test_dir("repo-config-fix-hook");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "[tasks.fix]\ncommand = \"yarn lint --fix\"\nkind = \"fix\"\n\n[hooks]\nbefore_push = [\"fix\"]\n",
        )
        .expect("write config");

        let error = read_workspace_automation_config(&root).expect_err("invalid config");
        assert!(error.contains("cannot run fix task"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_typographic_dashes_in_task_flags() {
        let config = RepoAutomationConfig {
            setup_command: None,
            tasks: vec![RepoAutomationTask {
                id: "lint_fix".to_string(),
                label: None,
                command: "yarn lint —fix".to_string(),
                kind: RepoTaskKind::Fix,
                cwd: None,
                timeout_seconds: 60,
            }],
            before_merge: Vec::new(),
            before_push: Vec::new(),
            source_path: ".dcc.toml".to_string(),
        };

        let error = validate_workspace_automation_config(&config).expect_err("invalid command");
        assert!(error.contains("typographic dash"));
        assert!(error.contains("--fix"));
    }

    #[test]
    fn rejects_task_working_directory_outside_workspace() {
        let root = temp_test_dir("repo-config-task-cwd");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(
            root.join(".dcc.toml"),
            "[tasks.lint]\ncommand = \"yarn lint\"\ncwd = \"../other\"\n",
        )
        .expect("write config");

        let error = read_workspace_automation_config(&root).expect_err("invalid config");
        assert!(error.contains("inside the workspace"));

        let _ = fs::remove_dir_all(root);
    }

    fn temp_test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dcc-repo-config-tests-{label}-{}", Uuid::new_v4()))
    }
}
