//! The iOS Simulators on this Mac, for the Device panel: listed, booted and
//! shut down with `xcrun simctl`; one opened is streamed and driven by the
//! Claude helper's `device` subcommand (`link`), its H.264 decoded here
//! (`decode`). The simulators stay the system's: Xcode installs runtimes.

#[cfg(target_os = "macos")]
pub mod decode;
pub mod link;
pub mod stream;

use std::process::Command;

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Simulator {
    pub udid: String,
    pub name: String,
    /// `iOS 26.1`.
    pub runtime: String,
    pub booted: bool,
    pub ipad: bool,
    /// When it last ran (`2026-10-15T19:22:05Z`); never, for one not used yet.
    pub last_booted: Option<String>,
}

/// How many simulators "Recentes" shows.
const RECENT: usize = 3;

/// The list in the panel's sections: the ones used lately (running first),
/// then the other iPhones, then the other iPads.
pub fn sections(simulators: &[Simulator]) -> [Vec<&Simulator>; 3] {
    let recent: Vec<&Simulator> = simulators
        .iter()
        .filter(|sim| sim.booted || sim.last_booted.is_some())
        .take(RECENT)
        .collect();
    let rest = |ipad: bool| {
        simulators
            .iter()
            .filter(|sim| sim.ipad == ipad && !recent.iter().any(|seen| seen.udid == sim.udid))
            .collect::<Vec<_>>()
    };
    let (iphones, ipads) = (rest(false), rest(true));
    [recent, iphones, ipads]
}

#[derive(Deserialize)]
struct Listing {
    devices: std::collections::BTreeMap<String, Vec<Entry>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    udid: String,
    name: String,
    state: String,
    #[serde(default)]
    is_available: bool,
    #[serde(default)]
    device_type_identifier: String,
    #[serde(default)]
    last_booted_at: Option<String>,
}

/// `com.apple.CoreSimulator.SimRuntime.iOS-26-1` → `iOS 26.1`; only iOS.
fn runtime_name(key: &str) -> Option<String> {
    let version = key.rsplit('.').next()?.strip_prefix("iOS-")?;
    Some(format!("iOS {}", version.replace('-', ".")))
}

fn version_of(runtime: &str) -> Vec<u32> {
    runtime
        .trim_start_matches("iOS ")
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// `simctl list devices available -j`: the iPhones and iPads, the running
/// ones first, then the most recently used, then iPhones before iPads, the
/// newest iOS first, then by name.
pub fn parse_list(json: &str) -> Vec<Simulator> {
    let Ok(listing) = serde_json::from_str::<Listing>(json) else {
        return Vec::new();
    };
    let mut simulators: Vec<Simulator> = listing
        .devices
        .into_iter()
        .filter_map(|(key, entries)| Some((runtime_name(&key)?, entries)))
        .flat_map(|(runtime, entries)| {
            entries
                .into_iter()
                .filter(|entry| entry.is_available)
                .map(move |entry| Simulator {
                    ipad: entry.device_type_identifier.contains("iPad")
                        || entry.name.starts_with("iPad"),
                    udid: entry.udid,
                    name: entry.name,
                    runtime: runtime.clone(),
                    booted: entry.state == "Booted",
                    last_booted: entry.last_booted_at,
                })
        })
        .collect();
    simulators.sort_by(|a, b| {
        b.booted
            .cmp(&a.booted)
            .then_with(|| b.last_booted.cmp(&a.last_booted))
            .then_with(|| a.ipad.cmp(&b.ipad))
            .then_with(|| version_of(&b.runtime).cmp(&version_of(&a.runtime)))
            .then_with(|| a.name.cmp(&b.name))
    });
    simulators
}

fn simctl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("xcrun")
        .arg("simctl")
        .args(args)
        .output()
        .map_err(|error| format!("xcrun: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// The simulators Xcode has, or why there are none (no Xcode).
pub fn list() -> Result<Vec<Simulator>, String> {
    simctl(&["list", "devices", "available", "-j"]).map(|json| parse_list(&json))
}

/// Boots `udid` without the Simulator window and waits until it is up;
/// `true` when this call booted it (not when it was running already).
pub fn boot(udid: &str) -> Result<bool, String> {
    let booted = match simctl(&["boot", udid]) {
        Ok(_) => true,
        // Already running (the person's, from Xcode or an earlier panel).
        Err(error) if error.contains("current state: Booted") => false,
        Err(error) => return Err(error),
    };
    simctl(&["bootstatus", udid, "-b"]).map(|_| booted)
}

pub fn shutdown(udid: &str) -> Result<(), String> {
    simctl(&["shutdown", udid]).map(|_| ())
}

/// Shuts `udid` down without waiting, even while the app quits.
pub fn shutdown_detached(udid: &str) {
    if cfg!(test) {
        return;
    }
    let _ = Command::new("xcrun")
        .args(["simctl", "shutdown", udid])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// The Claude helper that runs the simulator's capture (`device`): this
/// app's own, or the installed one in development.
pub fn helper() -> Option<std::path::PathBuf> {
    std::env::var_os("DCC_CLAUDE_SIDECAR_PATH")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_then_recent_then_iphones_before_ipads() {
        let json = r#"{"devices":{
            "com.apple.CoreSimulator.SimRuntime.iOS-18-3":[
                {"udid":"A","name":"iPad Air 11-inch (M3)","state":"Shutdown","isAvailable":true,"deviceTypeIdentifier":"com.apple.CoreSimulator.SimDeviceType.iPad-Air-11-inch-M3"},
                {"udid":"B","name":"iPhone 16","state":"Shutdown","isAvailable":true,"deviceTypeIdentifier":"com.apple.CoreSimulator.SimDeviceType.iPhone-16"},
                {"udid":"C","name":"iPhone 16 Pro","state":"Shutdown","isAvailable":true,"lastBootedAt":"2026-10-15T19:22:05Z"}],
            "com.apple.CoreSimulator.SimRuntime.iOS-26-1":[
                {"udid":"D","name":"iPhone 17 Pro","state":"Shutdown","isAvailable":true},
                {"udid":"E","name":"iPhone Air","state":"Shutdown","isAvailable":false}],
            "com.apple.CoreSimulator.SimRuntime.watchOS-11-0":[
                {"udid":"F","name":"Apple Watch","state":"Booted","isAvailable":true}],
            "com.apple.CoreSimulator.SimRuntime.iOS-17-5":[
                {"udid":"G","name":"iPhone 15","state":"Booted","isAvailable":true,"lastBootedAt":"2026-11-02T10:00:00Z"},
                {"udid":"H","name":"iPhone SE (3rd generation)","state":"Shutdown","isAvailable":true,"lastBootedAt":"2026-11-01T10:00:00Z"}]
        }}"#;
        let list = parse_list(json);
        let order: Vec<&str> = list.iter().map(|sim| sim.udid.as_str()).collect();
        assert_eq!(order, vec!["G", "H", "C", "D", "B", "A"]);
        assert!(list.iter().find(|sim| sim.udid == "A").unwrap().ipad);
        let [recent, iphones, ipads] = sections(&list);
        let ids =
            |sims: &Vec<&Simulator>| sims.iter().map(|sim| sim.udid.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&recent), vec!["G", "H", "C"]);
        assert_eq!(ids(&iphones), vec!["D", "B"]);
        assert_eq!(ids(&ipads), vec!["A"]);
        assert!(parse_list("not json").is_empty());
    }
}
