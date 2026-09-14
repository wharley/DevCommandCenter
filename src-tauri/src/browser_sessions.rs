//! Explicit, local import of an existing macOS Chromium profile into DCC's
//! WebKit cookie store. Discovery never opens a cookie database or Keychain.

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
use tauri::{State, Webview, Wry};
use url::Url;
use zeroize::Zeroize;

use crate::browser_commands::BrowserState;

const CHROMIUM_EPOCH_MICROS: i64 = 11_644_473_600_000_000;
const MAX_IMPORTED_COOKIES: usize = 200;
const MAX_COOKIE_NAME_BYTES: usize = 1024;
const MAX_COOKIE_VALUE_BYTES: usize = 16 * 1024;
const BROWSER_WEBVIEW_LABEL: &str = "main";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionProfile {
    pub id: String,
    pub browser: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionProfilesResult {
    pub profiles: Vec<BrowserSessionProfile>,
    pub supported: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BrowserSessionImportInput {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub lifecycle_token: u64,
    pub profile_id: String,
    /// The URL currently displayed in the DCC Browser. Only cookies whose
    /// domain can be sent to this host are considered.
    pub current_url: String,
}

struct PreparedImport {
    cookies: Vec<PreparedSessionCookie>,
    skipped: usize,
}

/// Secret-bearing data that never crosses IPC, logs, or serialization. The
/// native bridge receives the original Chromium domain string, including its
/// leading dot, because `cookie::Cookie` canonicalizes that distinction away.
pub(crate) struct PreparedSessionCookie {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) secure: bool,
    pub(crate) http_only: bool,
    pub(crate) same_site: i64,
    pub(crate) expires_unix_seconds: Option<i64>,
}

#[cfg(target_os = "macos")]
mod native_cookie_store {
    use super::PreparedSessionCookie;
    use std::{
        collections::HashMap,
        ffi::{c_char, c_void, CString},
        sync::{
            atomic::{AtomicU64, Ordering},
            Mutex, OnceLock,
        },
    };
    use tauri::{Webview, Wry};
    use tokio::sync::oneshot;
    #[repr(C)]
    struct NativeCookie {
        name: *const c_char,
        value: *const c_char,
        domain: *const c_char,
        path: *const c_char,
        secure: i32,
        http_only: i32,
        same_site: i32,
        expires: i64,
    }
    type ResultCounts = Result<(usize, usize), String>;
    type Pending = HashMap<u64, oneshot::Sender<ResultCounts>>;
    static NEXT: AtomicU64 = AtomicU64::new(1);
    static PENDING: OnceLock<Mutex<Pending>> = OnceLock::new();
    fn pending() -> &'static Mutex<Pending> {
        PENDING.get_or_init(|| Mutex::new(HashMap::new()))
    }
    struct PendingGuard(u64);
    impl Drop for PendingGuard {
        fn drop(&mut self) {
            if let Ok(mut pending) = pending().lock() {
                pending.remove(&self.0);
            }
        }
    }
    unsafe extern "C" {
        fn dcc_browser_set_session_cookies(
            view: *mut c_void,
            request: u64,
            items: *const NativeCookie,
            count: usize,
            callback: extern "C" fn(u64, usize, usize, *const c_char),
        );
    }
    extern "C" fn done(request: u64, imported: usize, skipped: usize, error: *const c_char) {
        if let Some(sender) = pending()
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request))
        {
            let result = if error.is_null() {
                Ok((imported, skipped))
            } else {
                Err("Browser cookie store could not apply the session".into())
            };
            let _ = sender.send(result);
        }
    }
    pub async fn apply(view: Webview<Wry>, cookies: Vec<PreparedSessionCookie>) -> ResultCounts {
        if cookies.is_empty() {
            return Ok((0, 0));
        }
        if cookies.len() > 200 {
            return Err("Browser session exceeds the import limit".into());
        }
        let strings: Result<Vec<_>, std::ffi::NulError> = cookies
            .iter()
            .map(|cookie| {
                Ok((
                    CString::new(cookie.name.as_str())?,
                    CString::new(cookie.value.as_str())?,
                    CString::new(cookie.domain.as_str())?,
                    CString::new(cookie.path.as_str())?,
                ))
            })
            .collect();
        let strings = strings.map_err(|_| "Browser cookie contains invalid text")?;
        let request = NEXT.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        pending()
            .lock()
            .map_err(|_| "Browser cookie import is unavailable")?
            .insert(request, sender);
        let _guard = PendingGuard(request);
        view.with_webview(move |view| {
            // Owned CStrings stay alive until Objective-C has copied every field.
            let native: Vec<_> = cookies
                .iter()
                .zip(&strings)
                .map(|(cookie, strings)| NativeCookie {
                    name: strings.0.as_ptr(),
                    value: strings.1.as_ptr(),
                    domain: strings.2.as_ptr(),
                    path: strings.3.as_ptr(),
                    secure: cookie.secure as i32,
                    http_only: cookie.http_only as i32,
                    same_site: cookie.same_site as i32,
                    expires: cookie.expires_unix_seconds.unwrap_or(-1),
                })
                .collect();
            unsafe {
                dcc_browser_set_session_cookies(
                    view.inner(),
                    request,
                    native.as_ptr(),
                    native.len(),
                    done,
                );
            }
        })
        .map_err(|_| "Browser cookie store is unavailable")?;
        tokio::time::timeout(std::time::Duration::from_secs(8), receiver)
            .await
            .map_err(|_| "Browser cookie import timed out")?
            .map_err(|_| "Browser cookie import was cancelled")?
    }
}

pub(crate) async fn apply_prepared_cookies(
    view: Webview<Wry>,
    cookies: Vec<PreparedSessionCookie>,
) -> Result<(usize, usize), String> {
    #[cfg(target_os = "macos")]
    {
        native_cookie_store::apply(view, cookies).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (view, cookies);
        Err("Session import is currently available on macOS only.".to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub supported: bool,
    pub message: Option<String>,
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

/// Safe metadata-only discovery. It intentionally neither opens the Cookies
/// SQLite DB nor asks macOS Keychain for access.
#[tauri::command]
pub fn browser_session_profiles(webview: Webview<Wry>) -> BrowserSessionProfilesResult {
    if webview.label() != BROWSER_WEBVIEW_LABEL {
        return BrowserSessionProfilesResult {
            profiles: Vec::new(),
            supported: false,
            message: Some("Session import must be requested from the DCC window.".to_string()),
        };
    }
    #[cfg(target_os = "macos")]
    {
        BrowserSessionProfilesResult {
            profiles: discover_profiles()
                .into_iter()
                .map(|(profile, _)| profile)
                .collect(),
            supported: true,
            message: None,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        BrowserSessionProfilesResult {
            profiles: Vec::new(),
            supported: false,
            message: Some("Session import is currently available on macOS only.".to_string()),
        }
    }
}

#[tauri::command]
pub async fn browser_session_import(
    webview: Webview<Wry>,
    browser: State<'_, BrowserState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
    profile_id: String,
    current_url: String,
) -> Result<BrowserSessionImportResult, String> {
    if webview.label() != BROWSER_WEBVIEW_LABEL {
        return Err("Session import must be requested from the DCC window.".to_string());
    }
    let input = BrowserSessionImportInput {
        workspace_id,
        session_id,
        lifecycle_token,
        profile_id,
        current_url,
    };
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (browser, input);
        return Ok(BrowserSessionImportResult {
            imported: 0,
            skipped: 0,
            supported: false,
            message: Some("Session import is currently available on macOS only.".to_string()),
        });
    }
    #[cfg(target_os = "macos")]
    {
        let browser = browser.inner().clone();
        // Validate the caller's native Browser surface before the potentially
        // interactive Keychain request, but never hold its operation lock while
        // the user responds to that dialog.
        let (_, expected_revision) = crate::browser_commands::browser_session_import_target(
            &browser,
            &input.workspace_id,
            input.session_id.as_deref(),
            input.lifecycle_token,
            &input.current_url,
        )
        .await?;
        let preparation_input = input.clone();
        let prepared = tokio::task::spawn_blocking(move || prepare_macos_import(preparation_input))
            .await
            .map_err(|_| "Session import preparation was interrupted.".to_string())??;
        let (imported, apply_skipped) = crate::browser_commands::apply_browser_session_cookies(
            &browser,
            &input.workspace_id,
            input.session_id.as_deref(),
            input.lifecycle_token,
            &input.current_url,
            expected_revision,
            prepared.cookies,
        )
        .await?;
        Ok(BrowserSessionImportResult {
            imported,
            skipped: prepared.skipped + apply_skipped,
            supported: true,
            message: None,
        })
    }
}

#[cfg(target_os = "macos")]
fn prepare_macos_import(input: BrowserSessionImportInput) -> Result<PreparedImport, String> {
    let url = Url::parse(&input.current_url)
        .map_err(|_| "The current Browser URL is invalid.".to_string())?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err(
            "Session import requires the current Browser to be on an HTTP(S) site.".to_string(),
        );
    }
    let host = url.host_str().unwrap().to_ascii_lowercase();
    let (_, profile) = discover_profiles()
        .into_iter()
        .find(|(candidate, _)| candidate.id == input.profile_id)
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
