//! What DCC's own data takes on disk, for the cleanup dialog, and "Compactar":
//! the same maintenance the background passes do, run now.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, Wry};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DccStorageUsage {
    /// The SQLite file plus its WAL.
    pub database_bytes: u64,
    /// Free pages inside the database file.
    pub reclaimable_bytes: u64,
    /// The free pages only leave the file when DCC opens again.
    pub reclaims_on_restart: bool,
    pub browser_cache_bytes: u64,
    /// The app's own webview storage (localStorage and its WAL).
    pub app_webview_bytes: u64,
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn database_path(app: &AppHandle<Wry>) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("database.sqlite"))
}

fn usage(app: &AppHandle<Wry>) -> Result<DccStorageUsage, String> {
    let db_path = database_path(app)?;
    let conn =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| error.to_string())?;
    let space = dcc_infra::db::database_space(&conn).map_err(|error| error.to_string())?;
    let app_webview_bytes = crate::home_dir_path()
        .map(|home| {
            crate::browser_data::dir_bytes(
                &home
                    .join("Library/WebKit")
                    .join(&app.config().identifier)
                    .join("WebsiteData"),
            )
        })
        .unwrap_or(0);
    let mut wal = db_path.clone().into_os_string();
    wal.push("-wal");
    Ok(DccStorageUsage {
        database_bytes: file_len(&db_path) + file_len(Path::new(&wal)),
        reclaimable_bytes: u64::try_from(space.free_bytes).unwrap_or(0),
        reclaims_on_restart: !space.incremental && space.vacuums_at_startup,
        browser_cache_bytes: crate::browser_data::usage(app).cache_bytes,
        app_webview_bytes,
    })
}

#[tauri::command]
pub async fn dcc_storage_usage(app: AppHandle<Wry>) -> Result<DccStorageUsage, String> {
    tauri::async_runtime::spawn_blocking(move || usage(&app))
        .await
        .map_err(|error| error.to_string())?
}

/// Merges settled streamed chunks, gives free pages back and drops the
/// browser's caches (logins stay). Returns the usage afterwards.
#[tauri::command]
pub async fn dcc_storage_compact(app: AppHandle<Wry>) -> Result<DccStorageUsage, String> {
    let db_path = database_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::delta_compaction_worker::run_pass(&db_path)
    })
    .await
    .map_err(|error| error.to_string())??;
    crate::browser_data::clear(&app, false).await?;
    let app_for_usage = app.clone();
    tauri::async_runtime::spawn_blocking(move || usage(&app_for_usage))
        .await
        .map_err(|error| error.to_string())?
}
