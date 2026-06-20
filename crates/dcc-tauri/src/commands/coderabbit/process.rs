use std::{
    ffi::{OsStr, OsString},
    path::Path,
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

pub(crate) const CODERABBIT_DEFAULT_TIMEOUT: Duration = Duration::from_secs(40 * 60);
pub(crate) const CODERABBIT_STATUS_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const CODERABBIT_DOCTOR_TIMEOUT: Duration = Duration::from_secs(120);

const CODERABBIT_CLI_CANDIDATES: [&str; 2] = ["cr", "coderabbit"];

#[derive(Clone, Debug)]
pub(crate) struct CodeRabbitCli {
    pub name: String,
    pub path: String,
    pub version: Option<String>,
}

pub(crate) fn detect_coderabbit_cli(
    cli_path: Option<&str>,
    current_dir: Option<&Path>,
) -> Result<CodeRabbitCli, String> {
    if let Some(cli_path) = cli_path.map(str::trim).filter(|value| !value.is_empty()) {
        return inspect_candidate(cli_path, current_dir);
    }

    let mut errors = Vec::new();
    for candidate in CODERABBIT_CLI_CANDIDATES {
        match inspect_candidate(candidate, current_dir) {
            Ok(cli) => return Ok(cli),
            Err(error) => errors.push(format!("{candidate}: {error}")),
        }
    }

    Err(format!(
        "CodeRabbit CLI not found. Install it and ensure `cr` or `coderabbit` is on PATH. Tried: {}",
        errors.join("; ")
    ))
}

fn inspect_candidate(candidate: &str, current_dir: Option<&Path>) -> Result<CodeRabbitCli, String> {
    let output = run_command_with_timeout(
        candidate,
        [OsString::from("--version")],
        current_dir,
        CODERABBIT_STATUS_TIMEOUT,
    )?;
    if !output.status.success() {
        return Err(command_output_detail(
            &output,
            "CodeRabbit CLI version check failed",
        ));
    }

    let version = command_stdout(&output);
    Ok(CodeRabbitCli {
        name: if candidate.ends_with("coderabbit") {
            "coderabbit".to_string()
        } else {
            "cr".to_string()
        },
        path: candidate.to_string(),
        version: non_empty(version),
    })
}

pub(crate) fn run_command_with_timeout<I, S>(
    program: &str,
    args: I,
    current_dir: Option<&Path>,
    timeout: Duration,
) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    for arg in args {
        command.arg(arg.as_ref());
    }
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }

    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let child = command.spawn().map_err(|error| error.to_string())?;
    let child_pid = child.id();
    let (tx, rx) = mpsc::channel();
    let waiter = thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            let _ = waiter.join();
            Ok(output)
        }
        Ok(Err(error)) => {
            let _ = waiter.join();
            Err(error.to_string())
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
            }
            #[cfg(windows)]
            {
                let child_pid = child_pid.to_string();
                let _ = Command::new("taskkill")
                    .arg("/PID")
                    .arg(&child_pid)
                    .arg("/T")
                    .arg("/F")
                    .output();
            }
            let _ = waiter.join();
            Err(format!(
                "CodeRabbit CLI command timed out after {}s",
                timeout.as_secs()
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = waiter.join();
            Err("CodeRabbit CLI waiter thread crashed before returning output".to_string())
        }
    }
}

pub(crate) fn command_output_detail(output: &Output, fallback: &str) -> String {
    let stderr = command_stderr(output);
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = command_stdout(output);
    if !stdout.is_empty() {
        return stdout;
    }

    fallback.to_string()
}

pub(crate) fn command_stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(crate) fn command_stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
