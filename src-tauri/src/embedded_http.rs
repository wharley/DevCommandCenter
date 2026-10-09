//! The HTTP API served from inside the desktop app.
//!
//! The router runs on the app's own `SessionCommandState`, so a paired phone
//! sees and answers the sessions the desktop runs. It listens on loopback
//! only, always: phones reach it through the relay connector
//! (`remote_connector`), which runs requests on this same router in process
//! and never through a socket.
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use tokio::sync::RwLock;

use crate::http_api::{build_router, HttpBackend};
use crate::http_config::HttpConfig;

/// A previous listener (or a `dccd-http` just told to exit) may hold the
/// port for a moment after it lets go.
const BIND_ATTEMPTS: u32 = 20;
const BIND_RETRY_DELAY: Duration = Duration::from_millis(100);

pub struct EmbeddedHttpServer {
    backend: Arc<HttpBackend>,
    router: Router,
    port: u16,
}

impl EmbeddedHttpServer {
    pub fn new(mut config: HttpConfig, backend: HttpBackend) -> Self {
        // A `host` in http-config.json or DCC_HTTP_HOST is for a headless
        // `dccd-http`; the desktop never listens beyond loopback.
        config.host = Ipv4Addr::LOCALHOST.to_string();
        let port = config.port;
        let config = Arc::new(RwLock::new(config));
        let backend = Arc::new(backend);
        let router = build_router(config, backend.clone());
        Self {
            backend,
            router,
            port,
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

    /// Starts serving the API on `127.0.0.1` for the life of the app.
    pub async fn listen(&self) -> Result<SocketAddr, String> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let listener = bind_with_retry(addr)
            .await
            .map_err(|error| format!("could not listen on {addr}: {error}"))?;
        let app = self.router.clone();
        tauri::async_runtime::spawn(async move {
            let served = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
            if let Err(error) = served {
                eprintln!("[DCC HTTP] listener on {addr} failed: {error}");
            }
        });
        Ok(addr)
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
