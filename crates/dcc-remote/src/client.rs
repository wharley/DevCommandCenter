//! The side that reaches a machine: a phone, or another DCC driving a task
//! on it. It builds the handshake, then turns requests and subscriptions
//! into wire messages and wire messages back into frames. No I/O here: the
//! caller owns the socket (`packages/remote-protocol` is the same thing in
//! TypeScript, with its socket).
//!
//! ```text
//! let (connecting, hello) = Connecting::pair(&offer, &keys, device)?;   // or ::resume
//! send(hello);
//! let (server, mut session) = connecting.finish(&receive())?;
//! let (id, wires) = session.request("GET", "/api/v1/status", headers, body)?;
//! for wire in wires { send(wire) }
//! while let Some(frame) = session.receive(&receive())? { … }
//! ```

use std::collections::BTreeMap;

use crate::channel::SecureChannel;
use crate::frames::{Body, Frame, Ping, Request, Subscribe};
use crate::noise::{
    ClientHello, DeviceInfo, InitiatorHandshake, Mode, ServerHello, StaticKeypair, KEY_LEN,
};
use crate::offer::{pair_id, psk_from_otp, PairingOffer};
use crate::wire::{self, WireMessage};
use crate::{RemoteError, PROTOCOL_VERSION};

/// A handshake whose first message was written and waits for its answer.
pub struct Connecting {
    handshake: InitiatorHandshake,
}

impl Connecting {
    /// Pairs from an offer (the link or QR the machine showed): the first
    /// wire message to send. The machine then asks its person to approve.
    pub fn pair(
        offer: &PairingOffer,
        local: &StaticKeypair,
        device: DeviceInfo,
    ) -> Result<(Self, Vec<u8>), RemoteError> {
        let psk = psk_from_otp(&offer.otp);
        let hello = ClientHello {
            v: PROTOCOL_VERSION,
            pair_id: Some(pair_id(&offer.otp)),
            device: Some(device),
        };
        Self::start(Mode::Pair, local, &offer.desktop_public, Some(&psk), &hello)
    }

    /// Reconnects with a key the machine already approved.
    pub fn resume(
        local: &StaticKeypair,
        machine_public: &[u8; KEY_LEN],
        device: DeviceInfo,
    ) -> Result<(Self, Vec<u8>), RemoteError> {
        let hello = ClientHello {
            v: PROTOCOL_VERSION,
            pair_id: None,
            device: Some(device),
        };
        Self::start(Mode::Resume, local, machine_public, None, &hello)
    }

    fn start(
        mode: Mode,
        local: &StaticKeypair,
        machine_public: &[u8; KEY_LEN],
        psk: Option<&[u8; KEY_LEN]>,
        hello: &ClientHello,
    ) -> Result<(Self, Vec<u8>), RemoteError> {
        let mut handshake = InitiatorHandshake::new(mode, local, machine_public, psk)?;
        let message = handshake.hello(hello)?;
        Ok((Self { handshake }, wire::handshake_init(mode, &message)))
    }

    /// Reads the machine's answer. The session is ready; after a pairing it
    /// only carries `pairing` frames until the person approves.
    pub fn finish(self, wire: &[u8]) -> Result<(ServerHello, ClientSession), RemoteError> {
        let WireMessage::HandshakeResponse { noise } = wire::decode(wire)? else {
            return Err(RemoteError::Malformed("expected the handshake response"));
        };
        let (hello, channel) = self.handshake.finish(noise)?;
        Ok((
            hello,
            ClientSession {
                channel,
                next_id: 1,
            },
        ))
    }
}

/// An open channel to a machine. Ids are this side's: unique among its open
/// requests and streams.
pub struct ClientSession {
    channel: SecureChannel,
    next_id: u64,
}

impl ClientSession {
    fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn seal(&mut self, frame: &Frame) -> Result<Vec<Vec<u8>>, RemoteError> {
        self.channel.seal(&frame.encode()?)
    }

    /// One request against the machine's API: its id (the response carries
    /// it) and the wire messages to send, in order.
    pub fn request(
        &mut self,
        method: &str,
        path: &str,
        headers: BTreeMap<String, String>,
        body: Vec<u8>,
    ) -> Result<(u64, Vec<Vec<u8>>), RemoteError> {
        let id = self.id();
        let wires = self.seal(&Frame::Req(Request {
            id,
            method: method.to_string(),
            path: path.to_string(),
            headers,
            body: Body::from_bytes(body),
        }))?;
        Ok((id, wires))
    }

    /// Opens a stream of the machine's API; its events come as `evt` frames
    /// with this id until `end`.
    pub fn subscribe(&mut self, path: &str) -> Result<(u64, Vec<Vec<u8>>), RemoteError> {
        let id = self.id();
        let wires = self.seal(&Frame::Sub(Subscribe {
            id,
            path: path.to_string(),
        }))?;
        Ok((id, wires))
    }

    /// Gives up on a request or closes a stream.
    pub fn cancel(&mut self, id: u64) -> Result<Vec<Vec<u8>>, RemoteError> {
        self.seal(&Frame::Cancel { id })
    }

    /// Liveness; after a pairing handshake it is also what proves to the
    /// machine that this side holds the offer's secret.
    pub fn ping(&mut self, ts: Option<u64>) -> Result<Vec<Vec<u8>>, RemoteError> {
        self.seal(&Frame::Ping(Ping { ts }))
    }

    /// The answer to the machine's own ping.
    pub fn pong(&mut self, ts: Option<u64>) -> Result<Vec<Vec<u8>>, RemoteError> {
        self.seal(&Frame::Pong(Ping { ts }))
    }

    /// Opens a wire message from the machine: a frame once its last chunk
    /// arrived, nothing while a large one is still coming.
    pub fn receive(&mut self, wire: &[u8]) -> Result<Option<Frame>, RemoteError> {
        match self.channel.open_wire(wire)? {
            Some(message) => Ok(Some(Frame::decode(&message)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::Response;
    use crate::noise::ResponderHandshake;
    use crate::offer::generate_otp;

    fn device() -> DeviceInfo {
        DeviceInfo {
            name: "MacBook do Wharley".into(),
            platform: Some("macos".into()),
        }
    }

    /// The machine's side of a handshake, as `remote_connector` runs it.
    fn answer(
        machine: &StaticKeypair,
        hello_wire: &[u8],
        otp: Option<&[u8]>,
    ) -> (ClientHello, Vec<u8>, SecureChannel) {
        let WireMessage::HandshakeInit { mode, noise } = wire::decode(hello_wire).unwrap() else {
            panic!("a handshake init");
        };
        let mut responder = ResponderHandshake::new(mode, machine).unwrap();
        let hello = responder.read_hello(noise).unwrap();
        if let Some(otp) = otp {
            responder.set_psk(&psk_from_otp(otp)).unwrap();
        }
        let (message, channel) = responder
            .respond(&ServerHello {
                v: PROTOCOL_VERSION,
                device_id: otp.is_none().then(|| "device-1".to_string()),
            })
            .unwrap();
        (hello, wire::handshake_response(&message), channel)
    }

    #[test]
    fn a_desktop_pairs_from_an_offer_and_asks_the_machine_something() {
        let machine = StaticKeypair::generate().unwrap();
        let local = StaticKeypair::generate().unwrap();
        let offer = PairingOffer {
            relay: "wss://relay.example".into(),
            room: "0123456789abcdef".into(),
            desktop_public: machine.public,
            otp: generate_otp(),
        };
        let (connecting, hello_wire) = Connecting::pair(&offer, &local, device()).unwrap();
        let (hello, response, mut machine_channel) =
            answer(&machine, &hello_wire, Some(&offer.otp));
        assert_eq!(hello.pair_id.as_deref(), Some(pair_id(&offer.otp).as_str()));
        assert_eq!(hello.device.unwrap().platform.as_deref(), Some("macos"));
        let (server, mut session) = connecting.finish(&response).unwrap();
        assert_eq!(server.device_id, None, "waits for the person's approval");

        // The proof of the secret, then a request and its answer.
        for wire in session.ping(None).unwrap() {
            let opened = machine_channel.open_wire(&wire).unwrap().unwrap();
            assert_eq!(
                Frame::decode(&opened).unwrap(),
                Frame::Ping(Ping { ts: None })
            );
        }
        let (id, wires) = session
            .request("GET", "/api/v1/status", BTreeMap::new(), Vec::new())
            .unwrap();
        let mut asked = None;
        for wire in wires {
            asked = machine_channel.open_wire(&wire).unwrap();
        }
        let Frame::Req(request) = Frame::decode(&asked.unwrap()).unwrap() else {
            panic!("a request");
        };
        assert_eq!((request.id, request.path.as_str()), (id, "/api/v1/status"));
        let reply = Frame::Res(Response {
            id,
            status: 200,
            headers: BTreeMap::new(),
            body: Body::from_bytes(b"{\"ok\":true}".to_vec()),
        });
        let mut got = None;
        for wire in machine_channel.seal(&reply.encode().unwrap()).unwrap() {
            got = session.receive(&wire).unwrap();
        }
        assert_eq!(got, Some(reply));
    }

    #[test]
    fn an_approved_key_reconnects_without_an_offer_and_ids_do_not_repeat() {
        let machine = StaticKeypair::generate().unwrap();
        let local = StaticKeypair::generate().unwrap();
        let (connecting, hello_wire) =
            Connecting::resume(&local, &machine.public, device()).unwrap();
        let (hello, response, _) = answer(&machine, &hello_wire, None);
        assert_eq!(hello.pair_id, None);
        let (server, mut session) = connecting.finish(&response).unwrap();
        assert_eq!(server.device_id.as_deref(), Some("device-1"));
        let (first, _) = session.subscribe("/api/v1/events/stream").unwrap();
        let (second, _) = session
            .request("POST", "/api/v1/x", BTreeMap::new(), b"{}".to_vec())
            .unwrap();
        assert_ne!(first, second);
        // Anything but the handshake's answer is refused.
        let (again, _) = Connecting::resume(&local, &machine.public, device()).unwrap();
        assert!(again.finish(&hello_wire).is_err());
    }
}
