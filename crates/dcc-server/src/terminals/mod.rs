//! The shells the desktop runs (`portable-pty`): spawning one for an owner,
//! its reader thread (scrollback, title, "waiting on you", activity), input,
//! resizing and killing it. The desktop's commands and the native app's
//! terminal routes both drive these; events go out by name (`TerminalEvents`)
//! and to the native app's stream.

mod reader;
mod session;

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use portable_pty::{Child as PtyChild, MasterPty};
use rusqlite::{params, Connection};
use serde::Serialize;

pub use reader::*;
pub use session::*;

/// What the terminals share with the rest of the app: the database (pane
/// scrollback) and the live shells by id.
#[derive(Clone)]
pub struct TerminalState {
    pub conn: Arc<Mutex<Connection>>,
    pub terminals: Arc<Mutex<HashMap<String, ManagedTerminal>>>,
    /// Shared with whoever samples processes (the activity's descendants).
    pub system: Arc<Mutex<sysinfo::System>>,
}

/// Where the terminals' events go by name (`terminal-attention`,
/// `terminal-activity`, `terminal-exit`): the desktop's webview, or nowhere.
#[derive(Clone)]
pub struct TerminalEvents(pub Arc<dyn dcc_tauri::app_events::AppEmitter>);

impl TerminalEvents {
    pub fn emit<T: Serialize>(&self, event: &str, payload: T) -> Result<(), String> {
        let payload = serde_json::to_value(payload).map_err(|error| error.to_string())?;
        self.0.emit(event, payload)
    }
}

/// Últimas N linhas de stdout/stderr por sessão (reidratação do xterm ao remontar).
pub const TERMINAL_OUTPUT_MAX_LINES: usize = 1000;

#[cfg(windows)]
pub mod windows_job {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::null_mut;

    type Handle = *mut c_void;

    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: u32 = 9;

    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[repr(C)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_counters: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(lp_job_attributes: *mut c_void, lp_name: *const u16) -> Handle;
        fn SetInformationJobObject(
            h_job: Handle,
            job_object_info_class: u32,
            lp_job_object_info: *mut c_void,
            cb_job_object_info_length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(h_job: Handle, h_process: Handle) -> i32;
        fn TerminateJobObject(h_job: Handle, u_exit_code: u32) -> i32;
        fn OpenProcess(dw_desired_access: u32, b_inherit_handle: i32, dw_process_id: u32)
            -> Handle;
        fn CloseHandle(h_object: Handle) -> i32;
    }

    pub struct WindowsJobObject {
        handle: Handle,
    }

    impl WindowsJobObject {
        pub fn attach_process(pid: u32) -> Result<Self, String> {
            unsafe {
                let handle = CreateJobObjectW(null_mut(), null_mut());
                if handle.is_null() {
                    return Err(format!(
                        "CreateJobObjectW falhou: {}",
                        std::io::Error::last_os_error()
                    ));
                }

                let mut info = JobObjectExtendedLimitInformation {
                    basic_limit_information: JobObjectBasicLimitInformation {
                        per_process_user_time_limit: 0,
                        per_job_user_time_limit: 0,
                        limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                        minimum_working_set_size: 0,
                        maximum_working_set_size: 0,
                        active_process_limit: 0,
                        affinity: 0,
                        priority_class: 0,
                        scheduling_class: 0,
                    },
                    io_counters: IoCounters {
                        read_operation_count: 0,
                        write_operation_count: 0,
                        other_operation_count: 0,
                        read_transfer_count: 0,
                        write_transfer_count: 0,
                        other_transfer_count: 0,
                    },
                    process_memory_limit: 0,
                    job_memory_limit: 0,
                    peak_process_memory_used: 0,
                    peak_job_memory_used: 0,
                };

                if SetInformationJobObject(
                    handle,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                    &mut info as *mut _ as *mut c_void,
                    size_of::<JobObjectExtendedLimitInformation>() as u32,
                ) == 0
                {
                    let err = std::io::Error::last_os_error();
                    let _ = CloseHandle(handle);
                    return Err(format!("SetInformationJobObject falhou: {err}"));
                }

                let process = OpenProcess(
                    PROCESS_TERMINATE | PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    pid,
                );
                if process.is_null() {
                    let err = std::io::Error::last_os_error();
                    let _ = CloseHandle(handle);
                    return Err(format!("OpenProcess falhou: {err}"));
                }

                if AssignProcessToJobObject(handle, process) == 0 {
                    let err = std::io::Error::last_os_error();
                    let _ = CloseHandle(process);
                    let _ = CloseHandle(handle);
                    return Err(format!("AssignProcessToJobObject falhou: {err}"));
                }

                let _ = CloseHandle(process);
                Ok(Self { handle })
            }
        }

        pub fn terminate(&self, exit_code: u32) -> Result<(), String> {
            unsafe {
                if TerminateJobObject(self.handle, exit_code) == 0 {
                    Err(format!(
                        "TerminateJobObject falhou: {}",
                        std::io::Error::last_os_error()
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }

    impl Drop for WindowsJobObject {
        fn drop(&mut self) {
            unsafe {
                if !self.handle.is_null() {
                    let _ = CloseHandle(self.handle);
                    self.handle = null_mut();
                }
            }
        }
    }
}

pub struct ManagedTerminal {
    pub pty_master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn PtyChild + Send>,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    #[cfg(windows)]
    pub windows_job: Option<windows_job::WindowsJobObject>,
    pub mission_id: Option<String>,
    pub pane_id: Option<String>,
    pub pty_owner_key: Option<String>,
    pub title: Arc<Mutex<Option<String>>>,
    pub cwd: String,
    pub command: String,
    pub args: Vec<String>,
    pub started_at: String,
    pub stop_flag: Arc<AtomicBool>,
    pub exit_notified: Arc<AtomicBool>,
    /// Buffer circular compartilhado entre as threads de leitura (lock curto só aqui).
    pub output_buffer: Arc<Mutex<VecDeque<String>>>,
    pub reader_thread: Option<JoinHandle<()>>,
}

#[cfg(windows)]
pub fn attach_windows_job_object(
    child: &Box<dyn PtyChild + Send>,
) -> Option<windows_job::WindowsJobObject> {
    child
        .process_id()
        .and_then(|pid| windows_job::WindowsJobObject::attach_process(pid).ok())
}

pub fn new_terminal_output_buffer() -> Arc<Mutex<VecDeque<String>>> {
    Arc::new(Mutex::new(VecDeque::with_capacity(
        TERMINAL_OUTPUT_MAX_LINES.min(256),
    )))
}

#[inline]
pub fn append_terminal_line(buffer: &Arc<Mutex<VecDeque<String>>>, line: &str) {
    if let Ok(mut guard) = buffer.lock() {
        while guard.len() >= TERMINAL_OUTPUT_MAX_LINES {
            guard.pop_front();
        }
        guard.push_back(line.to_string());
    }
}

/// Intervalo mínimo entre escritas do scrollback comprimido no SQLite (evita pressão no WAL).
pub const PANE_SCROLLBACK_PERSIST_INTERVAL: Duration = Duration::from_millis(1600);

pub fn persist_pane_scrollback_compressed(
    conn: &Connection,
    pane_id: &str,
    lines: &[String],
) -> Result<(), String> {
    if lines.is_empty() {
        conn.execute(
            "DELETE FROM pane_terminal_scrollback WHERE pane_id = ?1",
            params![pane_id],
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let json = serde_json::to_vec(lines).map_err(|e| e.to_string())?;
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&json).map_err(|e| e.to_string())?;
    let compressed = enc.finish().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO pane_terminal_scrollback (pane_id, payload_z, updated_at) VALUES (?1, ?2, datetime('now')) \
         ON CONFLICT(pane_id) DO UPDATE SET payload_z = excluded.payload_z, updated_at = excluded.updated_at",
        params![pane_id, compressed],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_pane_scrollback_deque(conn: &Connection, pane_id: &str) -> Option<VecDeque<String>> {
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT payload_z FROM pane_terminal_scrollback WHERE pane_id = ?1",
            params![pane_id],
            |row| row.get(0),
        )
        .ok()?;
    let mut decoder = GzDecoder::new(&blob[..]);
    let mut json = Vec::new();
    decoder.read_to_end(&mut json).ok()?;
    let lines: Vec<String> = serde_json::from_slice(&json).ok()?;
    let mut dq = VecDeque::with_capacity(lines.len().min(TERMINAL_OUTPUT_MAX_LINES));
    for line in lines {
        while dq.len() >= TERMINAL_OUTPUT_MAX_LINES {
            dq.pop_front();
        }
        dq.push_back(line);
    }
    Some(dq)
}

pub fn persist_managed_terminal_buffer(state: &TerminalState, t: &ManagedTerminal) {
    let Some(ref pane_id) = t.pane_id else {
        return;
    };
    let lines: Vec<String> = t
        .output_buffer
        .lock()
        .ok()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default();
    if let Ok(conn) = state.conn.lock() {
        if let Err(e) = persist_pane_scrollback_compressed(&conn, pane_id, &lines) {
            eprintln!("[DCC] pane scrollback persist failed: {e}");
        }
    }
}
