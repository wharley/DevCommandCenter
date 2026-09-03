use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const RELEASE_VERSION: &str = "agy_acp_server_20260818_01_RC01";

#[derive(Clone, Copy)]
struct Asset {
    platform: &'static str,
    url: &'static str,
    sha256: &'static str,
    archive_bytes: u64,
    executable: &'static str,
    executable_bytes: u64,
    harness: &'static str,
    harness_bytes: u64,
}

fn asset() -> Result<Asset, String> {
    let key = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    match key.as_str() {
        "macos-aarch64" => Ok(Asset {
            platform: "darwin-arm64",
            url: "https://dl.google.com/agy-extensions/releases/macos/agy-acp-server-agy_acp_server_20260818_01_RC01-darwin-arm64.zip",
            sha256: "f122ca7e7030a27f9649da4cf1a7d80e12c48c5f6118ff35affc34d56cbf83dd",
            archive_bytes: 314_500_221,
            executable: "agy_acp_server.par",
            executable_bytes: 792_105_680,
            harness: "localharness_external",
            harness_bytes: 101_551_680,
        }),
        "linux-x86_64" => Ok(Asset {
            platform: "linux-x64",
            url: "https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-agy_acp_server_20260818_01_RC01-linux-x86_64.zip",
            sha256: "ce3f09628575b25497cf5a3c19d073b49acb80f1dab1ff8592919e9c9b8799e1",
            archive_bytes: 543_411_011,
            executable: "agy_acp_server.par",
            executable_bytes: 1_529_513_909,
            harness: "localharness_external",
            harness_bytes: 117_532_520,
        }),
        "linux-aarch64" => Ok(Asset {
            platform: "linux-arm64",
            url: "https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-agy_acp_server_20260818_01_RC01-linux-arm64.zip",
            sha256: "70fcdac70684de60f7a0eb16ea497d6cc4498728420f060e0850cfc9a9329b40",
            archive_bytes: 524_995_159,
            executable: "agy_acp_server.par",
            executable_bytes: 1_519_373_648,
            harness: "localharness_external",
            harness_bytes: 110_601_552,
        }),
        "windows-x86_64" => Ok(Asset {
            platform: "win32-x64",
            url: "https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-agy_acp_server_20260818_01_RC01-windows-x86_64.zip",
            sha256: "35c7dd169c2794172ce02e9444a6db4a8ed4bb11398be07976cac2ee494f44e6",
            archive_bytes: 331_985_114,
            executable: "agy_acp_server.exe",
            executable_bytes: 297_200_088,
            harness: "localharness_external.exe",
            harness_bytes: 122_038_424,
        }),
        "windows-aarch64" => Ok(Asset {
            platform: "win32-arm64",
            url: "https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-agy_acp_server_20260818_01_RC01-windows-arm64.zip",
            sha256: "1522056748d45fbc34d0be72b41b99b0637be1b4caad0b34d37eb16d04ccb9c4",
            archive_bytes: 332_484_576,
            executable: "agy_acp_server.exe",
            executable_bytes: 301_449_928,
            harness: "localharness_external.exe",
            harness_bytes: 114_173_080,
        }),
        _ => Err(format!(
            "Google does not publish an Antigravity ACP runtime for {key}"
        )),
    }
}

pub fn managed_executable_path(app_data_dir: &Path) -> Option<PathBuf> {
    let asset = asset().ok()?;
    let directory = managed_version_dir(app_data_dir, asset);
    let path = directory.join(asset.executable);
    (valid_pair(&directory, asset)
        && fs::read_to_string(directory.join(".verified-release"))
            .ok()
            .as_deref()
            == Some(RELEASE_VERSION))
    .then_some(path)
}

pub fn mark_verified(executable: &Path) -> Result<(), String> {
    let directory = executable
        .parent()
        .ok_or_else(|| "managed Antigravity executable has no parent directory".to_string())?;
    fs::write(directory.join(".verified-release"), RELEASE_VERSION)
        .map_err(|error| format!("could not activate the verified Antigravity runtime: {error}"))
}

fn managed_root(app_data_dir: &Path, asset: Asset) -> PathBuf {
    app_data_dir
        .join("tools")
        .join("antigravity-acp")
        .join(asset.platform)
}

fn managed_version_dir(app_data_dir: &Path, asset: Asset) -> PathBuf {
    managed_root(app_data_dir, asset)
        .join("versions")
        .join(asset.sha256)
}

pub fn install(app_data_dir: &Path) -> Result<PathBuf, String> {
    static INSTALL_GATE: OnceLock<Mutex<()>> = OnceLock::new();
    let _install_guard = INSTALL_GATE
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "the Antigravity installer lock is unavailable".to_string())?;
    let asset = asset()?;
    let destination = managed_version_dir(app_data_dir, asset);
    if valid_pair(&destination, asset) {
        return Ok(destination.join(asset.executable));
    }
    if destination.exists() {
        return Err("an incomplete managed Antigravity release already exists".into());
    }
    let root = managed_root(app_data_dir, asset);
    fs::create_dir_all(root.join("versions"))
        .map_err(|error| format!("could not create the Antigravity tools directory: {error}"))?;
    clean_stale_operations(&root)?;
    let required = asset
        .archive_bytes
        .saturating_add(asset.executable_bytes)
        .saturating_add(asset.harness_bytes)
        .saturating_add(256 * 1024 * 1024);
    if available_bytes(&root).is_some_and(|available| available < required) {
        return Err(format!(
            "Antigravity needs at least {} MB of free disk space for verified installation",
            required.div_ceil(1024 * 1024)
        ));
    }
    let operation = Uuid::new_v4().to_string();
    let archive_path = root.join(format!("download-{operation}.zip"));
    let staging = root.join(format!("staging-{operation}"));
    let result = install_inner(asset, &archive_path, &staging, &destination);
    let _ = fs::remove_file(&archive_path);
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result.map(|()| destination.join(asset.executable))
}

fn clean_stale_operations(root: &Path) -> Result<(), String> {
    let entries = fs::read_dir(root)
        .map_err(|error| format!("could not inspect the Antigravity tools directory: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("could not inspect an Antigravity installer entry: {error}")
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(operation) = name
            .strip_prefix("download-")
            .and_then(|value| value.strip_suffix(".zip"))
        {
            if Uuid::parse_str(operation).is_ok() {
                fs::remove_file(entry.path()).map_err(|error| {
                    format!("could not clean an interrupted Antigravity download: {error}")
                })?;
            }
        } else if let Some(operation) = name.strip_prefix("staging-") {
            if Uuid::parse_str(operation).is_ok() {
                fs::remove_dir_all(entry.path()).map_err(|error| {
                    format!("could not clean an interrupted Antigravity installation: {error}")
                })?;
            }
        }
    }
    Ok(())
}

fn install_inner(
    asset: Asset,
    archive_path: &Path,
    staging: &Path,
    destination: &Path,
) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(45 * 60))
        .build()
        .map_err(|error| format!("could not prepare the Antigravity download: {error}"))?;
    let mut response = client
        .get(asset.url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("could not download the official Antigravity runtime: {error}"))?;
    if response
        .content_length()
        .is_some_and(|size| size != asset.archive_bytes)
    {
        return Err("the Antigravity archive size does not match the official registry".into());
    }
    let mut archive = File::create(archive_path)
        .map_err(|error| format!("could not create the Antigravity download: {error}"))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| format!("Antigravity download was interrupted: {error}"))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > asset.archive_bytes {
            return Err("the Antigravity download exceeded the official archive size".into());
        }
        hasher.update(&buffer[..read]);
        archive
            .write_all(&buffer[..read])
            .map_err(|error| format!("could not save the Antigravity download: {error}"))?;
    }
    archive
        .sync_all()
        .map_err(|error| format!("could not finalize the Antigravity download: {error}"))?;
    if total != asset.archive_bytes || format!("{:x}", hasher.finalize()) != asset.sha256 {
        return Err("the Antigravity download failed SHA-256 verification".into());
    }

    fs::create_dir(staging)
        .map_err(|error| format!("could not stage the Antigravity runtime: {error}"))?;
    let file = File::open(archive_path)
        .map_err(|error| format!("could not open the verified Antigravity archive: {error}"))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| format!("could not read the verified Antigravity archive: {error}"))?;
    let expected = [
        (asset.executable, asset.executable_bytes),
        (asset.harness, asset.harness_bytes),
    ];
    for (name, expected_bytes) in expected {
        let mut member = zip
            .by_name(name)
            .map_err(|_| format!("the official archive omitted {name}"))?;
        if member.size() != expected_bytes || member.name() != name {
            return Err(format!("the official archive contains an invalid {name}"));
        }
        let path = staging.join(name);
        let mut output =
            File::create(&path).map_err(|error| format!("could not extract {name}: {error}"))?;
        let copied = io::copy(&mut member, &mut output)
            .map_err(|error| format!("could not extract {name}: {error}"))?;
        output
            .sync_all()
            .map_err(|error| format!("could not finalize {name}: {error}"))?;
        if copied != expected_bytes {
            return Err(format!("the extracted {name} has an invalid size"));
        }
        make_executable(&path)?;
    }
    if !valid_pair(staging, asset) {
        return Err("the extracted Antigravity runtime is incomplete".into());
    }
    if destination.exists() {
        if valid_pair(destination, asset) {
            return Ok(());
        }
        return Err("an incomplete managed Antigravity release already exists".into());
    }
    fs::rename(staging, destination)
        .map_err(|error| format!("could not activate the Antigravity runtime: {error}"))?;
    Ok(())
}

fn valid_pair(directory: &Path, asset: Asset) -> bool {
    valid_file(&directory.join(asset.executable), asset.executable_bytes)
        && valid_file(&directory.join(asset.harness), asset.harness_bytes)
}

fn valid_file(path: &Path, bytes: u64) -> bool {
    fs::metadata(path).is_ok_and(|metadata| {
        if !metadata.is_file() || metadata.len() != bytes {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    })
}

#[cfg(unix)]
fn available_bytes(path: &Path) -> Option<u64> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a live NUL-terminated string and `stats` points to
    // writable storage for the duration of the libc call.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statvfs returned success and initialized the output structure.
    let stats = unsafe { stats.assume_init() };
    Some((stats.f_bavail as u64).saturating_mul(stats.f_frsize))
}

#[cfg(not(unix))]
fn available_bytes(_path: &Path) -> Option<u64> {
    None
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("could not inspect extracted executable: {error}"))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("could not make the Antigravity runtime executable: {error}"))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_release_has_a_pinned_sha_and_bounded_names() {
        let asset = asset().expect("supported test platform");
        assert_eq!(asset.sha256.len(), 64);
        assert!(!asset.executable.contains(['/', '\\']));
        assert!(!asset.harness.contains(['/', '\\']));
        assert!(asset.archive_bytes > 0);
    }

    #[test]
    fn interrupted_operation_cleanup_is_narrowly_scoped() {
        let root = tempfile::tempdir().expect("temporary installer root");
        let operation = Uuid::new_v4();
        let download = root.path().join(format!("download-{operation}.zip"));
        let staging = root.path().join(format!("staging-{operation}"));
        let unrelated = root.path().join("download-keep-me.zip");
        fs::write(&download, b"partial").expect("partial download");
        fs::create_dir(&staging).expect("partial staging directory");
        fs::write(&unrelated, b"user file").expect("unrelated file");

        clean_stale_operations(root.path()).expect("cleanup succeeds");

        assert!(!download.exists());
        assert!(!staging.exists());
        assert!(unrelated.exists());
    }
}
