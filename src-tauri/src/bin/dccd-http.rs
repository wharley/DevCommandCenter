use dev_command_center_tauri::daemon_client::default_app_data_dir;
use dev_command_center_tauri::http_api::{build_router, HttpBackend};
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

    // Loopback only unless the operator opted in with `DCC_HTTP_LAN=1` (the
    // desktop does so when it starts this server for mobile pairing). With
    // the opt-in we bind 0.0.0.0, which also covers loopback, and the config
    // forces remote auth so only paired devices get past the pairing routes.
    let primary_addr = match config.host.parse::<IpAddr>() {
        Ok(ip) => SocketAddr::from((ip, config.port)),
        Err(error) => {
            eprintln!("[DCC HTTP] Invalid host address: {error}");
            std::process::exit(1);
        }
    };
    let bind_addr = if config.lan && primary_addr.ip().is_loopback() {
        SocketAddr::from(([0, 0, 0, 0], config.port))
    } else {
        primary_addr
    };

    println!("[DCC HTTP] Auth mode: {:?}", config.effective_auth_mode());
    if let Some(expires_at) = config.bearer_token_expires_at {
        println!("[DCC HTTP] Bearer token expires at: {expires_at}");
    }

    let network_exposed = config.is_network_exposed();
    let backend = Arc::new(HttpBackend::headless(
        config.db_path.clone(),
        default_app_data_dir(),
    ));
    let config = Arc::new(RwLock::new(config));
    let app = build_router(config.clone(), backend);

    println!("[DCC HTTP] Listening on http://{bind_addr}");
    if network_exposed && bind_addr.ip().is_unspecified() {
        match dev_command_center_tauri::net_info::detect_lan_ip() {
            Some(ip) => println!(
                "[DCC HTTP] LAN reachable at:  http://{ip}:{}  (mobile clients connect here)",
                bind_addr.port()
            ),
            None => println!("[DCC HTTP] LAN listener ready but no LAN IPv4 detected yet."),
        }
    }
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

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("[DCC HTTP] Failed to bind to {bind_addr}: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) =
        dev_command_center_tauri::http_api::recover_mobile_tasks(&config.read().await.db_path)
    {
        eprintln!("[DCC HTTP] Failed to recover mobile task journal: {error}");
        std::process::exit(1);
    }

    // ConnectInfo gives handlers the socket peer, which rate limiting and the
    // pairing audit trust over any client-supplied header.
    if let Err(error) = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    {
        eprintln!("[DCC HTTP] Listener error: {error}");
    }
}

fn ensure_pairing_schema(db_path: &std::path::Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
    dev_command_center_tauri::pairing::ensure_pairing_schema(&conn).map_err(|e| e.to_string())
}
