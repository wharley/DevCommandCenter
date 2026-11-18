//! `dcc serve`: the whole backend the app hosts (`host::Host`), with no
//! screen, in the foreground, for a machine nobody sits at (a Linux server,
//! a Mac mini). The same sessions, agents' MCP, connectors, recovery and
//! relay as the app's; it leaves on SIGTERM or Ctrl-C, closing the shells,
//! the agents' MCP and the helpers it started.
//!
//! It refuses to start where the app (or another `dcc serve`) already runs
//! the backend for this data folder: two owners would run the same agents
//! twice.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dcc_tauri::app_events::{AppEmitter, AppEvents};

use crate::host::{Host, HostConfig};
use crate::http_config::HttpConfig;
use crate::named_events::NamedEvents;

/// Where `dcc serve` runs: its data folder and where its helpers are.
pub struct ServeOptions {
    pub app_data_dir: PathBuf,
    /// `dccd`, `ai-memory` and the Claude helper; the folder of the `dcc`
    /// binary, as the Linux package and a release build lay them out.
    pub resource_dir: Option<PathBuf>,
}

impl ServeOptions {
    pub fn from_environment() -> Self {
        Self {
            app_data_dir: crate::daemon_client::default_app_data_dir(),
            resource_dir: std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf)),
        }
    }
}

/// Nobody listens for events by name: clients read the API.
struct Quiet;

impl AppEmitter for Quiet {
    fn emit(&self, _event: &str, _payload: serde_json::Value) -> Result<(), String> {
        Ok(())
    }
}

/// Why `dcc serve` must not start here, if it must not.
pub fn refusal(app_data_dir: &Path, api_answers: bool, owner_free: bool) -> Option<String> {
    if api_answers {
        return Some(
            "o DCC já está rodando nesta máquina (o app ou outro `dcc serve`); \
             feche-o antes de rodar `dcc serve`"
                .to_string(),
        );
    }
    if !owner_free {
        return Some(format!(
            "outro processo do DCC usa a pasta {}; feche-o antes de rodar `dcc serve`",
            app_data_dir.display()
        ));
    }
    None
}

/// Runs the backend until a stop signal. Blocks the calling thread.
pub fn serve(options: ServeOptions) -> Result<(), String> {
    let ServeOptions {
        app_data_dir,
        resource_dir,
    } = options;
    std::fs::create_dir_all(&app_data_dir)
        .map_err(|error| format!("não deu para criar {}: {error}", app_data_dir.display()))?;
    let port = HttpConfig::load()
        .map(|config| config.port)
        .map_err(|error| format!("configuração do servidor ilegível: {error}"))?;
    let api_answers = crate::host::api_answers_on(port);
    let owner_free = dcc_infra::runtime_owner::RuntimeOwner::try_acquire(&app_data_dir)
        .map_err(|error| format!("trava da pasta de dados indisponível: {error}"))?
        // Free: let it go at once, `Host::start` takes it for good.
        .is_some();
    if let Some(reason) = refusal(&app_data_dir, api_answers, owner_free) {
        return Err(reason);
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_name("dcc-serve")
        .enable_all()
        .build()
        .map_err(|error| format!("sem runtime para o servidor: {error}"))?;
    runtime.block_on(async move {
        let host = Host::start(HostConfig {
            app_data_dir: app_data_dir.clone(),
            resource_dir,
            dev: false,
            events: NamedEvents(Arc::new(Quiet)),
            session_bus: None,
            task_events: AppEvents::silent(),
            prepare_database: None,
            keep_awake_listener: Box::new(|_| {}),
            shortcuts: None,
            // `restart_dcc` from a client: leave, and let the service manager
            // (systemd) start it again.
            restart: Arc::new(|| std::process::exit(0)),
        })
        .await?;
        report(&host, port);
        wait_for_stop().await;
        eprintln!("[DCC] saindo…");
        host.shutdown();
        if let Ok(info) = &host.daemon {
            stop_daemon(info.pid);
        }
        // The relay and the listeners end with the runtime.
        Ok::<(), String>(())
    })?;
    runtime.shutdown_timeout(Duration::from_secs(5));
    Ok(())
}

fn report(host: &Host, port: u16) {
    eprintln!("[DCC] servidor sem tela no ar");
    eprintln!("[DCC]   dados: {}", host.app_data_dir.display());
    match &host.server {
        Some(_) => eprintln!("[DCC]   API: http://127.0.0.1:{port} (só nesta máquina)"),
        None => eprintln!("[DCC]   API: desligada (veja ~/.dcc/http-config.json)"),
    }
    match host.remote.as_ref().and_then(|remote| remote.relay_url()) {
        Some(relay) => eprintln!("[DCC]   relay: {relay}"),
        None => eprintln!("[DCC]   relay: indisponível"),
    }
    match &host.daemon {
        Ok(info) => eprintln!("[DCC]   dccd: pid {}", info.pid),
        Err(error) => eprintln!("[DCC]   dccd: indisponível ({error})"),
    }
    if host.mcp_bridge.is_none() {
        eprintln!("[DCC]   aviso: ferramentas dos agentes (MCP) indisponíveis");
    }
    if host.gateway.is_none() {
        eprintln!("[DCC]   aviso: conectores indisponíveis");
    }
}

async fn wait_for_stop() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// `dccd` outlives the app on purpose (the CLI reuses it); a server that
/// stops takes it along, so nothing of it is left running.
fn stop_daemon(pid: u32) {
    let mut system = sysinfo::System::new();
    let pid = sysinfo::Pid::from_u32(pid);
    system.refresh_process(pid);
    if let Some(process) = system.process(pid) {
        let _ = process
            .kill_with(sysinfo::Signal::Term)
            .unwrap_or_else(|| process.kill());
    }
}

#[cfg(test)]
mod tests {
    use super::refusal;
    use std::path::Path;

    #[test]
    fn starts_only_where_nothing_else_runs_the_backend() {
        let dir = Path::new("/tmp/dcc");
        assert_eq!(refusal(dir, false, true), None);
        let app_running = refusal(dir, true, true).unwrap();
        assert!(app_running.contains("já está rodando"), "{app_running}");
        let folder_taken = refusal(dir, false, false).unwrap();
        assert!(folder_taken.contains("/tmp/dcc"), "{folder_taken}");
    }
}
