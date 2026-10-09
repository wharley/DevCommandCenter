//! Noise handshakes. The phone is always the initiator (it knows the desktop
//! key from the QR); the desktop is always the responder.
//!
//! - Pairing, `Noise_IKpsk2_25519_ChaChaPoly_SHA256`: the PSK comes from the
//!   QR's one-time secret. The desktop reads message 1 without the PSK (it
//!   names which offer it answers), mixes the PSK into message 2, and only a
//!   phone holding the same secret can read message 2 or send a transport
//!   message the desktop can open.
//! - Reconnecting, `Noise_IK_25519_ChaChaPoly_SHA256`: the desktop learns the
//!   phone's static key from message 1 and answers only if that key belongs
//!   to an approved device.

use serde::{Deserialize, Serialize};
use snow::{Builder, HandshakeState};
use zeroize::Zeroizing;

use crate::channel::SecureChannel;
use crate::{RemoteError, PROTOCOL_VERSION};

pub const PAIR_PATTERN: &str = "Noise_IKpsk2_25519_ChaChaPoly_SHA256";
pub const RESUME_PATTERN: &str = "Noise_IK_25519_ChaChaPoly_SHA256";
/// Bound into every handshake hash; a different protocol version never
/// completes a handshake with this one.
pub const PROLOGUE: &[u8] = b"dcc-remote/v1";
pub const KEY_LEN: usize = 32;
/// Handshake messages are small; this bounds what a peer can make us buffer.
const MAX_HANDSHAKE_MESSAGE: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pair,
    Resume,
}

impl Mode {
    pub fn pattern(self) -> &'static str {
        match self {
            Mode::Pair => PAIR_PATTERN,
            Mode::Resume => RESUME_PATTERN,
        }
    }

    pub fn to_wire(self) -> u8 {
        match self {
            Mode::Pair => 1,
            Mode::Resume => 2,
        }
    }

    pub fn from_wire(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Mode::Pair),
            2 => Some(Mode::Resume),
            _ => None,
        }
    }
}

/// An X25519 static key pair.
pub struct StaticKeypair {
    pub private: Zeroizing<[u8; KEY_LEN]>,
    pub public: [u8; KEY_LEN],
}

impl StaticKeypair {
    pub fn generate() -> Result<Self, RemoteError> {
        let pair = Builder::new(RESUME_PATTERN.parse()?).generate_keypair()?;
        Self::from_parts(&pair.private, &pair.public)
    }

    pub fn from_parts(private: &[u8], public: &[u8]) -> Result<Self, RemoteError> {
        let private: [u8; KEY_LEN] = private
            .try_into()
            .map_err(|_| RemoteError::Malformed("private key length"))?;
        let public: [u8; KEY_LEN] = public
            .try_into()
            .map_err(|_| RemoteError::Malformed("public key length"))?;
        Ok(Self {
            private: Zeroizing::new(private),
            public,
        })
    }
}

/// Payload of handshake message 1, phone to desktop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClientHello {
    pub v: u32,
    /// Which pairing offer this answers (see [`crate::offer::pair_id`]).
    /// Only in pairing mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

/// Payload of handshake message 2, desktop to phone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerHello {
    pub v: u32,
    /// The phone's device id; absent while a pairing waits for approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

fn builder(mode: Mode) -> Result<Builder<'static>, RemoteError> {
    Ok(Builder::new(mode.pattern().parse()?).prologue(PROLOGUE)?)
}

/// The desktop side of a handshake.
pub struct ResponderHandshake {
    state: HandshakeState,
    mode: Mode,
}

impl ResponderHandshake {
    pub fn new(mode: Mode, local: &StaticKeypair) -> Result<Self, RemoteError> {
        let state = builder(mode)?
            .local_private_key(local.private.as_slice())?
            .build_responder()?;
        Ok(Self { state, mode })
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Reads message 1 and returns its payload. In pairing mode the PSK is
    /// not needed yet; the payload says which offer to take it from.
    pub fn read_hello(&mut self, message: &[u8]) -> Result<ClientHello, RemoteError> {
        if message.len() > MAX_HANDSHAKE_MESSAGE {
            return Err(RemoteError::TooLarge);
        }
        let mut payload = vec![0u8; message.len()];
        let len = self.state.read_message(message, &mut payload)?;
        let hello: ClientHello = serde_json::from_slice(&payload[..len])?;
        if hello.v != PROTOCOL_VERSION {
            return Err(RemoteError::Version(hello.v));
        }
        Ok(hello)
    }

    /// The phone's static key, known once message 1 was read.
    pub fn remote_static(&self) -> Option<[u8; KEY_LEN]> {
        self.state
            .get_remote_static()
            .and_then(|key| key.try_into().ok())
    }

    pub fn set_psk(&mut self, psk: &[u8; KEY_LEN]) -> Result<(), RemoteError> {
        self.state.set_psk(2, psk)?;
        Ok(())
    }

    /// Writes message 2 and finishes the handshake.
    pub fn respond(mut self, hello: &ServerHello) -> Result<(Vec<u8>, SecureChannel), RemoteError> {
        let payload = serde_json::to_vec(hello)?;
        let mut message = vec![0u8; MAX_HANDSHAKE_MESSAGE];
        let len = self.state.write_message(&payload, &mut message)?;
        message.truncate(len);
        let channel = SecureChannel::new(self.state.into_transport_mode()?);
        Ok((message, channel))
    }
}

/// The phone side of a handshake. The app uses the TypeScript package; this
/// one serves tests and tools.
pub struct InitiatorHandshake {
    state: HandshakeState,
}

impl InitiatorHandshake {
    pub fn new(
        mode: Mode,
        local: &StaticKeypair,
        desktop_public: &[u8; KEY_LEN],
        psk: Option<&[u8; KEY_LEN]>,
    ) -> Result<Self, RemoteError> {
        let mut builder = builder(mode)?
            .local_private_key(local.private.as_slice())?
            .remote_public_key(desktop_public)?;
        if let Some(psk) = psk {
            builder = builder.psk(2, psk)?;
        }
        Ok(Self {
            state: builder.build_initiator()?,
        })
    }

    pub fn hello(&mut self, hello: &ClientHello) -> Result<Vec<u8>, RemoteError> {
        let payload = serde_json::to_vec(hello)?;
        let mut message = vec![0u8; MAX_HANDSHAKE_MESSAGE];
        let len = self.state.write_message(&payload, &mut message)?;
        message.truncate(len);
        Ok(message)
    }

    pub fn finish(mut self, message: &[u8]) -> Result<(ServerHello, SecureChannel), RemoteError> {
        let mut payload = vec![0u8; message.len()];
        let len = self.state.read_message(message, &mut payload)?;
        let hello: ServerHello = serde_json::from_slice(&payload[..len])?;
        if hello.v != PROTOCOL_VERSION {
            return Err(RemoteError::Version(hello.v));
        }
        Ok((hello, SecureChannel::new(self.state.into_transport_mode()?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offer::psk_from_otp;

    fn hello() -> ClientHello {
        ClientHello {
            v: PROTOCOL_VERSION,
            pair_id: Some("abcd".into()),
            device: Some(DeviceInfo {
                name: "Pixel".into(),
                platform: Some("android".into()),
            }),
        }
    }

    #[test]
    fn pairing_needs_the_same_otp_on_both_sides() {
        let desktop = StaticKeypair::generate().unwrap();
        let phone = StaticKeypair::generate().unwrap();
        let psk = psk_from_otp(&[7u8; 16]);

        let mut initiator =
            InitiatorHandshake::new(Mode::Pair, &phone, &desktop.public, Some(&psk)).unwrap();
        let message1 = initiator.hello(&hello()).unwrap();
        let mut responder = ResponderHandshake::new(Mode::Pair, &desktop).unwrap();
        assert_eq!(responder.read_hello(&message1).unwrap(), hello());
        assert_eq!(responder.remote_static(), Some(phone.public));
        responder.set_psk(&psk).unwrap();
        let (message2, mut desktop_channel) = responder
            .respond(&ServerHello {
                v: PROTOCOL_VERSION,
                device_id: None,
            })
            .unwrap();
        let (_, mut phone_channel) = initiator.finish(&message2).unwrap();
        let sealed = phone_channel.seal(b"ping").unwrap();
        assert_eq!(
            desktop_channel.open_wire(&sealed[0]).unwrap().as_deref(),
            Some(&b"ping"[..])
        );

        // A phone with another secret cannot read message 2.
        let mut initiator = InitiatorHandshake::new(
            Mode::Pair,
            &phone,
            &desktop.public,
            Some(&psk_from_otp(&[8u8; 16])),
        )
        .unwrap();
        let message1 = initiator.hello(&hello()).unwrap();
        let mut responder = ResponderHandshake::new(Mode::Pair, &desktop).unwrap();
        responder.read_hello(&message1).unwrap();
        responder.set_psk(&psk).unwrap();
        let (message2, _) = responder
            .respond(&ServerHello {
                v: PROTOCOL_VERSION,
                device_id: None,
            })
            .unwrap();
        assert!(initiator.finish(&message2).is_err());
    }

    #[test]
    fn resume_reveals_the_phone_key_and_rejects_a_wrong_desktop_key() {
        let desktop = StaticKeypair::generate().unwrap();
        let phone = StaticKeypair::generate().unwrap();
        let mut initiator =
            InitiatorHandshake::new(Mode::Resume, &phone, &desktop.public, None).unwrap();
        let message1 = initiator.hello(&hello()).unwrap();
        let mut responder = ResponderHandshake::new(Mode::Resume, &desktop).unwrap();
        responder.read_hello(&message1).unwrap();
        assert_eq!(responder.remote_static(), Some(phone.public));

        let impostor = StaticKeypair::generate().unwrap();
        let mut responder = ResponderHandshake::new(Mode::Resume, &impostor).unwrap();
        assert!(responder.read_hello(&message1).is_err());
    }

    #[test]
    fn refuses_another_version() {
        let desktop = StaticKeypair::generate().unwrap();
        let phone = StaticKeypair::generate().unwrap();
        let mut initiator =
            InitiatorHandshake::new(Mode::Resume, &phone, &desktop.public, None).unwrap();
        let message1 = initiator
            .hello(&ClientHello {
                v: 2,
                pair_id: None,
                device: None,
            })
            .unwrap();
        let mut responder = ResponderHandshake::new(Mode::Resume, &desktop).unwrap();
        assert!(matches!(
            responder.read_hello(&message1),
            Err(RemoteError::Version(2))
        ));
    }
}
