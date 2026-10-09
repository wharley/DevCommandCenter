//! Application frames: one JSON object per channel message, tagged by `t`.
//!
//! - `req`/`res`: one HTTP request against the desktop API and its response.
//! - `sub`: opens an SSE stream (only `/api/v1/events/stream`); the desktop
//!   answers with `evt` frames until `end`. The phone stops it with `cancel`.
//! - `ping`/`pong`: liveness.
//! - `pairing`: the desktop tells a pairing phone whether it was approved.
//!
//! Ids are chosen by the phone and only need to be unique among its open
//! requests and streams.

use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::RemoteError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Frame {
    Req(Request),
    Res(Response),
    Sub(Subscribe),
    Evt(StreamEvent),
    End(End),
    Cancel { id: u64 },
    Ping(Ping),
    Pong(Ping),
    Pairing(Pairing),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, flatten)]
    pub body: Body,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub id: u64,
    pub status: u16,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, flatten)]
    pub body: Body,
}

/// A body travels as text when it is UTF-8 (the API is JSON) and as base64
/// otherwise, with `bodyEncoding: "base64"`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_encoding: Option<String>,
}

impl Body {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        if bytes.is_empty() {
            return Self::default();
        }
        match String::from_utf8(bytes) {
            Ok(text) => Self {
                body: Some(text),
                body_encoding: None,
            },
            Err(error) => Self {
                body: Some(BASE64.encode(error.into_bytes())),
                body_encoding: Some("base64".into()),
            },
        }
    }

    pub fn into_bytes(self) -> Result<Vec<u8>, RemoteError> {
        let Some(body) = self.body else {
            return Ok(Vec::new());
        };
        match self.body_encoding.as_deref() {
            None => Ok(body.into_bytes()),
            Some("base64") => BASE64
                .decode(body)
                .map_err(|_| RemoteError::Malformed("body base64")),
            Some(_) => Err(RemoteError::Malformed("body encoding")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Subscribe {
    pub id: u64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StreamEvent {
    pub id: u64,
    pub event: String,
    pub data: Value,
    /// Durable sequence of the session event, when it has one. The phone
    /// keeps the highest one as its cursor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct End {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ping {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    pub status: PairingStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PairingStatus {
    /// The secret checked out; the desktop is asking the person.
    Pending,
    Approved,
    Denied,
}

impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>, RemoteError> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RemoteError> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_use_the_documented_json_shape() {
        let request = Frame::Req(Request {
            id: 1,
            method: "GET".into(),
            path: "/api/v1/sessions".into(),
            headers: BTreeMap::new(),
            body: Body::default(),
        });
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({"t": "req", "id": 1, "method": "GET", "path": "/api/v1/sessions"})
        );
        let response = Frame::Res(Response {
            id: 1,
            status: 200,
            headers: BTreeMap::from([("content-type".into(), "application/json".into())]),
            body: Body::from_bytes(b"[]".to_vec()),
        });
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            json!({"t": "res", "id": 1, "status": 200,
                   "headers": {"content-type": "application/json"}, "body": "[]"})
        );
        assert_eq!(
            serde_json::to_value(Frame::Cancel { id: 3 }).unwrap(),
            json!({"t": "cancel", "id": 3})
        );
        assert_eq!(
            serde_json::to_value(Frame::Pairing(Pairing {
                status: PairingStatus::Approved,
                device_id: Some("d".into())
            }))
            .unwrap(),
            json!({"t": "pairing", "status": "approved", "deviceId": "d"})
        );
        let decoded = Frame::decode(br#"{"t":"ping"}"#).unwrap();
        assert_eq!(decoded, Frame::Ping(Ping { ts: None }));
    }

    #[test]
    fn binary_bodies_travel_as_base64() {
        let body = Body::from_bytes(vec![0xff, 0x00, 0x10]);
        assert_eq!(body.body_encoding.as_deref(), Some("base64"));
        assert_eq!(body.into_bytes().unwrap(), vec![0xff, 0x00, 0x10]);
    }
}
