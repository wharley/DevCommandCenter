use std::{
    ffi::{OsStr, OsString},
    path::Path,
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

pub const GIT_LOCAL_TIMEOUT: Duration = Duration::from_secs(15);
pub const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(30);
pub const GIT_CLONE_TIMEOUT: Duration = Duration::from_secs(300);
pub const GIT_HARDENED_CONFIG_ARGS: [&str; 4] = [
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.untrackedCache=false",
];

pub fn configure_git_command(command: &mut Command) {
    command.args(GIT_HARDENED_CONFIG_ARGS);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GCM_INTERACTIVE", "Never");
    command.env_remove("GIT_ASKPASS");
    command.env_remove("SSH_ASKPASS");

    let base_ssh = std::env::var("GIT_SSH_COMMAND").unwrap_or_else(|_| "ssh".to_string());
    command.env(
        "GIT_SSH_COMMAND",
        format!("{base_ssh} -o BatchMode=yes -o ConnectTimeout=10 -o StrictHostKeyChecking=yes"),
    );
}

pub fn git_output_detail(output: &Output, fallback: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }

    fallback.to_string()
}

pub fn git_output_err(cmd: &str, stderr: &[u8]) -> String {
    let msg = String::from_utf8_lossy(stderr);
    format!("{cmd} failed: {}", msg.trim())
}

pub fn run_git_output_with_timeout_in_dir<I>(
    args: I,
    current_dir: Option<&Path>,
    timeout: Duration,
) -> Result<Output, String>
where
    I: IntoIterator,
    I::Item: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    configure_git_command(&mut command);
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
                "git command timed out after {}s",
                timeout.as_secs()
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = waiter.join();
            Err("git waiter thread crashed before returning output".to_string())
        }
    }
}

pub fn run_git_stdout_in_dir<I>(
    args: I,
    current_dir: Option<&Path>,
    timeout: Duration,
    fallback: &str,
) -> Result<String, String>
where
    I: IntoIterator,
    I::Item: AsRef<OsStr>,
{
    let output = run_git_output_with_timeout_in_dir(args, current_dir, timeout)?;
    if !output.status.success() {
        return Err(git_output_detail(&output, fallback));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn run_git_output_with_timeout<I>(
    root: &str,
    args: I,
    timeout: Duration,
) -> Result<Output, String>
where
    I: IntoIterator,
    I::Item: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    configure_git_command(&mut command);
    command.arg("-C").arg(root);
    for arg in args {
        command.arg(arg.as_ref());
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
                "git command timed out after {}s",
                timeout.as_secs()
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = waiter.join();
            Err("git waiter thread crashed before returning output".to_string())
        }
    }
}

pub fn run_git_output(root: &str, args: &[&str]) -> Result<Output, String> {
    run_git_output_with_timeout(root, args, GIT_LOCAL_TIMEOUT)
}

pub fn run_git_output_owned(root: &str, args: Vec<OsString>) -> Result<Output, String> {
    run_git_output_with_timeout(root, args, GIT_LOCAL_TIMEOUT)
}

pub fn run_git_network_output(root: &str, args: &[&str]) -> Result<Output, String> {
    run_git_output_with_timeout(root, args, GIT_NETWORK_TIMEOUT)
}

pub fn git_command_succeeds(root: &str, args: &[&str]) -> bool {
    run_git_output(root, args)
        .map(|output| output.status.success())
        .unwrap_or(false)
}
