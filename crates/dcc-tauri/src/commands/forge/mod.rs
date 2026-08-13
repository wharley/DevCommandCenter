pub(crate) mod accounts;
pub(crate) mod context;
pub(crate) mod detect;
pub(crate) mod github;
pub(crate) mod gitlab;
pub(crate) mod provider;
pub(crate) mod remote;

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use dcc_infra::process::run_command_with_timeout;

pub(crate) const FORGE_CLI_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn run_forge_cli_command<F>(
    program: &std::path::Path,
    configure: F,
) -> Result<std::process::Output, String>
where
    F: FnOnce(&mut Command),
{
    run_command_with_timeout(program, configure, FORGE_CLI_COMMAND_TIMEOUT)
}

pub(crate) fn resolve_cli_binary(program: &str) -> Result<PathBuf, String> {
    let candidates = [
        PathBuf::from(program),
        PathBuf::from(format!("/opt/homebrew/bin/{program}")),
        PathBuf::from(format!("/usr/local/bin/{program}")),
    ];

    for candidate in candidates {
        if candidate.as_os_str() == program {
            if run_forge_cli_command(&candidate, |command| {
                command.arg("--version");
            })
            .is_ok()
            {
                return Ok(candidate);
            }
            continue;
        }

        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let provider_name = match program {
        "gh" => "GitHub CLI",
        "glab" => "GitLab CLI",
        _ => "CLI",
    };

    Err(format!(
        "{provider_name} (`{program}`) is not available to the app. Install it or launch DCC from a shell where `{program}` is on PATH."
    ))
}
