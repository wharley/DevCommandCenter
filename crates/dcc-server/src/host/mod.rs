//! Starts the whole backend in the process that hosts it (the desktop today,
//! the native app next): the data directory's owner lock, the database, the
//! sessions and their runtimes, the connectors, the agents' MCP, the agents'
//! requests, keep-awake, the usage-limit resumes, the delegation watchdog,
//! the ai-memory sidecar and its outbox, the loopback API with the relay, and
//! the `dccd` daemon. The host passes what only it has: its windows' events,
//! the menu bar and global shortcuts, and how it restarts.

mod app;
mod browser;
mod computer;
mod http;
pub mod keep_awake;
mod terminals;
pub mod usage_limit;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dcc_core::ports::EventBus;
use dcc_infra::mcp_db::SqliteMcpRepo;
use dcc_tauri::app_events::AppEvents;
use dcc_tauri::connector_gateway::ConnectorGateway;
use dcc_tauri::mcp_connect::{ConnectStores, McpConnectCoordinator};
use dcc_tauri::state::{SessionCommandState, WorkspaceCommandState};
use rusqlite::Connection;

pub use app::HostApp;
pub use browser::HostBrowser;
pub use computer::HostComputer;
pub use terminals::HostTerminals;

use crate::ai_memory_sidecar::AiMemorySidecar;
use crate::browser_agent_requests::BrowserAgentRequestBroker;
use crate::browser_commands::BrowserState;
use crate::browser_mcp_bridge::BrowserMcpBridge;
use crate::computer_use_commands::ComputerUseState;
use crate::daemon_client::DaemonRuntimeInfo;
use crate::decision_provider_settings::DecisionProviderSettings;
use crate::desktop_agent_requests::DesktopAgentRequests;
use crate::embedded_http::EmbeddedHttpServer;
use crate::http_api::{DesktopMenuBar, DesktopShortcuts, HttpBackend};
use crate::named_events::NamedEvents;
use crate::remote_connector::RemoteHub;
use crate::terminals::TerminalState;

/// Runs on the database right after its schema, before compaction.
pub type PrepareDatabase = Box<dyn FnOnce(&Connection) -> Result<(), String>>;

pub struct HostConfig {
    pub app_data_dir: PathBuf,
    /// Where the bundle keeps `dccd` and `ai-memory`.
    pub resource_dir: Option<PathBuf>,
    /// A dev build: `dccd` runs through `cargo run`.
    pub dev: bool,
    /// Events by name for the host's windows.
    pub events: NamedEvents,
    /// The sessions' core events for the host's windows (the desktop's
    /// webview); none when the host reads them from the API.
    pub session_bus: Option<Arc<dyn EventBus>>,
    /// A created task's events.
    pub task_events: AppEvents,
    pub prepare_database: Option<PrepareDatabase>,
    pub keep_awake_listener: keep_awake::HoldListener,
    pub menu_bar: Option<Arc<dyn DesktopMenuBar>>,
    pub shortcuts: Option<Arc<dyn DesktopShortcuts>>,
    pub restart: Arc<dyn Fn() + Send + Sync>,
}

/// Everything the backend runs on, for the host to keep and share.
pub struct Host {
    pub app_data_dir: PathBuf,
    pub events: NamedEvents,
    pub db_path: PathBuf,
    pub sessions: SessionCommandState,
    pub workspace: WorkspaceCommandState,
    pub terminals: TerminalState,
    pub browser: BrowserState,
    pub broker: BrowserAgentRequestBroker,
    pub computer: ComputerUseState,
    pub agent_requests: DesktopAgentRequests,
    pub coordinator: Option<McpConnectCoordinator>,
    pub gateway: Option<ConnectorGateway>,
    pub mcp_bridge: Option<Arc<BrowserMcpBridge>>,
    pub ai_memory: Arc<AiMemorySidecar>,
    pub keep_awake: keep_awake::KeepAwakeState,
    pub usage_limit: usage_limit::UsageLimitState,
    pub server: Option<Arc<EmbeddedHttpServer>>,
    pub remote: Option<Arc<RemoteHub>>,
    /// `dccd`, or why it could not start (the desktop then runs its tick in
    /// process).
    pub daemon: Result<DaemonRuntimeInfo, String>,
}

impl Host {
    /// Called once, on the runtime the background work should live on.
    pub async fn start(config: HostConfig) -> Result<Host, String> {
        let HostConfig {
            app_data_dir,
            resource_dir,
            dev,
            events,
            session_bus,
            task_events,
            prepare_database,
            keep_awake_listener,
            menu_bar,
            shortcuts,
            restart,
        } = config;
        let _ = std::fs::create_dir_all(&app_data_dir);
        // This process owns the agents, terminals and git coordinator of
        // this data directory: deleting a task elsewhere waits for this lock
        // (`dcc_infra::runtime_owner`). A deletion in flight gets a few
        // seconds to finish.
        match dcc_infra::runtime_owner::RuntimeOwner::acquire_within(
            &app_data_dir,
            std::time::Duration::from_secs(10),
        ) {
            // Held until the process exits.
            Ok(Some(owner)) => std::mem::forget(owner),
            Ok(None) => eprintln!("[DCC] another process holds the runtime owner lock"),
            Err(error) => eprintln!("[DCC] runtime owner lock unavailable: {error}"),
        }
        if let Err(error) = AiMemorySidecar::load_persisted_settings(&app_data_dir).await {
            eprintln!("[DCC][ai-memory] persisted settings unavailable: {error}");
        }
        if let Err(error) = DecisionProviderSettings::load_persisted_settings(&app_data_dir).await {
            eprintln!("[DCC][decision-provider] persisted settings unavailable: {error}");
        }
        let ai_memory = Arc::new(
            match AiMemorySidecar::start(resource_dir.as_deref(), &app_data_dir) {
                Ok(sidecar) => sidecar,
                Err(error) => {
                    eprintln!("[DCC][ai-memory] sidecar unavailable: {error}");
                    AiMemorySidecar::unavailable(error)
                }
            },
        );
        let db_path = app_data_dir.join("database.sqlite");
        let conn = crate::database::open_app_database(&db_path)?;
        if let Some(prepare) = prepare_database {
            prepare(&conn)?;
        }
        crate::database::compact_once(&conn);
        eprintln!("[DCC] Database ready at {:?}", db_path);
        crate::delta_compaction_worker::spawn(db_path.clone());
        let sessions = match session_bus {
            Some(bus) => {
                SessionCommandState::new_recovering(db_path.clone(), app_data_dir.clone(), bus)
            }
            None => SessionCommandState::new_headless(db_path.clone(), app_data_dir.clone()),
        };
        let computer = ComputerUseState::default();
        let browser = BrowserState::default();
        let broker = BrowserAgentRequestBroker::default();
        let (coordinator, gateway) = connectors(&db_path, &events);
        let mcp_bridge = match BrowserMcpBridge::start(
            Some(events.clone()),
            browser.clone(),
            broker.clone(),
            sessions.clone(),
            computer.clone(),
            gateway.clone(),
        )
        .await
        {
            Ok(bridge) => match sessions.install_ephemeral_mcp_projection(bridge.clone()) {
                Ok(()) => Some(bridge),
                Err(_) => {
                    bridge.shutdown();
                    eprintln!("[DCC][browser-mcp] unavailable: projection installation failed");
                    None
                }
            },
            Err(_) => {
                eprintln!("[DCC][browser-mcp] unavailable: loopback listener not started");
                None
            }
        };
        let workspace = WorkspaceCommandState::from_session(&sessions);
        match dcc_tauri::commands::workspace_commands::reconcile_delegation_worktree_operations(
            &workspace,
        )
        .await
        {
            Ok(warnings) => {
                for warning in warnings {
                    eprintln!("[DCC][delegation-recovery] {warning}");
                }
            }
            Err(error) => eprintln!("[DCC][delegation-recovery] journal unavailable: {error}"),
        }
        let agent_requests = DesktopAgentRequests::new(
            events.clone(),
            broker.clone(),
            browser.clone(),
            computer.clone(),
        );
        let usage_limit = usage_limit::start(&sessions);
        let keep_awake = keep_awake::start(
            &sessions,
            usage_limit.scheduler.clone(),
            &app_data_dir,
            keep_awake_listener,
        );
        let terminals = TerminalState {
            conn: Arc::new(Mutex::new(conn)),
            terminals: Arc::new(Mutex::new(HashMap::new())),
            system: Arc::new(Mutex::new(sysinfo::System::new_all())),
        };
        let (server, remote) = match HttpBackend::embedded(sessions.clone()) {
            Ok(backend) => {
                let mut backend = backend
                    .with_workspace_state(workspace.clone())
                    .with_desktop_terminals(Arc::new(HostTerminals {
                        state: terminals.clone(),
                        events: events.clone(),
                    }))
                    .with_desktop_browser(Arc::new(HostBrowser {
                        browser: browser.clone(),
                        sessions: sessions.clone(),
                        broker: broker.clone(),
                        requests: agent_requests.clone(),
                    }))
                    .with_desktop_computer(Arc::new(HostComputer {
                        computer: computer.clone(),
                        requests: agent_requests.clone(),
                        events: events.clone(),
                    }))
                    .with_desktop_keep_awake(Arc::new(keep_awake::HostKeepAwake(
                        keep_awake.controller.clone(),
                    )))
                    .with_app(Arc::new(HostApp {
                        events: task_events,
                        connectors: coordinator.clone().zip(gateway.clone()),
                        ai_memory: ai_memory.clone(),
                        restart,
                    }));
                if let Some(menu_bar) = menu_bar {
                    backend = backend.with_desktop_menu_bar(menu_bar);
                }
                if let Some(shortcuts) = shortcuts {
                    backend = backend.with_desktop_shortcuts(shortcuts);
                }
                backend.attach_agent_requests(Arc::new(agent_requests.clone()));
                match http::serve(backend, db_path.clone(), events.clone()) {
                    Some((server, remote)) => (Some(server), remote),
                    None => (None, None),
                }
            }
            Err(error) => {
                eprintln!("[DCC HTTP] not started: {error}");
                (None, None)
            }
        };
        let draining = sessions.clone();
        crate::runtime::spawn(async move {
            loop {
                if let Err(error) = draining.drain_ai_memory_outbox(8).await {
                    eprintln!("[DCC] ai-memory outbox drain failed: {error}");
                }
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        });
        // Settles delegations whose child finished while nobody was
        // listening (reload, restart) and enforces their timeout budget.
        crate::runtime::spawn(dcc_tauri::delegation_runtime::run_delegation_watchdog(
            sessions.clone(),
        ));
        let daemon = crate::daemon_client::ensure_sidecar_running(
            resource_dir.as_deref(),
            dev,
            &db_path,
            &app_data_dir,
        );
        Ok(Host {
            app_data_dir,
            events,
            db_path,
            sessions,
            workspace,
            terminals,
            browser,
            broker,
            computer,
            agent_requests,
            coordinator,
            gateway,
            mcp_bridge,
            ai_memory,
            keep_awake,
            usage_limit,
            server,
            remote,
            daemon,
        })
    }

    /// Before the process exits: the shells, the agents' MCP, the Browser's
    /// evidence and the ai-memory sidecar.
    pub fn shutdown(&self) {
        crate::terminals::kill_all(&self.terminals, &self.events);
        if let Some(bridge) = &self.mcp_bridge {
            bridge.shutdown();
        }
        crate::browser_commands::shutdown(&self.browser);
        self.ai_memory.shutdown();
    }
}

/// One sign-in coordinator serves Settings and the connector gateway, so a
/// refresh never runs twice in parallel. Sign-ins the agents' own MCP login
/// stored are no longer read; they are retired so those connectors ask to
/// reconnect.
fn connectors(
    db_path: &std::path::Path,
    events: &NamedEvents,
) -> (Option<McpConnectCoordinator>, Option<ConnectorGateway>) {
    let coordinator = match McpConnectCoordinator::new() {
        Ok(coordinator) => Some(coordinator),
        Err(error) => {
            eprintln!("[DCC][mcp-connect] unavailable: {error}");
            None
        }
    };
    let stores = || {
        SqliteMcpRepo::open(db_path).ok().map(|repo| ConnectStores {
            repo: Arc::new(repo),
            credentials: Arc::new(dcc_infra::credential_store::SystemCredentialStore::default()),
        })
    };
    let gateway = coordinator.as_ref().and_then(|coordinator| {
        let needs_auth = events.clone();
        ConnectorGateway::new(
            coordinator.clone(),
            stores()?,
            Arc::new(
                move |definition_id: &dcc_core::domain::mcp::McpDefinitionId| {
                    let _ = needs_auth.emit("mcp-connector-needs-auth", definition_id.0.clone());
                },
            ),
        )
        .map_err(|error| eprintln!("[DCC][mcp-connect] gateway unavailable: {error}"))
        .ok()
    });
    if let Some(stores) = stores() {
        crate::runtime::spawn(async move {
            let retired = dcc_tauri::mcp_connect::retire_provider_grants(&stores).await;
            if retired > 0 {
                eprintln!("[DCC][mcp-connect] retired {retired} agent-held sign-ins");
            }
        });
    }
    (coordinator, gateway)
}
