#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rusqlite::{
    params, params_from_iter, types::ValueRef, Connection, OptionalExtension,
};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::{DialogExt, FilePath};
use portable_pty::{native_pty_system, Child as PtyChild, CommandBuilder, MasterPty, PtySize};

/// Schema SQLite compartilhado com `lib/database/schema.sql` (CREATE IF NOT EXISTS).
const APP_SCHEMA_SQL: &str = include_str!("../../lib/database/schema.sql");

#[derive(Clone)]
struct AppState {
    db_path: Arc<PathBuf>,
    conn: Arc<Mutex<Connection>>,
    terminals: Arc<Mutex<HashMap<String, ManagedTerminal>>>,
}

/// Últimas N linhas de stdout/stderr por sessão (reidratação do xterm ao remontar).
const TERMINAL_OUTPUT_MAX_LINES: usize = 1000;

struct ManagedTerminal {
    pty_master: Box<dyn MasterPty + Send>,
    child: Box<dyn PtyChild + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    mission_id: Option<String>,
    pane_id: Option<String>,
    cwd: String,
    command: String,
    args: Vec<String>,
    started_at: String,
    stop_flag: Arc<AtomicBool>,
    /// Buffer circular compartilhado entre as threads de leitura (lock curto só aqui).
    output_buffer: Arc<Mutex<VecDeque<String>>>,
    reader_thread: Option<JoinHandle<()>>,
}

fn new_terminal_output_buffer() -> Arc<Mutex<VecDeque<String>>> {
    Arc::new(Mutex::new(VecDeque::with_capacity(
        TERMINAL_OUTPUT_MAX_LINES.min(256),
    )))
}

#[inline]
fn append_terminal_line(buffer: &Arc<Mutex<VecDeque<String>>>, line: &str) {
    if let Ok(mut guard) = buffer.lock() {
        while guard.len() >= TERMINAL_OUTPUT_MAX_LINES {
            guard.pop_front();
        }
        guard.push_back(line.to_string());
    }
}

#[derive(Debug, Serialize)]
struct ApiError {
    code: &'static str,
    message: String,
}

impl ApiError {
    fn not_implemented(op: &'static str) -> Self {
        Self {
            code: "NOT_IMPLEMENTED",
            message: format!("{op} is mapped but not implemented yet"),
        }
    }
}

type ApiResult<T> = Result<T, ApiError>;

fn mapped_not_implemented(op: &'static str) -> ApiResult<Value> {
    Err(ApiError::not_implemented(op))
}

fn db_error(message: impl Into<String>) -> ApiError {
    ApiError {
        code: "DB_ERROR",
        message: message.into(),
    }
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma).map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let name: String = row.get(1).map_err(|e| e.to_string())?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_column(conn: &Connection, table: &str, column: &str, sql_type: &str) -> Result<(), String> {
    if table_has_column(conn, table, column)? {
        return Ok(());
    }
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {sql_type}");
    conn.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

fn run_legacy_schema_migrations(conn: &Connection) -> Result<(), String> {
    // Providers
    ensure_column(conn, "providers", "api_key_encrypted", "BLOB")?;
    ensure_column(conn, "providers", "cli_path", "TEXT")?;
    ensure_column(conn, "providers", "config", "TEXT")?;

    // Projects
    ensure_column(conn, "projects", "git_remote_url", "TEXT")?;
    ensure_column(conn, "projects", "last_opened_at", "TEXT")?;

    // Combs
    ensure_column(conn, "combs", "review_targets", "TEXT")?;
    ensure_column(conn, "combs", "branch", "TEXT")?;
    ensure_column(conn, "combs", "worktree_path", "TEXT")?;
    ensure_column(
        conn,
        "combs",
        "status",
        "TEXT NOT NULL DEFAULT 'active'",
    )?;
    ensure_column(conn, "combs", "last_opened_at", "TEXT")?;

    // Panes
    ensure_column(conn, "panes", "pty_owner_key", "TEXT")?;
    ensure_column(conn, "panes", "last_activity_at", "TEXT")?;

    Ok(())
}

fn next_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("dcc-{now:x}")
}

fn value_ref_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Value::from(f),
        ValueRef::Text(t) => Value::from(String::from_utf8_lossy(t).to_string()),
        ValueRef::Blob(_) => Value::Null,
    }
}

fn row_to_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let mut obj = serde_json::Map::new();
    let row_ref = row.as_ref();
    for i in 0..row_ref.column_count() {
        let name = row_ref
            .column_name(i)
            .map(ToString::to_string)
            .unwrap_or_else(|_| format!("col_{i}"));
        let raw = row.get_ref(i)?;
        obj.insert(name, value_ref_to_json(raw));
    }
    Ok(Value::Object(obj))
}

fn map_project_to_renderer(mut row: Value) -> Value {
    let Some(obj) = row.as_object_mut() else {
        return row;
    };
    let mut out = serde_json::Map::new();
    out.insert("id".into(), obj.remove("id").unwrap_or(Value::Null));
    out.insert("name".into(), obj.remove("name").unwrap_or(Value::Null));
    out.insert("path".into(), obj.remove("path").unwrap_or(Value::Null));
    out.insert("description".into(), obj.remove("description").unwrap_or(Value::Null));
    out.insert(
        "defaultProviderId".into(),
        obj.remove("default_provider_id").unwrap_or(Value::Null),
    );
    out.insert(
        "gitRemoteUrl".into(),
        obj.remove("git_remote_url").unwrap_or(Value::Null),
    );
    out.insert(
        "lastOpenedAt".into(),
        obj.remove("last_opened_at").unwrap_or(Value::Null),
    );
    out.insert("createdAt".into(), obj.remove("created_at").unwrap_or(Value::Null));
    out.insert("updatedAt".into(), obj.remove("updated_at").unwrap_or(Value::Null));
    Value::Object(out)
}

fn map_comb_to_renderer(mut row: Value) -> Value {
    let Some(obj) = row.as_object_mut() else {
        return row;
    };
    let mut review_targets_v = Value::Null;
    if let Some(v) = obj.remove("review_targets") {
        review_targets_v = match v {
            Value::String(ref s) if !s.trim().is_empty() => {
                serde_json::from_str::<Value>(s).unwrap_or(Value::Null)
            }
            Value::Null => Value::Null,
            other => other,
        };
    }
    let mut out = serde_json::Map::new();
    out.insert("id".into(), obj.remove("id").unwrap_or(Value::Null));
    out.insert("projectId".into(), obj.remove("project_id").unwrap_or(Value::Null));
    out.insert("name".into(), obj.remove("name").unwrap_or(Value::Null));
    out.insert("description".into(), obj.remove("description").unwrap_or(Value::Null));
    out.insert("baseBranch".into(), obj.remove("base_branch").unwrap_or(Value::Null));
    out.insert("branch".into(), obj.remove("branch").unwrap_or(Value::Null));
    out.insert("worktreePath".into(), obj.remove("worktree_path").unwrap_or(Value::Null));
    out.insert("reviewTargets".into(), review_targets_v);
    out.insert("status".into(), obj.remove("status").unwrap_or(Value::Null));
    out.insert("lastOpenedAt".into(), obj.remove("last_opened_at").unwrap_or(Value::Null));
    out.insert("createdAt".into(), obj.remove("created_at").unwrap_or(Value::Null));
    out.insert("updatedAt".into(), obj.remove("updated_at").unwrap_or(Value::Null));
    Value::Object(out)
}

fn map_pane_to_renderer(mut row: Value) -> Value {
    let Some(obj) = row.as_object_mut() else {
        return row;
    };
    let mut out = serde_json::Map::new();
    out.insert("id".into(), obj.remove("id").unwrap_or(Value::Null));
    out.insert("combId".into(), obj.remove("comb_id").unwrap_or(Value::Null));
    out.insert("type".into(), obj.remove("type").unwrap_or(Value::Null));
    out.insert("providerId".into(), obj.remove("provider_id").unwrap_or(Value::Null));
    out.insert("title".into(), obj.remove("title").unwrap_or(Value::Null));
    out.insert("initialPrompt".into(), obj.remove("initial_prompt").unwrap_or(Value::Null));
    out.insert("cwd".into(), obj.remove("cwd").unwrap_or(Value::Null));
    out.insert("ptyOwnerKey".into(), obj.remove("pty_owner_key").unwrap_or(Value::Null));
    out.insert("status".into(), obj.remove("status").unwrap_or(Value::Null));
    out.insert(
        "layoutOrder".into(),
        obj.remove("layout_order").unwrap_or_else(|| Value::from(0)),
    );
    out.insert("lastActivityAt".into(), obj.remove("last_activity_at").unwrap_or(Value::Null));
    out.insert("createdAt".into(), obj.remove("created_at").unwrap_or(Value::Null));
    out.insert("updatedAt".into(), obj.remove("updated_at").unwrap_or(Value::Null));
    Value::Object(out)
}

const PROVIDER_SELECT_SQL: &str = "SELECT id, name, type, api_key, api_key_encrypted, cli_path, config, is_active, created_at, updated_at FROM providers";

fn provider_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let typ: String = row.get(2)?;
    let api_key: Option<String> = row.get(3)?;
    let api_key_encrypted: Option<Vec<u8>> = row.get(4)?;
    let cli_path: Option<String> = row.get(5)?;
    let config_str: Option<String> = row.get(6)?;
    let is_active: i64 = row.get(7)?;
    let created_at: String = row.get(8)?;
    let updated_at: String = row.get(9)?;

    let has_api_key = api_key.is_some()
        || api_key_encrypted
            .as_ref()
            .map(|b| !b.is_empty())
            .unwrap_or(false);
    let config: Value = config_str
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or(Value::Null);

    Ok(serde_json::json!({
        "id": id,
        "name": name,
        "type": typ,
        "apiKey": api_key,
        "apiKeyEncrypted": null,
        "hasApiKey": has_api_key,
        "cliPath": cli_path,
        "config": config,
        "isActive": is_active != 0,
        "createdAt": created_at,
        "updatedAt": updated_at,
    }))
}

fn run_git(cwd: &str, args: &[&str]) -> ApiResult<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| db_error(e.to_string()))?;
    if !output.status.success() {
        return Err(db_error(String::from_utf8_lossy(&output.stderr).to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Branches locais (`refs/heads/`), ordenadas, sem duplicatas.
fn git_local_branch_names(project_path: &str) -> ApiResult<Vec<String>> {
    let raw = run_git(
        project_path,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    )?;
    let mut branches: Vec<String> = raw
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    branches.sort();
    branches.dedup();
    Ok(branches)
}

fn git_current_branch_impl(project_path: &str) -> ApiResult<String> {
    Ok(run_git(project_path, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string())
}

fn parse_git_status_porcelain(raw: &str) -> Vec<(String, String)> {
    raw.lines()
        .filter(|l| !l.trim().is_empty() && l.len() > 3)
        .map(|line| {
            let status = line[0..2].trim().to_string();
            let path = line[3..].trim().to_string();
            (status, path)
        })
        .collect()
}

fn count_diff_stats(diff: &str) -> (i64, i64) {
    let mut ins = 0i64;
    let mut del = 0i64;
    for line in diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            ins += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            del += 1;
        }
    }
    (ins, del)
}

fn build_review_diffs_for_path(target_path: &str) -> ApiResult<Value> {
    let status_raw = run_git(target_path, &["status", "--porcelain"])?;
    let files_meta = parse_git_status_porcelain(&status_raw);
    let mut files = Vec::new();
    let mut insertions = 0i64;
    let mut deletions = 0i64;

    for (git_status, path) in &files_meta {
        let is_untracked = git_status.contains('?');
        let diff = if is_untracked {
            run_git(target_path, &["diff", "--binary", "--no-index", "--", "/dev/null", path])
                .unwrap_or_default()
        } else {
            run_git(target_path, &["diff", "HEAD", "--", path]).unwrap_or_default()
        };
        let (ins, del) = count_diff_stats(&diff);
        insertions += ins;
        deletions += del;
        let status = if is_untracked {
            "untracked"
        } else if !git_status.is_empty() && git_status.chars().next().unwrap_or(' ') != ' ' {
            "staged"
        } else {
            "modified"
        };
        files.push(serde_json::json!({
            "path": path,
            "status": status,
            "diff": diff
        }));
    }

    Ok(serde_json::json!({
      "success": true,
      "files": files,
      "summary": {
        "changedFiles": files_meta.len(),
        "insertions": insertions,
        "deletions": deletions
      }
    }))
}

fn safe_branch_name(prefix: &str, raw_id: &str, raw_title: &str) -> String {
    let title = raw_title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let slug = if title.is_empty() { "item".to_string() } else { title };
    let id_suffix_source = raw_id.rsplit_once('-').map(|(_, tail)| tail).unwrap_or(raw_id);
    let id_short = id_suffix_source
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(8)
        .collect::<String>()
        .to_lowercase();
    let suffix = if id_short.is_empty() {
        "item".to_string()
    } else {
        id_short
    };
    format!("{prefix}-{slug}-{suffix}")
}

fn iso_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

#[allow(dead_code)]
fn copy_paths_from_worktree(
    worktree_root: &str,
    project_root: &str,
    relative_paths: &[String],
) -> ApiResult<()> {
    for rel in relative_paths {
        let src = PathBuf::from(worktree_root).join(rel);
        let dst = PathBuf::from(project_root).join(rel);
        if src.exists() {
            if src.is_dir() {
                continue;
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| db_error(e.to_string()))?;
            }
            std::fs::copy(&src, &dst).map_err(|e| db_error(e.to_string()))?;
        } else if dst.exists() {
            std::fs::remove_file(&dst).map_err(|e| db_error(e.to_string()))?;
        }
    }
    Ok(())
}

// ---------- App ----------
#[tauri::command]
fn app_get_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
fn app_check_for_updates() -> ApiResult<Value> {
    mapped_not_implemented("app:checkForUpdates")
}

#[tauri::command]
fn app_quit_and_install() -> ApiResult<Value> {
    mapped_not_implemented("app:quitAndInstall")
}

#[tauri::command]
fn app_show_notification(_payload: Value) -> ApiResult<Value> {
    mapped_not_implemented("app:showNotification")
}

// ---------- Dialog / Shell / Window ----------
#[tauri::command]
async fn dialog_select_directory(app: AppHandle) -> ApiResult<Value> {
    // Não usar `blocking_pick_*`: bloqueia o loop de eventos e o painel nativo fica inutilizável.
    let (tx, rx) = std::sync::mpsc::channel::<Option<FilePath>>();
    app.dialog()
        .file()
        .set_title("Selecionar pasta do repositório")
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });
    let folder = tauri::async_runtime::spawn_blocking(move || match rx.recv() {
        Ok(path) => path,
        Err(_) => None,
    })
    .await
    .map_err(|e| db_error(e.to_string()))?;
    Ok(match folder {
        Some(path) => Value::String(path.to_string()),
        None => Value::Null,
    })
}

#[tauri::command]
fn dialog_show_message(_options: Value) -> ApiResult<Value> {
    mapped_not_implemented("dialog:showMessage")
}

#[tauri::command]
fn dialog_confirm(_message: String) -> ApiResult<bool> {
    Err(ApiError::not_implemented("dialog:confirm"))
}

// ---------- CLI path (PATH ampliado: apps GUI no macOS costumam ter PATH mínimo) ----------
fn home_dir_path() -> Option<PathBuf> {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    #[cfg(windows)]
    if let Ok(h) = std::env::var("USERPROFILE") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    None
}

fn expand_user_path(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(h) = home_dir_path() {
            return h.join(rest);
        }
    }
    PathBuf::from(s)
}

fn path_looks_like_filesystem_path(s: &str) -> bool {
    s.contains('/')
        || s.contains('\\')
        || (cfg!(windows) && s.len() >= 2 && s.as_bytes()[1] == b':')
}

fn path_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |p: PathBuf| {
        let key = p.to_string_lossy().to_string();
        if seen.insert(key) {
            dirs.push(p);
        }
    };
    if let Some(h) = home_dir_path() {
        push(h.join(".local").join("bin"));
        push(h.join(".cargo").join("bin"));
    }
    #[cfg(target_os = "macos")]
    {
        push(PathBuf::from("/opt/homebrew/bin"));
        push(PathBuf::from("/usr/local/bin"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        push(PathBuf::from("/usr/local/bin"));
    }
    push(PathBuf::from("/usr/bin"));
    push(PathBuf::from("/bin"));
    let sep = if cfg!(windows) { ';' } else { ':' };
    if let Ok(p) = std::env::var("PATH") {
        for part in p.split(sep) {
            if !part.is_empty() {
                push(PathBuf::from(part));
            }
        }
    }
    dirs
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(windows)]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn search_in_augmented_path(command: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let exts: &[&str] = &[".exe", ".cmd", ".bat", ".EXE", ""];
    #[cfg(not(windows))]
    let exts: &[&str] = &[""];

    for dir in path_search_dirs() {
        for ext in exts {
            let name = if ext.is_empty() {
                command.to_string()
            } else {
                format!("{command}{ext}")
            };
            let full = dir.join(&name);
            if full.is_file() && is_executable_file(&full) {
                return std::fs::canonicalize(&full).ok().or(Some(full));
            }
        }
    }
    None
}

fn resolve_cli_path_impl(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    if path_looks_like_filesystem_path(command) {
        let probe = expand_user_path(command);
        if probe.exists() && probe.is_file() {
            let canon = std::fs::canonicalize(&probe).unwrap_or(probe);
            if is_executable_file(&canon) {
                return Some(canon);
            }
        }
        return None;
    }
    search_in_augmented_path(command)
}

fn provider_cli_command(provider_type: &str) -> Option<&'static str> {
    match provider_type {
        "claude-code" => Some("claude"),
        "codex" => Some("codex"),
        "cursor" => Some("agent"),
        "gemini" => Some("gemini"),
        _ => None,
    }
}

fn try_cli_invocation(path: &Path) -> bool {
    for args in [&["--version"][..], &["-V"][..], &["-v"][..], &["--help"][..]] {
        if let Ok(out) = Command::new(path).args(args).output() {
            if out.status.success() {
                return true;
            }
        }
    }
    false
}

#[tauri::command]
fn shell_open_external(_url: String) -> ApiResult<Value> {
    mapped_not_implemented("shell:openExternal")
}

#[tauri::command]
fn shell_open_path(_path: String) -> ApiResult<Value> {
    mapped_not_implemented("shell:openPath")
}

#[tauri::command]
fn shell_show_item_in_folder(_path: String) -> ApiResult<Value> {
    mapped_not_implemented("shell:showItemInFolder")
}

#[tauri::command]
fn shell_resolve_cli_path(command: String) -> ApiResult<Value> {
    let path = resolve_cli_path_impl(&command);
    let path_str = path.as_ref().map(|p| p.to_string_lossy().to_string());
    Ok(serde_json::json!({ "path": path_str }))
}

#[tauri::command]
fn shell_detect_cli_for_provider(provider_type: String) -> ApiResult<Value> {
    let Some(cmd) = provider_cli_command(provider_type.trim()) else {
        return Ok(serde_json::json!({ "path": null }));
    };
    let path = resolve_cli_path_impl(cmd);
    let path_str = path.as_ref().map(|p| p.to_string_lossy().to_string());
    Ok(serde_json::json!({ "path": path_str }))
}

#[tauri::command]
fn shell_validate_cli_path(_provider_type: String, cli_path: String) -> ApiResult<Value> {
    let trimmed = cli_path.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::json!({
            "valid": false,
            "message": "Caminho vazio"
        }));
    }
    let candidate = resolve_cli_path_impl(trimmed).unwrap_or_else(|| expand_user_path(trimmed));
    if !candidate.exists() {
        return Ok(serde_json::json!({
            "valid": false,
            "message": "Executável não encontrado"
        }));
    }
    if !candidate.is_file() || !is_executable_file(&candidate) {
        return Ok(serde_json::json!({
            "valid": false,
            "message": "Não é um executável válido"
        }));
    }
    if try_cli_invocation(&candidate) {
        return Ok(serde_json::json!({ "valid": true }));
    }
    Ok(serde_json::json!({
        "valid": false,
        "message": "Executável encontrado, mas não respondeu a --version / --help"
    }))
}

#[tauri::command]
fn shell_open_terminal_at_path(
    dir_path: String,
    _suggested_command: Option<Value>,
) -> ApiResult<Value> {
    let path = expand_user_path(dir_path.trim());
    if !path.exists() || !path.is_dir() {
        return Ok(serde_json::json!({
            "success": false,
            "error": format!("Diretório inválido: {}", path.display())
        }));
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg("-a")
            .arg("Terminal")
            .arg(&path)
            .status()
            .map_err(|e| ApiError {
                code: "SHELL_ERROR",
                message: format!("Falha ao abrir Terminal.app: {e}"),
            })?;

        if status.success() {
            return Ok(serde_json::json!({ "success": true }));
        }
        return Ok(serde_json::json!({
            "success": false,
            "error": "Terminal.app retornou erro ao abrir o diretório"
        }));
    }

    #[cfg(target_os = "linux")]
    {
        let attempts: &[(&str, &[&str])] = &[
            ("x-terminal-emulator", &["--working-directory"]),
            ("gnome-terminal", &["--working-directory"]),
            ("konsole", &["--workdir"]),
            ("xfce4-terminal", &["--working-directory"]),
        ];
        for (bin, flag) in attempts {
            let status = Command::new(bin).arg(flag[0]).arg(&path).status();
            if let Ok(s) = status {
                if s.success() {
                    return Ok(serde_json::json!({ "success": true }));
                }
            }
        }
        return Ok(serde_json::json!({
            "success": false,
            "error": "Nenhum terminal suportado encontrado no Linux"
        }));
    }

    #[cfg(target_os = "windows")]
    {
        let status = Command::new("cmd")
            .args(["/C", "start", "wt", "-d"])
            .arg(&path)
            .status()
            .map_err(|e| ApiError {
                code: "SHELL_ERROR",
                message: format!("Falha ao abrir Windows Terminal: {e}"),
            })?;
        if status.success() {
            return Ok(serde_json::json!({ "success": true }));
        }
        return Ok(serde_json::json!({
            "success": false,
            "error": "Windows Terminal retornou erro ao abrir o diretório"
        }));
    }

    #[allow(unreachable_code)]
    Ok(serde_json::json!({
        "success": false,
        "error": "Plataforma não suportada para abertura de terminal"
    }))
}

#[tauri::command]
fn window_minimize() -> ApiResult<Value> {
    mapped_not_implemented("window:minimize")
}

#[tauri::command]
fn window_maximize() -> ApiResult<Value> {
    mapped_not_implemented("window:maximize")
}

#[tauri::command]
fn window_close() -> ApiResult<Value> {
    mapped_not_implemented("window:close")
}

#[tauri::command]
fn window_is_maximized() -> ApiResult<bool> {
    Err(ApiError::not_implemented("window:isMaximized"))
}

// ---------- License ----------
#[tauri::command]
fn license_get_status() -> ApiResult<Value> {
    // Stub: UI de licença não bloqueia dev; integração real pode preencher depois.
    Ok(serde_json::json!({
        "activated": true,
        "tier": "dev"
    }))
}

#[tauri::command]
fn license_get_machine_id() -> ApiResult<String> {
    Err(ApiError::not_implemented("license:getMachineId"))
}

#[tauri::command]
fn license_activate(_email: String) -> ApiResult<Value> {
    mapped_not_implemented("license:activate")
}

#[tauri::command]
fn license_skip_activation() -> ApiResult<Value> {
    mapped_not_implemented("license:skipActivation")
}

// ---------- Providers / Projects / Missions / Logs / Combs / Panes ----------
#[tauri::command]
fn db_providers_find_all(state: State<'_, AppState>) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!("{PROVIDER_SELECT_SQL} ORDER BY name ASC LIMIT 100 OFFSET 0"))
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map([], provider_from_row)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(items.map_err(|e| db_error(e.to_string()))?))
}

#[tauri::command]
fn db_providers_find_by_id(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!("{PROVIDER_SELECT_SQL} WHERE id = ?1"))
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], provider_from_row)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(row.unwrap_or(Value::Null))
}

#[tauri::command]
fn db_providers_find_by_type(state: State<'_, AppState>, kind: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!("{PROVIDER_SELECT_SQL} WHERE type = ?1 ORDER BY name ASC"))
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map(params![kind], provider_from_row)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(items.map_err(|e| db_error(e.to_string()))?))
}

#[tauri::command]
fn db_providers_find_active(state: State<'_, AppState>) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!(
            "{PROVIDER_SELECT_SQL} WHERE is_active = 1 ORDER BY name ASC"
        ))
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map([], provider_from_row)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(items.map_err(|e| db_error(e.to_string()))?))
}

#[tauri::command]
fn db_providers_create(state: State<'_, AppState>, data: Value) -> ApiResult<Value> {
    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid provider payload"))?;
    let id = Uuid::new_v4().to_string();
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("name is required"))?;
    let typ = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("type is required"))?;
    let api_key = obj
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let cli_path = obj
        .get("cliPath")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let config_str: Option<String> = match obj.get("config") {
        None | Some(Value::Null) => None,
        Some(v) => Some(serde_json::to_string(v).map_err(|e| db_error(e.to_string()))?),
    };
    let is_active = obj
        .get("isActive")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let is_active_i: i64 = if is_active { 1 } else { 0 };

    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "INSERT INTO providers (id, name, type, api_key, api_key_encrypted, cli_path, config, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                name,
                typ,
                api_key,
                None::<Vec<u8>>,
                cli_path,
                config_str,
                is_active_i
            ],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_providers_find_by_id(state, id)
}

#[tauri::command]
fn db_providers_update(state: State<'_, AppState>, id: String, data: Value) -> ApiResult<Value> {
    use rusqlite::types::Value as SqlValue;

    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid provider payload"))?;
    let mut sets: Vec<String> = Vec::new();
    let mut bind: Vec<SqlValue> = Vec::new();

    if let Some(name) = obj.get("name").and_then(Value::as_str) {
        sets.push("name = ?".into());
        bind.push(SqlValue::Text(name.to_string()));
    }
    if let Some(typ) = obj.get("type").and_then(Value::as_str) {
        sets.push("type = ?".into());
        bind.push(SqlValue::Text(typ.to_string()));
    }
    if obj.contains_key("apiKey") {
        sets.push("api_key = ?".into());
        let plain = obj.get("apiKey").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        });
        bind.push(match plain {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
        sets.push("api_key_encrypted = ?".into());
        bind.push(SqlValue::Null);
    }
    if obj.contains_key("cliPath") {
        sets.push("cli_path = ?".into());
        let v = obj.get("cliPath").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) if !s.is_empty() => SqlValue::Text(s),
            _ => SqlValue::Null,
        });
    }
    if obj.contains_key("config") {
        sets.push("config = ?".into());
        let cfg = match obj.get("config") {
            None | Some(Value::Null) => SqlValue::Null,
            Some(v) => {
                let s = serde_json::to_string(v).map_err(|e| db_error(e.to_string()))?;
                SqlValue::Text(s)
            }
        };
        bind.push(cfg);
    }
    if let Some(ia) = obj.get("isActive").and_then(Value::as_bool) {
        sets.push("is_active = ?".into());
        bind.push(SqlValue::Integer(if ia { 1 } else { 0 }));
    }

    if sets.is_empty() {
        return db_providers_find_by_id(state, id);
    }

    bind.push(SqlValue::Text(id.clone()));
    let sql = format!("UPDATE providers SET {} WHERE id = ?", sets.join(", "));
    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(&sql, params_from_iter(bind))
            .map_err(|e| db_error(e.to_string()))?;
    }
    db_providers_find_by_id(state, id)
}

#[tauri::command]
fn db_providers_delete(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let changed = conn
        .execute("DELETE FROM providers WHERE id = ?1", params![id])
        .map_err(|e| db_error(e.to_string()))?;
    Ok(Value::Bool(changed > 0))
}

#[tauri::command]
fn db_providers_set_active(
    state: State<'_, AppState>,
    id: String,
    is_active: bool,
) -> ApiResult<Value> {
    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "UPDATE providers SET is_active = ?1 WHERE id = ?2",
            params![if is_active { 1 } else { 0 }, id],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_providers_find_by_id(state, id)
}

#[tauri::command]
fn db_providers_test_connection(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!("{PROVIDER_SELECT_SQL} WHERE id = ?1"))
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], provider_from_row)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    let Some(provider) = row else {
        return Ok(serde_json::json!({
            "success": false,
            "error": "Provider not found"
        }));
    };
    let typ = provider
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let has_key = provider
        .get("hasApiKey")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cli_path = provider
        .get("cliPath")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    let ok = match typ {
        "openai" | "anthropic" => has_key,
        "claude-code" | "codex" | "cursor" | "gemini" => cli_path.is_some(),
        _ => true,
    };
    if ok {
        Ok(serde_json::json!({ "success": true }))
    } else {
        Ok(serde_json::json!({
            "success": false,
            "error": "Provider not fully configured"
        }))
    }
}

#[tauri::command]
fn db_providers_is_encryption_available() -> ApiResult<bool> {
    Ok(false)
}

/// Fragmento dinâmico `coluna = ?` para UPDATEs (evita closure que captura `sets`/`bind_values`).
#[allow(dead_code)]
fn push_sql_set(
    column: &'static str,
    value: Option<String>,
    sets: &mut Vec<String>,
    bind_values: &mut Vec<String>,
) {
    if let Some(v) = value {
        sets.push(format!("{column} = ?"));
        bind_values.push(v);
    }
}

#[tauri::command]
fn db_projects_find_all(state: State<'_, AppState>) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(
            "SELECT * FROM projects
             ORDER BY (last_opened_at IS NULL), last_opened_at DESC
             LIMIT 100 OFFSET 0",
        )
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map([], row_to_json)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    let mapped = items
        .map_err(|e| db_error(e.to_string()))?
        .into_iter()
        .map(map_project_to_renderer)
        .collect::<Vec<_>>();
    Ok(Value::Array(mapped))
}
#[tauri::command]
fn db_projects_find_by_id(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT * FROM projects WHERE id = ?1")
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], row_to_json)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(row.map(map_project_to_renderer).unwrap_or(Value::Null))
}
#[tauri::command]
fn db_projects_find_by_path(state: State<'_, AppState>, path: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT * FROM projects WHERE path = ?1")
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![path], row_to_json)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(row.map(map_project_to_renderer).unwrap_or(Value::Null))
}
#[tauri::command]
fn db_projects_search(state: State<'_, AppState>, query: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let term = format!("%{query}%");
    let mut stmt = conn
        .prepare(
            "SELECT * FROM projects
             WHERE name LIKE ?1 OR description LIKE ?1 OR path LIKE ?1
             ORDER BY (last_opened_at IS NULL), last_opened_at DESC
             LIMIT 20",
        )
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map(params![term], row_to_json)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(
        items
            .map_err(|e| db_error(e.to_string()))?
            .into_iter()
            .map(map_project_to_renderer)
            .collect(),
    ))
}
#[tauri::command]
fn db_projects_create(state: State<'_, AppState>, data: Value) -> ApiResult<Value> {
    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid project payload"))?;
    let id = next_id();
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("name is required"))?;
    let path = obj
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("path is required"))?;
    let description = obj.get("description").and_then(Value::as_str);
    let default_provider_id = obj.get("defaultProviderId").and_then(Value::as_str);
    let git_remote_url = obj.get("gitRemoteUrl").and_then(Value::as_str);

    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "INSERT INTO projects (id, name, path, description, default_provider_id, git_remote_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, name, path, description, default_provider_id, git_remote_url],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_projects_find_by_id(state, id)
}
#[tauri::command]
fn db_projects_update(state: State<'_, AppState>, id: String, data: Value) -> ApiResult<Value> {
    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid project payload"))?;
    let mut sets = Vec::new();
    let mut values: Vec<String> = Vec::new();

    if let Some(name) = obj.get("name").and_then(Value::as_str) {
        sets.push("name = ?");
        values.push(name.to_string());
    }
    if obj.get("description").is_some() {
        sets.push("description = ?");
        values.push(
            obj.get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        );
    }
    if obj.get("defaultProviderId").is_some() {
        sets.push("default_provider_id = ?");
        values.push(
            obj.get("defaultProviderId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        );
    }
    if obj.get("gitRemoteUrl").is_some() {
        sets.push("git_remote_url = ?");
        values.push(
            obj.get("gitRemoteUrl")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        );
    }
    if sets.is_empty() {
        return db_projects_find_by_id(state, id);
    }

    let mut sql = format!("UPDATE projects SET {} WHERE id = ?", sets.join(", "));
    sql.push_str("");
    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        let mut dyn_params = values.iter().map(|v| v.as_str()).collect::<Vec<_>>();
        dyn_params.push(id.as_str());
        conn.execute(&sql, params_from_iter(dyn_params))
            .map_err(|e| db_error(e.to_string()))?;
    }
    db_projects_find_by_id(state, id)
}
#[tauri::command]
fn db_projects_delete(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let changed = conn
        .execute("DELETE FROM projects WHERE id = ?1", params![id])
        .map_err(|e| db_error(e.to_string()))?;
    Ok(Value::Bool(changed > 0))
}
#[tauri::command]
fn db_projects_get_stats(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(
            "SELECT
                COUNT(*) as total,
                SUM(CASE WHEN status = 'applied' THEN 1 ELSE 0 END) as applied,
                SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END) as active,
                SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END) as failed
             FROM combs WHERE project_id = ?1",
        )
        .map_err(|e| db_error(e.to_string()))?;
    let value = stmt
        .query_row(params![id], |r| {
            Ok(serde_json::json!({
              "totalWorkspaces": r.get::<_, i64>(0).unwrap_or(0),
              "appliedWorkspaces": r.get::<_, i64>(1).unwrap_or(0),
              "activeWorkspaces": r.get::<_, i64>(2).unwrap_or(0),
              "failedWorkspaces": r.get::<_, i64>(3).unwrap_or(0),
            }))
        })
        .map_err(|e| db_error(e.to_string()))?;
    Ok(value)
}
#[tauri::command]
fn db_projects_update_last_opened(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "UPDATE projects SET last_opened_at = datetime('now') WHERE id = ?1",
            params![id.clone()],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_projects_find_by_id(state, id)
}

#[tauri::command]
fn db_combs_find_by_project(state: State<'_, AppState>, project_id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(
            "SELECT * FROM combs WHERE project_id = ?1
             ORDER BY (last_opened_at IS NULL), last_opened_at DESC
             LIMIT 50",
        )
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map(params![project_id], row_to_json)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(
        items
            .map_err(|e| db_error(e.to_string()))?
            .into_iter()
            .map(map_comb_to_renderer)
            .collect(),
    ))
}
#[tauri::command]
fn db_combs_find_by_id(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT * FROM combs WHERE id = ?1")
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], row_to_json)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(row.map(map_comb_to_renderer).unwrap_or(Value::Null))
}
#[tauri::command]
fn db_combs_create(state: State<'_, AppState>, data: Value) -> ApiResult<Value> {
    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid comb payload"))?;
    let id = next_id();
    let project_id = obj
        .get("projectId")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("projectId is required"))?;
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("name is required"))?;
    let description = obj.get("description").and_then(Value::as_str);
    let base_branch = obj
        .get("baseBranch")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("baseBranch is required"))?;

    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "INSERT INTO combs (id, project_id, name, description, base_branch, review_targets, status, last_opened_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'active', datetime('now'), datetime('now'), datetime('now'))",
            params![id, project_id, name, description, base_branch],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_combs_find_by_id(state, id)
}
#[tauri::command]
fn db_combs_update(state: State<'_, AppState>, id: String, data: Value) -> ApiResult<Value> {
    use rusqlite::types::Value as SqlValue;

    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid comb payload"))?;
    let mut sets: Vec<String> = Vec::new();
    let mut bind: Vec<SqlValue> = Vec::new();

    if let Some(name) = obj.get("name").and_then(Value::as_str) {
        sets.push("name = ?".into());
        bind.push(SqlValue::Text(name.to_string()));
    }
    if obj.contains_key("description") {
        sets.push("description = ?".into());
        let v = obj.get("description").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("branch") {
        sets.push("branch = ?".into());
        let v = obj.get("branch").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("worktreePath") {
        sets.push("worktree_path = ?".into());
        let v = obj.get("worktreePath").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("reviewTargets") {
        sets.push("review_targets = ?".into());
        let cfg = match obj.get("reviewTargets") {
            None | Some(Value::Null) => SqlValue::Null,
            Some(Value::Array(a)) if a.is_empty() => SqlValue::Null,
            Some(v) => {
                let s = serde_json::to_string(v).map_err(|e| db_error(e.to_string()))?;
                SqlValue::Text(s)
            }
        };
        bind.push(cfg);
    }
    if let Some(st) = obj.get("status").and_then(Value::as_str) {
        sets.push("status = ?".into());
        bind.push(SqlValue::Text(st.to_string()));
    }
    if obj.contains_key("lastOpenedAt") {
        sets.push("last_opened_at = ?".into());
        let v = obj.get("lastOpenedAt").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }

    if sets.is_empty() {
        return db_combs_find_by_id(state, id);
    }

    bind.push(SqlValue::Text(id.clone()));
    let sql = format!("UPDATE combs SET {} WHERE id = ?", sets.join(", "));
    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(&sql, params_from_iter(bind))
            .map_err(|e| db_error(e.to_string()))?;
    }
    db_combs_find_by_id(state, id)
}
#[tauri::command]
fn db_combs_delete(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let changed = conn
        .execute("DELETE FROM combs WHERE id = ?1", params![id])
        .map_err(|e| db_error(e.to_string()))?;
    Ok(Value::Bool(changed > 0))
}

#[tauri::command]
fn db_panes_find_by_comb(state: State<'_, AppState>, comb_id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT * FROM panes WHERE comb_id = ?1 ORDER BY layout_order ASC")
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map(params![comb_id], row_to_json)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(
        items
            .map_err(|e| db_error(e.to_string()))?
            .into_iter()
            .map(map_pane_to_renderer)
            .collect(),
    ))
}
#[tauri::command]
fn db_panes_find_by_id(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT * FROM panes WHERE id = ?1")
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], row_to_json)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(row.map(map_pane_to_renderer).unwrap_or(Value::Null))
}
#[tauri::command]
fn db_panes_create(state: State<'_, AppState>, data: Value) -> ApiResult<Value> {
    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid pane payload"))?;
    let id = next_id();
    let comb_id = obj
        .get("combId")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("combId is required"))?;
    let typ = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("type is required"))?;
    let provider_id = obj.get("providerId").and_then(Value::as_str);
    let title = obj.get("title").and_then(Value::as_str);
    let initial_prompt = obj.get("initialPrompt").and_then(Value::as_str);
    let layout_order_opt = obj.get("layoutOrder").and_then(|v| v.as_i64());

    let layout_order: i64 = if let Some(lo) = layout_order_opt {
        lo
    } else {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn
            .query_row(
                "SELECT COALESCE(MAX(layout_order), -1) + 1 FROM panes WHERE comb_id = ?1",
                params![comb_id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
    };

    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "INSERT INTO panes (id, comb_id, type, provider_id, title, initial_prompt, status, layout_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'idle', ?7, datetime('now'), datetime('now'))",
            params![id, comb_id, typ, provider_id, title, initial_prompt, layout_order],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_panes_find_by_id(state, id)
}
#[tauri::command]
fn db_panes_update(state: State<'_, AppState>, id: String, data: Value) -> ApiResult<Value> {
    use rusqlite::types::Value as SqlValue;

    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid pane payload"))?;
    let mut sets: Vec<String> = Vec::new();
    let mut bind: Vec<SqlValue> = Vec::new();

    if obj.contains_key("title") {
        sets.push("title = ?".into());
        let v = obj.get("title").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("initialPrompt") {
        sets.push("initial_prompt = ?".into());
        let v = obj.get("initialPrompt").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("providerId") {
        sets.push("provider_id = ?".into());
        let v = obj.get("providerId").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("cwd") {
        sets.push("cwd = ?".into());
        let v = obj.get("cwd").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("ptyOwnerKey") {
        sets.push("pty_owner_key = ?".into());
        let v = obj.get("ptyOwnerKey").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if let Some(st) = obj.get("status").and_then(Value::as_str) {
        sets.push("status = ?".into());
        bind.push(SqlValue::Text(st.to_string()));
    }
    if let Some(lo) = obj.get("layoutOrder").and_then(|v| v.as_i64()) {
        sets.push("layout_order = ?".into());
        bind.push(SqlValue::Integer(lo));
    }
    if obj.contains_key("lastActivityAt") {
        sets.push("last_activity_at = ?".into());
        let v = obj.get("lastActivityAt").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }

    if sets.is_empty() {
        return db_panes_find_by_id(state, id);
    }

    bind.push(SqlValue::Text(id.clone()));
    let sql = format!("UPDATE panes SET {} WHERE id = ?", sets.join(", "));
    {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(&sql, params_from_iter(bind))
            .map_err(|e| db_error(e.to_string()))?;
    }
    db_panes_find_by_id(state, id)
}
#[tauri::command]
fn db_panes_delete(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let changed = conn
        .execute("DELETE FROM panes WHERE id = ?1", params![id])
        .map_err(|e| db_error(e.to_string()))?;
    Ok(Value::Bool(changed > 0))
}

// ---------- DB Utils ----------
#[tauri::command]
fn db_utils_get_path(state: State<'_, AppState>) -> String {
    state.db_path.to_string_lossy().to_string()
}

#[tauri::command]
fn db_utils_get_size(state: State<'_, AppState>) -> ApiResult<u64> {
    std::fs::metadata(state.db_path.as_path())
        .map(|m| m.len())
        .map_err(|e| ApiError {
            code: "DB_SIZE_ERROR",
            message: e.to_string(),
        })
}

#[tauri::command]
fn db_utils_backup(state: State<'_, AppState>, dest_path: String) -> ApiResult<bool> {
    std::fs::copy(state.db_path.as_path(), dest_path)
        .map(|_| true)
        .map_err(|e| ApiError {
            code: "DB_BACKUP_ERROR",
            message: e.to_string(),
        })
}

// ---------- Git / Worktree / Review ----------
#[tauri::command]
fn git_get_current_branch(project_path: String) -> ApiResult<String> {
    git_current_branch_impl(&project_path)
}
#[tauri::command]
fn git_get_local_branches(project_path: String) -> ApiResult<Value> {
    let branches = git_local_branch_names(&project_path)?;
    Ok(Value::Array(
        branches.into_iter().map(Value::String).collect(),
    ))
}
#[tauri::command]
fn git_get_review_diffs(worktree_path: String) -> ApiResult<Value> {
    match build_review_diffs_for_path(&worktree_path) {
        Ok(v) => Ok(v),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": e.message,
            "files": Value::Array(vec![]),
            "summary": Value::Null
        })),
    }
}
#[tauri::command]
fn review_get_diffs_bundle(worktree_paths: Vec<String>) -> ApiResult<Value> {
    let mut out = Vec::new();
    for worktree_path in worktree_paths {
        match build_review_diffs_for_path(&worktree_path) {
            Ok(payload) => {
                out.push(serde_json::json!({
                  "worktreePath": worktree_path,
                  "success": true,
                  "files": payload.get("files").cloned().unwrap_or(Value::Array(vec![])),
                  "summary": payload.get("summary").cloned().unwrap_or(Value::Null)
                }));
            }
            Err(e) => {
                out.push(serde_json::json!({
                  "worktreePath": worktree_path,
                  "success": false,
                  "error": e.message,
                  "files": [],
                  "summary": Value::Null
                }));
            }
        }
    }
    Ok(Value::Array(out))
}
#[tauri::command]
async fn comb_ensure_worktree(state: State<'_, AppState>, comb_id: String) -> ApiResult<Value> {
    let row: Option<(String, String, String, Option<String>, Option<String>)> = {
        let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
        conn
            .query_row(
                "SELECT c.name, p.path, c.base_branch, c.worktree_path, c.branch
                 FROM combs c JOIN projects p ON p.id = c.project_id
                 WHERE c.id = ?1",
                params![comb_id.clone()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .map_err(|e| db_error(e.to_string()))?
    };
    let Some((name, project_path, base_branch, existing_path, existing_branch)) = row else {
        return Ok(serde_json::json!({"success": false, "error": "Comb not found"}));
    };
    if let Some(p) = existing_path.clone() {
        return Ok(serde_json::json!({
            "success": true,
            "worktreePath": p,
            "worktreeBranch": existing_branch
        }));
    }

    let branch = safe_branch_name("dcc-comb", &comb_id, &name);
    let worktree_path = format!("{}/.dcc/worktrees/{}", project_path, branch);
    let from_ref = if base_branch.trim().is_empty() {
        "HEAD".to_string()
    } else {
        base_branch
    };

    let project_path_git = project_path.clone();
    let worktree_path_git = worktree_path.clone();
    let branch_git = branch.clone();
    let from_ref_git = from_ref.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let wt_parent = format!("{}/.dcc/worktrees", project_path_git);
        let _ = std::fs::create_dir_all(&wt_parent);
        run_git(
            &project_path_git,
            &[
                "worktree",
                "add",
                &worktree_path_git,
                "-b",
                &branch_git,
                &from_ref_git,
            ],
        )
    })
    .await
    .map_err(|e| db_error(e.to_string()))??;

    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    conn
        .execute(
            "UPDATE combs SET worktree_path = ?1, branch = ?2 WHERE id = ?3",
            params![worktree_path.clone(), branch.clone(), comb_id],
        )
        .map_err(|e| db_error(e.to_string()))?;
    Ok(serde_json::json!({
        "success": true,
        "worktreePath": worktree_path,
        "worktreeBranch": branch
    }))
}
#[tauri::command]
fn comb_discard(state: State<'_, AppState>, comb_id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let row: Option<(String, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT p.path, c.worktree_path, c.branch
             FROM combs c JOIN projects p ON p.id = c.project_id
             WHERE c.id = ?1",
            params![comb_id.clone()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    let Some((project_path, worktree_path, worktree_branch)) = row else {
        return Ok(serde_json::json!({"success": false, "error": "Comb not found"}));
    };
    drop(conn);
    if let Some(ref wp) = worktree_path {
        let _ = run_git(&project_path, &["worktree", "remove", "--force", wp]);
    }
    if let Some(wb) = worktree_branch {
        let _ = run_git(&project_path, &["branch", "-D", &wb]);
    }
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    conn
        .execute(
            "UPDATE combs SET worktree_path = NULL, branch = NULL, status = 'discarded' WHERE id = ?1",
            params![comb_id],
        )
        .map_err(|e| db_error(e.to_string()))?;
    Ok(serde_json::json!({"success": true}))
}
#[tauri::command]
fn comb_merge_into_main(_comb_id: String, _target_branch: Option<String>) -> ApiResult<Value> {
    mapped_not_implemented("comb:mergeIntoMain")
}
#[tauri::command]
fn comb_get_diffs(state: State<'_, AppState>, comb_id: String) -> ApiResult<Value> {
    let conn = state.conn.lock().map_err(|_| db_error("db lock poisoned"))?;
    let worktree_path: Option<String> = conn
        .query_row(
            "SELECT worktree_path FROM combs WHERE id = ?1",
            params![comb_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| db_error(e.to_string()))?
        .flatten();
    let Some(wp) = worktree_path else {
        return Ok(serde_json::json!({"success": false, "error": "Comb has no worktree", "files": [], "summary": Value::Null}));
    };
    build_review_diffs_for_path(&wp)
}
#[tauri::command]
fn comb_apply_patch(_comb_id: String, _target_branch: String, _options: Option<Value>) -> ApiResult<Value> {
    mapped_not_implemented("comb:applyPatch")
}

// ---------- Terminal ----------
#[tauri::command]
fn terminal_spawn(state: State<'_, AppState>, app: AppHandle, options: Value) -> ApiResult<Value> {
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

    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| db_error(e.to_string()))?;

    let reader = pty_pair.master.try_clone_reader().map_err(|e| db_error(e.to_string()))?;
    let writer = pty_pair.master.take_writer().map_err(|e| db_error(e.to_string()))?;
    
    let pty_id = format!("pty-{}", next_id());
    let stop_flag = Arc::new(AtomicBool::new(false));
    let output_buffer = new_terminal_output_buffer();
    
    let reader_thread = Some(spawn_terminal_reader_thread(
        reader,
        app.clone(),
        pty_id.clone(),
        stop_flag.clone(),
        output_buffer.clone(),
    ));

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
            mission_id: None,
            pane_id: None,
            cwd: cwd.to_string(),
            command: command.to_string(),
            args,
            started_at: iso_now(),
            stop_flag,
            output_buffer,
            reader_thread,
        },
    );

    Ok(serde_json::json!({ "ptyId": pty_id }))
}

fn terminal_session_json(pty_id: &str, t: &mut ManagedTerminal) -> Value {
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
      "status": status,
      "startedAt": t.started_at,
      "exitedAt": exited_at,
      "lastExitCode": exit_code
    })
}

fn spawn_terminal_reader_thread(
    mut reader: Box<dyn Read + Send>,
    app: AppHandle,
    pty_id: String,
    stop_flag: Arc<AtomicBool>,
    output_buffer: Arc<Mutex<VecDeque<String>>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                    append_terminal_line(&output_buffer, &data);
                    
                    // Simple pattern matching for "Attention"
                    // OSC 9;9; (Ghostty) or common waiting prompts
                    let needs_attention = data.contains("\x1b]9;9;") 
                        || data.contains("(y/n)?") 
                        || data.contains("Waiting for input...")
                        || data.contains("Continue? [Y/n]");
                    
                    if needs_attention {
                        let _ = app.emit(
                            "terminal-attention",
                            serde_json::json!({
                              "ptyId": pty_id,
                              "status": "waiting"
                            }),
                        );
                    }

                    let _ = app.emit(
                        "terminal-output",
                        serde_json::json!({
                          "ptyId": pty_id,
                          "data": data,
                          "stream": "stdout"
                        }),
                    );
                }
                Err(_) => break,
            }
        }
    })
}

#[tauri::command]
fn terminal_get_or_create(
    state: State<'_, AppState>,
    app: AppHandle,
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
            return Ok(serde_json::json!({ "ptyId": pty_id, "session": session }));
        }
    }

    // Drop lock before spawn to avoid deadlock or long stalls
    drop(terminals);
    
    let spawn_result = terminal_spawn(state.clone(), app, options)?;
    let pty_id = spawn_result.get("ptyId").and_then(Value::as_str).unwrap().to_string();
    
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    
    if let Some(t) = terminals.get_mut(&pty_id) {
        t.mission_id = Some(mission_id);
        let session = terminal_session_json(&pty_id, t);
        return Ok(serde_json::json!({ "ptyId": pty_id, "session": session }));
    }
    
    Err(db_error("Failed to re-acquire spawned terminal"))
}

#[tauri::command]
fn terminal_get_session(state: State<'_, AppState>, mission_id: String) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    if let Some((id, t)) = terminals
        .iter_mut()
        .find(|(_, t)| t.mission_id.as_deref() == Some(mission_id.as_str()))
    {
        return Ok(terminal_session_json(id, t));
    }
    Ok(Value::Null)
}

#[tauri::command]
fn terminal_write(state: State<'_, AppState>, pty_id: String, data: String) -> ApiResult<Value> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    
    if let Some(t) = terminals.get(&pty_id) {
        let mut writer = t.writer.lock().map_err(|_| db_error("writer lock poisoned"))?;
        writer
            .write_all(data.as_bytes())
            .map_err(|e| db_error(e.to_string()))?;
        return Ok(serde_json::json!({ "ok": true }));
    }
    Ok(serde_json::json!({ "ok": false }))
}

#[tauri::command]
fn terminal_resize(state: State<'_, AppState>, pty_id: String, cols: u16, rows: u16) -> ApiResult<Value> {
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

#[tauri::command]
fn terminal_kill(state: State<'_, AppState>, app: AppHandle, pty_id: String) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    
    if let Some(mut t) = terminals.remove(&pty_id) {
        t.stop_flag.store(true, Ordering::Relaxed);
        // On some OSs, closing the pty master will signal the child
        drop(t.pty_master); 
        if let Some(reader_thread) = t.reader_thread.take() {
            let _ = reader_thread.join();
        }
        
        let _ = app.emit(
            "terminal-exit",
            serde_json::json!({ "ptyId": pty_id, "code": -1 }),
        );
        return Ok(serde_json::json!({ "ok": true }));
    }
    Ok(serde_json::json!({ "ok": false }))
}

#[tauri::command]
fn terminal_kill_by_mission_id(
    state: State<'_, AppState>,
    app: AppHandle,
    mission_id: String,
) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    
    let ids = terminals
        .iter()
        .filter(|(_, t)| t.mission_id.as_deref() == Some(mission_id.as_str()))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    
    for id in ids {
        if let Some(mut t) = terminals.remove(&id) {
            t.stop_flag.store(true, Ordering::Relaxed);
            drop(t.pty_master);
            if let Some(reader_thread) = t.reader_thread.take() {
                let _ = reader_thread.join();
            }
            let _ = app.emit("terminal-exit", serde_json::json!({ "ptyId": id, "code": -1 }));
        }
    }
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
fn terminal_get_or_create_for_pane(
    state: State<'_, AppState>,
    app: AppHandle,
    pane_id: String,
    options: Value,
) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    
    let existing_id = terminals
        .iter_mut()
        .find(|(_, t)| t.pane_id.as_deref() == Some(pane_id.as_str()))
        .map(|(id, _)| id.clone());

    if let Some(pty_id) = existing_id {
        if let Some(t) = terminals.get_mut(&pty_id) {
            let session = terminal_session_json(&pty_id, t);
            return Ok(serde_json::json!({ "ptyId": pty_id, "session": session }));
        }
    }

    drop(terminals);
    
    let spawn_result = terminal_spawn(state.clone(), app, options)?;
    let pty_id = spawn_result.get("ptyId").and_then(Value::as_str).unwrap().to_string();
    
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    
    if let Some(t) = terminals.get_mut(&pty_id) {
        t.pane_id = Some(pane_id);
        let session = terminal_session_json(&pty_id, t);
        return Ok(serde_json::json!({ "ptyId": pty_id, "session": session }));
    }
    
    Err(db_error("Failed to re-acquire spawned terminal for pane"))
}

#[tauri::command]
fn terminal_get_pane_session(state: State<'_, AppState>, pane_id: String) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    if let Some((id, t)) = terminals
        .iter_mut()
        .find(|(_, t)| t.pane_id.as_deref() == Some(pane_id.as_str()))
    {
        return Ok(terminal_session_json(id, t));
    }
    Ok(Value::Null)
}

#[tauri::command]
fn terminal_kill_by_pane_id(
    state: State<'_, AppState>,
    app: AppHandle,
    pane_id: String,
) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    
    let ids = terminals
        .iter()
        .filter(|(_, t)| t.pane_id.as_deref() == Some(pane_id.as_str()))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    
    for id in ids {
        if let Some(mut t) = terminals.remove(&id) {
            t.stop_flag.store(true, Ordering::Relaxed);
            drop(t.pty_master);
            if let Some(reader_thread) = t.reader_thread.take() {
                let _ = reader_thread.join();
            }
            let _ = app.emit("terminal-exit", serde_json::json!({ "ptyId": id, "code": -1 }));
        }
    }
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
fn terminal_get_backlog(state: State<'_, AppState>, pty_id: String) -> ApiResult<Value> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    if let Some(t) = terminals.get(&pty_id) {
        let lines = t
            .output_buffer
            .lock()
            .map_err(|_| db_error("terminal output buffer lock poisoned"))?
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        return Ok(serde_json::json!({ "lines": lines }));
    }
    Ok(serde_json::json!({ "lines": [] }))
}

#[tauri::command]
fn terminal_get_project_activity(_project_id: String) -> ApiResult<Value> {
    Ok(serde_json::json!({
      "totalRunningPanes": 0,
      "runningPanesByCombId": {}
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            app_get_version,
            app_check_for_updates,
            app_quit_and_install,
            app_show_notification,
            dialog_select_directory,
            dialog_show_message,
            dialog_confirm,
            shell_open_external,
            shell_open_path,
            shell_show_item_in_folder,
            shell_resolve_cli_path,
            shell_detect_cli_for_provider,
            shell_validate_cli_path,
            shell_open_terminal_at_path,
            window_minimize,
            window_maximize,
            window_close,
            window_is_maximized,
            license_get_status,
            license_get_machine_id,
            license_activate,
            license_skip_activation,
            db_providers_find_all,
            db_providers_find_by_id,
            db_providers_find_by_type,
            db_providers_find_active,
            db_providers_create,
            db_providers_update,
            db_providers_delete,
            db_providers_set_active,
            db_providers_test_connection,
            db_providers_is_encryption_available,
            db_projects_find_all,
            db_projects_find_by_id,
            db_projects_find_by_path,
            db_projects_search,
            db_projects_create,
            db_projects_update,
            db_projects_delete,
            db_projects_get_stats,
            db_projects_update_last_opened,
            db_combs_find_by_project,
            db_combs_find_by_id,
            db_combs_create,
            db_combs_update,
            db_combs_delete,
            db_panes_find_by_comb,
            db_panes_find_by_id,
            db_panes_create,
            db_panes_update,
            db_panes_delete,
            db_utils_backup,
            db_utils_get_path,
            db_utils_get_size,
            git_get_current_branch,
            git_get_local_branches,
            git_get_review_diffs,
            review_get_diffs_bundle,
            comb_ensure_worktree,
            comb_discard,
            comb_merge_into_main,
            comb_get_diffs,
            comb_apply_patch,
            terminal_spawn,
            terminal_get_or_create,
            terminal_get_session,
            terminal_write,
            terminal_resize,
            terminal_kill,
            terminal_kill_by_mission_id,
            terminal_get_or_create_for_pane,
            terminal_get_pane_session,
            terminal_kill_by_pane_id,
            terminal_get_backlog,
            terminal_get_project_activity
        ])
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let _ = std::fs::create_dir_all(&app_data_dir);
            let db_path = app_data_dir.join("database.sqlite");
            let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
            conn.execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 5000;
                ",
            )
            .map_err(|e| e.to_string())?;
            conn.execute_batch(APP_SCHEMA_SQL)
                .map_err(|e| format!("failed to apply schema: {e}"))?;
            run_legacy_schema_migrations(&conn)
                .map_err(|e| format!("failed to migrate legacy schema: {e}"))?;
            eprintln!("[DCC] Database ready at {:?}", db_path);
            app.manage(AppState {
                db_path: Arc::new(db_path),
                conn: Arc::new(Mutex::new(conn)),
                terminals: Arc::new(Mutex::new(HashMap::new())),
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() {
    run();
}
