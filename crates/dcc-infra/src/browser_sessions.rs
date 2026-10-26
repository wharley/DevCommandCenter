//! Using a login the person already has in Chrome or Arc on this Mac
//! (`browser_sessions.rs`, shared by the desktop and the native app): the
//! profiles found from metadata alone, then — only when the person picks
//! one for the site open in the Browser — that site's cookies read from the
//! profile's database (read-only, one snapshot, partitioned cookies left
//! out, never a public suffix's), decrypted with the browser's Keychain key
//! and bounded. Values never reach a log or a serialized answer; each app
//! sets them in its own Browser's store.
#![cfg_attr(not(target_os = "macos"), allow(dead_code, unused_imports))]

use std::fs;
use std::path::{Path, PathBuf};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use aes::Aes128;
use cbc::Decryptor;
use pbkdf2::pbkdf2_hmac;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use url::Url;
use zeroize::Zeroize;

const CHROMIUM_EPOCH_MICROS: i64 = 11_644_473_600_000_000;
const MAX_IMPORTED_COOKIES: usize = 200;
const MAX_COOKIE_NAME_BYTES: usize = 1024;
const MAX_COOKIE_VALUE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionProfile {
    pub id: String,
    pub browser: String,
    pub name: String,
}

/// What an import found for the site: the cookies to set, and how many
/// were left out (beyond the limit, undecryptable, expired, malformed).
pub struct PreparedImport {
    pub cookies: Vec<PreparedSessionCookie>,
    pub skipped: usize,
}

/// Secret-bearing data that never crosses IPC, logs, or serialization. The
/// native bridge receives the original Chromium domain string, including its
/// leading dot, because `cookie::Cookie` canonicalizes that distinction away.
pub struct PreparedSessionCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: i64,
    pub expires_unix_seconds: Option<i64>,
}

#[derive(Clone)]
struct ProfileSource {
    browser: &'static str,
    keychain_service: &'static str,
    root: PathBuf,
}

#[derive(Clone)]
struct ResolvedProfile {
    source: ProfileSource,
    directory: PathBuf,
}

#[derive(Debug)]
struct ChromiumCookie {
    name: String,
    host_key: String,
    path: String,
    encrypted_value: Vec<u8>,
    expires_utc: i64,
    is_secure: bool,
    is_httponly: bool,
    samesite: i64,
    host_digest: bool,
}

fn sources() -> Vec<ProfileSource> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(home) = home else { return Vec::new() };
    vec![
        ProfileSource {
            browser: "chrome",
            keychain_service: "Chrome Safe Storage",
            root: home.join("Library/Application Support/Google/Chrome"),
        },
        ProfileSource {
            browser: "arc",
            keychain_service: "Arc Safe Storage",
            root: home.join("Library/Application Support/Arc/User Data"),
        },
    ]
}

fn profile_id(browser: &str, directory_name: &str) -> String {
    format!("{browser}:{directory_name}")
}

fn supported_profile_directory(name: &str) -> bool {
    name == "Default"
        || name
            .strip_prefix("Profile ")
            .is_some_and(|suffix| suffix.parse::<u32>().is_ok())
}

fn cookie_db(directory: &Path) -> PathBuf {
    let current = directory.join("Network/Cookies");
    if current.is_file() {
        current
    } else {
        directory.join("Cookies")
    }
}

fn discover_profiles() -> Vec<(BrowserSessionProfile, ResolvedProfile)> {
    let mut found = Vec::new();
    for source in sources() {
        let Ok(entries) = fs::read_dir(&source.root) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let directory = entry.path();
            if supported_profile_directory(&name) && cookie_db(&directory).is_file() {
                found.push((
                    BrowserSessionProfile {
                        id: profile_id(source.browser, &name),
                        browser: source.browser.to_string(),
                        name,
                    },
                    ResolvedProfile {
                        source: source.clone(),
                        directory,
                    },
                ));
            }
        }
    }
    found.sort_by(|left, right| left.0.id.cmp(&right.0.id));
    found
}

/// The profiles found, from metadata only: neither the cookie database is
/// opened nor the Keychain asked.
pub fn list_profiles() -> Vec<BrowserSessionProfile> {
    discover_profiles()
        .into_iter()
        .map(|(profile, _)| profile)
        .collect()
}

/// `profile_id`'s cookies that can be sent to `current_url`'s host,
/// decrypted with the browser's Keychain key (macOS asks the person). The
/// only path that reads encrypted cookies; values are never logged.
#[cfg(not(target_os = "macos"))]
pub fn prepare_import(_profile_id: &str, _current_url: &str) -> Result<PreparedImport, String> {
    Err("Session import is currently available on macOS only.".to_string())
}

#[cfg(target_os = "macos")]
pub fn prepare_import(profile_id: &str, current_url: &str) -> Result<PreparedImport, String> {
    let url =
        Url::parse(current_url).map_err(|_| "The current Browser URL is invalid.".to_string())?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err(
            "Session import requires the current Browser to be on an HTTP(S) site.".to_string(),
        );
    }
    let host = url.host_str().unwrap().to_ascii_lowercase();
    let (_, profile) = discover_profiles()
        .into_iter()
        .find(|(candidate, _)| candidate.id == profile_id)
        .ok_or_else(|| "The selected browser profile is unavailable.".to_string())?;

    // This is the only path that may read encrypted cookies or request the
    // browser's locally stored Keychain item. Neither values nor failures are
    // logged or returned to the renderer.
    let encrypted = read_matching_cookies(&cookie_db(&profile.directory), &host)?;
    if encrypted.is_empty() {
        return Ok(PreparedImport {
            cookies: Vec::new(),
            skipped: 0,
        });
    }
    let mut key = macos_safe_storage_key(profile.source.keychain_service)?;
    // Bounded work protects the UI thread. Cookies beyond the limit are
    // truthfully counted as skipped rather than silently discarded.
    let mut skipped = encrypted.len().saturating_sub(MAX_IMPORTED_COOKIES);
    let mut cookies = Vec::new();
    for cookie in encrypted.into_iter().take(MAX_IMPORTED_COOKIES) {
        match decrypt_and_map_cookie(cookie, &key, &host) {
            Ok(cookie) => cookies.push(cookie),
            Err(_) => skipped += 1,
        }
    }
    key.zeroize();
    Ok(PreparedImport { cookies, skipped })
}

#[cfg(target_os = "macos")]
fn read_matching_cookies(path: &Path, host: &str) -> Result<Vec<ChromiumCookie>, String> {
    // SQLite read-only mode prevents schema, journal, or cookie mutations. A
    // transaction pins one coherent snapshot while the browser may be running.
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "The selected profile's cookie store could not be opened.".to_string())?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| "The selected profile's cookie store could not be read.".to_string())?;
    let has_partition_key = transaction
        .prepare("PRAGMA table_info(cookies)")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|_| "The selected profile's cookie schema is unsupported.".to_string())?
        .iter()
        .any(|column| column == "top_frame_site_key");
    // Schema version 24 introduced the SHA-256 host prefix. Unknown metadata
    // is treated as current to avoid accepting a value without verification.
    let host_digest = transaction
        .query_row("SELECT value FROM meta WHERE key = 'version'", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|version| version >= 24)
        .unwrap_or(true);
    // Filter in SQLite: the importer never materializes encrypted values from
    // unrelated sites. The second predicate matches only leading-dot domain
    // cookies that can be sent to a subdomain of the current host.
    let query = if has_partition_key {
        "SELECT name, host_key, path, encrypted_value, expires_utc, is_secure, is_httponly, samesite FROM cookies WHERE (host_key = ?1 OR host_key = '.' || ?1 OR (substr(host_key, 1, 1) = '.' AND ?1 LIKE '%.' || substr(host_key, 2))) AND (top_frame_site_key IS NULL OR top_frame_site_key = '') LIMIT ?2"
    } else {
        "SELECT name, host_key, path, encrypted_value, expires_utc, is_secure, is_httponly, samesite FROM cookies WHERE host_key = ?1 OR host_key = '.' || ?1 OR (substr(host_key, 1, 1) = '.' AND ?1 LIKE '%.' || substr(host_key, 2)) LIMIT ?2"
    };
    let mut statement = transaction
        .prepare(query)
        .map_err(|_| "The selected profile's cookie schema is unsupported.".to_string())?;
    let rows = statement
        .query_map((host, (MAX_IMPORTED_COOKIES + 1) as i64), |row| {
            Ok(ChromiumCookie {
                name: row.get(0)?,
                host_key: row.get(1)?,
                path: row.get(2)?,
                encrypted_value: row.get(3)?,
                expires_utc: row.get(4)?,
                is_secure: row.get(5)?,
                is_httponly: row.get(6)?,
                samesite: row.get(7)?,
                host_digest,
            })
        })
        .map_err(|_| "The selected profile's cookie store could not be read.".to_string())?;
    let mut cookies = Vec::new();
    for cookie in rows.flatten() {
        if cookie_domain_matches(&cookie.host_key, host) {
            cookies.push(cookie);
        }
    }
    Ok(cookies)
}

#[cfg(target_os = "macos")]
fn macos_safe_storage_key(service: &str) -> Result<[u8; 16], String> {
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
    // The native Security framework attributes its prompt to DCC itself.
    // Only the selected browser's encryption key is requested, after consent.
    let mut options = ItemSearchOptions::new();
    options
        .class(ItemClass::generic_password())
        .service(service)
        .load_data(true);
    let result = options
        .search()
        .map_err(|_| "macOS Keychain did not grant access to this browser profile.".to_string())?;
    let mut password = result
        .into_iter()
        .find_map(|item| match item {
            SearchResult::Data(data) => Some(data),
            _ => None,
        })
        .ok_or_else(|| "The browser encryption key was unavailable.".to_string())?;
    let key = derive_chromium_key(&password);
    password.zeroize();
    Ok(key)
}

fn derive_chromium_key(password: &[u8]) -> [u8; 16] {
    let mut key = [0u8; 16];
    // Chromium's macOS OSCrypt path deliberately uses PBKDF2-HMAC-SHA1.
    pbkdf2_hmac::<Sha1>(password, b"saltysalt", 1003, &mut key);
    key
}

fn cookie_domain_matches(raw_domain: &str, host: &str) -> bool {
    let domain = raw_domain.trim_start_matches('.').to_ascii_lowercase();
    if raw_domain.starts_with('.') {
        // A domain attribute must name a registrable domain, never a public
        // suffix such as `.com` or `.co.uk`.
        if psl::domain(domain.as_bytes()).is_none() {
            return false;
        }
        host == domain || host.strip_suffix(&format!(".{domain}")).is_some()
    } else {
        // Chromium records host-only cookies without a leading dot.
        host == domain
    }
}

fn decrypt_chromium_cookie(
    encrypted: &[u8],
    key: &[u8; 16],
    host_key: &str,
    host_digest: bool,
) -> Result<Vec<u8>, String> {
    let cipher = encrypted
        .strip_prefix(b"v10")
        .or_else(|| encrypted.strip_prefix(b"v11"))
        .ok_or_else(|| "unsupported encrypted cookie format".to_string())?;
    let mut bytes = cipher.to_vec();
    let decrypted = Decryptor::<Aes128>::new(key.into(), (&[b' '; 16]).into())
        .decrypt_padded_mut::<Pkcs7>(&mut bytes)
        .map_err(|_| "cookie could not be decrypted".to_string())?;
    if !host_digest {
        return Ok(decrypted.to_vec());
    }
    let digest = Sha256::digest(host_key.as_bytes());
    if decrypted.len() < digest.len() || decrypted[..digest.len()] != digest[..] {
        return Err("cookie host verification failed".to_string());
    }
    Ok(decrypted[digest.len()..].to_vec())
}

fn decrypt_and_map_cookie(
    raw: ChromiumCookie,
    key: &[u8; 16],
    current_host: &str,
) -> Result<PreparedSessionCookie, String> {
    if !cookie_domain_matches(&raw.host_key, current_host)
        || raw.name.is_empty()
        || raw.name.len() > MAX_COOKIE_NAME_BYTES
        || raw.path.is_empty()
        || !raw.path.starts_with('/')
    {
        return Err("invalid cookie metadata".to_string());
    }
    let value = decrypt_chromium_cookie(&raw.encrypted_value, key, &raw.host_key, raw.host_digest)?;
    if value.len() > MAX_COOKIE_VALUE_BYTES {
        return Err("cookie is too large".to_string());
    }
    let value = String::from_utf8(value).map_err(|_| "cookie value is not text".to_string())?;
    let expires_unix_seconds = if raw.expires_utc == 0 {
        None
    } else {
        if raw.expires_utc <= CHROMIUM_EPOCH_MICROS {
            return Err("invalid cookie expiry".to_string());
        }
        let unix_seconds = (raw.expires_utc - CHROMIUM_EPOCH_MICROS) / 1_000_000;
        if unix_seconds <= chrono::Utc::now().timestamp() {
            return Err("expired cookie".to_string());
        }
        Some(unix_seconds)
    };
    Ok(PreparedSessionCookie {
        name: raw.name,
        value,
        domain: raw.host_key,
        path: raw.path,
        secure: raw.is_secure,
        http_only: raw.is_httponly,
        same_site: raw.samesite,
        expires_unix_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncryptMut, KeyIvInit};
    use cbc::Encryptor;

    fn encrypted_fixture(key: &[u8; 16], host: &str, value: &[u8]) -> Vec<u8> {
        let mut plaintext = Sha256::digest(host.as_bytes()).to_vec();
        plaintext.extend_from_slice(value);
        let message_len = plaintext.len();
        plaintext.resize(message_len + 16, 0);
        let encrypted = Encryptor::<Aes128>::new(key.into(), (&[b' '; 16]).into())
            .encrypt_padded_mut::<Pkcs7>(&mut plaintext, message_len)
            .unwrap();
        [b"v10".as_slice(), encrypted].concat()
    }

    #[test]
    fn chromium_v10_decryption_verifies_its_host_digest() {
        let key = [7; 16];
        let encrypted = encrypted_fixture(&key, ".example.test", b"fixture-value");
        assert_eq!(
            decrypt_chromium_cookie(&encrypted, &key, ".example.test", true).unwrap(),
            b"fixture-value"
        );
        assert!(decrypt_chromium_cookie(&encrypted, &key, ".other.test", true).is_err());
    }

    #[test]
    fn chromium_macos_key_derivation_uses_the_documented_sha1_parameters() {
        // Independent PBKDF2-HMAC-SHA1 fixture: password `fixture-password`,
        // salt `saltysalt`, 1003 rounds, output length 16.
        assert_eq!(
            derive_chromium_key(b"fixture-password"),
            [
                0x5d, 0x84, 0xe8, 0x8b, 0x8d, 0x26, 0x28, 0xe2, 0x31, 0x02, 0xb4, 0x64, 0xd7, 0x7a,
                0x5b, 0xbd
            ]
        );
    }

    #[test]
    fn import_only_accepts_domains_that_can_be_sent_to_current_host() {
        assert!(cookie_domain_matches(".example.test", "app.example.test"));
        assert!(cookie_domain_matches(
            "app.example.test",
            "app.example.test"
        ));
        assert!(!cookie_domain_matches(
            "app.example.test",
            "other.app.example.test"
        ));
        assert!(!cookie_domain_matches(".com", "example.com"));
        assert!(!cookie_domain_matches(
            "evil-example.test",
            "app.example.test"
        ));
        assert!(!cookie_domain_matches(
            "example.test",
            "example.test.evil.test"
        ));
    }

    #[test]
    fn mapped_cookie_preserves_security_attributes_without_exposing_value() {
        let key = [5; 16];
        let encrypted_value = encrypted_fixture(&key, ".example.test", b"synthetic");
        let future = chrono::Utc::now().timestamp() + 86_400;
        let raw = ChromiumCookie {
            name: "session".into(),
            host_key: ".example.test".into(),
            path: "/".into(),
            encrypted_value,
            expires_utc: CHROMIUM_EPOCH_MICROS + future * 1_000_000,
            is_secure: true,
            is_httponly: true,
            samesite: 2,
            host_digest: true,
        };
        let cookie = decrypt_and_map_cookie(raw, &key, "app.example.test").unwrap();
        assert_eq!(cookie.domain, ".example.test");
        assert_eq!(cookie.path, "/");
        assert!(cookie.secure);
        assert!(cookie.http_only);
        assert_eq!(cookie.same_site, 2);
        assert_eq!(cookie.expires_unix_seconds, Some(future));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sqlite_filter_keeps_only_current_site_and_unpartitioned_rows() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("Cookies");
        let connection = Connection::open(&database).unwrap();
        connection.execute_batch("CREATE TABLE cookies (name TEXT, host_key TEXT, path TEXT, encrypted_value BLOB, expires_utc INTEGER, is_secure INTEGER, is_httponly INTEGER, samesite INTEGER, top_frame_site_key TEXT); CREATE TABLE meta (key TEXT, value INTEGER); INSERT INTO meta VALUES ('version', 24);").unwrap();
        for (host, partition) in [
            ("example.test", ""),
            (".example.test", ""),
            (".other.test", ""),
            (".example.test", "https://embedder.test"),
        ] {
            connection
                .execute(
                    "INSERT INTO cookies VALUES ('fixture', ?1, '/', X'763130', 0, 1, 1, 1, ?2)",
                    (host, partition),
                )
                .unwrap();
        }
        drop(connection);
        let rows = read_matching_cookies(&database, "example.test").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| row.host_key == "example.test"));
        assert!(rows.iter().any(|row| row.host_key == ".example.test"));
    }

    #[test]
    fn nonzero_expiry_before_chromium_epoch_is_rejected() {
        let key = [9; 16];
        let encrypted_value = encrypted_fixture(&key, "example.test", b"fixture");
        let raw = ChromiumCookie {
            name: "fixture".into(),
            host_key: "example.test".into(),
            path: "/".into(),
            encrypted_value,
            expires_utc: CHROMIUM_EPOCH_MICROS,
            is_secure: true,
            is_httponly: true,
            samesite: 1,
            host_digest: true,
        };
        assert!(decrypt_and_map_cookie(raw, &key, "example.test").is_err());
    }
}
