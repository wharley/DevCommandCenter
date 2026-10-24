//! What the toast says once a task is created (`notifyWorkspaceCreationResult`
//! and `workspace-setup-report.ts`): created, created with a setup warning,
//! or created with its setup failed, with the commands involved.

use dcc_core::domain::workspace::{WorkspaceSetupReport, WorkspaceSetupStatus};
use serde::Deserialize;

/// A setup command the project suggests (`WorkspaceSetupHint`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetupHint {
    pub label: String,
    pub command: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Success,
    Warning,
    Error,
}

/// The toast: its tone, the title key, and the description (a catalog key
/// with `{{commands}}`, or a step's own words).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreationNotice {
    pub tone: Tone,
    pub title_key: &'static str,
    pub description: Option<Description>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Description {
    Key { key: &'static str, commands: String },
    Text(String),
}

fn word_starts_with(word: &str, prefix: &str) -> bool {
    word.strip_prefix(prefix).is_some_and(|rest| {
        rest.chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric() && next != '_')
    })
}

/// `setupDisplayCommand`: `yarn install` out of `cd x && corepack yarn install --immutable`.
pub fn setup_display_command(command: &str) -> String {
    let words: Vec<&str> = command.split_whitespace().collect();
    for (index, pair) in words.windows(2).enumerate() {
        if matches!(pair[0], "pnpm" | "yarn" | "npm" | "bun")
            && word_starts_with(pair[1], "install")
        {
            let start = if index > 0 && words[index - 1] == "corepack" {
                index - 1
            } else {
                index
            };
            return format!("{} install", words[start..=index].join(" "));
        }
    }
    for pair in words.windows(2) {
        if pair[0] == "cargo" && word_starts_with(pair[1], "build") {
            return "cargo build".to_string();
        }
    }
    command.to_string()
}

fn commands(hints: &[SetupHint]) -> String {
    hints
        .iter()
        .map(|hint| setup_display_command(&hint.command))
        .collect::<Vec<_>>()
        .join(" • ")
}

/// `notifyWorkspaceCreationResult`; `clone` is its "clone" mode.
pub fn creation_notice(
    report: &WorkspaceSetupReport,
    hints: &[SetupHint],
    clone: bool,
) -> CreationNotice {
    let created = if clone {
        "workspaceDialog.toastCloneSuccess"
    } else {
        "workspaceDialog.toastCreateSuccess"
    };
    let commands = commands(hints);
    let problem = report
        .steps
        .iter()
        .find(|step| {
            matches!(
                step.status,
                WorkspaceSetupStatus::Warning | WorkspaceSetupStatus::Failed
            )
        })
        .and_then(|step| step.detail.clone())
        .or_else(|| report.message.clone());
    let key = |key: &'static str| {
        Some(Description::Key {
            key,
            commands: commands.clone(),
        })
    };
    match report.status {
        WorkspaceSetupStatus::Pending => CreationNotice {
            tone: Tone::Success,
            title_key: created,
            description: None,
        },
        WorkspaceSetupStatus::Completed => CreationNotice {
            tone: Tone::Success,
            title_key: created,
            description: key("workspaceDialog.toastSetupCompleted"),
        },
        WorkspaceSetupStatus::Warning => CreationNotice {
            tone: Tone::Warning,
            title_key: created,
            description: problem
                .map(Description::Text)
                .or_else(|| key("workspaceDialog.toastSetupWarning")),
        },
        WorkspaceSetupStatus::Failed => CreationNotice {
            tone: Tone::Error,
            title_key: "workspaceDialog.toastSetupFailedTitle",
            description: problem
                .map(Description::Text)
                .or_else(|| key("workspaceDialog.toastSetupFailed")),
        },
        WorkspaceSetupStatus::Skipped => CreationNotice {
            tone: Tone::Success,
            title_key: created,
            description: (!hints.is_empty()).then_some(Description::Key {
                key: "workspaceDialog.toastSetupSuggestions",
                commands,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::workspace::WorkspaceSetupStepReport;

    fn hint(command: &str) -> SetupHint {
        SetupHint {
            label: String::new(),
            command: command.into(),
        }
    }

    #[test]
    fn commands_read_like_the_desktop_shows_them() {
        assert_eq!(
            setup_display_command("cd web && corepack yarn install --immutable"),
            "corepack yarn install"
        );
        assert_eq!(setup_display_command("pnpm install"), "pnpm install");
        assert_eq!(setup_display_command("npm installer"), "npm installer");
        assert_eq!(
            setup_display_command("cargo build --release"),
            "cargo build"
        );
        assert_eq!(setup_display_command("make setup"), "make setup");
    }

    #[test]
    fn the_notice_follows_the_setup_outcome() {
        let report = |status: WorkspaceSetupStatus, detail: Option<&str>| WorkspaceSetupReport {
            status: status.clone(),
            steps: vec![WorkspaceSetupStepReport {
                label: "deps".into(),
                command: "yarn install".into(),
                source_path: String::new(),
                status,
                detail: detail.map(str::to_string),
            }],
            message: None,
        };
        let hints = [hint("yarn install"), hint("cargo build")];
        let pending = creation_notice(&report(WorkspaceSetupStatus::Pending, None), &hints, false);
        assert_eq!((pending.tone, pending.description), (Tone::Success, None));
        let failed = creation_notice(
            &report(WorkspaceSetupStatus::Failed, Some("exit 1")),
            &hints,
            false,
        );
        assert_eq!(failed.tone, Tone::Error);
        assert_eq!(failed.title_key, "workspaceDialog.toastSetupFailedTitle");
        assert_eq!(failed.description, Some(Description::Text("exit 1".into())));
        let warning = creation_notice(&report(WorkspaceSetupStatus::Warning, None), &hints, false);
        assert_eq!(
            warning.description,
            Some(Description::Key {
                key: "workspaceDialog.toastSetupWarning",
                commands: "yarn install • cargo build".into()
            })
        );
        let skipped = creation_notice(&report(WorkspaceSetupStatus::Skipped, None), &[], true);
        assert_eq!(skipped.description, None);
        assert_eq!(skipped.title_key, "workspaceDialog.toastCloneSuccess");
    }
}
