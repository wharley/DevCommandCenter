//! The HTTP API served from inside the desktop app.
//!
//! The router runs on the app's own `SessionCommandState`, so a paired phone
//! sees and answers the sessions the desktop runs. It listens on loopback
//! while the app is open and moves to every interface only when the person
//! opens mobile pairing (the LAN opt-in); with that the config forces remote
//! auth, so only paired devices get past the pairing routes.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use tokio::sync::{oneshot, Mutex, RwLock};

use crate::http_api::{build_router, HttpBackend};
use crate::http_config::HttpConfig;

/// A previous listener (or a `dccd-http` just told to exit) may hold the
/// port for a moment after it lets go.
const BIND_ATTEMPTS: u32 = 20;
const BIND_RETRY_DELAY: Duration = Duration::from_millis(100);

pub struct EmbeddedHttpServer {
    config: Arc<RwLock<HttpConfig>>,
    backend: Arc<HttpBackend>,
    router: Router,
    port: u16,
    listener: Mutex<Option<RunningListener>>,
}

struct RunningListener {
    addr: SocketAddr,
    lan: bool,
    stop: oneshot::Sender<()>,
}

impl EmbeddedHttpServer {
    pub fn new(config: HttpConfig, backend: HttpBackend) -> Self {
        let port = config.port;
        let config = Arc::new(RwLock::new(config));
        let backend = Arc::new(backend);
        let router = build_router(config.clone(), backend.clone());
        Self {
            config,
            backend,
            router,
            port,
            listener: Mutex::new(None),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// The API router, for the relay connector to run phone requests on.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    pub fn backend(&self) -> &Arc<HttpBackend> {
        &self.backend
    }

    /// Makes sure the API is listening, on every interface when `lan` is set.
    /// Once the LAN is on it stays on for the life of the app; asking for
    /// loopback then is already satisfied.
    pub async fn ensure_listening(&self, lan: bool) -> Result<SocketAddr, String> {
        let mut current = self.listener.lock().await;
        if let Some(running) = current.as_ref() {
            if running.lan || !lan {
                return Ok(running.addr);
            }
        }

        let host = self.config.read().await.host.clone();
        let loopback = host
            .parse::<IpAddr>()
            .map_err(|error| format!("invalid HTTP host {host}: {error}"))?;
        let addr = if lan && loopback.is_loopback() {
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, self.port))
        } else {
            SocketAddr::from((loopback, self.port))
        };

        // Remote auth must be in force before the wider socket accepts.
        let previous_lan = {
            let mut config = self.config.write().await;
            std::mem::replace(&mut config.lan, lan)
        };
        // Graceful stop: the old socket stops accepting and frees the port;
        // streams already open on it finish on their own.
        let previous = current.take().map(|running| {
            let _ = running.stop.send(());
            (running.addr, running.lan)
        });

        match bind_with_retry(addr).await {
            Ok(listener) => {
                *current = Some(self.serve(listener, addr, lan));
                Ok(addr)
            }
            Err(error) => {
                self.config.write().await.lan = previous_lan;
                if let Some((previous_addr, previous_lan)) = previous {
                    if let Ok(listener) = bind_with_retry(previous_addr).await {
                        *current = Some(self.serve(listener, previous_addr, previous_lan));
                    }
                }
                Err(format!("could not listen on {addr}: {error}"))
            }
        }
    }

    fn serve(
        &self,
        listener: tokio::net::TcpListener,
        addr: SocketAddr,
        lan: bool,
    ) -> RunningListener {
        let (stop, stopped) = oneshot::channel::<()>();
        let app = self.router.clone();
        tauri::async_runtime::spawn(async move {
            let served = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = stopped.await;
            })
            .await;
            if let Err(error) = served {
                eprintln!("[DCC HTTP] listener on {addr} failed: {error}");
            }
        });
        RunningListener { addr, lan, stop }
    }
}

async fn bind_with_retry(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    let mut attempt = 1;
    loop {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => return Ok(listener),
            Err(error) if attempt >= BIND_ATTEMPTS => return Err(error),
            Err(_) => {
                attempt += 1;
                tokio::time::sleep(BIND_RETRY_DELAY).await;
            }
        }
    }
}
