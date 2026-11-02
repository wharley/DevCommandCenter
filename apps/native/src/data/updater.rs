//! The DCC's own updates: the release's `latest.json` on GitHub, the same
//! feed and minisign key the Tauri desktop used (so a v0.2.0 desktop moves to
//! this app by its own updater, and this app keeps updating from it). A
//! newer version for this platform is downloaded, its signature checked
//! against the key built in here, and installed: on macOS the `.app` is
//! swapped for the new one (its id, version and code signature checked
//! first, the old one kept until the swap holds); on Linux the `.deb` goes
//! through `pkexec dpkg -i`, as the desktop's updater did. Blocking: off the
//! UI thread.

use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

use base64::Engine;
use serde::Deserialize;

/// Where each release says what it is (`plugins.updater.endpoints`).
pub const FEED: &str =
    "https://github.com/wharley/DevCommandCenter/releases/latest/download/latest.json";
/// `plugins.updater.pubkey`: base64 of the minisign public key file.
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDk5RDRDNzU2MjJBM0REREMKUldUYzNhTWlWc2ZVbVRqZC9GSzZBeU9TbFlBTVNOcE5oenhtZVdZUDNaeDlEaFdrNEJmUWRTdVcK";
/// The installed app's identity, checked on what is about to replace it.
pub const BUNDLE_ID: &str = "com.devcommandcenter.app";
/// An update larger than this is refused.
const MAX_BYTES: u64 = 600 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Feed {
    version: String,
    notes: Option<String>,
    pub_date: Option<String>,
    #[serde(default)]
    platforms: HashMap<String, Asset>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Asset {
    pub signature: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
    pub asset: Asset,
}

/// The feed's names for this machine, the most specific first (Tauri's
/// `darwin-aarch64-app` before `darwin-aarch64`).
pub fn platform_keys() -> &'static [&'static str] {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        &["darwin-aarch64-app", "darwin-aarch64"]
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        &["darwin-x86_64-app", "darwin-x86_64"]
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        &["linux-x86_64-deb"]
    } else {
        &[]
    }
}

/// Whether `offered` is newer than `current` (semver: a local build
/// `0.3.0-local.g…` is older than the released 0.3.0, so it takes it).
pub fn newer(current: &str, offered: &str) -> bool {
    match (
        semver::Version::parse(current.trim_start_matches('v')),
        semver::Version::parse(offered.trim_start_matches('v')),
    ) {
        (Ok(current), Ok(offered)) => offered > current,
        _ => false,
    }
}

/// The feed's update for `keys`, when newer than `current`.
pub fn pick(feed: &str, current: &str, keys: &[&str]) -> Result<Option<Update>, String> {
    let feed: Feed = serde_json::from_str(feed).map_err(|error| error.to_string())?;
    if !newer(current, &feed.version) {
        return Ok(None);
    }
    let Some(asset) = keys.iter().find_map(|key| feed.platforms.get(*key)) else {
        return Ok(None);
    };
    Ok(Some(Update {
        version: feed.version.trim_start_matches('v').to_string(),
        notes: feed.notes,
        date: feed.pub_date,
        asset: asset.clone(),
    }))
}

fn client(timeout: Duration) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("DevCommandCenter")
        .timeout(timeout)
        .build()
        .map_err(|error| error.to_string())
}

/// Asks the feed; `None` when this is the newest.
pub fn check(current: &str) -> Result<Option<Update>, String> {
    let response = client(Duration::from_secs(30))?
        .get(FEED)
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("{} ({FEED})", response.status()));
    }
    let feed = response.text().map_err(|error| error.to_string())?;
    pick(&feed, current, platform_keys())
}

/// The update's file, its signature checked.
pub fn download(update: &Update) -> Result<Vec<u8>, String> {
    // GitHub's asset API answers the file itself only when asked so.
    let response = client(Duration::from_secs(15 * 60))?
        .get(&update.asset.url)
        .header("Accept", "application/octet-stream")
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("{}", response.status()));
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("the update is too large".into());
    }
    verify(&bytes, &update.asset.signature, PUBLIC_KEY)?;
    Ok(bytes)
}

/// Tauri's signature: base64 of a minisign signature file, against base64
/// of a minisign public key file.
fn verify(bytes: &[u8], signature: &str, public_key: &str) -> Result<(), String> {
    let decode = |text: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .ok_or_else(|| "the update's signature is unreadable".to_string())
    };
    let key = minisign_verify::PublicKey::decode(&decode(public_key)?)
        .map_err(|_| "the update key is unreadable".to_string())?;
    let signature = minisign_verify::Signature::decode(&decode(signature)?)
        .map_err(|_| "the update's signature is unreadable".to_string())?;
    key.verify(bytes, &signature, true)
        .map_err(|_| "the update's signature does not match".to_string())
}

#[cfg(target_os = "macos")]
mod install_macos;
#[cfg(target_os = "macos")]
pub use install_macos::install;

/// Linux: the `.deb`, as root through polkit (`pkexec dpkg -i`).
#[cfg(target_os = "linux")]
pub fn install(update: &Update, bytes: &[u8]) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("dev-command-center_{}.deb", update.version));
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    let status = std::process::Command::new("pkexec")
        .args(["dpkg", "-i"])
        .arg(&path)
        .status()
        .map_err(|error| format!("pkexec: {error}"))?;
    let _ = std::fs::remove_file(&path);
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("dpkg -i ended with {status}"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn install(_update: &Update, _bytes: &[u8]) -> Result<(), String> {
    Err("updates are not supported here".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED_JSON: &str = r#"{
        "version": "0.3.0", "notes": "Nativo", "pub_date": "2026-11-02T10:00:00Z",
        "platforms": {
            "darwin-aarch64": { "signature": "a", "url": "https://x/aarch64.tar.gz" },
            "darwin-aarch64-app": { "signature": "b", "url": "https://x/aarch64-app.tar.gz" },
            "linux-x86_64-deb": { "signature": "c", "url": "https://x/amd64.deb" }
        }
    }"#;

    #[test]
    fn a_newer_release_for_this_platform_is_offered() {
        let update = pick(
            FEED_JSON,
            "0.2.0",
            &["darwin-aarch64-app", "darwin-aarch64"],
        )
        .unwrap()
        .unwrap();
        assert_eq!(update.version, "0.3.0");
        assert_eq!(update.asset.url, "https://x/aarch64-app.tar.gz");
        assert_eq!(pick(FEED_JSON, "0.3.0", &["darwin-aarch64"]).unwrap(), None);
        assert_eq!(pick(FEED_JSON, "0.2.0", &["windows-x86_64"]).unwrap(), None);
        assert!(pick("{", "0.2.0", &["darwin-aarch64"]).is_err());
    }

    #[test]
    fn a_local_build_is_older_than_its_release() {
        assert!(newer("0.3.0-local.g45d3ae7f", "0.3.0"));
        assert!(!newer("0.3.0-local.g45d3ae7f", "0.2.0"));
        assert!(newer("0.3.0", "0.3.1"));
        assert!(!newer("0.3.0", "0.3.0"));
        assert!(
            !newer("unknown", "0.3.0"),
            "an unreadable version never updates"
        );
    }

    #[test]
    fn only_what_the_key_signed_installs() {
        let engine = base64::engine::general_purpose::STANDARD;
        let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let public_key = engine.encode(pair.pk.to_box().unwrap().to_string());
        let bytes = b"the new app".to_vec();
        let signature = minisign::sign(
            None,
            &pair.sk,
            std::io::Cursor::new(&bytes),
            Some("trusted"),
            None,
        )
        .unwrap();
        let signature = engine.encode(signature.to_string());
        assert_eq!(verify(&bytes, &signature, &public_key), Ok(()));
        assert!(verify(b"something else", &signature, &public_key).is_err());
        let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let stranger = engine.encode(other.pk.to_box().unwrap().to_string());
        assert!(verify(&bytes, &signature, &stranger).is_err());
        assert!(
            minisign_verify::PublicKey::decode(
                &String::from_utf8(engine.decode(PUBLIC_KEY).unwrap()).unwrap()
            )
            .is_ok(),
            "the release key is readable"
        );
    }
}
