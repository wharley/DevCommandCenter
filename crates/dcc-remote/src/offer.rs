//! The pairing QR and what both sides derive from it.
//!
//! `dcc://pair#v=1&relay=<wss url>&room=<id>&pk=<desktop key>&otp=<secret>`
//!
//! Everything sits in the fragment, which no server ever receives. `pk` and
//! `otp` are unpadded base64url. The `otp` is 128 random bits, valid for five
//! minutes and once; it never crosses the relay: the phone proves it holds it
//! through the Noise PSK derived here.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine as _};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::noise::KEY_LEN;
use crate::RemoteError;

pub const OFFER_SCHEME_PREFIX: &str = "dcc://pair#";
pub const OTP_LEN: usize = 16;
pub const ROOM_ID_LEN: usize = 16;
const HKDF_SALT: &[u8] = b"dcc-remote/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingOffer {
    pub relay: String,
    pub room: String,
    pub desktop_public: [u8; KEY_LEN],
    pub otp: [u8; OTP_LEN],
}

impl PairingOffer {
    pub fn to_url(&self) -> String {
        let fragment = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("v", "1")
            .append_pair("relay", &self.relay)
            .append_pair("room", &self.room)
            .append_pair("pk", &B64URL.encode(self.desktop_public))
            .append_pair("otp", &B64URL.encode(self.otp))
            .finish();
        format!("{OFFER_SCHEME_PREFIX}{fragment}")
    }

    pub fn parse(url: &str) -> Result<Self, RemoteError> {
        let fragment = url
            .strip_prefix(OFFER_SCHEME_PREFIX)
            .ok_or(RemoteError::Offer("not a dcc://pair link"))?;
        let mut version = None;
        let mut relay = None;
        let mut room = None;
        let mut pk = None;
        let mut otp = None;
        for (key, value) in url::form_urlencoded::parse(fragment.as_bytes()) {
            let value = value.into_owned();
            match key.as_ref() {
                "v" => version = Some(value),
                "relay" => relay = Some(value),
                "room" => room = Some(value),
                "pk" => pk = Some(value),
                "otp" => otp = Some(value),
                _ => {}
            }
        }
        if version.as_deref() != Some("1") {
            return Err(RemoteError::Offer("unsupported version"));
        }
        let relay = relay.ok_or(RemoteError::Offer("missing relay"))?;
        if !(relay.starts_with("wss://") || relay.starts_with("ws://")) {
            return Err(RemoteError::Offer("relay must be a WebSocket URL"));
        }
        let room = room.ok_or(RemoteError::Offer("missing room"))?;
        if !is_valid_room_id(&room) {
            return Err(RemoteError::Offer("invalid room"));
        }
        let desktop_public = decode_fixed::<KEY_LEN>(pk.as_deref(), "invalid pk")?;
        let otp = decode_fixed::<OTP_LEN>(otp.as_deref(), "invalid otp")?;
        Ok(Self {
            relay,
            room,
            desktop_public,
            otp,
        })
    }
}

fn decode_fixed<const N: usize>(
    value: Option<&str>,
    error: &'static str,
) -> Result<[u8; N], RemoteError> {
    let bytes = B64URL
        .decode(value.ok_or(RemoteError::Offer(error))?)
        .map_err(|_| RemoteError::Offer(error))?;
    bytes.try_into().map_err(|_| RemoteError::Offer(error))
}

fn hkdf_expand<const N: usize>(otp: &[u8], info: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    Hkdf::<Sha256>::new(Some(HKDF_SALT), otp)
        .expand(info, &mut out)
        .expect("HKDF output length is valid");
    out
}

/// The Noise PSK for the pairing handshake.
pub fn psk_from_otp(otp: &[u8]) -> [u8; KEY_LEN] {
    hkdf_expand::<KEY_LEN>(otp, b"noise psk")
}

/// Names the offer in handshake message 1 without revealing the secret.
pub fn pair_id(otp: &[u8]) -> String {
    hex_lower(&hkdf_expand::<8>(otp, b"pair id"))
}

/// Short form of a static key that a person compares on both screens, e.g.
/// `3F9A-0C12-B7E4`.
pub fn fingerprint(public: &[u8]) -> String {
    let digest = Sha256::digest(public);
    let hex = hex_lower(&digest[..6]).to_uppercase();
    format!("{}-{}-{}", &hex[0..4], &hex[4..8], &hex[8..12])
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn generate_otp() -> [u8; OTP_LEN] {
    let mut otp = [0u8; OTP_LEN];
    OsRng.fill_bytes(&mut otp);
    otp
}

/// 128 random bits, base64url: the relay's address for one desktop.
pub fn generate_room_id() -> String {
    let mut bytes = [0u8; ROOM_ID_LEN];
    OsRng.fill_bytes(&mut bytes);
    B64URL.encode(bytes)
}

/// Proves to the relay that a control connection comes from the room's
/// owner. The relay keeps only its hash.
pub fn generate_room_secret() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    B64URL.encode(bytes)
}

pub fn is_valid_room_id(room: &str) -> bool {
    room.len() == 22
        && room
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_round_trips_through_the_url() {
        let offer = PairingOffer {
            relay: "wss://relay.example/".into(),
            room: generate_room_id(),
            desktop_public: [9u8; KEY_LEN],
            otp: generate_otp(),
        };
        let url = offer.to_url();
        assert!(url.starts_with("dcc://pair#v=1&relay=wss%3A%2F%2Frelay.example%2F&room="));
        assert_eq!(PairingOffer::parse(&url).unwrap(), offer);
        assert!(PairingOffer::parse("dcc://pair#v=2").is_err());
        assert!(PairingOffer::parse(&url.replace("room=", "room=x")).is_err());
    }

    #[test]
    fn derivations_are_stable() {
        // Pinned so the TypeScript package can assert the same values.
        let otp: Vec<u8> = (0..16).collect();
        assert_eq!(
            hex_lower(&psk_from_otp(&otp)),
            "4fc722feee1d0da95f23518afb4857dc3ad7173746506c13551d673d62e5075b"
        );
        assert_eq!(pair_id(&otp), "a3ad52c635e97f88");
        assert_eq!(fingerprint(&[1u8; 32]), "72CD-6E84-22C4");
    }
}
