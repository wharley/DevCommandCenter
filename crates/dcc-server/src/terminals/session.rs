//! Spawning, finding, writing to, resizing and killing a shell; the
//! activity list.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::path::Path;
use sysinfo::{Pid, System};

use super::*;
use crate::command_api::*;
use crate::http_api::{publish_desktop_terminal_event, DesktopTerminalEvent};

// ---------- Terminal ----------
pub fn terminal_spawn(
    state: &TerminalState,
    app: TerminalEvents,
    options: Value,
) -> ApiResult<Value> {
    let cwd = options
        .get("cwd")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("cwd is required"))?;
    let command = options
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("bash");
    let args = options
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<Vec<_>>();

    let cols = options.get("cols").and_then(Value::as_u64).unwrap_or(80) as u16;
    let rows = options.get("rows").and_then(Value::as_u64).unwrap_or(24) as u16;

    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| db_error(e.to_string()))?;

    let mut cmd = CommandBuilder::new(command);
    cmd.args(&args);
    cmd.cwd(cwd);
    // Alinha o PTY com o que o xterm.js emula; sem TERM adequado, `clear` e escapes podem falhar
    // quando o app é lançado pela GUI (env mínima).
    #[cfg(unix)]
    {
        cmd.env("TERM", "xterm-256color");

        // Detecta worktree e configura Git corretamente
        // Worktrees têm um arquivo .git (não diretório) apontando para gitdir
        let git_file_path = Path::new(cwd).join(".git");
        if git_file_path.is_file() {
            // Lê o arquivo .git para obter o gitdir
            if let Ok(git_content) = std::fs::read_to_string(&git_file_path) {
                // Formato: "gitdir: /path/to/.git/worktrees/name"
                if let Some(gitdir_line) = git_content.lines().find(|l| l.starts_with("gitdir:")) {
                    let gitdir = gitdir_line.trim_start_matches("gitdir:").trim();
                    // Configura GIT_DIR para apontar para o gitdir do worktree
                    cmd.env("GIT_DIR", gitdir);
                    // GIT_WORK_TREE aponta para o diretório de trabalho atual (o worktree)
                    cmd.env("GIT_WORK_TREE", cwd);
                }
            }
        }
    }

    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| db_error(e.to_string()))?;
    #[cfg(windows)]
    let windows_job = attach_windows_job_object(&child);

    let reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|e| db_error(e.to_string()))?;
    let writer = pty_pair
        .master
        .take_writer()
        .map_err(|e| db_error(e.to_string()))?;

    let pty_id = format!("pty-{}", next_id());
    let stop_flag = Arc::new(AtomicBool::new(false));

    let pane_id_opt = options
        .get("paneId")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let skip_scrollback_restore = options
        .get("restart")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let pty_owner_key_opt = options
        .get("ptyOwnerKey")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let output_buffer = new_terminal_output_buffer();
    let title_state = Arc::new(Mutex::new(None));
    if skip_scrollback_restore {
        if let Some(ref pid) = pane_id_opt {
            if let Ok(conn) = state.conn.lock() {
                let _ = conn.execute(
                    "DELETE FROM pane_terminal_scrollback WHERE pane_id = ?1",
                    params![pid],
                );
            }
        }
    } else if let Some(ref pid) = pane_id_opt {
        if let Ok(conn) = state.conn.lock() {
            if let Some(loaded) = load_pane_scrollback_deque(&conn, pid) {
                if let Ok(mut guard) = output_buffer.lock() {
                    *guard = loaded;
                }
            }
        }
    }

    let reader_thread = Some(spawn_terminal_reader_thread(
        reader,
        app.clone(),
        pty_id.clone(),
        pane_id_opt.clone(),
        title_state.clone(),
        stop_flag.clone(),
        output_buffer.clone(),
        state.conn.clone(),
    ));

    if let Some(pane_id) = pane_id_opt.as_ref() {
        if let Ok(conn) = state.conn.lock() {
            let _ = conn.execute(
                "UPDATE panes SET status = 'running', last_activity_at = datetime('now') WHERE id = ?1",
                params![pane_id],
            );
        }
    }

    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    terminals.insert(
        pty_id.clone(),
        ManagedTerminal {
            pty_master: pty_pair.master,
            child,
            writer: Arc::new(Mutex::new(writer)),
            #[cfg(windows)]
            windows_job,
            mission_id: None,
            pane_id: pane_id_opt,
            pty_owner_key: pty_owner_key_opt,
            title: title_state,
            cwd: cwd.to_string(),
            command: command.to_string(),
            args,
            started_at: iso_now(),
            stop_flag,
            exit_notified: Arc::new(AtomicBool::new(false)),
            output_buffer,
            reader_thread,
        },
    );

    Ok(serde_json::json!({ "ptyId": pty_id }))
}

pub fn terminal_session_json(pty_id: &str, t: &mut ManagedTerminal) -> Value {
    let wait = t.child.try_wait().ok().flatten();
    let (status, exited_at, exit_code) = if let Some(s) = wait {
        ("exited", Some(iso_now()), Some(s.exit_code()))
    } else {
        ("running", None, None)
    };
    serde_json::json!({
      "ptyId": pty_id,
      "cwd": t.cwd,
      "command": t.command,
      "args": t.args,
      "ptyOwnerKey": t.pty_owner_key,
      "status": status,
      "startedAt": t.started_at,
      "exitedAt": exited_at,
      "lastExitCode": exit_code
    })
}

pub fn terminal_get_or_create(
    state: &TerminalState,
    app: TerminalEvents,
    mission_id: String,
    options: Value,
) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    let existing_id = terminals
        .iter_mut()
        .find(|(_, t)| t.mission_id.as_deref() == Some(mission_id.as_str()))
        .map(|(id, _)| id.clone());

    if let Some(pty_id) = existing_id {
        if let Some(t) = terminals.get_mut(&pty_id) {
            let session = terminal_session_json(&pty_id, t);
            let chunks = t
                .output_buffer
                .lock()
                .map(|buffer| buffer.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            return Ok(serde_json::json!({
                "ptyId": pty_id,
                "existing": true,
                "session": session,
                "chunks": chunks,
                "truncated": false
            }));
        }
    }

    // Drop lock before spawn to avoid deadlock or long stalls
    drop(terminals);

    let spawn_result = terminal_spawn(state, app, options)?;
    let pty_id = spawn_result
        .get("ptyId")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();

    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    if let Some(t) = terminals.get_mut(&pty_id) {
        t.mission_id = Some(mission_id);
        let session = terminal_session_json(&pty_id, t);
        let chunks = t
            .output_buffer
            .lock()
            .map(|buffer| buffer.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        return Ok(serde_json::json!({
            "ptyId": pty_id,
            "existing": false,
            "session": session,
            "chunks": chunks,
            "truncated": false
        }));
    }

    Err(db_error("Failed to re-acquire spawned terminal"))
}

/// Escreve no PTY em pedaços para reduzir EIO/backpressure em colagens grandes (um único `write` enorme).
pub const PTY_INPUT_CHUNK_BYTES: usize = 8192;

pub fn pty_write_all_chunked(writer: &mut dyn Write, data: &[u8]) -> std::io::Result<()> {
    if data.is_empty() {
        return Ok(());
    }
    for chunk in data.chunks(PTY_INPUT_CHUNK_BYTES) {
        writer.write_all(chunk)?;
    }
    Ok(())
}

pub fn terminal_write(state: &TerminalState, pty_id: String, data: String) -> ApiResult<Value> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    if let Some(t) = terminals.get(&pty_id) {
        let mut writer = t
            .writer
            .lock()
            .map_err(|_| db_error("writer lock poisoned"))?;
        pty_write_all_chunked(&mut *writer, data.as_bytes())
            .map_err(|e| db_error(e.to_string()))?;
        return Ok(serde_json::json!({ "ok": true }));
    }
    Ok(serde_json::json!({ "ok": false }))
}

pub fn terminal_resize(
    state: &TerminalState,
    pty_id: String,
    cols: u16,
    rows: u16,
) -> ApiResult<Value> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    if let Some(t) = terminals.get(&pty_id) {
        t.pty_master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| db_error(e.to_string()))?;
        return Ok(serde_json::json!({ "ok": true }));
    }
    Ok(serde_json::json!({ "ok": false }))
}

#[cfg(windows)]
pub fn terminate_managed_terminal_tree(t: &ManagedTerminal, force: bool) -> Result<(), String> {
    if let Some(job) = t.windows_job.as_ref() {
        if job.terminate(if force { 1 } else { 0 }).is_ok() {
            return Ok(());
        }
    }

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let pid_u = t
        .child
        .process_id()
        .ok_or_else(|| "PID da sessão PTY indisponível".to_string())?;
    let mut args = vec!["/PID".to_string(), pid_u.to_string(), "/T".to_string()];
    if force {
        args.push("/F".to_string());
    }
    let status = std::process::Command::new("taskkill")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("taskkill falhou: {status}"))
    }
}

#[cfg(not(windows))]
pub fn terminate_managed_terminal_tree(_t: &ManagedTerminal, _force: bool) -> Result<(), String> {
    Ok(())
}

pub fn finalize_managed_terminal_removal(
    state: &TerminalState,
    app: &TerminalEvents,
    pty_id: String,
    mut terminal: ManagedTerminal,
    emit_exit_event: bool,
) {
    persist_managed_terminal_buffer(state, &terminal);
    terminal.stop_flag.store(true, Ordering::Relaxed);
    let _ = terminate_managed_terminal_tree(&terminal, true);
    let _ = terminal.child.kill();
    drop(terminal.pty_master);
    // Não aguardamos o reader thread aqui. Se ele demorar a encerrar,
    // o delete ficaria preso e a UI pareceria travada.
    let _ = terminal.reader_thread.take();

    if emit_exit_event {
        let _ = app.emit(
            "terminal-exit",
            serde_json::json!({ "ptyId": pty_id, "code": -1 }),
        );
        publish_desktop_terminal_event(DesktopTerminalEvent::Exit {
            pty_id,
            code: Some(-1),
        });
    }
}

pub fn terminal_kill(
    state: &TerminalState,
    app: TerminalEvents,
    pty_id: String,
) -> ApiResult<Value> {
    let terminal = {
        let mut terminals = state
            .terminals
            .lock()
            .map_err(|_| db_error("terminals lock poisoned"))?;
        terminals.remove(&pty_id)
    };

    if let Some(t) = terminal {
        finalize_managed_terminal_removal(&state, &app, pty_id, t, true);
        return Ok(serde_json::json!({ "ok": true }));
    }
    Ok(serde_json::json!({ "ok": false }))
}

pub fn process_is_descendant_of(system: &System, pid: Pid, root_pid: Pid) -> bool {
    let mut current = system.process(pid).and_then(|process| process.parent());
    for _ in 0..64 {
        let Some(parent) = current else {
            return false;
        };
        if parent == root_pid {
            return true;
        }
        current = system.process(parent).and_then(|process| process.parent());
    }
    false
}

/// Returns semantic activity for every managed terminal. A live login shell is
/// `idle`; only a descendant process makes it `running`. This keeps the UI from
/// presenting an idle shell as active work and lets hidden workspaces surface
/// real background resource usage.
pub fn terminal_list_activity(state: &TerminalState) -> ApiResult<Vec<Value>> {
    let mut system = state
        .system
        .lock()
        .map_err(|_| db_error("system lock poisoned"))?;
    system.refresh_processes();

    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    let mut result = Vec::with_capacity(terminals.len());

    for (pty_id, terminal) in terminals.iter_mut() {
        let shell_running = terminal.child.try_wait().ok().flatten().is_none();
        let shell_pid = terminal.child.process_id().map(Pid::from_u32);
        let descendants = shell_pid
            .map(|root_pid| {
                system
                    .processes()
                    .iter()
                    .filter(|(pid, _)| process_is_descendant_of(&system, **pid, root_pid))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let active_process = descendants
            .iter()
            .copied()
            .find(|(_, process)| {
                shell_pid.is_some_and(|pid| process.parent() == Some(pid))
                    && !process.name().trim().is_empty()
            })
            .or_else(|| descendants.first().copied());
        let waiting = shell_running
            && active_process.is_some()
            && waiting_excerpt_from_buffer(&terminal.output_buffer).is_some();
        let status = if !shell_running {
            "exited"
        } else if waiting {
            "waiting"
        } else if active_process.is_some() {
            "running"
        } else {
            "idle"
        };
        let (process_id, process_label) = active_process
            .map(|(pid, process)| (Some(pid.as_u32()), Some(process.name().to_string())))
            .unwrap_or((None, None));

        result.push(serde_json::json!({
            "ptyId": pty_id,
            "ptyOwnerKey": terminal.pty_owner_key,
            "cwd": terminal.cwd,
            "status": status,
            "processLabel": process_label,
            "processId": process_id,
            "startedAt": terminal.started_at
        }));
    }

    Ok(result)
}
