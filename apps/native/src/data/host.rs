//! The backend in this process (`dcc_server::host`) when no desktop serves
//! the API: sessions and their agents, the terminals, the agents' MCP and
//! Browser, computer use, keep-awake, the relay and the API itself. It starts
//! at boot on a runtime of its own, off the UI thread; the windows reach it
//! through the same loopback API as the desktop's. `DCC_NATIVE_HOST=0` never
//! hosts (a desktop started later then serves).

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use dcc_server::host::{Host, HostConfig};
use dcc_server::named_events::NamedEvents;
use dcc_tauri::app_events::{AppEmitter, AppEvents};

use crate::data::desktop_api::DesktopApi;

struct Hosted {
    host: Host,
    runtime: tokio::runtime::Runtime,
}

static HOSTED: OnceLock<Hosted> = OnceLock::new();

/// The windows read the API; nothing listens for events by name.
struct Quiet;

impl AppEmitter for Quiet {
    fn emit(&self, _event: &str, _payload: serde_json::Value) -> Result<(), String> {
        Ok(())
    }
}

/// Hosts unless a desktop already answers on the port (or it is turned off).
pub fn start() {
    if cfg!(test) || std::env::var("DCC_NATIVE_HOST").as_deref() == Ok("0") {
        return;
    }
    std::thread::Builder::new()
        .name("dcc-host".into())
        .spawn(|| {
            if DesktopApi::load().is_some_and(|api| api.get("/").is_ok()) {
                eprintln!("[dcc-native] the desktop serves the API; not hosting");
                return;
            }
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .thread_name("dcc-server")
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("[dcc-native] no runtime for the backend: {error}");
                    return;
                }
            };
            match runtime.block_on(Host::start(config())) {
                Ok(host) => {
                    if let Err(error) = &host.daemon {
                        eprintln!("[dcc-native] dccd unavailable: {error}");
                    }
                    let _ = HOSTED.set(Hosted { host, runtime });
                }
                Err(error) => eprintln!("[dcc-native] the backend did not start: {error}"),
            }
        })
        .map(drop)
        .unwrap_or_else(|error| eprintln!("[dcc-native] no thread for the backend: {error}"));
}

fn config() -> HostConfig {
    let db_path = crate::data::loader::database_path();
    let app_data_dir = db_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    HostConfig {
        app_data_dir,
        resource_dir: resource_dir(),
        dev: cfg!(debug_assertions),
        events: NamedEvents(Arc::new(Quiet)),
        session_bus: None,
        task_events: AppEvents::silent(),
        prepare_database: None,
        // The menu bar's ☕ reads the hold through the API.
        keep_awake_listener: Box::new(|_| {}),
        // The menu bar's snapshot and the global shortcuts come next (12e).
        menu_bar: None,
        shortcuts: None,
        restart: Arc::new(relaunch),
    }
}

/// The bundle's `Contents/Resources`, beside `Contents/MacOS/dcc-native`.
fn resource_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let resources = exe.parent()?.parent()?.join("Resources");
    resources.is_dir().then_some(resources)
}

/// `restart_dcc`: a new copy of this app, then this one leaves.
fn relaunch() {
    shutdown();
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(0);
}

/// The backend this process hosts, once it started.
pub fn current() -> Option<&'static Host> {
    HOSTED.get().map(|hosted| &hosted.host)
}

/// Runs `future` on the backend's runtime; `None` when not hosting.
pub fn block_on<F: Future>(future: F) -> Option<F::Output> {
    HOSTED
        .get()
        .map(|hosted| hosted.runtime.handle().block_on(future))
}

/// Phones are not notified while the person is looking at this window.
pub fn set_focused(focused: bool) {
    if let Some(remote) = current().and_then(|host| host.remote.as_ref()) {
        remote.set_desktop_focused(focused);
    }
}

/// Before the app quits: the shells, the agents' MCP and the sidecar.
pub fn shutdown() {
    if let Some(host) = current() {
        host.shutdown();
    }
}
