//! The loopback API and the relay: a `dccd-http` left by an older version is
//! asked to leave the port first; the relay connector serves the same router.

use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
#[cfg(windows)]
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::System;

use crate::embedded_http::EmbeddedHttpServer;
use crate::http_api::HttpBackend;
use crate::http_config::HttpConfig;
use crate::named_events::NamedEvents;
use crate::remote_connector::RemoteHub;

pub(crate) fn http_get_local(port: u16, path: &str) -> Option<(u16, String)> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(250))
    else {
        return None;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return None;
    }
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0_u8; 1024];
    while buf.len() < 8192 {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) if !buf.is_empty() => break,
            Err(_) => return None,
        }
    }
    let response = String::from_utf8_lossy(&buf).to_string();
    let status = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())?;
    Some((status, response))
}

/// True when a DCC HTTP API answers on the port (`/` needs no daemon, so it
/// answers fast even when `/health` would wait on one).
pub(crate) fn http_port_held_by_dcc(port: u16) -> bool {
    http_get_local(port, "/")
        .map(|(_status, response)| response.contains("DCC HTTP API"))
        .unwrap_or(false)
}

fn terminate_existing_http_sidecars(force: bool) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut args = vec!["/IM", "dccd-http*.exe", "/T"];
        if force {
            args.push("/F");
        }
        let _ = Command::new("taskkill")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }

    #[cfg(not(windows))]
    {
        let mut system = System::new_all();
        system.refresh_processes();
        let current_pid = std::process::id() as usize;
        for (pid, process) in system.processes() {
            if pid.as_u32() as usize == current_pid {
                continue;
            }
            let name = process.name();
            let exe_matches = process
                .exe()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("dccd-http"))
                .unwrap_or(false);
            if name.starts_with("dccd-http") || exe_matches {
                let _ = if force {
                    process.kill()
                } else {
                    process
                        .kill_with(sysinfo::Signal::Term)
                        .unwrap_or_else(|| process.kill())
                };
            }
        }
    }
}

/// The desktop serves the HTTP API itself now. A `dccd-http` left running by
/// an older version would keep the port (and answer for the same database),
/// so ask it to exit first; whatever still holds the port afterwards (e.g. a
/// second copy of the app) only costs a warning.
pub(crate) fn stop_stale_http_sidecar(port: u16) {
    if !http_port_held_by_dcc(port) {
        return;
    }
    terminate_existing_http_sidecars(false);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) && http_port_held_by_dcc(port) {
        std::thread::sleep(Duration::from_millis(100));
    }
    if http_port_held_by_dcc(port) {
        terminate_existing_http_sidecars(true);
    }
}

/// The API on loopback for the life of the process, with the relay; `None`
/// when it is turned off or could not be built.
pub(crate) fn serve(
    backend: HttpBackend,
    db_path: PathBuf,
    events: NamedEvents,
) -> Option<(Arc<EmbeddedHttpServer>, Option<Arc<RemoteHub>>)> {
    let mut config = match HttpConfig::load() {
        Ok(config) if config.enabled => config,
        Ok(_) => {
            eprintln!("[DCC HTTP] disabled in configuration");
            return None;
        }
        Err(error) => {
            eprintln!("[DCC HTTP] not started: {error}");
            return None;
        }
    };
    config.db_path = db_path.clone();
    let server = Arc::new(EmbeddedHttpServer::new(config, backend));
    let remote = start_remote_connector(&server, db_path, events);
    let listening = server.clone();
    crate::runtime::spawn(async move {
        let port = listening.port();
        let _ = tokio::task::spawn_blocking(move || stop_stale_http_sidecar(port)).await;
        match listening.listen().await {
            Ok(addr) => eprintln!("[DCC HTTP] listening on http://{addr}"),
            Err(error) => eprintln!("[DCC HTTP] WARN: {error}; local HTTP API unavailable"),
        }
    });
    Some((server, remote))
}

/// The relay connector lives as long as the app and serves the same router as
/// the loopback API. It stays idle until a relay is configured and a phone is
/// paired (or a pairing QR is open), so this costs nothing by default.
fn start_remote_connector(
    server: &Arc<EmbeddedHttpServer>,
    db_path: PathBuf,
    events: NamedEvents,
) -> Option<Arc<RemoteHub>> {
    let hub = match RemoteHub::new(server.router(), db_path) {
        Ok(hub) => hub,
        Err(error) => {
            eprintln!("[DCC Remote] not started: {error}");
            return None;
        }
    };
    server.backend().attach_remote(&hub);
    let forwarding = hub.clone();
    crate::runtime::spawn(async move {
        let mut changes = forwarding.subscribe();
        forwarding.start();
        loop {
            match changes.recv().await {
                Ok(event) => {
                    let _ = events.emit("remote-access-event", event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    Some(hub)
}
