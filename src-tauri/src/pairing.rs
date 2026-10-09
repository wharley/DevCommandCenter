//! Paired phones, their revocation and the pairing audit trail.
//!
//! Phones pair through the relay (`remote_connector`): the desktop shows a
//! single-use QR, the phone completes a Noise IKpsk2 handshake with it and the
//! person approves the request on the desktop. The approved phone's Noise
//! static key is pinned here in `noise_public_key`, and every reconnect must
//! prove that key again (Noise IK), so this table only stores public keys.
//!
//! Storage: `paired_devices` (pinned key + soft-revoke) and `pair_audit_log`.
//! Rows without a Noise key come from the retired LAN pairing (signed requests
//! or bearer sessions); `ensure_pairing_schema` revokes them, and they stay
//! only as history.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

pub const MAX_ACTIVE_DEVICES: i64 = 10;

/// Public representation of a paired device (no key material).
#[derive(Debug, Clone, Serialize)]
pub struct PairedDeviceSummary {
    pub device_id: String,
    pub device_name: String,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub last_ip: Option<String>,
    pub revoked: bool,
    /// `relay` (pinned Noise key) or `legacy` (paired over the retired LAN
    /// flow; always revoked).
    pub kind: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("database error: {0}")]
    Database(String),
    #[error("device limit reached; revoke an existing device first")]
    DeviceLimitReached,
}

// ---------- schema ----------

/// Idempotently creates pairing tables and applies forward migrations.
///
/// Safe to call repeatedly: it never drops tables or columns. Databases from
/// the LAN pairing era keep their `pairing_nonces` table and
/// `session_token_hash` column untouched; nothing reads them any more.
pub fn ensure_pairing_schema(conn: &Connection) -> Result<(), PairingError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS paired_devices (
            device_id            TEXT PRIMARY KEY,
            device_name          TEXT NOT NULL,
            public_key_spki      BLOB NOT NULL,
            user_agent           TEXT,
            created_at           TEXT NOT NULL DEFAULT (datetime('now')),
            last_used_at         TEXT,
            last_ip              TEXT,
            revoked_at           TEXT,
            session_token_hash   TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_paired_devices_revoked ON paired_devices(revoked_at);

        CREATE TABLE IF NOT EXISTS pair_audit_log (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            event         TEXT NOT NULL,
            device_id     TEXT,
            ip            TEXT,
            user_agent    TEXT,
            details_json  TEXT,
            created_at    TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_pair_audit_log_device ON pair_audit_log(device_id);
        CREATE INDEX IF NOT EXISTS idx_pair_audit_log_created ON pair_audit_log(created_at);
        "#,
    )
    .map_err(|e| PairingError::Database(e.to_string()))?;

    // Phones paired through the relay: their pinned Noise static key.
    // Order matters: add the column before creating its index, otherwise the
    // CREATE INDEX fails on databases that pre-date this column (where the
    // CREATE TABLE IF NOT EXISTS is a no-op and the column is still missing).
    add_column_if_missing(conn, "paired_devices", "noise_public_key", "BLOB")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_paired_devices_noise ON paired_devices(noise_public_key)",
        [],
    )
    .map_err(|e| PairingError::Database(e.to_string()))?;

    // Expo push token of the phone and the end of its lease (the phone
    // renews it on every connection). Cleared on revocation.
    add_column_if_missing(conn, "paired_devices", "push_token", "TEXT")?;
    add_column_if_missing(conn, "paired_devices", "push_platform", "TEXT")?;
    add_column_if_missing(conn, "paired_devices", "push_token_expires_at", "TEXT")?;

    retire_legacy_devices(conn)?;

    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    column_def: &str,
) -> Result<(), PairingError> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn
        .prepare(&pragma)
        .map_err(|e| PairingError::Database(e.to_string()))?;
    let names: Result<Vec<String>, _> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| PairingError::Database(e.to_string()))?
        .collect();
    let names = names.map_err(|e| PairingError::Database(e.to_string()))?;
    if names.iter().any(|n| n == column) {
        return Ok(());
    }
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {column_def}");
    conn.execute(&sql, [])
        .map_err(|e| PairingError::Database(e.to_string()))?;
    Ok(())
}

/// Devices paired over the retired LAN flow (no Noise key) can no longer
/// authenticate. Revoking them keeps them out of the device cap; the rows and
/// their history stay. Only active ones are touched, so this runs once.
fn retire_legacy_devices(conn: &Connection) -> Result<(), PairingError> {
    let legacy: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT device_id FROM paired_devices
                 WHERE noise_public_key IS NULL AND revoked_at IS NULL",
            )
            .map_err(|e| PairingError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| PairingError::Database(e.to_string()))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| PairingError::Database(e.to_string()))?
    };
    for device_id in legacy {
        let retired = conn
            .execute(
                "UPDATE paired_devices SET revoked_at = ?1, push_token = NULL,
                        push_platform = NULL, push_token_expires_at = NULL
                 WHERE device_id = ?2 AND revoked_at IS NULL",
                params![now_iso(), device_id],
            )
            .map_err(|e| PairingError::Database(e.to_string()))?;
        if retired > 0 {
            record_audit(
                conn,
                "legacy_device_retired",
                Some(&device_id),
                None,
                None,
                None,
            );
        }
    }
    Ok(())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn sanitize_device_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "Unnamed device".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

// ---------- relay (Noise) devices ----------

/// A phone paired through the relay, found by its Noise static key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayDevice {
    pub device_id: String,
    pub device_name: String,
    pub revoked: bool,
}

pub fn find_relay_device(
    conn: &Connection,
    public_key: &[u8],
) -> Result<Option<RelayDevice>, PairingError> {
    conn.query_row(
        "SELECT device_id, device_name, revoked_at FROM paired_devices
         WHERE noise_public_key = ?1
         ORDER BY revoked_at IS NULL DESC, created_at DESC LIMIT 1",
        params![public_key],
        |row| {
            Ok(RelayDevice {
                device_id: row.get(0)?,
                device_name: row.get(1)?,
                revoked: row.get::<_, Option<String>>(2)?.is_some(),
            })
        },
    )
    .optional()
    .map_err(|e| PairingError::Database(e.to_string()))
}

/// True while at least one relay phone is paired: the connector stays up.
pub fn has_active_relay_devices(conn: &Connection) -> Result<bool, PairingError> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM paired_devices
         WHERE noise_public_key IS NOT NULL AND revoked_at IS NULL)",
        [],
        |row| row.get(0),
    )
    .map_err(|e| PairingError::Database(e.to_string()))
}

pub fn active_device_count(conn: &Connection) -> Result<i64, PairingError> {
    conn.query_row(
        "SELECT COUNT(*) FROM paired_devices WHERE revoked_at IS NULL",
        [],
        |row| row.get(0),
    )
    .map_err(|e| PairingError::Database(e.to_string()))
}

/// Pins an approved phone's Noise key. Pairing again with a key that is
/// already active keeps its device id and takes the new name.
pub fn register_relay_device(
    conn: &Connection,
    public_key: &[u8],
    device_name: &str,
    platform: Option<&str>,
    fingerprint: &str,
) -> Result<String, PairingError> {
    let device_name = sanitize_device_name(device_name);
    if let Some(existing) = find_relay_device(conn, public_key)?.filter(|d| !d.revoked) {
        conn.execute(
            "UPDATE paired_devices SET device_name = ?1, user_agent = ?2, last_used_at = ?3
             WHERE device_id = ?4",
            params![device_name, platform, now_iso(), existing.device_id],
        )
        .map_err(|e| PairingError::Database(e.to_string()))?;
        record_audit(
            conn,
            "pair",
            Some(&existing.device_id),
            None,
            platform,
            Some(&format!(
                "{{\"device_name\":{device_name:?},\"mode\":\"relay\",\"fingerprint\":{fingerprint:?},\"repaired\":true}}"
            )),
        );
        return Ok(existing.device_id);
    }

    let active = active_device_count(conn)?;
    if active >= MAX_ACTIVE_DEVICES {
        record_audit(
            conn,
            "limit_reached",
            None,
            None,
            platform,
            Some(&format!("{{\"active\":{active},\"mode\":\"relay\"}}")),
        );
        return Err(PairingError::DeviceLimitReached);
    }

    let device_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO paired_devices
            (device_id, device_name, public_key_spki, user_agent, noise_public_key, last_used_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            device_id,
            device_name,
            Vec::<u8>::new(),
            platform,
            public_key,
            now_iso()
        ],
    )
    .map_err(|e| PairingError::Database(e.to_string()))?;
    record_audit(
        conn,
        "pair",
        Some(&device_id),
        None,
        platform,
        Some(&format!(
            "{{\"device_name\":{device_name:?},\"mode\":\"relay\",\"fingerprint\":{fingerprint:?}}}"
        )),
    );
    Ok(device_id)
}

pub fn device_is_active(conn: &Connection, device_id: &str) -> Result<bool, PairingError> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM paired_devices WHERE device_id = ?1 AND revoked_at IS NULL)",
        params![device_id],
        |row| row.get(0),
    )
    .map_err(|e| PairingError::Database(e.to_string()))
}

pub fn touch_device(conn: &Connection, device_id: &str) {
    let _ = conn.execute(
        "UPDATE paired_devices SET last_used_at = ?1 WHERE device_id = ?2",
        params![now_iso(), device_id],
    );
}

// ---------- device management ----------

pub fn list_paired_devices(
    conn: &Connection,
    include_revoked: bool,
) -> Result<Vec<PairedDeviceSummary>, PairingError> {
    let sql = if include_revoked {
        "SELECT device_id, device_name, user_agent, created_at, last_used_at, last_ip, revoked_at,
                CASE WHEN noise_public_key IS NOT NULL THEN 'relay' ELSE 'legacy' END
         FROM paired_devices ORDER BY created_at DESC"
    } else {
        "SELECT device_id, device_name, user_agent, created_at, last_used_at, last_ip, revoked_at,
                CASE WHEN noise_public_key IS NOT NULL THEN 'relay' ELSE 'legacy' END
         FROM paired_devices WHERE revoked_at IS NULL ORDER BY created_at DESC"
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| PairingError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            let revoked_at: Option<String> = row.get(6)?;
            Ok(PairedDeviceSummary {
                device_id: row.get(0)?,
                device_name: row.get(1)?,
                user_agent: row.get(2)?,
                created_at: row.get(3)?,
                last_used_at: row.get(4)?,
                last_ip: row.get(5)?,
                revoked: revoked_at.is_some(),
                kind: row.get(7)?,
            })
        })
        .map_err(|e| PairingError::Database(e.to_string()))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| PairingError::Database(e.to_string()))?);
    }
    Ok(out)
}

pub fn revoke_device(
    conn: &Connection,
    device_id: &str,
    ip: Option<&str>,
) -> Result<bool, PairingError> {
    let affected = conn
        .execute(
            "UPDATE paired_devices SET revoked_at = ?1, push_token = NULL,
                    push_platform = NULL, push_token_expires_at = NULL
             WHERE device_id = ?2 AND revoked_at IS NULL",
            params![now_iso(), device_id],
        )
        .map_err(|e| PairingError::Database(e.to_string()))?;

    if affected > 0 {
        record_audit(conn, "revoke", Some(device_id), ip, None, None);
    }
    Ok(affected > 0)
}

// ---------- push tokens ----------

/// A phone that gets push notifications: its Expo token while the lease lasts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    pub device_id: String,
    pub token: String,
    pub platform: Option<String>,
}

/// Stores the phone's Expo push token until `expires_at` (RFC 3339). Only an
/// active relay phone can hold one, and a token belongs to one phone: a
/// re-paired phone takes it over from its old row. Returns whether the
/// token is new for this phone (renewals return false).
pub fn set_push_token(
    conn: &Connection,
    device_id: &str,
    token: &str,
    platform: Option<&str>,
    expires_at: &str,
) -> Result<Option<bool>, PairingError> {
    let previous: Option<Option<String>> = conn
        .query_row(
            "SELECT push_token FROM paired_devices
             WHERE device_id = ?1 AND revoked_at IS NULL AND noise_public_key IS NOT NULL",
            params![device_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| PairingError::Database(e.to_string()))?;
    let Some(previous) = previous else {
        return Ok(None);
    };
    conn.execute(
        "UPDATE paired_devices SET push_token = NULL, push_platform = NULL,
                push_token_expires_at = NULL
         WHERE push_token = ?1 AND device_id != ?2",
        params![token, device_id],
    )
    .map_err(|e| PairingError::Database(e.to_string()))?;
    conn.execute(
        "UPDATE paired_devices SET push_token = ?1, push_platform = ?2,
                push_token_expires_at = ?3
         WHERE device_id = ?4",
        params![token, platform, expires_at, device_id],
    )
    .map_err(|e| PairingError::Database(e.to_string()))?;
    Ok(Some(previous.as_deref() != Some(token)))
}

/// The phone turned notifications off. Returns whether it had a token.
pub fn clear_push_token(conn: &Connection, device_id: &str) -> Result<bool, PairingError> {
    conn.execute(
        "UPDATE paired_devices SET push_token = NULL, push_platform = NULL,
                push_token_expires_at = NULL
         WHERE device_id = ?1 AND push_token IS NOT NULL",
        params![device_id],
    )
    .map(|affected| affected > 0)
    .map_err(|e| PairingError::Database(e.to_string()))
}

/// Expo said the token is dead (`DeviceNotRegistered`): forget it wherever it
/// is. Returns the phones that held it.
pub fn forget_push_token(conn: &Connection, token: &str) -> Result<Vec<String>, PairingError> {
    let devices: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT device_id FROM paired_devices WHERE push_token = ?1")
            .map_err(|e| PairingError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![token], |row| row.get::<_, String>(0))
            .map_err(|e| PairingError::Database(e.to_string()))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| PairingError::Database(e.to_string()))?
    };
    conn.execute(
        "UPDATE paired_devices SET push_token = NULL, push_platform = NULL,
                push_token_expires_at = NULL
         WHERE push_token = ?1",
        params![token],
    )
    .map_err(|e| PairingError::Database(e.to_string()))?;
    Ok(devices)
}

/// Active relay phones whose token lease has not run out at `now` (RFC 3339).
pub fn push_targets(conn: &Connection, now: &str) -> Result<Vec<PushTarget>, PairingError> {
    let mut stmt = conn
        .prepare(
            "SELECT device_id, push_token, push_platform FROM paired_devices
             WHERE revoked_at IS NULL AND noise_public_key IS NOT NULL
               AND push_token IS NOT NULL AND push_token_expires_at > ?1
             ORDER BY created_at",
        )
        .map_err(|e| PairingError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![now], |row| {
            Ok(PushTarget {
                device_id: row.get(0)?,
                token: row.get(1)?,
                platform: row.get(2)?,
            })
        })
        .map_err(|e| PairingError::Database(e.to_string()))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| PairingError::Database(e.to_string()))
}

/// Read recent audit-log entries, newest first.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub event: String,
    pub device_id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub details_json: Option<String>,
    pub created_at: String,
}

pub fn list_audit_log(conn: &Connection, limit: i64) -> Result<Vec<AuditEntry>, PairingError> {
    let limit = limit.clamp(1, 500);
    let mut stmt = conn
        .prepare(
            "SELECT id, event, device_id, ip, user_agent, details_json, created_at
             FROM pair_audit_log ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| PairingError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                event: row.get(1)?,
                device_id: row.get(2)?,
                ip: row.get(3)?,
                user_agent: row.get(4)?,
                details_json: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| PairingError::Database(e.to_string()))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| PairingError::Database(e.to_string()))?);
    }
    Ok(out)
}

pub fn record_audit(
    conn: &Connection,
    event: &str,
    device_id: Option<&str>,
    ip: Option<&str>,
    user_agent: Option<&str>,
    details_json: Option<&str>,
) {
    let _ = conn.execute(
        "INSERT INTO pair_audit_log (event, device_id, ip, user_agent, details_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![event, device_id, ip, user_agent, details_json],
    );
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        let schema = include_str!("../sql/schema.sql");
        conn.execute_batch(schema).unwrap();
        ensure_pairing_schema(&conn).unwrap();
        conn
    }

    /// A row as the retired LAN pairing wrote it: no Noise key.
    fn insert_legacy_device(conn: &Connection, device_id: &str) {
        conn.execute(
            "INSERT INTO paired_devices (device_id, device_name, public_key_spki)
             VALUES (?1, 'old phone', X'')",
            params![device_id],
        )
        .unwrap();
    }

    fn audit_events(conn: &Connection, event: &str) -> Vec<Option<String>> {
        list_audit_log(conn, 500)
            .unwrap()
            .into_iter()
            .filter(|entry| entry.event == event)
            .map(|entry| entry.device_id)
            .collect()
    }

    #[test]
    fn relay_device_round_trip() {
        let conn = fresh_db();
        let key = [7_u8; 32];

        let device_id =
            register_relay_device(&conn, &key, "  iPhone  ", Some("ios"), "AB12").unwrap();
        assert!(device_is_active(&conn, &device_id).unwrap());
        assert!(has_active_relay_devices(&conn).unwrap());
        assert_eq!(
            find_relay_device(&conn, &key).unwrap(),
            Some(RelayDevice {
                device_id: device_id.clone(),
                device_name: "iPhone".to_string(),
                revoked: false,
            })
        );

        let devices = list_paired_devices(&conn, false).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].kind, "relay");

        assert!(revoke_device(&conn, &device_id, None).unwrap());
        assert!(!device_is_active(&conn, &device_id).unwrap());
        assert!(!has_active_relay_devices(&conn).unwrap());
        assert!(find_relay_device(&conn, &key).unwrap().unwrap().revoked);
        assert!(!revoke_device(&conn, &device_id, None).unwrap());
    }

    #[test]
    fn push_tokens_follow_the_lease_and_die_with_the_device() {
        let conn = fresh_db();
        let phone = register_relay_device(&conn, &[1_u8; 32], "Phone", Some("ios"), "AB").unwrap();
        let other = register_relay_device(&conn, &[2_u8; 32], "Other", None, "CD").unwrap();
        let now = "2026-10-09T12:00:00.000Z";
        let later = "2026-10-11T12:00:00.000Z";

        assert_eq!(
            set_push_token(&conn, &phone, "ExponentPushToken[a]", Some("ios"), later).unwrap(),
            Some(true)
        );
        // Renewing the same token is not news.
        assert_eq!(
            set_push_token(&conn, &phone, "ExponentPushToken[a]", Some("ios"), later).unwrap(),
            Some(false)
        );
        assert_eq!(
            push_targets(&conn, now).unwrap(),
            vec![PushTarget {
                device_id: phone.clone(),
                token: "ExponentPushToken[a]".to_string(),
                platform: Some("ios".to_string()),
            }]
        );
        // The lease ran out.
        assert!(push_targets(&conn, "2026-10-12T00:00:00.000Z")
            .unwrap()
            .is_empty());

        // A token belongs to one phone.
        set_push_token(&conn, &other, "ExponentPushToken[a]", None, later).unwrap();
        let targets = push_targets(&conn, now).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].device_id, other);

        // Turned off on the phone, or dead at Expo.
        assert!(clear_push_token(&conn, &other).unwrap());
        assert!(!clear_push_token(&conn, &other).unwrap());
        set_push_token(&conn, &phone, "ExponentPushToken[b]", None, later).unwrap();
        assert_eq!(
            forget_push_token(&conn, "ExponentPushToken[b]").unwrap(),
            vec![phone.clone()]
        );
        assert!(push_targets(&conn, now).unwrap().is_empty());

        // Revoking takes the token with it, and a revoked phone cannot set one.
        set_push_token(&conn, &phone, "ExponentPushToken[c]", None, later).unwrap();
        assert!(revoke_device(&conn, &phone, None).unwrap());
        let stored: Option<String> = conn
            .query_row(
                "SELECT push_token FROM paired_devices WHERE device_id = ?1",
                params![phone],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, None);
        assert_eq!(
            set_push_token(&conn, &phone, "ExponentPushToken[c]", None, later).unwrap(),
            None
        );
        assert!(push_targets(&conn, now).unwrap().is_empty());
    }

    #[test]
    fn pairing_an_active_key_again_keeps_its_device_id() {
        let conn = fresh_db();
        let key = [9_u8; 32];
        let first = register_relay_device(&conn, &key, "Phone", None, "AB12").unwrap();
        let second = register_relay_device(&conn, &key, "Renamed", None, "AB12").unwrap();
        assert_eq!(first, second);
        assert_eq!(active_device_count(&conn).unwrap(), 1);
        assert_eq!(
            list_paired_devices(&conn, false).unwrap()[0].device_name,
            "Renamed"
        );
    }

    #[test]
    fn relay_pairing_stops_at_the_device_limit() {
        let conn = fresh_db();
        for i in 0..MAX_ACTIVE_DEVICES {
            register_relay_device(&conn, &[i as u8; 32], "phone", None, "fp").unwrap();
        }
        let over = register_relay_device(&conn, &[0xff; 32], "one more", None, "fp");
        assert!(matches!(over, Err(PairingError::DeviceLimitReached)));

        let first = list_paired_devices(&conn, false).unwrap()[0]
            .device_id
            .clone();
        revoke_device(&conn, &first, None).unwrap();
        assert!(register_relay_device(&conn, &[0xff; 32], "one more", None, "fp").is_ok());
    }

    #[test]
    fn legacy_devices_are_retired_once_and_free_the_device_cap() {
        let conn = fresh_db();
        let relay = register_relay_device(&conn, &[1_u8; 32], "relay phone", None, "fp").unwrap();
        for i in 0..MAX_ACTIVE_DEVICES {
            insert_legacy_device(&conn, &format!("legacy-{i}"));
        }
        insert_legacy_device(&conn, "already-revoked");
        revoke_device(&conn, "already-revoked", None).unwrap();

        ensure_pairing_schema(&conn).unwrap();
        ensure_pairing_schema(&conn).unwrap();

        // Only the relay phone is still active, and it was left alone.
        let active = list_paired_devices(&conn, false).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].device_id, relay);
        assert_eq!(active[0].kind, "relay");

        let all = list_paired_devices(&conn, true).unwrap();
        assert!(all
            .iter()
            .filter(|device| device.device_id != relay)
            .all(|device| device.revoked && device.kind == "legacy"));

        // One audit event per device that was still active, none on reruns.
        let retired = audit_events(&conn, "legacy_device_retired");
        assert_eq!(retired.len(), MAX_ACTIVE_DEVICES as usize);
        assert!(!retired.contains(&Some("already-revoked".to_string())));

        assert!(register_relay_device(&conn, &[2_u8; 32], "new phone", None, "fp").is_ok());
    }

    #[test]
    fn schema_upgrade_keeps_the_lan_era_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE pairing_nonces (nonce TEXT PRIMARY KEY, pin_hash TEXT NOT NULL,
                 expires_at TEXT NOT NULL);
             CREATE TABLE paired_devices (device_id TEXT PRIMARY KEY, device_name TEXT NOT NULL,
                 public_key_spki BLOB NOT NULL, user_agent TEXT,
                 created_at TEXT NOT NULL DEFAULT (datetime('now')), last_used_at TEXT,
                 last_ip TEXT, revoked_at TEXT, session_token_hash TEXT);
             INSERT INTO pairing_nonces VALUES ('n', 'h', 'e');
             INSERT INTO paired_devices (device_id, device_name, public_key_spki, session_token_hash)
                 VALUES ('bearer-phone', 'browser', X'', 'hash');",
        )
        .unwrap();

        ensure_pairing_schema(&conn).unwrap();

        let nonces: i64 = conn
            .query_row("SELECT COUNT(*) FROM pairing_nonces", [], |row| row.get(0))
            .unwrap();
        assert_eq!(nonces, 1);
        let token: Option<String> = conn
            .query_row(
                "SELECT session_token_hash FROM paired_devices WHERE device_id = 'bearer-phone'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(token.as_deref(), Some("hash"));
        assert_eq!(active_device_count(&conn).unwrap(), 0);
        assert_eq!(
            audit_events(&conn, "legacy_device_retired"),
            vec![Some("bearer-phone".to_string())]
        );
    }

    #[test]
    fn new_databases_get_no_lan_pairing_tables() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_pairing_schema(&conn).unwrap();
        let nonces: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('pairing_nonces', 'idx_paired_devices_token')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(nonces, 0);
    }
}
