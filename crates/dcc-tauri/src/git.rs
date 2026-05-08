use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub const GIT_LOCAL_TIMEOUT: Duration = Duration::from_secs(15);
pub const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(30);
pub const GIT_HARDENED_CONFIG_ARGS: [&str; 4] = [
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.untrackedCache=false",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitNameStatusEntry {
    pub status: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitStatusPorcelainEntry {
    pub index_status: char,
    pub worktree_status: char,
    pub path: String,
}

pub fn git_output_err(cmd: &str, stderr: &[u8]) -> String {
    let msg = String::from_utf8_lossy(stderr);
    format!("{cmd} failed: {}", msg.trim())
}

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

pub fn split_null_terminated_fields(stdout: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .split('\0')
        .filter(|field| !field.is_empty())
        .map(|field| field.to_string())
        .collect()
}

pub fn parse_name_status_z(stdout: &[u8]) -> Vec<GitNameStatusEntry> {
    let fields = split_null_terminated_fields(stdout);
    let mut entries = Vec::new();
    let mut index = 0usize;

    while index < fields.len() {
        let raw_status = fields[index].trim().to_string();
        index += 1;
        if raw_status.is_empty() || index >= fields.len() {
            break;
        }

        let mut path = fields[index].to_string();
        index += 1;
        if (raw_status.starts_with('R') || raw_status.starts_with('C')) && index < fields.len() {
            path = fields[index].to_string();
            index += 1;
        }

        if path.is_empty() {
            continue;
        }

        entries.push(GitNameStatusEntry {
            status: raw_status.chars().next().unwrap_or('M').to_string(),
            path,
        });
    }

    entries
}

pub fn parse_numstat_z(stdout: &[u8]) -> HashMap<String, (u32, u32)> {
    let mut stats = HashMap::new();
    let mut cursor = 0usize;

    while cursor < stdout.len() {
        let Some(ins_end) = stdout[cursor..].iter().position(|byte| *byte == b'\t') else {
            break;
        };
        let ins_end = cursor + ins_end;
        let Some(del_end_rel) = stdout[ins_end + 1..].iter().position(|byte| *byte == b'\t') else {
            break;
        };
        let del_end = ins_end + 1 + del_end_rel;
        let Some(path_end_rel) = stdout[del_end + 1..].iter().position(|byte| *byte == b'\0')
        else {
            break;
        };
        let path_end = del_end + 1 + path_end_rel;

        let ins_s = std::str::from_utf8(&stdout[cursor..ins_end]).unwrap_or_default();
        let del_s = std::str::from_utf8(&stdout[ins_end + 1..del_end]).unwrap_or_default();
        let path_bytes = &stdout[del_end + 1..path_end];

        let path = if path_bytes.is_empty() {
            let old_start = path_end + 1;
            let Some(old_end_rel) = stdout[old_start..].iter().position(|byte| *byte == b'\0')
            else {
                break;
            };
            let old_end = old_start + old_end_rel;
            let new_start = old_end + 1;
            let Some(new_end_rel) = stdout[new_start..].iter().position(|byte| *byte == b'\0')
            else {
                break;
            };
            let new_end = new_start + new_end_rel;
            cursor = new_end + 1;
            String::from_utf8_lossy(&stdout[new_start..new_end]).to_string()
        } else {
            cursor = path_end + 1;
            String::from_utf8_lossy(path_bytes).to_string()
        };

        let insertions = if ins_s == "-" {
            0
        } else {
            ins_s.parse().unwrap_or(0)
        };
        let deletions = if del_s == "-" {
            0
        } else {
            del_s.parse().unwrap_or(0)
        };

        if !path.is_empty() {
            stats.insert(path, (insertions, deletions));
        }
    }

    stats
}

pub fn parse_git_status_porcelain_z(stdout: &[u8]) -> Vec<GitStatusPorcelainEntry> {
    let mut entries = Vec::new();
    let mut fields = stdout
        .split(|byte| *byte == b'\0')
        .filter(|field| !field.is_empty());

    while let Some(field) = fields.next() {
        if field.len() < 3 {
            continue;
        }
        let header = String::from_utf8_lossy(field);
        let index_status = header.chars().next().unwrap_or(' ');
        let worktree_status = header.chars().nth(1).unwrap_or(' ');
        let path = header.get(3..).unwrap_or("").trim().to_string();
        if path.is_empty() {
            continue;
        }
        entries.push(GitStatusPorcelainEntry {
            index_status,
            worktree_status,
            path,
        });

        if matches!(index_status, 'R' | 'C') || matches!(worktree_status, 'R' | 'C') {
            let _ = fields.next();
        }
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::{parse_git_status_porcelain_z, parse_numstat_z};

    #[test]
    fn parses_numstat_z_regular_records() {
        let stats = parse_numstat_z(b"1\t0\tsrc/main.rs\0");
        assert_eq!(stats.get("src/main.rs"), Some(&(1, 0)));
    }

    #[test]
    fn parses_numstat_z_rename_records() {
        let stats = parse_numstat_z(b"0\t0\t\0old name.rs\0new name.rs\0");
        assert_eq!(stats.get("new name.rs"), Some(&(0, 0)));
    }

    #[test]
    fn parses_numstat_z_binary_records() {
        let stats = parse_numstat_z(b"-\t-\tassets/logo.png\0");
        assert_eq!(stats.get("assets/logo.png"), Some(&(0, 0)));
    }

    #[test]
    fn parses_porcelain_z_rename_records() {
        let entries = parse_git_status_porcelain_z(b"R  new name.rs\0old name.rs\0");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].index_status, 'R');
        assert_eq!(entries[0].worktree_status, ' ');
        assert_eq!(entries[0].path, "new name.rs");
    }
}
