//! This DCC reaching another one (a "machine") through the relay: the
//! socket and the task that drives `dcc_remote::client` over it. It is the
//! other end of `remote_connector`, which answers on the machine.
//!
//! - [`MachineLink::pair`] takes the link the machine showed
//!   (`dcc://pair#…`), proves it holds its secret and waits for the person
//!   there to approve this computer.
//! - [`MachineLink::connect`] reconnects with the approved key.
//! - [`MachineLink::request`] is one call of the machine's HTTP API;
//!   [`MachineLink::subscribe`] one of its event streams.
//!
//! The relay only forwards bytes; everything is encrypted end to end with
//! the keys the two machines pinned when pairing.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use dcc_remote::client::{ClientSession, Connecting};
use dcc_remote::frames::{Frame, PairingStatus, Ping, StreamEvent};
use dcc_remote::noise::{DeviceInfo, StaticKeypair, KEY_LEN};
use dcc_remote::offer::PairingOffer;
use dcc_remote::wire::close;
use dcc_remote::RemoteError;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// The relay connection and the handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// The relay's own close when the machine's side is not connected to it.
/// Right after a machine opens a pairing link (or restarts) it is still
/// connecting, so this is tried again a few times before giving up.
const RELAY_MACHINE_OFFLINE: u16 = 4603;
const OFFLINE_ATTEMPTS: usize = 6;
const OFFLINE_WAIT: Duration = Duration::from_secs(1);
/// The person on the machine has this long to approve a pairing.
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(180);

/// Where a machine answers: its room on the relay and its pinned key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineTarget {
    pub relay: String,
    pub room: String,
    pub public_key: [u8; KEY_LEN],
}

impl MachineTarget {
    pub fn of(offer: &PairingOffer) -> Self {
        Self {
            relay: offer.relay.clone(),
            room: offer.room.clone(),
            public_key: offer.desktop_public,
        }
    }

    fn url(&self) -> String {
        format!(
            "{}/v1/rooms/{}/connect",
            self.relay.trim_end_matches('/'),
            self.room
        )
    }
}

/// This computer as the machine sees it: its key and the name the person
/// there reads when approving. `platform` names the system (`macos`,
/// `linux`), which is what makes it a desktop on the other side.
pub struct LinkIdentity {
    pub keys: StaticKeypair,
    pub name: String,
    pub platform: String,
}

impl LinkIdentity {
    fn device(&self) -> DeviceInfo {
        DeviceInfo {
            name: self.name.clone(),
            platform: Some(self.platform.clone()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("could not reach the relay: {0}")]
    Relay(String),
    #[error("timed out")]
    Timeout,
    /// The machine is not connected to the relay: it is off, asleep or
    /// without network.
    #[error("the machine is offline")]
    Offline,
    /// The machine closed the channel with one of `wire::close`'s codes.
    #[error("the machine refused the connection ({0})")]
    Refused(u16),
    #[error("pairing was refused on the machine")]
    Denied,
    #[error("the connection to the machine closed")]
    Closed,
    #[error(transparent)]
    Protocol(#[from] RemoteError),
}

/// A response of the machine's API.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// One item of a subscription: an event, or its end (with the machine's
/// reason when it ended in error).
#[derive(Debug, Clone, PartialEq)]
pub enum StreamItem {
    Event(StreamEvent),
    End(Option<String>),
}

enum Command {
    Request {
        method: String,
        path: String,
        headers: BTreeMap<String, String>,
        body: Vec<u8>,
        reply: oneshot::Sender<Result<LinkResponse, LinkError>>,
    },
    Subscribe {
        path: String,
        events: mpsc::UnboundedSender<StreamItem>,
    },
}

/// An open channel to a machine. Cloning shares it; it closes when the last
/// clone is dropped or the machine leaves.
#[derive(Clone)]
pub struct MachineLink {
    commands: mpsc::UnboundedSender<Command>,
    device_id: String,
}

async fn send_all(socket: &mut Socket, wires: Vec<Vec<u8>>) -> Result<(), LinkError> {
    for wire in wires {
        socket
            .send(Message::Binary(wire.into()))
            .await
            .map_err(|_| LinkError::Closed)?;
    }
    Ok(())
}

/// The next binary message; a close becomes the machine's refusal.
async fn next_wire(socket: &mut Socket) -> Result<Vec<u8>, LinkError> {
    loop {
        match socket.next().await {
            Some(Ok(Message::Binary(bytes))) => return Ok(bytes.to_vec()),
            Some(Ok(Message::Close(frame))) => {
                return Err(match frame.map(|frame| u16::from(frame.code)) {
                    Some(close::PAIRING_DENIED) => LinkError::Denied,
                    Some(RELAY_MACHINE_OFFLINE) => LinkError::Offline,
                    Some(code) if code >= 4000 => LinkError::Refused(code),
                    _ => LinkError::Closed,
                });
            }
            Some(Ok(_)) => continue,
            Some(Err(_)) | None => return Err(LinkError::Closed),
        }
    }
}

/// The socket and the handshake, tried again while the relay says the
/// machine is not there yet. `start` writes a fresh first message each time.
async fn open(
    target: &MachineTarget,
    start: impl Fn() -> Result<(Connecting, Vec<u8>), RemoteError>,
) -> Result<(Socket, Option<String>, ClientSession), LinkError> {
    let mut attempt = 1;
    loop {
        let (connecting, hello) = start()?;
        match open_once(target, connecting, hello).await {
            Err(LinkError::Offline) if attempt < OFFLINE_ATTEMPTS => {
                attempt += 1;
                tokio::time::sleep(OFFLINE_WAIT).await;
            }
            other => return other,
        }
    }
}

async fn open_once(
    target: &MachineTarget,
    connecting: Connecting,
    hello: Vec<u8>,
) -> Result<(Socket, Option<String>, ClientSession), LinkError> {
    let handshake = async {
        let (mut socket, _) = tokio_tungstenite::connect_async(target.url())
            .await
            .map_err(|error| LinkError::Relay(error.to_string()))?;
        send_all(&mut socket, vec![hello]).await?;
        let answer = next_wire(&mut socket).await?;
        let (server, session) = connecting.finish(&answer)?;
        Ok::<_, LinkError>((socket, server.device_id, session))
    };
    tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake)
        .await
        .map_err(|_| LinkError::Timeout)?
}

impl MachineLink {
    /// Reconnects with a key the machine approved before.
    pub async fn connect(
        target: &MachineTarget,
        identity: &LinkIdentity,
    ) -> Result<Self, LinkError> {
        let (socket, device_id, session) = open(target, || {
            Connecting::resume(&identity.keys, &target.public_key, identity.device())
        })
        .await?;
        let device_id = device_id.ok_or(LinkError::Refused(close::UNKNOWN_DEVICE))?;
        Ok(Self::drive(socket, session, device_id))
    }

    /// Pairs from the machine's link and waits for its person to approve
    /// this computer. `on_pending` runs once the machine is asking them.
    pub async fn pair(
        offer: &PairingOffer,
        identity: &LinkIdentity,
        on_pending: impl FnOnce() + Send,
    ) -> Result<Self, LinkError> {
        let target = MachineTarget::of(offer);
        let (mut socket, _, mut session) = open(&target, || {
            Connecting::pair(offer, &identity.keys, identity.device())
        })
        .await?;
        // Proves to the machine that this side holds the link's secret.
        send_all(&mut socket, session.ping(None)?).await?;
        let mut on_pending = Some(on_pending);
        let approval = async {
            loop {
                let wire = next_wire(&mut socket).await?;
                let Some(Frame::Pairing(pairing)) = session.receive(&wire)? else {
                    continue;
                };
                match (pairing.status, pairing.device_id) {
                    (PairingStatus::Pending, _) => {
                        if let Some(notify) = on_pending.take() {
                            notify();
                        }
                    }
                    (PairingStatus::Approved, Some(device_id)) => return Ok(device_id),
                    _ => return Err(LinkError::Denied),
                }
            }
        };
        let device_id = tokio::time::timeout(APPROVAL_TIMEOUT, approval)
            .await
            .map_err(|_| LinkError::Timeout)??;
        Ok(Self::drive(socket, session, device_id))
    }

    /// The id the machine gave this computer.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// False once the machine left or the channel broke.
    pub fn is_open(&self) -> bool {
        !self.commands.is_closed()
    }

    /// One call of the machine's API.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        headers: BTreeMap<String, String>,
        body: Vec<u8>,
    ) -> Result<LinkResponse, LinkError> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Request {
                method: method.to_string(),
                path: path.to_string(),
                headers,
                body,
                reply,
            })
            .map_err(|_| LinkError::Closed)?;
        answer.await.map_err(|_| LinkError::Closed)?
    }

    /// One of the machine's event streams. Dropping the receiver closes it
    /// on the machine at its next event.
    pub fn subscribe(&self, path: &str) -> Result<mpsc::UnboundedReceiver<StreamItem>, LinkError> {
        let (events, stream) = mpsc::unbounded_channel();
        self.commands
            .send(Command::Subscribe {
                path: path.to_string(),
                events,
            })
            .map_err(|_| LinkError::Closed)?;
        Ok(stream)
    }

    fn drive(socket: Socket, session: ClientSession, device_id: String) -> Self {
        let (commands, inbox) = mpsc::unbounded_channel();
        tokio::spawn(run(socket, session, inbox));
        Self {
            commands,
            device_id,
        }
    }
}

/// Owns the socket: sends what the handles ask and hands each frame to
/// whoever waits for it, until either side leaves.
async fn run(
    mut socket: Socket,
    mut session: ClientSession,
    mut inbox: mpsc::UnboundedReceiver<Command>,
) {
    let mut pending: HashMap<u64, oneshot::Sender<Result<LinkResponse, LinkError>>> =
        HashMap::new();
    let mut streams: HashMap<u64, mpsc::UnboundedSender<StreamItem>> = HashMap::new();
    loop {
        tokio::select! {
            command = inbox.recv() => {
                let Some(command) = command else { break };
                let sent = match command {
                    Command::Request { method, path, headers, body, reply } => {
                        match session.request(&method, &path, headers, body) {
                            Ok((id, wires)) => {
                                pending.insert(id, reply);
                                send_all(&mut socket, wires).await
                            }
                            Err(error) => {
                                let _ = reply.send(Err(error.into()));
                                Ok(())
                            }
                        }
                    }
                    Command::Subscribe { path, events } => match session.subscribe(&path) {
                        Ok((id, wires)) => {
                            streams.insert(id, events);
                            send_all(&mut socket, wires).await
                        }
                        Err(error) => {
                            let _ = events.send(StreamItem::End(Some(error.to_string())));
                            Ok(())
                        }
                    },
                };
                if sent.is_err() {
                    break;
                }
            }
            wire = next_wire(&mut socket) => {
                let Ok(wire) = wire else { break };
                let frame = match session.receive(&wire) {
                    Ok(Some(frame)) => frame,
                    Ok(None) => continue,
                    Err(_) => break,
                };
                let reply = match frame {
                    Frame::Res(response) => {
                        if let Some(waiting) = pending.remove(&response.id) {
                            let _ = waiting.send(response.body.into_bytes().map_err(Into::into).map(
                                |body| LinkResponse {
                                    status: response.status,
                                    headers: response.headers,
                                    body,
                                },
                            ));
                        }
                        None
                    }
                    Frame::Evt(event) => {
                        let id = event.id;
                        let gone = streams
                            .get(&id)
                            .is_some_and(|events| events.send(StreamItem::Event(event)).is_err());
                        // Nobody reads it any more: tell the machine to stop.
                        gone.then(|| {
                            streams.remove(&id);
                            session.cancel(id)
                        })
                    }
                    Frame::End(end) => {
                        if let Some(events) = streams.remove(&end.id) {
                            let _ = events.send(StreamItem::End(end.error));
                        }
                        None
                    }
                    Frame::Ping(Ping { ts }) => Some(session.pong(ts)),
                    _ => None,
                };
                if let Some(wires) = reply {
                    let Ok(wires) = wires else { break };
                    if send_all(&mut socket, wires).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
    let _ = socket.close(None).await;
    for (_, waiting) in pending {
        let _ = waiting.send(Err(LinkError::Closed));
    }
    for (_, events) in streams {
        let _ = events.send(StreamItem::End(Some("the connection closed".into())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_remote::channel::SecureChannel;
    use dcc_remote::frames::{Body, End, Pairing, Response};
    use dcc_remote::noise::{ResponderHandshake, ServerHello};
    use dcc_remote::offer::{generate_otp, psk_from_otp};
    use dcc_remote::wire::{self, WireMessage};
    use dcc_remote::PROTOCOL_VERSION;
    use tokio::net::TcpListener;

    type Server = WebSocketStream<TcpStream>;

    async fn send(socket: &mut Server, channel: &mut SecureChannel, frame: Frame) {
        for wire in channel.seal(&frame.encode().unwrap()).unwrap() {
            socket.send(Message::Binary(wire.into())).await.unwrap();
        }
    }

    async fn read(socket: &mut Server, channel: &mut SecureChannel) -> Frame {
        loop {
            let Some(Ok(Message::Binary(bytes))) = socket.next().await else {
                panic!("the link left");
            };
            if let Some(plain) = channel.open_wire(&bytes).unwrap() {
                return Frame::decode(&plain).unwrap();
            }
        }
    }

    /// A machine in one socket: answers the handshake as `remote_connector`
    /// does (approving a pairing), then each request with its own path, and
    /// a subscription with one event and its end.
    async fn machine(listener: TcpListener, keys: StaticKeypair, otp: Option<[u8; 16]>) {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let Some(Ok(Message::Binary(hello))) = socket.next().await else {
            panic!("a handshake");
        };
        let WireMessage::HandshakeInit { mode, noise } = wire::decode(&hello).unwrap() else {
            panic!("a handshake init");
        };
        let mut responder = ResponderHandshake::new(mode, &keys).unwrap();
        let client = responder.read_hello(noise).unwrap();
        assert_eq!(client.device.unwrap().platform.as_deref(), Some("macos"));
        if let Some(otp) = &otp {
            responder.set_psk(&psk_from_otp(otp)).unwrap();
        }
        let (message, mut channel) = responder
            .respond(&ServerHello {
                v: PROTOCOL_VERSION,
                device_id: otp.is_none().then(|| "device-1".to_string()),
            })
            .unwrap();
        socket
            .send(Message::Binary(wire::handshake_response(&message).into()))
            .await
            .unwrap();
        if otp.is_some() {
            assert!(matches!(
                read(&mut socket, &mut channel).await,
                Frame::Ping(_)
            ));
            for (status, device_id) in [
                (PairingStatus::Pending, None),
                (PairingStatus::Approved, Some("device-1".to_string())),
            ] {
                let pairing = Frame::Pairing(Pairing { status, device_id });
                send(&mut socket, &mut channel, pairing).await;
            }
        }
        loop {
            match read(&mut socket, &mut channel).await {
                Frame::Req(request) => {
                    let response = Frame::Res(Response {
                        id: request.id,
                        status: 200,
                        headers: BTreeMap::new(),
                        body: Body::from_bytes(request.path.into_bytes()),
                    });
                    send(&mut socket, &mut channel, response).await;
                }
                Frame::Sub(subscribe) => {
                    let event = Frame::Evt(StreamEvent {
                        id: subscribe.id,
                        event: "session".into(),
                        data: serde_json::json!({ "n": 1 }),
                        seq: Some(7),
                    });
                    send(&mut socket, &mut channel, event).await;
                    let end = Frame::End(End {
                        id: subscribe.id,
                        error: None,
                    });
                    send(&mut socket, &mut channel, end).await;
                }
                _ => {}
            }
        }
    }

    fn identity() -> LinkIdentity {
        LinkIdentity {
            keys: StaticKeypair::generate().unwrap(),
            name: "MacBook".into(),
            platform: "macos".into(),
        }
    }

    #[tokio::test]
    async fn a_computer_pairs_with_a_machine_then_asks_and_listens() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let keys = StaticKeypair::generate().unwrap();
        let offer = PairingOffer {
            relay: format!("ws://{}", listener.local_addr().unwrap()),
            room: "0123456789abcdef".into(),
            desktop_public: keys.public,
            otp: generate_otp(),
        };
        tokio::spawn(machine(listener, keys, Some(offer.otp)));
        let (asked, was_asked) = std::sync::mpsc::channel();
        let link = MachineLink::pair(&offer, &identity(), move || {
            let _ = asked.send(());
        })
        .await
        .unwrap();
        assert!(was_asked.try_recv().is_ok(), "the machine asked its person");
        assert_eq!(link.device_id(), "device-1");

        // Two calls at once come back each with its own answer.
        let (health, status) = tokio::join!(
            link.request("GET", "/health", BTreeMap::new(), Vec::new()),
            link.request("GET", "/api/v1/status", BTreeMap::new(), Vec::new()),
        );
        assert_eq!(health.unwrap().body, b"/health");
        assert_eq!(status.unwrap().body, b"/api/v1/status");

        let mut stream = link.subscribe("/api/v1/events/stream").unwrap();
        let Some(StreamItem::Event(event)) = stream.recv().await else {
            panic!("an event");
        };
        assert_eq!(event.seq, Some(7));
        assert_eq!(stream.recv().await, Some(StreamItem::End(None)));
    }

    #[tokio::test]
    async fn an_approved_key_reconnects_and_a_lost_machine_ends_the_calls() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let keys = StaticKeypair::generate().unwrap();
        let target = MachineTarget {
            relay: format!("ws://{}/", listener.local_addr().unwrap()),
            room: "0123456789abcdef".into(),
            public_key: keys.public,
        };
        let served = tokio::spawn(machine(listener, keys, None));
        let link = MachineLink::connect(&target, &identity()).await.unwrap();
        let answer = link
            .request("GET", "/health", BTreeMap::new(), Vec::new())
            .await
            .unwrap();
        assert_eq!(
            (answer.status, answer.body.as_slice()),
            (200, &b"/health"[..])
        );

        served.abort();
        let _ = served.await;
        let lost = link
            .request("GET", "/health", BTreeMap::new(), Vec::new())
            .await;
        assert!(matches!(lost, Err(LinkError::Closed)));
        assert!(!link.is_open());
    }
}
