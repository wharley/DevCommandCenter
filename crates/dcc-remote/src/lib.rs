//! The encrypted channel between the DCC desktop and a paired phone.
//!
//! The phone reaches the desktop through a relay that only forwards bytes.
//! Everything that matters runs end to end between the two:
//!
//! - [`noise`]: `Noise_IKpsk2_25519_ChaChaPoly_SHA256` to pair (the phone
//!   learns the desktop key and a one-time secret from the QR) and
//!   `Noise_IK_25519_ChaChaPoly_SHA256` to reconnect with an approved key.
//! - [`wire`]: the one-byte envelope every relay message carries.
//! - [`channel`]: transport encryption with fragmentation, since a Noise
//!   message cannot exceed 65535 bytes.
//! - [`frames`]: the JSON frames of the application protocol.
//! - [`offer`]: the `dcc://pair` QR payload and what derives from it.
//! - [`coalesce`] and [`sse`]: how the desktop turns its SSE stream into
//!   `evt` frames without sending every token on its own.
//!
//! `packages/remote-protocol` is the TypeScript side of the same protocol;
//! both are checked against the same vectors.

pub mod channel;
pub mod coalesce;
pub mod frames;
pub mod noise;
pub mod offer;
pub mod sse;
pub mod wire;

/// Carried in the handshake payloads; a peer on another version is refused.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("noise: {0}")]
    Noise(#[from] snow::Error),
    #[error("malformed message: {0}")]
    Malformed(&'static str),
    #[error("message too large")]
    TooLarge,
    #[error("unsupported protocol version {0}")]
    Version(u32),
    #[error("invalid pairing offer: {0}")]
    Offer(&'static str),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
