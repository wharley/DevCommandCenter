//! The phones paired with this desktop and the relay (`pairing-api.ts`):
//! the devices, revoking one, the pairing audit log, the relay's status and
//! address, a pairing offer (the QR's content) and the phones waiting for
//! approval. All local: the relay never forwards `/api/v1/remote/`.
//! Blocking: call it off the UI thread.

use serde::Deserialize;
use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi, encode};

/// `PairedDevice`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Device {
    pub device_id: String,
    pub device_name: String,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    /// `relay` (a pinned key) or `legacy` (the retired LAN pairing).
    pub kind: String,
}

/// `AuditEntry`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuditEntry {
    pub id: i64,
    pub event: String,
    pub device_id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
}

/// `RemoteAccessStatus`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RelayStatus {
    pub relay_url: Option<String>,
    /// Set by `DCC_RELAY_URL`; the screen cannot change it.
    pub relay_url_from_env: bool,
    pub connected: bool,
    pub desktop_fingerprint: Option<String>,
}

/// `RelayPairingOffer`: `dcc://pair#…`, holding a one-time secret.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Offer {
    pub url: String,
    pub expires_at: String,
    pub desktop_fingerprint: String,
}

/// `RelayApprovalRequest`: a phone that scanned the QR and proved it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Approval {
    pub id: String,
    pub device_name: String,
    pub platform: Option<String>,
    pub fingerprint: String,
}

/// RFC 3339, or SQLite's `datetime('now')` (UTC without a zone) that the
/// pairing tables default to.
fn parse_time(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|at| at.with_timezone(&chrono::Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|at| at.and_utc())
        })
}

/// `formatTime`: `27/10/2026, 12:00:00` in local time.
pub fn audit_time(value: &str) -> String {
    parse_time(value).map_or_else(
        || value.to_string(),
        |at| {
            at.with_timezone(&chrono::Local)
                .format("%d/%m/%Y, %H:%M:%S")
                .to_string()
        },
    )
}

/// `formatRelative`: `agora`, `5 min`, `3h`, `12d`, then the date.
pub fn relative(value: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    let Some(at) = parse_time(value) else {
        return value.to_string();
    };
    let seconds = (now - at).num_seconds();
    match seconds {
        ..60 => "agora".into(),
        60..3600 => format!("{} min", seconds / 60),
        3600..86_400 => format!("{}h", seconds / 3600),
        _ if seconds / 86_400 < 30 => format!("{}d", seconds / 86_400),
        _ => at
            .with_timezone(&chrono::Local)
            .format("%d/%m/%Y")
            .to_string(),
    }
}

/// `describeEvent`: the label and its tone (green, amber, red, quiet).
pub fn event_label(event: &str) -> (&str, &'static str, Tone) {
    match event {
        "pair" => ("Pareamento concluído", "circle-check", Tone::Success),
        "revoke" => ("Dispositivo revogado", "circle-x", Tone::Warn),
        // The retired LAN pairing wrote these; older logs still have them.
        "pin_failure" => ("PIN incorreto", "triangle-alert", Tone::Warn),
        "pin_locked" => (
            "Nonce travado por brute-force",
            "triangle-alert",
            Tone::Danger,
        ),
        "relay_offer" => ("QR do relay gerado", "circle-check", Tone::Quiet),
        "relay_offer_invalid" => (
            "Código do relay inválido, expirado ou já usado",
            "triangle-alert",
            Tone::Warn,
        ),
        "relay_pair_denied" => ("Pareamento pelo relay recusado", "circle-x", Tone::Warn),
        "legacy_device_retired" => (
            "Pareamento antigo pela rede local desativado",
            "circle-x",
            Tone::Quiet,
        ),
        "relay_refused" => (
            "Aparelho não pareado tentou conectar pelo relay",
            "triangle-alert",
            Tone::Danger,
        ),
        "push_token_set" => (
            "Notificações ligadas no aparelho",
            "circle-check",
            Tone::Quiet,
        ),
        "push_token_cleared" => (
            "Notificações desligadas no aparelho",
            "circle-x",
            Tone::Quiet,
        ),
        "push_token_forgotten" => (
            "App removido do aparelho: notificações paradas",
            "circle-x",
            Tone::Quiet,
        ),
        other => (other, "circle-check", Tone::Quiet),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Success,
    Warn,
    Danger,
    Quiet,
}

/// The QR's modules, row by row (`errorCorrectionLevel: "M"`), without the
/// quiet zone.
pub fn qr_modules(value: &str) -> Option<Vec<Vec<bool>>> {
    let code = qrcode::QrCode::with_error_correction_level(value, qrcode::EcLevel::M).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    Some(
        colors
            .chunks(width)
            .map(|row| {
                row.iter()
                    .map(|color| *color == qrcode::Color::Dark)
                    .collect()
            })
            .collect(),
    )
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    /// `pairListDevices(false)`
    pub fn paired_devices(&self) -> Result<Vec<Device>, ApiError> {
        let value = self.get("/api/v1/remote/devices")?;
        decode(
            value
                .get("devices")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
    }

    /// `pairRevokeDevice`: its live connection closes too.
    pub fn revoke_device(&self, device_id: &str) -> Result<bool, ApiError> {
        let value = self.post(
            "/api/v1/remote/devices/revoke",
            &json!({ "deviceId": device_id }),
        )?;
        Ok(value
            .get("revoked")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }

    /// `pairAuditLog(limit)`, newest first.
    pub fn pairing_audit(&self, limit: u32) -> Result<Vec<AuditEntry>, ApiError> {
        let value = self.get(&format!("/api/v1/remote/audit?limit={limit}"))?;
        decode(
            value
                .get("entries")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
    }

    /// `remoteAccessStatus`
    pub fn relay_status(&self) -> Result<RelayStatus, ApiError> {
        decode(self.get("/api/v1/remote/status")?)
    }

    /// `remoteAccessSetRelay`: `None` turns remote access off.
    pub fn set_relay(&self, url: Option<&str>) -> Result<RelayStatus, ApiError> {
        decode(self.post("/api/v1/remote/relay", &json!({ "relayUrl": url }))?)
    }

    /// `remoteAccessCreateOffer`: valid for five minutes and one phone.
    pub fn pairing_offer(&self) -> Result<Offer, ApiError> {
        decode(self.post("/api/v1/remote/pairing/offers", &json!({}))?)
    }

    /// `remoteAccessPendingApprovals`
    pub fn pairing_approvals(&self) -> Result<Vec<Approval>, ApiError> {
        let value = self.get("/api/v1/remote/pairing/approvals")?;
        decode(
            value
                .get("approvals")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
    }

    /// `remoteAccessRespond`: `false` when the phone gave up or it expired.
    pub fn answer_pairing(&self, id: &str, approve: bool) -> Result<bool, ApiError> {
        match self.post(
            &format!("/api/v1/remote/pairing/approvals/{}", encode(id)),
            &json!({ "approve": approve }),
        ) {
            Ok(_) => Ok(true),
            Err(ApiError::Failed(message)) if message.contains("not found or expired") => Ok(false),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_and_events_read_like_the_desktop() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-27T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(relative(None, now), "—");
        assert_eq!(relative(Some("2026-10-27T11:59:30Z"), now), "agora");
        assert_eq!(relative(Some("2026-10-27T11:50:00Z"), now), "10 min");
        assert_eq!(relative(Some("2026-10-27T09:00:00Z"), now), "3h");
        assert_eq!(relative(Some("2026-10-20T12:00:00Z"), now), "7d");
        assert_eq!(relative(Some("nope"), now), "nope");
        assert_eq!(relative(Some("2026-10-27 11:00:00"), now), "1h");
        assert_eq!(event_label("revoke").0, "Dispositivo revogado");
        assert_eq!(event_label("relay_refused").2, Tone::Danger);
        assert_eq!(event_label("novo").0, "novo");
        let modules = qr_modules("dcc://pair#abc").unwrap();
        assert_eq!(modules.len(), modules[0].len());
        assert!(modules[0][0], "the finder pattern starts dark");
    }
}
