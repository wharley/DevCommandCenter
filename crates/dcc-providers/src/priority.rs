//! Agents run below the person's own apps. Their builds and tests (and
//! everything else they start, which inherits it) give way to the window,
//! the editor and the rest of the Mac when the CPU is busy, and use all of
//! it when it is not. A light `nice`, not macOS's background class, which
//! would keep a build on the efficiency cores only.

use std::ffi::OsStr;

use tokio::process::Command;

/// The agents' niceness: a step below the person's apps, far from the floor.
pub const AGENT_NICENESS: i32 = 5;

/// A command for an agent's process, started at `AGENT_NICENESS`.
pub(crate) fn agent_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    lower(&mut command);
    command
}

#[cfg(unix)]
fn lower(command: &mut Command) {
    // SAFETY: `setpriority` is a single system call, safe between fork and
    // exec; it changes only the child about to run the agent.
    unsafe {
        command.pre_exec(|| {
            // A priority it cannot change is no reason not to start.
            let _ = libc::setpriority(libc::PRIO_PROCESS, 0, AGENT_NICENESS);
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn lower(_command: &mut Command) {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_agent_and_what_it_starts_run_below_the_app() {
        // `ps` reads the niceness of the shell the agent would be, and the
        // shell's own child inherits it.
        let output = agent_command("/bin/sh")
            .args(["-c", "ps -o nice= -p $$; /bin/sh -c 'ps -o nice= -p $$'"])
            .output()
            .await
            .unwrap();
        let values: Vec<i32> = String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        let own = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
        assert_eq!(values, [own.max(AGENT_NICENESS); 2]);
    }
}
