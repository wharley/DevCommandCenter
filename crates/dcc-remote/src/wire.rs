//! The envelope of every binary message the relay forwards between a phone
//! and the desktop. One leading byte says what follows:
//!
//! ```text
//! [0x01][mode][noise handshake message 1]   phone -> desktop
//! [0x02][noise handshake message 2]         desktop -> phone
//! [0x03][noise transport message]           both ways, see `channel`
//! ```
//!
//! `mode` picks the Noise pattern (see [`crate::noise::Mode`]). Nothing in the
//! envelope is secret: it only lets the desktop pick a pattern before it can
//! decrypt anything.

use crate::noise::Mode;
use crate::RemoteError;

pub const KIND_HANDSHAKE_INIT: u8 = 0x01;
pub const KIND_HANDSHAKE_RESPONSE: u8 = 0x02;
pub const KIND_TRANSPORT: u8 = 0x03;

/// Largest envelope: kind byte plus the largest Noise message.
pub const MAX_WIRE_MESSAGE: usize = 2 + crate::channel::MAX_NOISE_MESSAGE;

/// WebSocket close codes the desktop uses to refuse a phone. Only the code
/// crosses the relay; the phone maps it to a message.
pub mod close {
    /// Malformed envelope, bad version, wrong order.
    pub const PROTOCOL: u16 = 4400;
    /// Reconnect with a key that is not an approved device (or was revoked).
    pub const UNKNOWN_DEVICE: u16 = 4401;
    /// The person refused the pairing on the desktop, or it timed out.
    pub const PAIRING_DENIED: u16 = 4403;
    /// The QR secret is unknown, already used or expired.
    pub const OFFER_INVALID: u16 = 4410;
    /// Ten devices already paired.
    pub const DEVICE_LIMIT: u16 = 4429;
    /// The device was revoked while connected.
    pub const REVOKED: u16 = 4499;
}

#[derive(Debug, PartialEq, Eq)]
pub enum WireMessage<'a> {
    HandshakeInit { mode: Mode, noise: &'a [u8] },
    HandshakeResponse { noise: &'a [u8] },
    Transport { noise: &'a [u8] },
}

pub fn decode(bytes: &[u8]) -> Result<WireMessage<'_>, RemoteError> {
    if bytes.len() > MAX_WIRE_MESSAGE {
        return Err(RemoteError::TooLarge);
    }
    let (&kind, rest) = bytes.split_first().ok_or(RemoteError::Malformed("empty"))?;
    match kind {
        KIND_HANDSHAKE_INIT => {
            let (&mode, noise) = rest
                .split_first()
                .ok_or(RemoteError::Malformed("handshake without mode"))?;
            let mode = Mode::from_wire(mode).ok_or(RemoteError::Malformed("unknown mode"))?;
            Ok(WireMessage::HandshakeInit { mode, noise })
        }
        KIND_HANDSHAKE_RESPONSE => Ok(WireMessage::HandshakeResponse { noise: rest }),
        KIND_TRANSPORT => Ok(WireMessage::Transport { noise: rest }),
        _ => Err(RemoteError::Malformed("unknown kind")),
    }
}

pub fn handshake_init(mode: Mode, noise: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + noise.len());
    out.push(KIND_HANDSHAKE_INIT);
    out.push(mode.to_wire());
    out.extend_from_slice(noise);
    out
}

pub fn handshake_response(noise: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + noise.len());
    out.push(KIND_HANDSHAKE_RESPONSE);
    out.extend_from_slice(noise);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_each_kind() {
        let init = handshake_init(Mode::Pair, b"abc");
        assert_eq!(
            decode(&init).unwrap(),
            WireMessage::HandshakeInit {
                mode: Mode::Pair,
                noise: b"abc"
            }
        );
        let response = handshake_response(b"xyz");
        assert_eq!(
            decode(&response).unwrap(),
            WireMessage::HandshakeResponse { noise: b"xyz" }
        );
        assert!(decode(&[]).is_err());
        assert!(decode(&[KIND_HANDSHAKE_INIT]).is_err());
        assert!(decode(&[KIND_HANDSHAKE_INIT, 9]).is_err());
        assert!(decode(&[0x7f, 1, 2]).is_err());
    }
}
