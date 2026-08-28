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
const GIT_REPOSITORY_STEERING_ENV: [&str; 7] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
];

pub fn configure_git_command(command: &mut Command) {
    command.args(GIT_HARDENED_CONFIG_ARGS);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GCM_INTERACTIVE", "Never");
    command.env_remove("GIT_ASKPASS");
    command.env_remove("SSH_ASKPASS");
    // `git -C <root>` does not override repository-steering environment
    // variables inherited from the shell that launched DCC. Every caller must
    // start from the requested root; specialized shadow-index callers can add
    // their explicit, validated environment after this shared hardening step.
    for key in GIT_REPOSITORY_STEERING_ENV {
        command.env_remove(key);
    }

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

pub fn run_git_output_with_timeout_and_env<I, K, V>(
    root: &str,
    args: I,
    timeout: Duration,
    envs: &[(K, V)],
) -> Result<Output, String>
where
    I: IntoIterator,
    I::Item: AsRef<OsStr>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    configure_git_command(&mut command);
    command.arg("-C").arg(root);
    for arg in args {
        command.arg(arg.as_ref());
    }
    for (key, value) in envs {
        command.env(key.as_ref(), value.as_ref());
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

pub fn run_git_network_output_with_env<K, V>(
    root: &str,
    args: &[&str],
    envs: &[(K, V)],
) -> Result<Output, String>
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    run_git_output_with_timeout_and_env(root, args, GIT_NETWORK_TIMEOUT, envs)
}

pub fn git_command_succeeds(root: &str, args: &[&str]) -> bool {
    run_git_output(root, args)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_removes_inherited_repository_steering_environment() {
        let mut command = Command::new("git");
        for key in GIT_REPOSITORY_STEERING_ENV {
            command.env(key, "/tmp/poisoned-git-scope");
        }

        configure_git_command(&mut command);

        let configured: Vec<_> = command.get_envs().collect();
        for key in GIT_REPOSITORY_STEERING_ENV {
            assert!(configured
                .iter()
                .any(|(name, value)| { *name == OsStr::new(key) && value.is_none() }));
        }
    }
}
