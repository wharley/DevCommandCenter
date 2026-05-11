pub(crate) mod accounts;
pub(crate) mod context;
pub(crate) mod detect;
pub(crate) mod github;
pub(crate) mod gitlab;
pub(crate) mod provider;
pub(crate) mod remote;

use std::path::PathBuf;
use std::process::Command;

pub(crate) fn resolve_cli_binary(program: &str) -> Result<PathBuf, String> {
    let candidates = [
        PathBuf::from(program),
        PathBuf::from(format!("/opt/homebrew/bin/{program}")),
        PathBuf::from(format!("/usr/local/bin/{program}")),
    ];

    for candidate in candidates {
        if candidate.as_os_str() == program {
            if Command::new(&candidate).arg("--version").output().is_ok() {
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
