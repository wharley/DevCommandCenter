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

    // Dual-bind policy: always listen on the configured host (loopback by
    // default — preserves the existing safe local-only contract), and *also*
    // open a LAN listener whenever an IPv4 LAN interface is up unless the
    // operator opted out via `DCC_HTTP_DISABLE_LAN=1`. This eliminates the
    // chicken-and-egg around first-time mobile pairing without changing the
    // default loopback contract for headless deployments. Authentication is
    // unchanged: API key + ECDSA-signed mobile requests gate every route.
    let host_was_explicit = std::env::var("DCC_HTTP_HOST").is_ok();
    let lan_disabled = matches!(
        std::env::var("DCC_HTTP_DISABLE_LAN").ok().as_deref(),
        Some("1") | Some("true")
    );

    let primary_addr = match config.host.parse::<IpAddr>() {
        Ok(ip) => SocketAddr::from((ip, config.port)),
        Err(error) => {
            eprintln!("[DCC HTTP] Invalid host address: {error}");
            std::process::exit(1);
        }
    };

    // The LAN listener uses 0.0.0.0:<port> — same router, separate socket —
    // and is skipped when the primary bind already covers every interface
    // (so we never double-bind the same port).
    let lan_addr: Option<SocketAddr> = if lan_disabled {
        None
    } else if !host_was_explicit && is_loopback(&config.host) {
        Some(SocketAddr::from(([0, 0, 0, 0], config.port)))
    } else if config.host == "0.0.0.0" {
        None
    } else if host_was_explicit && !is_loopback(&config.host) {
        // Operator picked a specific interface; respect it, don't second-guess.
        None
    } else {
        Some(SocketAddr::from(([0, 0, 0, 0], config.port)))
    };

    println!("[DCC HTTP] Auth mode: {:?}", config.effective_auth_mode());
    if let Some(expires_at) = config.bearer_token_expires_at {
        println!("[DCC HTTP] Bearer token expires at: {expires_at}");
    }

    let config = Arc::new(RwLock::new(config));
    let app = build_router(config.clone());

    println!("[DCC HTTP] Listening on http://{primary_addr}");
    if let Some(ip) = dev_command_center_tauri::net_info::detect_lan_ip() {
        if lan_addr.is_some() {
            println!(
                "[DCC HTTP] LAN reachable at:  http://{ip}:{}  (mobile clients connect here)",
                primary_addr.port()
            );
        }
    } else if lan_addr.is_some() {
        println!("[DCC HTTP] LAN listener ready but no LAN IPv4 detected yet.");
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

    let primary_listener = match tokio::net::TcpListener::bind(&primary_addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("[DCC HTTP] Failed to bind to {primary_addr}: {error}");
            std::process::exit(1);
        }
    };

    let lan_listener = if let Some(addr) = lan_addr {
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => Some(listener),
            Err(error) => {
                eprintln!(
                    "[DCC HTTP] WARN: could not open LAN listener on {addr}: {error}. Continuing with primary bind only."
                );
                None
            }
        }
    } else {
        None
    };

    let primary_app = app.clone();
    let primary_task = tokio::spawn(async move {
        if let Err(error) = axum::serve(primary_listener, primary_app).await {
            eprintln!("[DCC HTTP] Primary listener error: {error}");
        }
    });

    let lan_task = lan_listener.map(|listener| {
        let lan_app = app.clone();
        tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, lan_app).await {
                eprintln!("[DCC HTTP] LAN listener error: {error}");
            }
        })
    });

    // Block until any listener exits (which should only happen on shutdown).
    if let Some(lan) = lan_task {
        let _ = tokio::try_join!(primary_task, lan);
    } else {
        let _ = primary_task.await;
    }
}

fn ensure_pairing_schema(db_path: &std::path::Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
    dev_command_center_tauri::pairing::ensure_pairing_schema(&conn).map_err(|e| e.to_string())
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}
