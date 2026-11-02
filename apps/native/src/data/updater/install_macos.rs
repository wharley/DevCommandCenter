//! The swap on macOS: the update unpacked next to the running app, checked
//! (one `.app`, this app's id, the announced version, a valid code
//! signature), then put in the running app's place with the old one moved
//! aside first and back if the swap fails. Where the folder is not
//! writable the moves go through an administrator prompt, as the Tauri
//! updater's did. The caller relaunches.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{BUNDLE_ID, Update};

pub fn install(update: &Update, bytes: &[u8]) -> Result<(), String> {
    let app = running_bundle()?;
    let folder = app.parent().ok_or("the app has no folder")?;
    let staging = folder.join(format!(".dcc-update-{}", std::process::id()));
    let (staging, admin) = match std::fs::create_dir_all(&staging) {
        Ok(()) => (staging, false),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let staging = std::env::temp_dir().join(format!("dcc-update-{}", std::process::id()));
            std::fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
            (staging, true)
        }
        Err(error) => return Err(error.to_string()),
    };
    let installed = unpack_and_swap(update, bytes, &app, &staging, admin);
    let _ = std::fs::remove_dir_all(&staging);
    installed
}

fn unpack_and_swap(
    update: &Update,
    bytes: &[u8],
    app: &Path,
    staging: &Path,
    admin: bool,
) -> Result<(), String> {
    let archive = staging.join("update.tar.gz");
    std::fs::write(&archive, bytes).map_err(|error| error.to_string())?;
    let unpacked = staging.join("unpacked");
    std::fs::create_dir_all(&unpacked).map_err(|error| error.to_string())?;
    run(Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&unpacked))?;
    let new_app = single_app(&unpacked)?;
    check_bundle(&new_app, &update.version)?;
    let previous = staging.join("previous.app");
    if admin {
        swap_as_admin(app, &new_app, &previous)
    } else {
        swap(app, &new_app, &previous)
    }
}

/// The running app's bundle (`…/Dev Command Center.app`).
fn running_bundle() -> Result<PathBuf, String> {
    let path = PathBuf::from(
        objc2_foundation::NSBundle::mainBundle()
            .bundlePath()
            .to_string(),
    );
    if path.extension().and_then(|extension| extension.to_str()) != Some("app") {
        return Err("this build runs outside an app bundle".into());
    }
    Ok(path)
}

/// The archive holds exactly one `.app`, a real folder (no link).
pub(super) fn single_app(unpacked: &Path) -> Result<PathBuf, String> {
    let entries: Vec<PathBuf> = std::fs::read_dir(unpacked)
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    match entries.as_slice() {
        [only]
            if only.extension().and_then(|extension| extension.to_str()) == Some("app")
                && std::fs::symlink_metadata(only).is_ok_and(|meta| meta.is_dir()) =>
        {
            Ok(only.clone())
        }
        _ => Err("the update does not hold one app".into()),
    }
}

fn plist_value(app: &Path, key: &str) -> Result<String, String> {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg(format!("Print :{key}"))
        .arg(app.join("Contents").join("Info.plist"))
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("the update has no {key}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// This app's id, the announced version, and a code signature that holds.
fn check_bundle(app: &Path, version: &str) -> Result<(), String> {
    if plist_value(app, "CFBundleIdentifier")? != BUNDLE_ID {
        return Err("the update is another app".into());
    }
    if plist_value(app, "CFBundleShortVersionString")? != version {
        return Err("the update's version is not the announced one".into());
    }
    run(Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app))
    .map_err(|_| "the update's code signature does not hold".to_string())
}

/// The old app aside, the new one in its place; the old one back when that fails.
pub(super) fn swap(app: &Path, new_app: &Path, previous: &Path) -> Result<(), String> {
    std::fs::rename(app, previous).map_err(|error| error.to_string())?;
    if let Err(error) = std::fs::rename(new_app, app) {
        let _ = std::fs::rename(previous, app);
        return Err(error.to_string());
    }
    Ok(())
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

fn swap_as_admin(app: &Path, new_app: &Path, previous: &Path) -> Result<(), String> {
    let script = format!(
        "mv {app} {previous} && (mv {new} {app} || (mv {previous} {app}; exit 1)) && rm -rf {previous}",
        app = quoted(app),
        previous = quoted(previous),
        new = quoted(new_app),
    );
    let apple = format!(
        "do shell script \"{}\" with administrator privileges",
        script.replace('\\', "\\\\").replace('"', "\\\"")
    );
    run(Command::new("/usr/bin/osascript").arg("-e").arg(apple))
}

fn run(command: &mut Command) -> Result<(), String> {
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_app_takes_the_old_ones_place() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Dev Command Center.app");
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        std::fs::write(app.join("Contents/old"), "v0.2").unwrap();
        let unpacked = dir.path().join("unpacked");
        let new_app = unpacked.join("Dev Command Center.app");
        std::fs::create_dir_all(new_app.join("Contents")).unwrap();
        std::fs::write(new_app.join("Contents/new"), "v0.3").unwrap();

        assert_eq!(single_app(&unpacked).unwrap(), new_app);
        swap(&app, &new_app, &dir.path().join("previous.app")).unwrap();
        assert!(app.join("Contents/new").exists());
        assert!(dir.path().join("previous.app/Contents/old").exists());

        // A failed swap puts the old app back.
        let missing = dir.path().join("missing.app");
        assert!(swap(&app, &missing, &dir.path().join("again.app")).is_err());
        assert!(app.join("Contents/new").exists());
    }

    #[test]
    fn an_archive_with_anything_but_one_app_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("A.app")).unwrap();
        std::fs::create_dir_all(dir.path().join("B.app")).unwrap();
        assert!(single_app(dir.path()).is_err());
        let lone = tempfile::tempdir().unwrap();
        std::fs::write(lone.path().join("evil.app"), "not a folder").unwrap();
        assert!(single_app(lone.path()).is_err());
    }
}
