use dev_command_center_tauri::daemon_client::default_app_data_dir;
use dev_command_center_tauri::http_api::build_router;
use dev_command_center_tauri::http_config::HttpConfig;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    let mut config = match HttpConfig::load() {
        Ok(config) => {
            if !config.enabled {
                eprintln!("[DCC HTTP] Server disabled in configuration");
                std::process::exit(0);
            }
            config
        }
        Err(error) => {
            eprintln!("[DCC HTTP] Failed to load configuration: {error}");
            std::process::exit(1);
        }
    };

    // If db_path is the default relative "dcc.db" or doesn't exist, fall back to
    // the platform app data dir where the local dccd daemon stores its database.
    if std::env::var("DCC_HTTP_DB_PATH").is_err() {
        let is_relative_default = config.db_path == PathBuf::from("dcc.db");
        if is_relative_default || !config.db_path.exists() {
            let candidate = default_app_data_dir().join("database.sqlite");
            if candidate.exists() {
                println!("[DCC HTTP] Using app data database: {candidate:?}");
                config.db_path = candidate;
            }
        }
    }

    println!("[DCC HTTP] Starting server...");
    println!("[DCC HTTP] Database: {:?}", config.db_path);
    println!("[DCC HTTP] Host: {}", config.host);
    println!("[DCC HTTP] Port: {}", config.port);

    // Ensure pairing tables exist. Other tables come from the desktop daemon; the
    // pairing schema is idempotent (CREATE TABLE IF NOT EXISTS) so it is safe to run here.
    if let Err(error) = ensure_pairing_schema(&config.db_path) {
        eprintln!("[DCC HTTP] Failed to ensure pairing schema: {error}");
        std::process::exit(1);
    }

    // Auto-open LAN bind when there's at least one paired (or pairing-in-progress)
    // mobile device, so the phone can actually reach this backend. Loopback bind
    // is the safe default for headless / API-only deployments. The user can
    // force loopback by setting DCC_HTTP_HOST=127.0.0.1 explicitly.
    let host_was_explicit = std::env::var("DCC_HTTP_HOST").is_ok();
    let pair_demand = count_pairing_demand(&config.db_path).unwrap_or(0);
    if !host_was_explicit && is_loopback(&config.host) && pair_demand > 0 {
        config.host = "0.0.0.0".to_string();
        println!(
            "[DCC HTTP] Detected {pair_demand} mobile device(s) — switching bind to 0.0.0.0 so the phone(s) can connect."
        );
        if let Some(lan_ip) = dev_command_center_tauri::net_info::detect_lan_ip() {
            println!("[DCC HTTP] LAN reachable at:  http://{lan_ip}:{}", config.port);
        }
    } else if is_loopback(&config.host) {
        println!(
            "[DCC HTTP] Bound to loopback only. To pair a mobile device, set DCC_HTTP_HOST=0.0.0.0 or pair a device first (LAN bind enables itself)."
        );
    }

    let addr = match config.host.parse::<IpAddr>() {
        Ok(ip) => SocketAddr::from((ip, config.port)),
        Err(error) => {
            eprintln!("[DCC HTTP] Invalid host address: {error}");
            std::process::exit(1);
        }
    };

    println!("[DCC HTTP] Auth mode: {:?}", config.effective_auth_mode());
    if let Some(expires_at) = config.bearer_token_expires_at {
        println!("[DCC HTTP] Bearer token expires at: {expires_at}");
    }

    let config = Arc::new(RwLock::new(config));
    let app = build_router(config.clone());

    println!("[DCC HTTP] Listening on http://{addr}");
    println!("[DCC HTTP] Endpoints:");
    println!("[DCC HTTP]   GET  /             - Server info");
    println!("[DCC HTTP]   GET  /health       - Health check");
    println!("[DCC HTTP]   GET  /openapi.json - OpenAPI document");
    println!("[DCC HTTP]   POST /rpc          - Compatibility RPC endpoint");
    println!("[DCC HTTP]   GET  /api/v1/shell/default");
    println!("[DCC HTTP]   GET  /api/v1/status");
    println!("[DCC HTTP]   GET  /api/v1/events/stream");
    println!("[DCC HTTP]   GET  /api/v1/terminals/events/stream");
    println!("[DCC HTTP]   GET  /api/v1/tasks");
    println!("[DCC HTTP]   POST /api/v1/tasks/:task_id/run");
    println!("[DCC HTTP]   POST /api/v1/tasks/:task_id/attach");
    println!("[DCC HTTP]   DELETE /api/v1/tasks/:task_id/attach");
    println!("[DCC HTTP]   GET  /api/v1/processes");
    println!("[DCC HTTP]   POST /api/v1/processes/:process_id/start");
    println!("[DCC HTTP]   POST /api/v1/processes/:process_id/stop");
    println!("[DCC HTTP]   POST /api/v1/processes/:process_id/restart");
    println!("[DCC HTTP]   GET  /api/v1/combs");
    println!("[DCC HTTP]   GET  /api/v1/panes");
    println!("[DCC HTTP]   POST /api/v1/diffs/bundle");
    println!("[DCC HTTP]   POST /api/v1/terminals/spawn");
    println!("[DCC HTTP]   POST /api/v1/terminals/by-owner/:owner_key");
    println!("[DCC HTTP]   POST /api/v1/terminals/:pty_id/write");
    println!("[DCC HTTP]   POST /api/v1/terminals/:pty_id/resize");
    println!("[DCC HTTP]   POST /api/v1/terminals/:pty_id/kill");
    println!("[DCC HTTP]   POST /api/v1/sessions/start");
    println!("[DCC HTTP]   GET  /api/v1/sessions");
    println!("[DCC HTTP]   GET  /api/v1/sessions/search");
    println!("[DCC HTTP]   GET  /api/v1/sessions/:session_id/events");
    println!("[DCC HTTP]   POST /api/v1/sessions/:session_id/turns");
    println!("[DCC HTTP]   POST /api/v1/sessions/:session_id/abort");
    println!("[DCC HTTP]   POST /api/v1/sessions/:session_id/resume");
    println!("[DCC HTTP]   POST /api/v1/sessions/:session_id/close");
    println!("[DCC HTTP]   POST /api/v1/sessions/:session_id/restore");
    println!("[DCC HTTP]   POST /api/v1/sessions/:session_id/respond-user-input");
    println!("[DCC HTTP]   POST /api/v1/sessions/:session_id/respond-permission");

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("[DCC HTTP] Failed to bind to {addr}: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("[DCC HTTP] Server error: {error}");
        std::process::exit(1);
    }
}

fn ensure_pairing_schema(db_path: &std::path::Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
    dev_command_center_tauri::pairing::ensure_pairing_schema(&conn).map_err(|e| e.to_string())
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Counts active paired devices plus unconsumed (still-valid) pairing nonces.
/// Returns 0 — instead of bubbling errors — if the DB cannot be opened, since
/// this only affects whether LAN bind auto-engages.
fn count_pairing_demand(db_path: &std::path::Path) -> Result<i64, String> {
    let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
    let devices: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM paired_devices WHERE revoked_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let pending: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pairing_nonces
             WHERE consumed_at IS NULL AND locked_at IS NULL AND expires_at > datetime('now')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(devices + pending)
}
