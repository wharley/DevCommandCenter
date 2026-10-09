//! Transport encryption after the handshake.
//!
//! A Noise message holds at most 65535 bytes, tag included, so an
//! application message is cut into chunks. Each chunk's plaintext is one flag
//! byte followed by data; [`FLAG_FINAL`] marks the last chunk of a message.
//! Chunks are encrypted in order with Noise's implicit nonces, so a dropped,
//! repeated or reordered chunk fails to decrypt instead of corrupting a
//! message.

use snow::TransportState;

use crate::wire::KIND_TRANSPORT;
use crate::RemoteError;

pub const MAX_NOISE_MESSAGE: usize = 65535;
pub const TAG_LEN: usize = 16;
/// Data bytes per chunk: the Noise limit minus the tag and the flag byte.
pub const MAX_CHUNK: usize = MAX_NOISE_MESSAGE - TAG_LEN - 1;
/// Largest application message either side accepts after reassembly.
pub const MAX_APP_MESSAGE: usize = 32 * 1024 * 1024;
pub const FLAG_FINAL: u8 = 0x01;

pub struct SecureChannel {
    transport: TransportState,
    partial: Vec<u8>,
}

impl SecureChannel {
    /// Wraps a finished snow handshake. The handshake types in
    /// [`crate::noise`] call this; it is public for the interop vectors.
    pub fn new(transport: TransportState) -> Self {
        Self {
            transport,
            partial: Vec::new(),
        }
    }

    /// Encrypts one application message into the wire messages to send, in
    /// order.
    pub fn seal(&mut self, message: &[u8]) -> Result<Vec<Vec<u8>>, RemoteError> {
        if message.len() > MAX_APP_MESSAGE {
            return Err(RemoteError::TooLarge);
        }
        let mut out = Vec::with_capacity(message.len() / MAX_CHUNK + 1);
        let mut chunks = message.chunks(MAX_CHUNK).peekable();
        // An empty message is still one (final) chunk.
        if chunks.peek().is_none() {
            out.push(self.seal_chunk(&[], true)?);
        }
        while let Some(chunk) = chunks.next() {
            let last = chunks.peek().is_none();
            out.push(self.seal_chunk(chunk, last)?);
        }
        Ok(out)
    }

    fn seal_chunk(&mut self, data: &[u8], last: bool) -> Result<Vec<u8>, RemoteError> {
        let mut plaintext = Vec::with_capacity(1 + data.len());
        plaintext.push(if last { FLAG_FINAL } else { 0 });
        plaintext.extend_from_slice(data);
        let mut wire = vec![0u8; 1 + plaintext.len() + TAG_LEN];
        wire[0] = KIND_TRANSPORT;
        let len = self.transport.write_message(&plaintext, &mut wire[1..])?;
        wire.truncate(1 + len);
        Ok(wire)
    }

    /// Opens a transport wire message (kind byte included). Returns the
    /// application message once its final chunk arrived.
    pub fn open_wire(&mut self, wire: &[u8]) -> Result<Option<Vec<u8>>, RemoteError> {
        match wire.split_first() {
            Some((&KIND_TRANSPORT, noise)) => self.open(noise),
            _ => Err(RemoteError::Malformed("not a transport message")),
        }
    }

    /// Opens the Noise part of a transport message.
    pub fn open(&mut self, noise: &[u8]) -> Result<Option<Vec<u8>>, RemoteError> {
        if noise.len() > MAX_NOISE_MESSAGE || noise.len() < TAG_LEN + 1 {
            return Err(RemoteError::Malformed("transport message size"));
        }
        let mut plaintext = vec![0u8; noise.len()];
        let len = self.transport.read_message(noise, &mut plaintext)?;
        let (&flags, data) = plaintext[..len]
            .split_first()
            .ok_or(RemoteError::Malformed("empty chunk"))?;
        if self.partial.len() + data.len() > MAX_APP_MESSAGE {
            self.partial = Vec::new();
            return Err(RemoteError::TooLarge);
        }
        self.partial.extend_from_slice(data);
        if flags & FLAG_FINAL == 0 {
            return Ok(None);
        }
        Ok(Some(std::mem::take(&mut self.partial)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::{
        ClientHello, InitiatorHandshake, Mode, ResponderHandshake, ServerHello, StaticKeypair,
    };
    use crate::PROTOCOL_VERSION;

    pub(crate) fn pair() -> (SecureChannel, SecureChannel) {
        let desktop = StaticKeypair::generate().unwrap();
        let phone = StaticKeypair::generate().unwrap();
        let mut initiator =
            InitiatorHandshake::new(Mode::Resume, &phone, &desktop.public, None).unwrap();
        let message1 = initiator
            .hello(&ClientHello {
                v: PROTOCOL_VERSION,
                pair_id: None,
                device: None,
            })
            .unwrap();
        let mut responder = ResponderHandshake::new(Mode::Resume, &desktop).unwrap();
        responder.read_hello(&message1).unwrap();
        let (message2, desktop_channel) = responder
            .respond(&ServerHello {
                v: PROTOCOL_VERSION,
                device_id: None,
            })
            .unwrap();
        let (_, phone_channel) = initiator.finish(&message2).unwrap();
        (phone_channel, desktop_channel)
    }

    #[test]
    fn fragments_large_messages_and_reassembles_them() {
        let (mut phone, mut desktop) = pair();
        let message: Vec<u8> = (0..(MAX_CHUNK * 2 + 10)).map(|i| i as u8).collect();
        let wires = phone.seal(&message).unwrap();
        assert_eq!(wires.len(), 3);
        assert!(wires.iter().all(|w| w.len() <= 1 + MAX_NOISE_MESSAGE));
        assert_eq!(desktop.open_wire(&wires[0]).unwrap(), None);
        assert_eq!(desktop.open_wire(&wires[1]).unwrap(), None);
        assert_eq!(desktop.open_wire(&wires[2]).unwrap(), Some(message));
    }

    #[test]
    fn exact_chunk_boundary_and_empty_messages() {
        let (mut phone, mut desktop) = pair();
        let message = vec![1u8; MAX_CHUNK];
        let wires = phone.seal(&message).unwrap();
        assert_eq!(wires.len(), 1);
        assert_eq!(desktop.open_wire(&wires[0]).unwrap(), Some(message));
        let wires = desktop.seal(&[]).unwrap();
        assert_eq!(wires.len(), 1);
        assert_eq!(phone.open_wire(&wires[0]).unwrap(), Some(Vec::new()));
    }

    #[test]
    fn reordered_or_replayed_chunks_fail() {
        let (mut phone, mut desktop) = pair();
        let first = phone.seal(b"one").unwrap();
        let second = phone.seal(b"two").unwrap();
        assert!(desktop.open_wire(&second[0]).is_err());
        let (mut phone, mut desktop) = pair();
        let first_again = phone.seal(b"one").unwrap();
        desktop.open_wire(&first_again[0]).unwrap();
        assert!(desktop.open_wire(&first_again[0]).is_err());
        drop(first);
    }
}
