//! Remote access through the relay.
//!
//! The process that owns the sessions (the desktop app, or `dccd-http`) opens
//! an outbound WebSocket to a relay and serves phones from there; nothing
//! listens on the network. The relay only forwards bytes: each phone runs a
//! Noise handshake with this process (`dcc_remote`), so the relay never sees
//! a request, a response or an event.
//!
//! - One control connection per room. The relay announces each phone that
//!   connects; we answer with a data connection for it.
//! - Pairing (`Noise_IKpsk2`): the phone scanned a QR carrying a one-time
//!   secret (`create_offer`). Once it proves it holds the secret, the person
//!   approves it on the desktop, and its static key is pinned in
//!   `paired_devices`.
//! - Reconnecting (`Noise_IK`): only a pinned, non-revoked key gets in.
//! - A `req` frame runs against the same axum router the loopback API uses
//!   (`tower::ServiceExt::oneshot`), with `PairedDeviceIdentity` put in the
//!   request extensions here; it never comes from the network. A `sub` frame
//!   opens the SSE stream, replays from the phone's cursor and coalesces
//!   streaming deltas before encrypting.
//! - The connector runs while a relay is configured and there is a paired
//!   relay phone or an open pairing offer; it reconnects with backoff and
//!   comes back with the app.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::Router;
use dcc_remote::channel::SecureChannel;
use dcc_remote::coalesce::{DeltaCoalescer, OutgoingEvent, DEFAULT_WINDOW};
use dcc_remote::frames::{
    Body as FrameBody, End, Frame, Pairing, PairingStatus, Request as FrameRequest,
    Response as FrameResponse, StreamEvent, Subscribe,
};
use dcc_remote::noise::{DeviceInfo, Mode, ResponderHandshake, ServerHello, StaticKeypair};
use dcc_remote::offer::{self, PairingOffer};
use dcc_remote::sse::SseParser;
use dcc_remote::wire::{self, close, WireMessage};
use dcc_remote::PROTOCOL_VERSION;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot, watch, Notify};
use tokio::task::AbortHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};
use tower::ServiceExt;

use crate::http_auth::PairedDeviceIdentity;
use crate::pairing::{self, PairingError};

const OFFER_TTL: Duration = Duration::from_secs(5 * 60);
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const CONTROL_PING_INTERVAL: Duration = Duration::from_secs(25);
const DEVICE_RECHECK_INTERVAL: Duration = Duration::from_secs(15);
const IDLE_RECHECK_INTERVAL: Duration = Duration::from_secs(30);
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
const MAX_RESPONSE_BODY: usize = 16 * 1024 * 1024;
const OUTBOUND_QUEUE: usize = 256;
/// Frames a pairing phone may send before it is approved; they are answered
/// after approval.
const MAX_FRAMES_BEFORE_APPROVAL: usize = 32;
const REPLAY_PAGE: usize = 500;
const ROOM_SECRET_HEADER: &str = "x-dcc-room-secret";
pub const RELAY_URL_ENV: &str = "DCC_RELAY_URL";
/// The relay the project runs (apps/relay). Used until the user saves another
/// one or turns remote access off; nothing connects to it before a phone is
/// paired or a QR is open.
pub const DEFAULT_RELAY_URL: &str = "wss://relay.devcommandcenter.com";

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Sink = SplitSink<Socket, Message>;
type Stream = SplitStream<Socket>;

// ---------- identity ----------

/// This machine's side of the relay: its Noise static key (in every QR), the
/// room it registers on the relay and the secret that proves it owns it.
pub struct RelayIdentity {
    pub keypair: StaticKeypair,
    pub room_id: String,
    pub room_secret: String,
}

impl RelayIdentity {
    pub fn fingerprint(&self) -> String {
        offer::fingerprint(&self.keypair.public)
    }
}

/// The relay identity lives in the same database as the paired devices it
/// authenticates, so the desktop and `dccd-http` on one database agree on it.
pub fn ensure_remote_schema(conn: &Connection) -> Result<(), String> {
    pairing::ensure_pairing_schema(conn).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS remote_relay (
            id              INTEGER PRIMARY KEY CHECK (id = 1),
            static_private  BLOB NOT NULL,
            static_public   BLOB NOT NULL,
            room_id         TEXT NOT NULL,
            room_secret     TEXT NOT NULL,
            relay_url       TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|error| error.to_string())
}

fn load_identity(conn: &Connection) -> Result<Option<RelayIdentity>, String> {
    let row: Option<(Vec<u8>, Vec<u8>, String, String)> = conn
        .query_row(
            "SELECT static_private, static_public, room_id, room_secret FROM remote_relay WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((private, public, room_id, room_secret)) = row else {
        return Ok(None);
    };
    let keypair = StaticKeypair::from_parts(&private, &public).map_err(|e| e.to_string())?;
    Ok(Some(RelayIdentity {
        keypair,
        room_id,
        room_secret,
    }))
}

/// Creates the identity on first use. `INSERT OR IGNORE` keeps a second
/// process racing on the same database from replacing it.
fn load_or_create_identity(conn: &Connection) -> Result<RelayIdentity, String> {
    if let Some(identity) = load_identity(conn)? {
        return Ok(identity);
    }
    let keypair = StaticKeypair::generate().map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO remote_relay (id, static_private, static_public, room_id, room_secret)
         VALUES (1, ?1, ?2, ?3, ?4)",
        params![
            keypair.private.as_slice(),
            keypair.public.as_slice(),
            offer::generate_room_id(),
            offer::generate_room_secret()
        ],
    )
    .map_err(|error| error.to_string())?;
    load_identity(conn)?.ok_or_else(|| "relay identity missing after insert".to_string())
}

/// No row or NULL means never configured: the project relay. An empty string
/// is the user turning remote access off.
fn stored_relay_url(conn: &Connection) -> Result<Option<String>, String> {
    let stored = conn
        .query_row(
            "SELECT relay_url FROM remote_relay WHERE id = 1",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map(Option::flatten)
        .map_err(|error| error.to_string())?;
    Ok(match stored {
        None => Some(DEFAULT_RELAY_URL.to_string()),
        Some(url) if url.is_empty() => None,
        Some(url) => Some(url),
    })
}

pub fn validate_relay_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim().trim_end_matches('/');
    let parsed = url::Url::parse(trimmed).map_err(|error| format!("invalid relay URL: {error}"))?;
    match parsed.scheme() {
        "wss" => {}
        // Plain ws only for a relay on this machine (wrangler dev).
        "ws" if matches!(
            parsed.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("[::1]")
        ) => {}
        _ => return Err("the relay URL must start with wss:// (ws:// only for localhost)".into()),
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("the relay URL cannot have a query or fragment".into());
    }
    Ok(trimmed.to_string())
}

// ---------- hub ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferView {
    /// `dcc://pair#...`, the QR content. Holds the one-time secret.
    pub url: String,
    pub expires_at: String,
    pub desktop_fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub device_name: String,
    pub platform: Option<String>,
    /// Short form of the phone's key; the phone shows the same.
    pub fingerprint: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RemoteEvent {
    #[serde(rename_all = "camelCase")]
    ApprovalRequested { approval: ApprovalRequest },
    #[serde(rename_all = "camelCase")]
    ApprovalResolved { id: String, approved: bool },
    #[serde(rename_all = "camelCase")]
    StatusChanged { connected: bool },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub relay_url: Option<String>,
    /// Set by `DCC_RELAY_URL`; the screen cannot change it.
    pub relay_url_from_env: bool,
    pub connected: bool,
    pub room_id: Option<String>,
    pub desktop_fingerprint: Option<String>,
    pub active_connections: usize,
}

struct PendingOffer {
    psk: [u8; 32],
    expires_at: Instant,
    consumed: bool,
}

struct PendingApproval {
    request: ApprovalRequest,
    decide: oneshot::Sender<bool>,
}

struct LiveConnection {
    device_id: String,
    stop: Arc<watch::Sender<Option<u16>>>,
}

pub struct RemoteHub {
    router: Router,
    db_path: PathBuf,
    relay_url: StdMutex<Option<String>>,
    relay_url_from_env: bool,
    identity: StdMutex<Option<Arc<RelayIdentity>>>,
    offers: StdMutex<HashMap<String, PendingOffer>>,
    approvals: StdMutex<HashMap<String, PendingApproval>>,
    connections: StdMutex<HashMap<u64, LiveConnection>>,
    next_connection: AtomicU64,
    events: broadcast::Sender<RemoteEvent>,
    wake: Notify,
    connected: AtomicBool,
    started: AtomicBool,
}

fn lock<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn rfc3339_in(duration: Duration) -> String {
    (chrono::Utc::now() + chrono::Duration::from_std(duration).unwrap_or_default()).to_rfc3339()
}

impl RemoteHub {
    /// `router` is the API router the loopback server also serves.
    pub fn new(router: Router, db_path: PathBuf) -> Result<Arc<Self>, String> {
        let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
        ensure_remote_schema(&conn)?;
        let env_url = std::env::var(RELAY_URL_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty());
        let relay_url = match env_url.as_deref() {
            Some(url) => Some(validate_relay_url(url)?),
            None => stored_relay_url(&conn)?,
        };
        let identity = load_identity(&conn)?.map(Arc::new);
        let (events, _) = broadcast::channel(64);
        Ok(Arc::new(Self {
            router,
            db_path,
            relay_url: StdMutex::new(relay_url),
            relay_url_from_env: env_url.is_some(),
            identity: StdMutex::new(identity),
            offers: StdMutex::new(HashMap::new()),
            approvals: StdMutex::new(HashMap::new()),
            connections: StdMutex::new(HashMap::new()),
            next_connection: AtomicU64::new(1),
            events,
            wake: Notify::new(),
            connected: AtomicBool::new(false),
            started: AtomicBool::new(false),
        }))
    }

    /// Starts the supervisor on the current Tokio runtime. Idempotent.
    pub fn start(self: &Arc<Self>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        tokio::spawn(self.clone().supervise());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RemoteEvent> {
        self.events.subscribe()
    }

    pub fn relay_url(&self) -> Option<String> {
        lock(&self.relay_url).clone()
    }

    pub fn status(&self) -> RemoteStatus {
        let identity = lock(&self.identity).clone();
        RemoteStatus {
            relay_url: self.relay_url(),
            relay_url_from_env: self.relay_url_from_env,
            connected: self.connected.load(Ordering::SeqCst),
            room_id: identity.as_ref().map(|identity| identity.room_id.clone()),
            desktop_fingerprint: identity.as_ref().map(|identity| identity.fingerprint()),
            active_connections: lock(&self.connections).len(),
        }
    }

    /// Saves the relay endpoint (`None` turns remote access off).
    pub async fn set_relay_url(&self, url: Option<String>) -> Result<(), String> {
        if self.relay_url_from_env {
            return Err(format!("the relay URL is set by {RELAY_URL_ENV}"));
        }
        let url = match url.filter(|value| !value.trim().is_empty()) {
            Some(url) => Some(validate_relay_url(&url)?),
            None => None,
        };
        let stored = url.clone().unwrap_or_default();
        self.with_db(move |conn| {
            load_or_create_identity(conn)?;
            conn.execute(
                "UPDATE remote_relay SET relay_url = ?1 WHERE id = 1",
                params![stored],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .await?;
        *lock(&self.relay_url) = url;
        self.wake.notify_waiters();
        Ok(())
    }

    /// Opens a pairing offer: the QR content, valid for five minutes and for
    /// one phone.
    pub async fn create_offer(&self) -> Result<OfferView, String> {
        self.create_offer_expiring_in(OFFER_TTL).await
    }

    /// `create_offer` with another lifetime; tests use it for expired offers.
    #[doc(hidden)]
    pub async fn create_offer_expiring_in(&self, ttl: Duration) -> Result<OfferView, String> {
        let relay = self
            .relay_url()
            .ok_or_else(|| "no relay configured for remote access".to_string())?;
        let identity = self.identity().await?;
        let otp = offer::generate_otp();
        let psk = offer::psk_from_otp(&otp);
        {
            let mut offers = lock(&self.offers);
            let now = Instant::now();
            offers.retain(|_, offer| offer.expires_at > now && !offer.consumed);
            offers.insert(
                offer::pair_id(&otp),
                PendingOffer {
                    psk,
                    expires_at: now + ttl,
                    consumed: false,
                },
            );
        }
        self.audit("relay_offer", None, None).await;
        self.wake.notify_waiters();
        Ok(OfferView {
            url: PairingOffer {
                relay,
                room: identity.room_id.clone(),
                desktop_public: identity.keypair.public,
                otp,
            }
            .to_url(),
            expires_at: rfc3339_in(ttl),
            desktop_fingerprint: identity.fingerprint(),
        })
    }

    pub fn pending_approvals(&self) -> Vec<ApprovalRequest> {
        lock(&self.approvals)
            .values()
            .map(|approval| approval.request.clone())
            .collect()
    }

    /// The person's answer to "Approve <device>?". False when the request is
    /// gone (answered, timed out, or the phone left).
    pub fn respond_approval(&self, id: &str, approve: bool) -> bool {
        let Some(approval) = lock(&self.approvals).remove(id) else {
            return false;
        };
        let delivered = approval.decide.send(approve).is_ok();
        let _ = self.events.send(RemoteEvent::ApprovalResolved {
            id: id.to_string(),
            approved: approve && delivered,
        });
        delivered
    }

    /// Drops the live connections of a revoked device right away.
    pub fn device_revoked(&self, device_id: &str) {
        for connection in lock(&self.connections).values() {
            if connection.device_id == device_id {
                let _ = connection.stop.send(Some(close::REVOKED));
            }
        }
        self.wake.notify_waiters();
    }

    async fn with_db<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = Connection::open(&path).map_err(|error| error.to_string())?;
            conn.busy_timeout(Duration::from_secs(5))
                .map_err(|error| error.to_string())?;
            work(&conn)
        })
        .await
        .map_err(|error| error.to_string())?
    }

    async fn identity(&self) -> Result<Arc<RelayIdentity>, String> {
        if let Some(identity) = lock(&self.identity).clone() {
            return Ok(identity);
        }
        let identity = Arc::new(self.with_db(load_or_create_identity).await?);
        *lock(&self.identity) = Some(identity.clone());
        Ok(identity)
    }

    async fn audit(&self, event: &'static str, device_id: Option<String>, details: Option<Value>) {
        let _ = self
            .with_db(move |conn| {
                pairing::record_audit(
                    conn,
                    event,
                    device_id.as_deref(),
                    None,
                    None,
                    details.map(|value| value.to_string()).as_deref(),
                );
                Ok(())
            })
            .await;
    }

    fn has_open_offer(&self) -> bool {
        let now = Instant::now();
        lock(&self.offers)
            .values()
            .any(|offer| !offer.consumed && offer.expires_at > now)
    }

    /// What the supervisor should connect to now, if anything.
    async fn plan(&self) -> Option<(String, Arc<RelayIdentity>)> {
        let url = self.relay_url()?;
        let needed = self.has_open_offer()
            || !lock(&self.approvals).is_empty()
            || !lock(&self.connections).is_empty()
            || self
                .with_db(|conn| {
                    pairing::has_active_relay_devices(conn).map_err(|error| error.to_string())
                })
                .await
                .unwrap_or(false);
        if !needed {
            return None;
        }
        match self.identity().await {
            Ok(identity) => Some((url, identity)),
            Err(error) => {
                eprintln!("[DCC Remote] relay identity unavailable: {error}");
                None
            }
        }
    }

    fn set_connected(&self, connected: bool) {
        if self.connected.swap(connected, Ordering::SeqCst) != connected {
            let _ = self.events.send(RemoteEvent::StatusChanged { connected });
        }
    }

    async fn supervise(self: Arc<Self>) {
        let mut backoff = BACKOFF_MIN;
        loop {
            let woken = self.wake.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();
            let Some((url, identity)) = self.plan().await else {
                self.set_connected(false);
                tokio::select! {
                    _ = &mut woken => {}
                    _ = tokio::time::sleep(IDLE_RECHECK_INTERVAL) => {}
                }
                continue;
            };
            match self.connect_control(&url, &identity).await {
                Ok(socket) => {
                    backoff = BACKOFF_MIN;
                    self.set_connected(true);
                    eprintln!("[DCC Remote] connected to the relay");
                    let ended = self.clone().run_control(socket, url, identity).await;
                    self.set_connected(false);
                    if ended == ControlEnd::Stop {
                        continue;
                    }
                    eprintln!("[DCC Remote] relay connection lost; reconnecting");
                }
                Err(error) => {
                    eprintln!("[DCC Remote] cannot reach the relay: {error}");
                }
            }
            let delay = jittered(backoff);
            backoff = (backoff * 2).min(BACKOFF_MAX);
            let woken = self.wake.notified();
            tokio::select! {
                _ = woken => {}
                _ = tokio::time::sleep(delay) => {}
            }
        }
    }

    async fn connect_control(&self, url: &str, identity: &RelayIdentity) -> Result<Socket, String> {
        connect(
            &format!("{url}/v1/rooms/{}/control", identity.room_id),
            &identity.room_secret,
        )
        .await
    }

    async fn run_control(
        self: Arc<Self>,
        socket: Socket,
        url: String,
        identity: Arc<RelayIdentity>,
    ) -> ControlEnd {
        let (mut sink, mut stream) = socket.split();
        let mut ping = tokio::time::interval(CONTROL_PING_INTERVAL);
        ping.tick().await;
        loop {
            let woken = self.wake.notified();
            tokio::select! {
                message = stream.next() => match message {
                    Some(Ok(Message::Text(text))) => {
                        let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else { continue };
                        if value["type"] == "connected" {
                            if let Some(connection_id) = value["connectionId"].as_str() {
                                if is_connection_id(connection_id) {
                                    tokio::spawn(self.clone().serve_phone(
                                        url.clone(),
                                        identity.clone(),
                                        connection_id.to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return ControlEnd::Lost,
                    Some(Ok(_)) => {}
                },
                _ = ping.tick() => {
                    if sink.send(Message::Text("ping".into())).await.is_err() {
                        return ControlEnd::Lost;
                    }
                    // Also catches a wake-up that fell between two loop turns.
                    if self.plan().await.as_ref().map(|(next, _)| next) != Some(&url) {
                        let _ = sink.send(Message::Close(None)).await;
                        return ControlEnd::Stop;
                    }
                }
                _ = woken => {
                    let still = self.plan().await;
                    if still.as_ref().map(|(next, _)| next) != Some(&url) {
                        let _ = sink.send(Message::Close(None)).await;
                        return ControlEnd::Stop;
                    }
                }
            }
        }
    }

    async fn serve_phone(
        self: Arc<Self>,
        url: String,
        identity: Arc<RelayIdentity>,
        connection_id: String,
    ) {
        let socket = match connect(
            &format!("{url}/v1/rooms/{}/data/{connection_id}", identity.room_id),
            &identity.room_secret,
        )
        .await
        {
            Ok(socket) => socket,
            Err(error) => {
                eprintln!("[DCC Remote] could not open a phone connection: {error}");
                return;
            }
        };
        let (mut sink, mut stream) = socket.split();
        if let Err(code) = self.handshake(&mut sink, &mut stream, &identity).await {
            close_with(&mut sink, code).await;
        }
    }

    /// Runs the handshake and, on success, the channel. Returns the close code
    /// when the phone is refused.
    async fn handshake(
        self: &Arc<Self>,
        sink: &mut Sink,
        stream: &mut Stream,
        identity: &RelayIdentity,
    ) -> Result<(), u16> {
        let first = tokio::time::timeout(HANDSHAKE_TIMEOUT, next_binary(stream))
            .await
            .map_err(|_| close::PROTOCOL)?
            .ok_or(close::PROTOCOL)?;
        let WireMessage::HandshakeInit { mode, noise } =
            wire::decode(&first).map_err(|_| close::PROTOCOL)?
        else {
            return Err(close::PROTOCOL);
        };
        let mut responder =
            ResponderHandshake::new(mode, &identity.keypair).map_err(|_| close::PROTOCOL)?;
        let hello = responder.read_hello(noise).map_err(|_| close::PROTOCOL)?;
        let phone_key = responder.remote_static().ok_or(close::PROTOCOL)?;
        let fingerprint = offer::fingerprint(&phone_key);

        match mode {
            Mode::Resume => {
                let key = phone_key.to_vec();
                let device = self
                    .with_db(move |conn| {
                        pairing::find_relay_device(conn, &key).map_err(|error| error.to_string())
                    })
                    .await
                    .map_err(|_| close::PROTOCOL)?;
                let Some(device) = device.filter(|device| !device.revoked) else {
                    self.audit(
                        "relay_refused",
                        None,
                        Some(json!({ "fingerprint": fingerprint })),
                    )
                    .await;
                    return Err(close::UNKNOWN_DEVICE);
                };
                let (message, mut channel) = responder
                    .respond(&ServerHello {
                        v: PROTOCOL_VERSION,
                        device_id: Some(device.device_id.clone()),
                    })
                    .map_err(|_| close::PROTOCOL)?;
                send_raw(sink, wire::handshake_response(&message))
                    .await
                    .map_err(|_| close::PROTOCOL)?;
                let device_id = device.device_id.clone();
                let _ = self
                    .with_db(move |conn| {
                        pairing::touch_device(conn, &device_id);
                        Ok(())
                    })
                    .await;
                self.run_channel(sink, stream, &mut channel, device.device_id, Vec::new())
                    .await;
                Ok(())
            }
            Mode::Pair => {
                let pair_id = hello.pair_id.clone().ok_or(close::PROTOCOL)?;
                let Some(psk) = self.offer_psk(&pair_id) else {
                    self.audit(
                        "relay_offer_invalid",
                        None,
                        Some(json!({ "fingerprint": fingerprint })),
                    )
                    .await;
                    return Err(close::OFFER_INVALID);
                };
                responder.set_psk(&psk).map_err(|_| close::PROTOCOL)?;
                let (message, mut channel) = responder
                    .respond(&ServerHello {
                        v: PROTOCOL_VERSION,
                        device_id: None,
                    })
                    .map_err(|_| close::PROTOCOL)?;
                send_raw(sink, wire::handshake_response(&message))
                    .await
                    .map_err(|_| close::PROTOCOL)?;

                // Only a phone holding the QR secret can produce a transport
                // message we can open.
                let proof = tokio::time::timeout(HANDSHAKE_TIMEOUT, next_binary(stream))
                    .await
                    .map_err(|_| close::PROTOCOL)?
                    .ok_or(close::PROTOCOL)?;
                let mut queued = Vec::new();
                match channel.open_wire(&proof) {
                    Ok(Some(bytes)) => queued.extend(Frame::decode(&bytes).ok()),
                    Ok(None) => {}
                    Err(_) => return Err(close::PROTOCOL),
                }
                if !self.consume_offer(&pair_id) {
                    return Err(close::OFFER_INVALID);
                }

                let key = phone_key.to_vec();
                let (already_paired, active) = self
                    .with_db(move |conn| {
                        let existing = pairing::find_relay_device(conn, &key)
                            .map_err(|error| error.to_string())?;
                        let active = pairing::active_device_count(conn)
                            .map_err(|error| error.to_string())?;
                        Ok((existing.is_some_and(|device| !device.revoked), active))
                    })
                    .await
                    .map_err(|_| close::PROTOCOL)?;
                if !already_paired && active >= pairing::MAX_ACTIVE_DEVICES {
                    return Err(close::DEVICE_LIMIT);
                }

                send_frame(
                    sink,
                    &mut channel,
                    &Frame::Pairing(Pairing {
                        status: PairingStatus::Pending,
                        device_id: None,
                    }),
                )
                .await
                .map_err(|_| close::PROTOCOL)?;

                let device = hello.device.clone().unwrap_or(DeviceInfo {
                    name: String::new(),
                    platform: None,
                });
                let (approval_id, decision) = self.request_approval(&device, &fingerprint);
                let approved = self
                    .await_approval(stream, &mut channel, decision, &mut queued)
                    .await;
                // Answered requests are already gone; a timeout or a phone
                // that left must not leave the dialog (or the connector) up.
                self.cancel_approval(&approval_id);
                let Some(approved) = approved else {
                    // The phone left or broke the channel while waiting.
                    return Ok(());
                };
                if !approved {
                    self.audit(
                        "relay_pair_denied",
                        None,
                        Some(json!({ "device_name": device.name, "fingerprint": fingerprint })),
                    )
                    .await;
                    let _ = send_frame(
                        sink,
                        &mut channel,
                        &Frame::Pairing(Pairing {
                            status: PairingStatus::Denied,
                            device_id: None,
                        }),
                    )
                    .await;
                    return Err(close::PAIRING_DENIED);
                }

                let key = phone_key.to_vec();
                let name = device.name.clone();
                let platform = device.platform.clone();
                let print = fingerprint.clone();
                let registered = self
                    .with_db(move |conn| {
                        pairing::register_relay_device(
                            conn,
                            &key,
                            &name,
                            platform.as_deref(),
                            &print,
                        )
                        .map_err(|error| match error {
                            PairingError::DeviceLimitReached => "limit".to_string(),
                            other => other.to_string(),
                        })
                    })
                    .await;
                let device_id = match registered {
                    Ok(device_id) => device_id,
                    Err(error) if error == "limit" => return Err(close::DEVICE_LIMIT),
                    Err(error) => {
                        eprintln!("[DCC Remote] could not save the paired device: {error}");
                        return Err(close::PROTOCOL);
                    }
                };
                send_frame(
                    sink,
                    &mut channel,
                    &Frame::Pairing(Pairing {
                        status: PairingStatus::Approved,
                        device_id: Some(device_id.clone()),
                    }),
                )
                .await
                .map_err(|_| close::PROTOCOL)?;
                // From now on the supervisor keeps the relay connection up.
                self.wake.notify_waiters();
                self.run_channel(sink, stream, &mut channel, device_id, queued)
                    .await;
                Ok(())
            }
        }
    }

    fn offer_psk(&self, pair_id: &str) -> Option<[u8; 32]> {
        let offers = lock(&self.offers);
        let offer = offers.get(pair_id)?;
        (!offer.consumed && offer.expires_at > Instant::now()).then_some(offer.psk)
    }

    /// Marks the offer used; false when another phone got there first or it
    /// expired meanwhile.
    fn consume_offer(&self, pair_id: &str) -> bool {
        let mut offers = lock(&self.offers);
        match offers.get_mut(pair_id) {
            Some(offer) if !offer.consumed && offer.expires_at > Instant::now() => {
                offer.consumed = true;
                true
            }
            _ => false,
        }
    }

    fn request_approval(
        &self,
        device: &DeviceInfo,
        fingerprint: &str,
    ) -> (String, oneshot::Receiver<bool>) {
        let (decide, decision) = oneshot::channel();
        let name = device.name.trim();
        let request = ApprovalRequest {
            id: uuid::Uuid::new_v4().to_string(),
            device_name: if name.is_empty() {
                "Unnamed device".to_string()
            } else {
                name.chars().take(80).collect()
            },
            platform: device.platform.clone(),
            fingerprint: fingerprint.to_string(),
            expires_at: rfc3339_in(APPROVAL_TIMEOUT),
        };
        let id = request.id.clone();
        lock(&self.approvals).insert(
            id.clone(),
            PendingApproval {
                request: request.clone(),
                decide,
            },
        );
        let _ = self
            .events
            .send(RemoteEvent::ApprovalRequested { approval: request });
        (id, decision)
    }

    fn cancel_approval(&self, id: &str) {
        if lock(&self.approvals).remove(id).is_some() {
            let _ = self.events.send(RemoteEvent::ApprovalResolved {
                id: id.to_string(),
                approved: false,
            });
        }
    }

    /// Waits for the person while still reading the phone (its frames must
    /// be decrypted in order). `None` when the phone went away.
    async fn await_approval(
        &self,
        stream: &mut Stream,
        channel: &mut SecureChannel,
        mut decision: oneshot::Receiver<bool>,
        queued: &mut Vec<Frame>,
    ) -> Option<bool> {
        let deadline = tokio::time::sleep(APPROVAL_TIMEOUT);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                answer = &mut decision => return Some(answer.unwrap_or(false)),
                _ = &mut deadline => return Some(false),
                message = stream.next() => match message {
                    Some(Ok(Message::Binary(bytes))) => match channel.open_wire(&bytes) {
                        Ok(Some(plain)) => {
                            if queued.len() >= MAX_FRAMES_BEFORE_APPROVAL {
                                return None;
                            }
                            queued.extend(Frame::decode(&plain).ok());
                        }
                        Ok(None) => {}
                        Err(_) => return None,
                    },
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
                    Some(Ok(_)) => {}
                },
            }
        }
    }

    async fn run_channel(
        self: &Arc<Self>,
        sink: &mut Sink,
        stream: &mut Stream,
        channel: &mut SecureChannel,
        device_id: String,
        queued: Vec<Frame>,
    ) {
        let (tx, mut rx) = mpsc::channel::<Frame>(OUTBOUND_QUEUE);
        let (stop, mut stopped) = watch::channel(None::<u16>);
        let stop = Arc::new(stop);
        let connection = self.next_connection.fetch_add(1, Ordering::SeqCst);
        lock(&self.connections).insert(
            connection,
            LiveConnection {
                device_id: device_id.clone(),
                stop: stop.clone(),
            },
        );
        let context = Arc::new(ChannelContext {
            router: self.router.clone(),
            hub: Arc::downgrade(self),
            device_id: device_id.clone(),
            stop: stop.clone(),
        });
        let mut tasks: HashMap<u64, AbortHandle> = HashMap::new();
        for frame in queued {
            dispatch(&context, frame, &tx, &mut tasks);
        }
        let mut recheck = tokio::time::interval(DEVICE_RECHECK_INTERVAL);
        recheck.tick().await;

        let close_code = loop {
            tokio::select! {
                message = stream.next() => match message {
                    Some(Ok(Message::Binary(bytes))) => match channel.open_wire(&bytes) {
                        Ok(Some(plain)) => match Frame::decode(&plain) {
                            Ok(frame) => dispatch(&context, frame, &tx, &mut tasks),
                            Err(_) => break Some(close::PROTOCOL),
                        },
                        Ok(None) => {}
                        Err(_) => break Some(close::PROTOCOL),
                    },
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break None,
                    Some(Ok(_)) => {}
                },
                outbound = rx.recv() => {
                    let Some(frame) = outbound else { break None };
                    if send_frame(sink, channel, &frame).await.is_err() {
                        break None;
                    }
                }
                _ = stopped.changed() => break *stopped.borrow(),
                _ = recheck.tick() => {
                    if !context.device_active().await {
                        break Some(close::REVOKED);
                    }
                }
            }
        };
        for task in tasks.values() {
            task.abort();
        }
        lock(&self.connections).remove(&connection);
        if let Some(code) = close_code {
            close_with(sink, code).await;
        }
        self.wake.notify_waiters();
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ControlEnd {
    /// Nothing to serve any more (or the relay URL changed).
    Stop,
    Lost,
}

fn jittered(base: Duration) -> Duration {
    use rand::Rng;
    let jitter = rand::thread_rng().gen_range(0..=base.as_millis() as u64 / 2);
    base + Duration::from_millis(jitter)
}

fn is_connection_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn tls_connector() -> Connector {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    let config = CONFIG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs().certs {
            let _ = roots.add(cert);
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("ring supports the default TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(config)
    });
    Connector::Rustls(config.clone())
}

async fn connect(url: &str, room_secret: &str) -> Result<Socket, String> {
    let mut request = url
        .into_client_request()
        .map_err(|error| error.to_string())?;
    request.headers_mut().insert(
        ROOM_SECRET_HEADER,
        room_secret
            .parse()
            .map_err(|_| "invalid room secret".to_string())?,
    );
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(wire::MAX_WIRE_MESSAGE + 1024);
    config.max_frame_size = Some(wire::MAX_WIRE_MESSAGE + 1024);
    let connector = url.starts_with("wss://").then(tls_connector);
    let (socket, _) = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        tokio_tungstenite::connect_async_tls_with_config(request, Some(config), false, connector),
    )
    .await
    .map_err(|_| "timed out".to_string())?
    .map_err(|error| error.to_string())?;
    Ok(socket)
}

async fn next_binary(stream: &mut Stream) -> Option<Vec<u8>> {
    loop {
        match stream.next().await? {
            Ok(Message::Binary(bytes)) => return Some(bytes.to_vec()),
            Ok(Message::Close(_)) | Err(_) => return None,
            Ok(_) => {}
        }
    }
}

async fn send_raw(sink: &mut Sink, bytes: Vec<u8>) -> Result<(), String> {
    sink.send(Message::Binary(bytes.into()))
        .await
        .map_err(|error| error.to_string())
}

async fn send_frame(
    sink: &mut Sink,
    channel: &mut SecureChannel,
    frame: &Frame,
) -> Result<(), String> {
    let encoded = frame.encode().map_err(|error| error.to_string())?;
    for wire in channel.seal(&encoded).map_err(|error| error.to_string())? {
        send_raw(sink, wire).await?;
    }
    Ok(())
}

async fn close_with(sink: &mut Sink, code: u16) {
    let _ = sink
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::from(code),
            reason: "".into(),
        })))
        .await;
    let _ = sink.close().await;
}

// ---------- frames → router ----------

struct ChannelContext {
    router: Router,
    hub: std::sync::Weak<RemoteHub>,
    device_id: String,
    stop: Arc<watch::Sender<Option<u16>>>,
}

impl ChannelContext {
    async fn device_active(&self) -> bool {
        let Some(hub) = self.hub.upgrade() else {
            return false;
        };
        let device_id = self.device_id.clone();
        hub.with_db(move |conn| {
            pairing::device_is_active(conn, &device_id).map_err(|error| error.to_string())
        })
        .await
        // A transient database error does not drop a phone.
        .unwrap_or(true)
    }

    /// Checked before each request too, so a revocation from another
    /// process (or `dccd-http`) takes effect on the next frame.
    async fn ensure_active(&self) -> bool {
        if self.device_active().await {
            return true;
        }
        let _ = self.stop.send(Some(close::REVOKED));
        false
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        headers: &BTreeMap<String, String>,
        body: Vec<u8>,
    ) -> Result<axum::http::Request<Body>, String> {
        let mut builder = axum::http::Request::builder().method(method).uri(path);
        for (name, value) in headers {
            if !forwardable_header(name) {
                continue;
            }
            builder = builder.header(name.as_str(), value.as_str());
        }
        let mut request = builder
            .body(Body::from(body))
            .map_err(|error| error.to_string())?;
        request
            .extensions_mut()
            .insert(PairedDeviceIdentity(self.device_id.clone()));
        Ok(request)
    }

    async fn get_json(&self, path: &str) -> Result<Value, String> {
        let request = self.request("GET", path, &BTreeMap::new(), Vec::new())?;
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("{path} answered {}", response.status()));
        }
        let bytes = axum::body::to_bytes(response.into_body(), MAX_RESPONSE_BODY)
            .await
            .map_err(|error| error.to_string())?;
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())
    }
}

/// Phones reach the session API only: never pairing, device management,
/// remote administration or the compatibility RPC.
pub fn remote_path_allowed(path: &str) -> bool {
    let route = path.split(['?', '#']).next().unwrap_or_default();
    route.starts_with("/api/v1/")
        && !route.contains("..")
        && !route.contains("//")
        && !route.starts_with("/api/v1/auth/")
        && !route.starts_with("/api/v1/remote/")
        && !route.starts_with("/api/v1/terminals/")
}

fn forwardable_header(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "x-api-key"
            | "x-device-id"
            | "x-timestamp"
            | "x-signature"
            | "x-forwarded-for"
            | "host"
            | "connection"
            | "upgrade"
            | "transfer-encoding"
            | "content-length"
    )
}

fn error_response(id: u64, status: u16, message: &str) -> Frame {
    Frame::Res(FrameResponse {
        id,
        status,
        headers: BTreeMap::from([("content-type".to_string(), "application/json".to_string())]),
        body: FrameBody::from_bytes(
            json!({ "ok": false, "error": { "message": message } })
                .to_string()
                .into_bytes(),
        ),
    })
}

fn dispatch(
    context: &Arc<ChannelContext>,
    frame: Frame,
    tx: &mpsc::Sender<Frame>,
    tasks: &mut HashMap<u64, AbortHandle>,
) {
    tasks.retain(|_, task| !task.is_finished());
    match frame {
        Frame::Req(request) => {
            let id = request.id;
            if !remote_path_allowed(&request.path) {
                let _ = tx.try_send(error_response(id, 403, "not available remotely"));
                return;
            }
            let task = tokio::spawn(handle_request(context.clone(), request, tx.clone()));
            if let Some(previous) = tasks.insert(id, task.abort_handle()) {
                previous.abort();
            }
        }
        Frame::Sub(subscribe) => {
            let id = subscribe.id;
            let task = tokio::spawn(handle_subscription(context.clone(), subscribe, tx.clone()));
            if let Some(previous) = tasks.insert(id, task.abort_handle()) {
                previous.abort();
            }
        }
        Frame::Cancel { id } => {
            if let Some(task) = tasks.remove(&id) {
                task.abort();
            }
        }
        Frame::Ping(ping) => {
            let _ = tx.try_send(Frame::Pong(ping));
        }
        _ => {}
    }
}

async fn handle_request(
    context: Arc<ChannelContext>,
    request: FrameRequest,
    tx: mpsc::Sender<Frame>,
) {
    let id = request.id;
    if !context.ensure_active().await {
        return;
    }
    let body = match request.body.into_bytes() {
        Ok(body) => body,
        Err(_) => {
            let _ = tx.send(error_response(id, 400, "invalid body")).await;
            return;
        }
    };
    let http_request = match context.request(&request.method, &request.path, &request.headers, body)
    {
        Ok(request) => request,
        Err(error) => {
            let _ = tx.send(error_response(id, 400, &error)).await;
            return;
        }
    };
    let response = match context.router.clone().oneshot(http_request).await {
        Ok(response) => response,
        Err(error) => {
            let _ = tx.send(error_response(id, 500, &error.to_string())).await;
            return;
        }
    };
    let status = response.status().as_u16();
    let mut headers = BTreeMap::new();
    for (name, value) in response.headers() {
        if let Ok(value) = value.to_str() {
            headers
                .entry(name.as_str().to_string())
                .and_modify(|existing: &mut String| {
                    existing.push_str(", ");
                    existing.push_str(value);
                })
                .or_insert_with(|| value.to_string());
        }
    }
    let frame = match axum::body::to_bytes(response.into_body(), MAX_RESPONSE_BODY).await {
        Ok(bytes) => Frame::Res(FrameResponse {
            id,
            status,
            headers,
            body: FrameBody::from_bytes(bytes.to_vec()),
        }),
        Err(_) => error_response(id, 502, "response too large for the remote channel"),
    };
    let _ = tx.send(frame).await;
}

/// `path` without `after`, plus the cursor and session it named.
fn split_stream_path(path: &str) -> Option<(String, Option<u64>, Option<String>)> {
    let url = url::Url::parse(&format!("http://remote{path}")).ok()?;
    if url.path() != "/api/v1/events/stream" {
        return None;
    }
    let mut after = None;
    let mut session = None;
    let mut kept = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "after" => after = value.parse().ok(),
            "sessionId" => {
                session = Some(value.to_string());
                kept.append_pair(&key, &value);
            }
            _ => {
                kept.append_pair(&key, &value);
            }
        }
    }
    let query = kept.finish();
    let stripped = if query.is_empty() {
        url.path().to_string()
    } else {
        format!("{}?{query}", url.path())
    };
    Some((stripped, after, session.filter(|id| !id.is_empty())))
}

async fn handle_subscription(
    context: Arc<ChannelContext>,
    subscribe: Subscribe,
    tx: mpsc::Sender<Frame>,
) {
    let id = subscribe.id;
    let end = |error: Option<String>| Frame::End(End { id, error });
    let Some((path, after, session)) = split_stream_path(&subscribe.path) else {
        let _ = tx
            .send(end(Some(
                "only /api/v1/events/stream can be subscribed".into(),
            )))
            .await;
        return;
    };
    if !context.ensure_active().await {
        return;
    }
    // Subscribe first, replay second: nothing falls between the two. Live
    // events the replay already covered are dropped by sequence below.
    let request = match context.request("GET", &path, &BTreeMap::new(), Vec::new()) {
        Ok(request) => request,
        Err(error) => {
            let _ = tx.send(end(Some(error))).await;
            return;
        }
    };
    let response = match context.router.clone().oneshot(request).await {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            let _ = tx
                .send(end(Some(format!("stream answered {}", response.status()))))
                .await;
            return;
        }
        Err(error) => {
            let _ = tx.send(end(Some(error.to_string()))).await;
            return;
        }
    };

    let mut replayed_up_to = None;
    if let (Some(mut cursor), Some(session)) = (after, session.as_deref()) {
        let session = url::form_urlencoded::byte_serialize(session.as_bytes()).collect::<String>();
        loop {
            let page = match context
                .get_json(&format!(
                    "/api/v1/sessions/{session}/events?after={cursor}&limit={REPLAY_PAGE}"
                ))
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    let _ = tx.send(end(Some(error))).await;
                    return;
                }
            };
            for event in page["events"].as_array().cloned().unwrap_or_default() {
                let seq = event["sequence"].as_u64();
                if let Some(seq) = seq {
                    cursor = cursor.max(seq);
                }
                let frame = Frame::Evt(StreamEvent {
                    id,
                    event: "replay".into(),
                    data: event,
                    seq,
                });
                if tx.send(frame).await.is_err() {
                    return;
                }
            }
            if page["hasMore"].as_bool() != Some(true) {
                break;
            }
        }
        replayed_up_to = Some(cursor);
    }

    let mut body = response.into_body().into_data_stream();
    let mut parser = SseParser::new();
    let mut coalescer = DeltaCoalescer::new(DEFAULT_WINDOW);
    loop {
        let deadline = coalescer.deadline();
        let ready = tokio::select! {
            chunk = body.next() => match chunk {
                Some(Ok(bytes)) => match parser.feed(&bytes) {
                    Ok(events) => {
                        let mut ready = Vec::new();
                        for event in events {
                            let seq = event.id.as_deref().and_then(|id| id.parse::<u64>().ok());
                            if let (Some(seq), Some(limit)) = (seq, replayed_up_to) {
                                if seq <= limit {
                                    continue;
                                }
                            }
                            let data = serde_json::from_str(&event.data)
                                .unwrap_or(Value::String(event.data));
                            ready.extend(coalescer.push(
                                OutgoingEvent {
                                    event: event.event.unwrap_or_else(|| "message".into()),
                                    data,
                                    seq,
                                },
                                Instant::now(),
                            ));
                        }
                        ready
                    }
                    Err(error) => {
                        let _ = tx.send(end(Some(error.into()))).await;
                        return;
                    }
                },
                Some(Err(_)) | None => {
                    for event in coalescer.flush() {
                        let _ = tx.send(Frame::Evt(StreamEvent { id, event: event.event, data: event.data, seq: event.seq })).await;
                    }
                    let _ = tx.send(end(None)).await;
                    return;
                }
            },
            _ = async {
                match deadline {
                    Some(deadline) => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
                    None => std::future::pending().await,
                }
            } => coalescer.flush(),
        };
        for event in ready {
            let frame = Frame::Evt(StreamEvent {
                id,
                event: event.event,
                data: event.data,
                seq: event.seq,
            });
            if tx.send(frame).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_only_reach_the_session_api() {
        assert!(remote_path_allowed("/api/v1/sessions"));
        assert!(remote_path_allowed("/api/v1/sessions/abc/events?after=3"));
        assert!(remote_path_allowed("/api/v1/events/stream?sessionId=x"));
        assert!(!remote_path_allowed("/auth/devices"));
        assert!(!remote_path_allowed("/auth/pair-init"));
        assert!(!remote_path_allowed("/api/v1/auth/bearer/rotate"));
        assert!(!remote_path_allowed("/api/v1/remote/pairing/offers"));
        assert!(!remote_path_allowed("/api/v1/terminals/spawn"));
        assert!(!remote_path_allowed("/rpc"));
        assert!(!remote_path_allowed("/api/v1/../auth/devices"));
        assert!(!remote_path_allowed("http://elsewhere/api/v1/sessions"));
    }

    #[test]
    fn stream_paths_keep_the_session_and_lift_the_cursor() {
        assert_eq!(
            split_stream_path("/api/v1/events/stream?sessionId=s%201&after=42"),
            Some((
                "/api/v1/events/stream?sessionId=s+1".to_string(),
                Some(42),
                Some("s 1".to_string())
            ))
        );
        assert_eq!(
            split_stream_path("/api/v1/events/stream"),
            Some(("/api/v1/events/stream".to_string(), None, None))
        );
        assert_eq!(split_stream_path("/api/v1/sessions"), None);
    }

    #[test]
    fn relay_urls_must_be_websockets() {
        assert_eq!(
            validate_relay_url("wss://relay.example/").unwrap(),
            "wss://relay.example"
        );
        assert!(validate_relay_url("ws://127.0.0.1:8787").is_ok());
        assert!(validate_relay_url("ws://relay.example").is_err());
        assert!(validate_relay_url("https://relay.example").is_err());
        assert!(validate_relay_url("wss://relay.example/?x=1").is_err());
    }

    fn hub() -> (tempfile::TempDir, Arc<RemoteHub>) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db.sqlite");
        let hub = RemoteHub::new(Router::new(), db).unwrap();
        (dir, hub)
    }

    #[tokio::test]
    async fn the_project_relay_is_the_default_until_changed_or_turned_off() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db.sqlite");
        let reopen = || RemoteHub::new(Router::new(), db.clone()).unwrap();

        assert_eq!(reopen().relay_url().as_deref(), Some(DEFAULT_RELAY_URL));

        reopen().set_relay_url(None).await.unwrap();
        assert_eq!(reopen().relay_url(), None, "turning it off sticks");

        reopen()
            .set_relay_url(Some("wss://relay.example".into()))
            .await
            .unwrap();
        assert_eq!(reopen().relay_url().as_deref(), Some("wss://relay.example"));
    }

    #[tokio::test]
    async fn offers_are_single_use_and_need_a_relay() {
        let (_dir, hub) = hub();
        hub.set_relay_url(None).await.unwrap();
        assert!(hub.create_offer().await.is_err());
        hub.set_relay_url(Some("ws://127.0.0.1:8787".into()))
            .await
            .unwrap();
        let view = hub.create_offer().await.unwrap();
        let parsed = PairingOffer::parse(&view.url).unwrap();
        let pair_id = offer::pair_id(&parsed.otp);
        assert_eq!(
            hub.offer_psk(&pair_id),
            Some(offer::psk_from_otp(&parsed.otp))
        );
        assert!(hub.consume_offer(&pair_id));
        assert!(!hub.consume_offer(&pair_id));
        assert_eq!(hub.offer_psk(&pair_id), None);
        assert_eq!(hub.offer_psk("unknown"), None);
        // The identity persists: a new hub on the same database keeps the key.
        let status = hub.status();
        assert_eq!(
            status.desktop_fingerprint.as_deref(),
            Some(view.desktop_fingerprint.as_str())
        );
    }

    #[tokio::test]
    async fn approvals_resolve_once() {
        let (_dir, hub) = hub();
        let mut events = hub.subscribe();
        let (id, decision) = hub.request_approval(
            &DeviceInfo {
                name: "Pixel".into(),
                platform: Some("android".into()),
            },
            "AAAA-BBBB-CCCC",
        );
        assert!(matches!(
            events.recv().await.unwrap(),
            RemoteEvent::ApprovalRequested { .. }
        ));
        assert_eq!(hub.pending_approvals().len(), 1);
        assert!(hub.respond_approval(&id, true));
        assert!(decision.await.unwrap());
        assert!(!hub.respond_approval(&id, false));
        assert!(hub.pending_approvals().is_empty());
    }
}
