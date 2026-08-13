//! Process adapters shared by the infrastructure and application layers.

use std::ffi::OsStr;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Runs a child process with a hard timeout.
///
/// The child is placed in its own process group on Unix so a timeout also
/// terminates descendants that may otherwise keep stdout/stderr pipes open.
pub fn run_command_with_timeout<F>(
    program: impl AsRef<OsStr>,
    configure: F,
    timeout: Duration,
) -> Result<Output, String>
where
    F: FnOnce(&mut Command),
{
    let program = program.as_ref();
    let program_name = program.to_string_lossy().into_owned();
    let mut command = Command::new(program);
    configure(&mut command);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let child = command
        .spawn()
        .map_err(|error| format!("failed to start `{program_name}`: {error}"))?;
    let child_pid = child.id();
    let (sender, receiver) = mpsc::channel();

    let waiter = thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = sender.send(result);
    });

    match receiver.recv_timeout(timeout) {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => Err(format!("`{program_name}` failed while waiting: {error}")),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            terminate_process_group(child_pid);
            let _ = waiter.join();
            Err(format!(
                "`{program_name}` timed out after {} ms",
                timeout.as_millis()
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = waiter.join();
            Err(format!(
                "`{program_name}` exited without returning its output"
            ))
        }
    }
}

fn terminate_process_group(pid: u32) {
    #[cfg(unix)]
    {
        // The command is started as the leader of its own process group.
        let _ = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }

    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn terminates_a_command_that_exceeds_the_timeout() {
        let started = Instant::now();
        let result = run_command_with_timeout(
            "/bin/sh",
            |command| {
                command.args(["-c", "sleep 5"]);
            },
            Duration::from_millis(50),
        );

        assert!(result
            .expect_err("the command should have timed out")
            .contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
