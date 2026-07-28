use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use chrono::Utc;
use dcc_core::{
    domain::mcp::{McpDefinition, McpErrorCategory, McpProbeReport, McpSecretTarget, McpTransport},
    ports::{CredentialStore, McpProbeResult},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, BufReader},
    process::{Child, Command},
    task::JoinHandle,
    time::timeout,
};

use super::{
    probe_error,
    protocol::{
        initialize_request, initialized_notification, list_tools_request,
        parse_initialize_response, parse_tools_response, read_response, write_message,
    },
    SecureMcpProbe,
};

pub(super) async fn probe_stdio<C>(
    probe: &SecureMcpProbe<C>,
    definition: &McpDefinition,
) -> McpProbeResult<McpProbeReport>
where
    C: CredentialStore + ?Sized,
{
    let McpTransport::Stdio {
        executable,
        args,
        cwd,
    } = &definition.transport
    else {
        unreachable!("stdio probe called for another transport");
    };

    let executable = canonical_executable(executable)?;
    let cwd = canonical_working_directory(cwd.as_deref())?;
    let mut command = Command::new(&executable);
    command
        .args(args)
        .current_dir(&cwd)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    apply_secret_environment(probe, definition, &mut command).await?;

    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => probe_error(
            McpErrorCategory::ExecutableNotFound,
            "MCP executable was not found",
        ),
        std::io::ErrorKind::PermissionDenied => probe_error(
            McpErrorCategory::PermissionBoundary,
            "MCP executable could not be started due to permissions",
        ),
        _ => probe_error(
            McpErrorCategory::Transport,
            "failed to start the MCP executable",
        ),
    })?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        probe_error(
            McpErrorCategory::Transport,
            "MCP process did not expose stdin",
        )
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        probe_error(
            McpErrorCategory::Transport,
            "MCP process did not expose stdout",
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        probe_error(
            McpErrorCategory::Transport,
            "MCP process did not expose stderr",
        )
    })?;
    let mut stdout = BufReader::new(stdout);
    let stderr_task = tokio::spawn(drain_stderr(stderr, probe.limits.max_stderr_bytes));

    let result = async {
        let initialize = timeout(probe.limits.initialize_timeout, async {
            write_message(&mut stdin, &initialize_request()).await?;
            read_response(&mut stdout, 1, probe.limits.max_response_bytes).await
        })
        .await
        .map_err(|_| probe_error(McpErrorCategory::Timeout, "MCP initialization timed out"))??;
        let protocol_version = parse_initialize_response(initialize)?;

        write_message(&mut stdin, &initialized_notification()).await?;
        let tools = timeout(probe.limits.list_tools_timeout, async {
            write_message(&mut stdin, &list_tools_request()).await?;
            read_response(&mut stdout, 2, probe.limits.max_response_bytes).await
        })
        .await
        .map_err(|_| probe_error(McpErrorCategory::Timeout, "MCP tool discovery timed out"))??;
        let tools = parse_tools_response(tools, &probe.limits)?;

        Ok(McpProbeReport {
            definition_id: definition.id.clone(),
            transport: definition.transport.kind(),
            protocol_version,
            tools,
            checked_at: Utc::now().to_rfc3339(),
        })
    }
    .await;

    drop(stdin);
    shutdown_child(&mut child, probe.limits.shutdown_timeout).await;
    finish_stderr_task(stderr_task, probe.limits.shutdown_timeout).await;
    result
}

fn canonical_executable(executable: &str) -> McpProbeResult<PathBuf> {
    let configured = Path::new(executable);
    if !configured.is_absolute() {
        return Err(probe_error(
            McpErrorCategory::InvalidDefinition,
            "MCP command probe requires an absolute executable path",
        ));
    }
    let resolved = std::fs::canonicalize(configured).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => probe_error(
            McpErrorCategory::ExecutableNotFound,
            "MCP executable was not found",
        ),
        std::io::ErrorKind::PermissionDenied => probe_error(
            McpErrorCategory::PermissionBoundary,
            "MCP executable path could not be inspected",
        ),
        _ => probe_error(
            McpErrorCategory::Transport,
            "MCP executable path could not be resolved",
        ),
    })?;
    #[cfg(not(windows))]
    if resolved != configured {
        return Err(probe_error(
            McpErrorCategory::InvalidDefinition,
            "MCP executable path must already be canonical",
        ));
    }
    if !resolved.is_file() {
        return Err(probe_error(
            McpErrorCategory::ExecutableNotFound,
            "MCP executable path is not a file",
        ));
    }
    Ok(resolved)
}

fn canonical_working_directory(cwd: Option<&str>) -> McpProbeResult<PathBuf> {
    let cwd = cwd.ok_or_else(|| {
        probe_error(
            McpErrorCategory::InvalidDefinition,
            "MCP command probe requires an explicit working directory",
        )
    })?;
    let configured = Path::new(cwd);
    if !configured.is_absolute() {
        return Err(probe_error(
            McpErrorCategory::InvalidDefinition,
            "MCP working directory must be absolute",
        ));
    }
    let resolved = std::fs::canonicalize(configured).map_err(|_| {
        probe_error(
            McpErrorCategory::InvalidDefinition,
            "MCP working directory could not be resolved",
        )
    })?;
    #[cfg(not(windows))]
    if resolved != configured {
        return Err(probe_error(
            McpErrorCategory::InvalidDefinition,
            "MCP working directory must be an existing canonical directory",
        ));
    }
    if !resolved.is_dir() {
        return Err(probe_error(
            McpErrorCategory::InvalidDefinition,
            "MCP working directory must be an existing canonical directory",
        ));
    }
    Ok(resolved)
}

async fn apply_secret_environment<C>(
    probe: &SecureMcpProbe<C>,
    definition: &McpDefinition,
    command: &mut Command,
) -> McpProbeResult<()>
where
    C: CredentialStore + ?Sized,
{
    for binding in &definition.secret_refs {
        let McpSecretTarget::EnvironmentVariable { name } = &binding.target else {
            return Err(probe_error(
                McpErrorCategory::InvalidDefinition,
                "MCP secret target does not match stdio transport",
            ));
        };
        let secret = probe
            .credentials
            .resolve_secret(&binding.secret_ref)
            .await
            .map_err(|_| credential_error())?
            .ok_or_else(credential_error)?;
        let value = std::str::from_utf8(secret.expose_secret()).map_err(|_| credential_error())?;
        if value.contains('\0') {
            return Err(credential_error());
        }
        command.env(name, value);
    }
    Ok(())
}

fn credential_error() -> dcc_core::domain::mcp::McpRuntimeError {
    probe_error(
        McpErrorCategory::Authentication,
        "MCP credential is unavailable or invalid",
    )
}

async fn drain_stderr<R>(mut stderr: R, max_bytes: usize) -> usize
where
    R: AsyncRead + Unpin,
{
    let mut observed = 0_usize;
    let mut buffer = [0_u8; 4096];
    loop {
        let read = match stderr.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        observed = observed.saturating_add(read).min(max_bytes);
        buffer[..read].fill(0);
    }
    buffer.fill(0);
    observed
}

async fn finish_stderr_task(mut task: JoinHandle<usize>, shutdown_timeout: std::time::Duration) {
    if timeout(shutdown_timeout, &mut task).await.is_err() {
        task.abort();
        let _ = task.await;
    }
}

async fn shutdown_child(child: &mut Child, shutdown_timeout: std::time::Duration) {
    if timeout(shutdown_timeout, child.wait()).await.is_ok() {
        return;
    }

    #[cfg(unix)]
    if let Some(process_id) = child.id() {
        // The child was created as the leader of a new process group above.
        // A negative PID targets only that group, never the DCC process group.
        unsafe {
            libc::kill(-(process_id as i32), libc::SIGKILL);
        }
    }
    let _ = child.start_kill();
    let _ = timeout(shutdown_timeout, child.wait()).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_resolution_rejects_path_lookup_and_missing_files() {
        let relative = canonical_executable("npx").expect_err("PATH lookup must be rejected");
        assert_eq!(relative.category, McpErrorCategory::InvalidDefinition);

        #[cfg(unix)]
        {
            let missing = canonical_executable("/definitely/missing/dcc-mcp")
                .expect_err("missing executable");
            assert_eq!(missing.category, McpErrorCategory::ExecutableNotFound);
        }
    }

    #[tokio::test]
    async fn stderr_observation_is_bounded_while_the_stream_is_fully_drained() {
        let input = vec![b'x'; 16 * 1024];
        let observed = drain_stderr(input.as_slice(), 1024).await;
        assert_eq!(observed, 1024);
    }
}
