//! Opt-in: boots the whole backend (`dcc_server::host`) in a throwaway home,
//! data directory and port, asks the API for its index and shuts it down.
//! Run with `cargo test -p dcc-server --test host_boot -- --ignored`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dcc_server::host::{Host, HostConfig};
use dcc_server::named_events::NamedEvents;
use dcc_tauri::app_events::{AppEmitter, AppEvents};

struct Quiet;

impl AppEmitter for Quiet {
    fn emit(&self, _event: &str, _payload: serde_json::Value) -> Result<(), String> {
        Ok(())
    }
}

fn get(port: u16, path: &str, key: Option<&str>) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let auth = key
        .map(|key| format!("X-API-Key: {key}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Connection: close\r\n\r\n"
    )
    .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
}

#[test]
#[ignore = "boots the backend: spawns dccd and binds a loopback port"]
fn the_host_boots_and_serves_the_api() {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    // SAFETY: the only test in this binary; nothing else reads them yet.
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::set_var("DCC_HTTP_PORT", port.to_string());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let host = runtime
        .block_on(Host::start(HostConfig {
            // The runtime registry refuses symlinked paths (macOS's /var).
            app_data_dir: data.path().canonicalize().unwrap(),
            resource_dir: None,
            dev: false,
            events: NamedEvents(Arc::new(Quiet)),
            session_bus: None,
            task_events: AppEvents::silent(),
            prepare_database: None,
            keep_awake_listener: Box::new(|_| {}),
            shortcuts: None,
            restart: Arc::new(|| {}),
        }))
        .expect("the host starts");
    assert!(host.db_path.exists());
    assert!(host.server.is_some(), "the API is served");
    let started = Instant::now();
    let index = loop {
        if let Some(response) = get(port, "/", None) {
            break response;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the API answers"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(index.contains("DCC HTTP API"), "{index}");
    // The menu bar's snapshot is the host's own now.
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.path().join(".dcc/http-config.json")).unwrap(),
    )
    .unwrap();
    let key = config["api_key"].as_str().unwrap();
    let snapshot = get(port, "/api/v1/menu-bar", Some(key)).unwrap();
    assert!(snapshot.starts_with("HTTP/1.1 200"), "{snapshot}");
    assert!(snapshot.contains("\"tasks\":[]"), "{snapshot}");
    eprintln!("daemon: {:?}", host.daemon.as_ref().map(|info| info.pid));
    host.shutdown();
    if let Ok(info) = &host.daemon {
        let _ = std::process::Command::new("kill")
            .arg(info.pid.to_string())
            .status();
    }
}
