//! Mobile pairing with ECDSA P-256 signed requests.
//!
//! The flow:
//! 1. Desktop calls `create_pairing_nonce` → gets `(nonce, pin)` and shows the QR + PIN.
//! 2. Mobile scans the QR, generates a non-extractable ECDSA P-256 keypair via WebCrypto,
//!    asks the user for the PIN, then calls `complete_pairing` with `{nonce, pin, pubkey_spki, device_name}`.
//! 3. Mobile signs every subsequent request: `signature = ECDSA(method || "\n" || path || "\n" || timestamp || "\n" || sha256(body))`.
//!    Backend verifies via `verify_signed_request`.
//!
//! Storage: `pairing_nonces` (60s TTL, one-shot), `paired_devices` (per-device pubkey + soft-revoke), `pair_audit_log`.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use rand::rngs::OsRng;
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

const NONCE_BYTES: usize = 32;
const PAIRING_TTL_SECS: i64 = 60;
const REPLAY_WINDOW_SECS: i64 = 60;
const PIN_DIGITS: usize = 6;
const MAX_PIN_ATTEMPTS: i64 = 5;
const NONCE_GARBAGE_KEEP_SECS: i64 = 600;
const MAX_ACTIVE_DEVICES: i64 = 10;
const SESSION_MAX_AGE_DAYS: i64 = 30;

/// Created when the desktop initiates a pairing flow.
#[derive(Debug, Clone, Serialize)]
pub struct PairingChallenge {
    pub nonce: String,
    pub pin: String,
    pub expires_at: String,
}

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
}

/// Headers a mobile client must send on every authenticated request.
#[derive(Debug, Clone)]
pub struct SignedRequestHeaders<'a> {
    pub device_id: &'a str,
    pub timestamp: &'a str,
    pub signature_b64: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("database error: {0}")]
    Database(String),
    #[error("invalid nonce")]
    InvalidNonce,
    #[error("invalid PIN")]
    InvalidPin,
    #[error("pairing expired")]
    Expired,
    #[error("pairing already consumed")]
    AlreadyConsumed,
    #[error("too many invalid PIN attempts; nonce locked")]
    NonceLocked,
    #[error("invalid public key encoding")]
    InvalidPublicKey,
    #[error("invalid signature encoding")]
    InvalidSignature,
    #[error("signature verification failed")]
    SignatureMismatch,
    #[error("device unknown")]
    UnknownDevice,
    #[error("device revoked")]
    DeviceRevoked,
    #[error("timestamp outside replay window")]
    TimestampOutOfWindow,
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("device limit reached; revoke an existing device first")]
    DeviceLimitReached,
    #[error("device session expired; please pair again")]
    SessionExpired,
}

// ---------- schema ----------

/// Idempotently creates pairing tables and applies forward migrations.
///
/// Safe to call repeatedly: it never drops data, only adds missing columns
/// for old installations that pre-date the lockout / hardening fields.
pub fn ensure_pairing_schema(conn: &Connection) -> Result<(), PairingError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS pairing_nonces (
            nonce            TEXT PRIMARY KEY,
            pin_hash         TEXT NOT NULL,
            expires_at       TEXT NOT NULL,
            consumed_at      TEXT,
            failed_attempts  INTEGER NOT NULL DEFAULT 0,
            locked_at        TEXT,
            created_at       TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_pairing_nonces_expires ON pairing_nonces(expires_at);

        CREATE TABLE IF NOT EXISTS paired_devices (
            device_id        TEXT PRIMARY KEY,
            device_name      TEXT NOT NULL,
            public_key_spki  BLOB NOT NULL,
            user_agent       TEXT,
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            last_used_at     TEXT,
            last_ip          TEXT,
            revoked_at       TEXT
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

    // Forward-migrate: add lockout columns on installs that pre-date Phase 5.
    add_column_if_missing(
        conn,
        "pairing_nonces",
        "failed_attempts",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(conn, "pairing_nonces", "locked_at", "TEXT")?;

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

// ---------- crypto helpers ----------

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn generate_nonce_base64url() -> String {
    let mut bytes = [0u8; NONCE_BYTES];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_numeric_pin() -> String {
    // 6 decimal digits, leading zeros preserved. ~20 bits of entropy — fine paired with the
    // single-use nonce, especially with rate-limit + audit.
    let mut pin = String::with_capacity(PIN_DIGITS);
    let mut buf = [0u8; PIN_DIGITS];
    OsRng.fill_bytes(&mut buf);
    for b in buf.iter() {
        let digit = (*b % 10) as u32;
        pin.push(char::from_digit(digit, 10).unwrap());
    }
    pin
}

fn hash_pin(pin: &str, nonce: &str) -> String {
    // PINs are bounded (6 decimal digits, 10^6 possibilities). Binding the hash to the unique
    // nonce makes precomputed rainbow tables useless across nonces, and rate-limiting on
    // /auth/pair makes brute force impractical even with a fast hash.
    let mut hasher = Sha256::new();
    hasher.update(b"dcc-pair-pin\0");
    hasher.update(nonce.as_bytes());
    hasher.update(b"\0");
    hasher.update(pin.as_bytes());
    let digest = hasher.finalize();
    BASE64.encode(digest)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

// ---------- pairing lifecycle ----------

/// Inserts a fresh nonce+PIN and returns the challenge to display.
///
/// Also opportunistically deletes pairing rows older than NONCE_GARBAGE_KEEP_SECS
/// — keeps the table tidy without needing a background sweeper.
pub fn create_pairing_nonce(conn: &Connection) -> Result<PairingChallenge, PairingError> {
    let nonce = generate_nonce_base64url();
    let pin = generate_numeric_pin();
    let pin_hash = hash_pin(&pin, &nonce);
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(PAIRING_TTL_SECS);
    let expires_at_iso = expires_at.to_rfc3339();

    // Garbage-collect old rows on the way in.
    let cutoff = (now - chrono::Duration::seconds(NONCE_GARBAGE_KEEP_SECS)).to_rfc3339();
    let _ = conn.execute(
        "DELETE FROM pairing_nonces WHERE created_at < ?1",
        params![cutoff],
    );

    conn.execute(
        "INSERT INTO pairing_nonces (nonce, pin_hash, expires_at) VALUES (?1, ?2, ?3)",
        params![nonce, pin_hash, expires_at_iso],
    )
    .map_err(|e| PairingError::Database(e.to_string()))?;

    Ok(PairingChallenge {
        nonce,
        pin,
        expires_at: expires_at_iso,
    })
}

/// Number of expired/consumed/locked nonces purged since installation.
/// Exposed for the maintenance Tauri command.
pub fn purge_expired_nonces(conn: &Connection) -> Result<usize, PairingError> {
    let cutoff =
        (chrono::Utc::now() - chrono::Duration::seconds(NONCE_GARBAGE_KEEP_SECS)).to_rfc3339();
    let n = conn
        .execute(
            "DELETE FROM pairing_nonces WHERE created_at < ?1",
            params![cutoff],
        )
        .map_err(|e| PairingError::Database(e.to_string()))?;
    Ok(n)
}

/// Validates a `(nonce, pin)` pair, registers the device pubkey and returns its id.
pub fn complete_pairing(
    conn: &Connection,
    nonce: &str,
    pin: &str,
    public_key_spki_b64: &str,
    device_name: &str,
    user_agent: Option<&str>,
    client_ip: Option<&str>,
) -> Result<String, PairingError> {
    // Decode the SPKI pubkey upfront — fails fast and avoids burning the nonce on bogus input.
    let spki_bytes = BASE64
        .decode(public_key_spki_b64.trim())
        .map_err(|_| PairingError::InvalidPublicKey)?;
    VerifyingKey::from_public_key_der(&spki_bytes).map_err(|_| PairingError::InvalidPublicKey)?;

    let row: Option<(String, String, Option<String>, i64, Option<String>)> = conn
        .query_row(
            "SELECT pin_hash, expires_at, consumed_at, failed_attempts, locked_at
             FROM pairing_nonces WHERE nonce = ?1",
            params![nonce],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|e| PairingError::Database(e.to_string()))?;

    let (stored_pin_hash, expires_at_iso, consumed_at, failed_attempts, locked_at) =
        row.ok_or(PairingError::InvalidNonce)?;

    if consumed_at.is_some() {
        return Err(PairingError::AlreadyConsumed);
    }
    if locked_at.is_some() || failed_attempts >= MAX_PIN_ATTEMPTS {
        record_audit(
            conn,
            "pin_locked",
            None,
            client_ip,
            user_agent,
            Some(&format!(
                "{{\"nonce\":\"{}\",\"attempts\":{}}}",
                nonce, failed_attempts
            )),
        );
        return Err(PairingError::NonceLocked);
    }

    let expires_at = chrono::DateTime::parse_from_rfc3339(&expires_at_iso)
        .map_err(|_| PairingError::Database("invalid expires_at".into()))?
        .with_timezone(&chrono::Utc);
    if chrono::Utc::now() > expires_at {
        return Err(PairingError::Expired);
    }

    let candidate_hash = hash_pin(pin, nonce);
    if !constant_time_eq(&candidate_hash, &stored_pin_hash) {
        // Increment failed_attempts; if we just hit the cap, lock the nonce.
        let next_attempts = failed_attempts + 1;
        let new_lock = if next_attempts >= MAX_PIN_ATTEMPTS {
            Some(now_iso())
        } else {
            None
        };
        let _ = conn.execute(
            "UPDATE pairing_nonces
             SET failed_attempts = ?1,
                 locked_at = COALESCE(locked_at, ?2)
             WHERE nonce = ?3",
            params![next_attempts, new_lock, nonce],
        );
        record_audit(
            conn,
            "pin_failure",
            None,
            client_ip,
            user_agent,
            Some(&format!(
                "{{\"nonce\":\"{}\",\"attempts\":{}}}",
                nonce, next_attempts
            )),
        );
        if new_lock.is_some() {
            return Err(PairingError::NonceLocked);
        }
        return Err(PairingError::InvalidPin);
    }

    // Cap active devices to avoid runaway accumulation. Revoked rows do not
    // count — the user is expected to revoke an old device before adding a new
    // one when this hits.
    let active_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM paired_devices WHERE revoked_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|e| PairingError::Database(e.to_string()))?;
    if active_count >= MAX_ACTIVE_DEVICES {
        record_audit(
            conn,
            "limit_reached",
            None,
            client_ip,
            user_agent,
            Some(&format!("{{\"active\":{}}}", active_count)),
        );
        return Err(PairingError::DeviceLimitReached);
    }

    let device_id = Uuid::new_v4().to_string();
    let device_name = sanitize_device_name(device_name);

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| PairingError::Database(e.to_string()))?;

    tx.execute(
        "UPDATE pairing_nonces SET consumed_at = ?1 WHERE nonce = ?2",
        params![now_iso(), nonce],
    )
    .map_err(|e| PairingError::Database(e.to_string()))?;

    tx.execute(
        "INSERT INTO paired_devices (device_id, device_name, public_key_spki, user_agent, last_ip)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![device_id, device_name, spki_bytes, user_agent, client_ip],
    )
    .map_err(|e| PairingError::Database(e.to_string()))?;

    tx.commit()
        .map_err(|e| PairingError::Database(e.to_string()))?;

    record_audit(
        conn,
        "pair",
        Some(&device_id),
        client_ip,
        user_agent,
        Some(&format!("{{\"device_name\":{:?}}}", device_name)),
    );

    Ok(device_id)
}

fn sanitize_device_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "Unnamed device".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

// ---------- signature verification ----------

/// Canonical string the mobile signs:
/// `METHOD\nPATH\nTIMESTAMP\nSHA256_HEX(body)`
pub fn canonical_request_bytes(
    method: &str,
    path: &str,
    timestamp: &str,
    body: &[u8],
) -> Vec<u8> {
    let body_digest = hex::encode(Sha256::digest(body));
    let mut out = Vec::with_capacity(method.len() + path.len() + timestamp.len() + body_digest.len() + 3);
    out.extend_from_slice(method.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(path.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(timestamp.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(body_digest.as_bytes());
    out
}

/// Verifies an incoming signed request and, on success, returns the device summary
/// and bumps `last_used_at` / `last_ip`.
pub fn verify_signed_request(
    conn: &Connection,
    headers: SignedRequestHeaders<'_>,
    method: &str,
    path: &str,
    body: &[u8],
    client_ip: Option<&str>,
) -> Result<PairedDeviceSummary, PairingError> {
    let timestamp = chrono::DateTime::parse_from_rfc3339(headers.timestamp)
        .map_err(|_| PairingError::InvalidTimestamp)?
        .with_timezone(&chrono::Utc);
    let delta = (chrono::Utc::now() - timestamp).num_seconds().abs();
    if delta > REPLAY_WINDOW_SECS {
        return Err(PairingError::TimestampOutOfWindow);
    }

    let signature_bytes = BASE64
        .decode(headers.signature_b64.trim())
        .map_err(|_| PairingError::InvalidSignature)?;
    let signature =
        Signature::from_der(&signature_bytes).or_else(|_| Signature::from_slice(&signature_bytes))
            .map_err(|_| PairingError::InvalidSignature)?;

    let row: Option<(String, Vec<u8>, Option<String>, String, Option<String>)> = conn
        .query_row(
            "SELECT device_name, public_key_spki, user_agent, created_at, revoked_at
             FROM paired_devices WHERE device_id = ?1",
            params![headers.device_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|e| PairingError::Database(e.to_string()))?;

    let (device_name, pubkey_spki, user_agent, created_at, revoked_at) =
        row.ok_or(PairingError::UnknownDevice)?;

    if revoked_at.is_some() {
        return Err(PairingError::DeviceRevoked);
    }

    // Soft-expire devices older than SESSION_MAX_AGE_DAYS. Forces a re-pair
    // periodically so leaked phones don't keep API access forever.
    if let Ok(created_dt) = chrono::DateTime::parse_from_rfc3339(&created_at) {
        let age = chrono::Utc::now() - created_dt.with_timezone(&chrono::Utc);
        if age.num_days() > SESSION_MAX_AGE_DAYS {
            return Err(PairingError::SessionExpired);
        }
    }

    let verifying_key =
        VerifyingKey::from_public_key_der(&pubkey_spki).map_err(|_| PairingError::InvalidPublicKey)?;

    let canonical = canonical_request_bytes(method, path, headers.timestamp, body);
    verifying_key
        .verify(&canonical, &signature)
        .map_err(|_| PairingError::SignatureMismatch)?;

    let now = now_iso();
    let _ = conn.execute(
        "UPDATE paired_devices SET last_used_at = ?1, last_ip = ?2 WHERE device_id = ?3",
        params![now, client_ip, headers.device_id],
    );

    Ok(PairedDeviceSummary {
        device_id: headers.device_id.to_string(),
        device_name,
        user_agent,
        created_at,
        last_used_at: Some(now),
        last_ip: client_ip.map(str::to_string),
        revoked: false,
    })
}

// ---------- device management ----------

pub fn list_paired_devices(
    conn: &Connection,
    include_revoked: bool,
) -> Result<Vec<PairedDeviceSummary>, PairingError> {
    let sql = if include_revoked {
        "SELECT device_id, device_name, user_agent, created_at, last_used_at, last_ip, revoked_at
         FROM paired_devices ORDER BY created_at DESC"
    } else {
        "SELECT device_id, device_name, user_agent, created_at, last_used_at, last_ip, revoked_at
         FROM paired_devices WHERE revoked_at IS NULL ORDER BY created_at DESC"
    };

    let mut stmt = conn.prepare(sql).map_err(|e| PairingError::Database(e.to_string()))?;
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
            "UPDATE paired_devices SET revoked_at = ?1 WHERE device_id = ?2 AND revoked_at IS NULL",
            params![now_iso(), device_id],
        )
        .map_err(|e| PairingError::Database(e.to_string()))?;

    if affected > 0 {
        record_audit(conn, "revoke", Some(device_id), ip, None, None);
    }
    Ok(affected > 0)
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

fn record_audit(
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
    use p256::ecdsa::{signature::Signer, SigningKey};
    use p256::pkcs8::EncodePublicKey;
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        let schema = include_str!("../sql/schema.sql");
        conn.execute_batch(schema).unwrap();
        conn
    }

    fn fresh_keypair() -> (SigningKey, String) {
        let signing = SigningKey::random(&mut OsRng);
        let verifying = signing.verifying_key();
        let spki_der = verifying.to_public_key_der().unwrap();
        let spki_b64 = BASE64.encode(spki_der.as_bytes());
        (signing, spki_b64)
    }

    #[test]
    fn complete_pairing_succeeds() {
        let conn = fresh_db();
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (_, spki_b64) = fresh_keypair();

        let device_id = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "iPhone teste",
            Some("Mozilla/5.0"),
            Some("192.168.1.10"),
        )
        .unwrap();
        assert!(!device_id.is_empty());

        let devices = list_paired_devices(&conn, false).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_name, "iPhone teste");
    }

    #[test]
    fn complete_pairing_rejects_wrong_pin() {
        let conn = fresh_db();
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (_, spki_b64) = fresh_keypair();

        let result = complete_pairing(
            &conn,
            &challenge.nonce,
            "000000",
            &spki_b64,
            "iPhone",
            None,
            None,
        );
        assert!(matches!(result, Err(PairingError::InvalidPin)));
    }

    #[test]
    fn pin_brute_force_locks_after_max_attempts() {
        let conn = fresh_db();
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (_, spki_b64) = fresh_keypair();

        // 4 wrong attempts return InvalidPin
        for _ in 0..(MAX_PIN_ATTEMPTS - 1) {
            let res = complete_pairing(
                &conn,
                &challenge.nonce,
                "000000",
                &spki_b64,
                "iPhone",
                None,
                None,
            );
            assert!(matches!(res, Err(PairingError::InvalidPin)));
        }

        // 5th wrong attempt locks the nonce
        let res = complete_pairing(
            &conn,
            &challenge.nonce,
            "000000",
            &spki_b64,
            "iPhone",
            None,
            None,
        );
        assert!(matches!(res, Err(PairingError::NonceLocked)));

        // Even the correct PIN cannot rescue it now
        let res = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "iPhone",
            None,
            None,
        );
        assert!(matches!(res, Err(PairingError::NonceLocked)));
    }

    #[test]
    fn complete_pairing_rejects_replay() {
        let conn = fresh_db();
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (_, spki_b64) = fresh_keypair();

        complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "iPhone",
            None,
            None,
        )
        .unwrap();

        let result = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "iPhone",
            None,
            None,
        );
        assert!(matches!(result, Err(PairingError::AlreadyConsumed)));
    }

    #[test]
    fn signature_round_trip() {
        let conn = fresh_db();
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (signing, spki_b64) = fresh_keypair();
        let device_id = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "iPhone",
            None,
            None,
        )
        .unwrap();

        let method = "GET";
        let path = "/api/v1/status";
        let body = b"";
        let timestamp = chrono::Utc::now().to_rfc3339();
        let canonical = canonical_request_bytes(method, path, &timestamp, body);
        let sig: Signature = signing.sign(&canonical);
        let sig_b64 = BASE64.encode(sig.to_der().as_bytes());

        let summary = verify_signed_request(
            &conn,
            SignedRequestHeaders {
                device_id: &device_id,
                timestamp: &timestamp,
                signature_b64: &sig_b64,
            },
            method,
            path,
            body,
            Some("192.168.1.20"),
        )
        .unwrap();

        assert_eq!(summary.device_id, device_id);
        assert_eq!(summary.last_ip.as_deref(), Some("192.168.1.20"));
    }

    #[test]
    fn signature_rejects_tampered_path() {
        let conn = fresh_db();
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (signing, spki_b64) = fresh_keypair();
        let device_id = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "iPhone",
            None,
            None,
        )
        .unwrap();

        let timestamp = chrono::Utc::now().to_rfc3339();
        let canonical = canonical_request_bytes("GET", "/api/v1/status", &timestamp, b"");
        let sig: Signature = signing.sign(&canonical);
        let sig_b64 = BASE64.encode(sig.to_der().as_bytes());

        let result = verify_signed_request(
            &conn,
            SignedRequestHeaders {
                device_id: &device_id,
                timestamp: &timestamp,
                signature_b64: &sig_b64,
            },
            "GET",
            "/api/v1/admin", // tampered
            b"",
            None,
        );
        assert!(matches!(result, Err(PairingError::SignatureMismatch)));
    }

    #[test]
    fn signature_rejects_revoked_device() {
        let conn = fresh_db();
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (signing, spki_b64) = fresh_keypair();
        let device_id = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "iPhone",
            None,
            None,
        )
        .unwrap();
        revoke_device(&conn, &device_id, None).unwrap();

        let timestamp = chrono::Utc::now().to_rfc3339();
        let canonical = canonical_request_bytes("GET", "/api/v1/status", &timestamp, b"");
        let sig: Signature = signing.sign(&canonical);
        let sig_b64 = BASE64.encode(sig.to_der().as_bytes());

        let result = verify_signed_request(
            &conn,
            SignedRequestHeaders {
                device_id: &device_id,
                timestamp: &timestamp,
                signature_b64: &sig_b64,
            },
            "GET",
            "/api/v1/status",
            b"",
            None,
        );
        assert!(matches!(result, Err(PairingError::DeviceRevoked)));
    }

    #[test]
    fn complete_pairing_rejects_when_at_device_limit() {
        let conn = fresh_db();

        // Fill the table up to the cap with already-paired devices.
        for i in 0..MAX_ACTIVE_DEVICES {
            let challenge = create_pairing_nonce(&conn).unwrap();
            let (_, spki_b64) = fresh_keypair();
            complete_pairing(
                &conn,
                &challenge.nonce,
                &challenge.pin,
                &spki_b64,
                &format!("device-{i}"),
                None,
                None,
            )
            .unwrap();
        }

        // The next pairing attempt must be rejected.
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (_, spki_b64) = fresh_keypair();
        let res = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "over-the-limit",
            None,
            None,
        );
        assert!(matches!(res, Err(PairingError::DeviceLimitReached)));

        // Revoking any of the existing devices makes room again.
        let devices = list_paired_devices(&conn, false).unwrap();
        revoke_device(&conn, &devices[0].device_id, None).unwrap();

        let challenge = create_pairing_nonce(&conn).unwrap();
        let (_, spki_b64) = fresh_keypair();
        let ok = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "back-under-limit",
            None,
            None,
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn signature_rejects_aged_session() {
        let conn = fresh_db();
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (signing, spki_b64) = fresh_keypair();
        let device_id = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "old-iphone",
            None,
            None,
        )
        .unwrap();

        // Backdate created_at past the SESSION_MAX_AGE_DAYS cap.
        let way_back = (chrono::Utc::now() - chrono::Duration::days(SESSION_MAX_AGE_DAYS + 1))
            .to_rfc3339();
        conn.execute(
            "UPDATE paired_devices SET created_at = ?1 WHERE device_id = ?2",
            rusqlite::params![way_back, device_id],
        )
        .unwrap();

        let timestamp = chrono::Utc::now().to_rfc3339();
        let canonical = canonical_request_bytes("GET", "/api/v1/status", &timestamp, b"");
        let sig: Signature = signing.sign(&canonical);
        let sig_b64 = BASE64.encode(sig.to_der().as_bytes());

        let result = verify_signed_request(
            &conn,
            SignedRequestHeaders {
                device_id: &device_id,
                timestamp: &timestamp,
                signature_b64: &sig_b64,
            },
            "GET",
            "/api/v1/status",
            b"",
            None,
        );
        assert!(matches!(result, Err(PairingError::SessionExpired)));
    }

    #[test]
    fn signature_rejects_stale_timestamp() {
        let conn = fresh_db();
        let challenge = create_pairing_nonce(&conn).unwrap();
        let (signing, spki_b64) = fresh_keypair();
        let device_id = complete_pairing(
            &conn,
            &challenge.nonce,
            &challenge.pin,
            &spki_b64,
            "iPhone",
            None,
            None,
        )
        .unwrap();

        let timestamp = (chrono::Utc::now() - chrono::Duration::seconds(120)).to_rfc3339();
        let canonical = canonical_request_bytes("GET", "/api/v1/status", &timestamp, b"");
        let sig: Signature = signing.sign(&canonical);
        let sig_b64 = BASE64.encode(sig.to_der().as_bytes());

        let result = verify_signed_request(
            &conn,
            SignedRequestHeaders {
                device_id: &device_id,
                timestamp: &timestamp,
                signature_b64: &sig_b64,
            },
            "GET",
            "/api/v1/status",
            b"",
            None,
        );
        assert!(matches!(result, Err(PairingError::TimestampOutOfWindow)));
    }
}
