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

/// `simctl list devices available -j`: the iPhones and iPads, the booted
/// ones first, then the newest iOS, then by name.
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
                    udid: entry.udid,
                    name: entry.name,
                    runtime: runtime.clone(),
                    booted: entry.state == "Booted",
                })
        })
        .collect();
    simulators.sort_by(|a, b| {
        b.booted
            .cmp(&a.booted)
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

/// Boots `udid` without the Simulator window and waits until it is up.
pub fn boot(udid: &str) -> Result<(), String> {
    if let Err(error) = simctl(&["boot", udid]) {
        // Already booted is fine.
        if !error.contains("current state: Booted") {
            return Err(error);
        }
    }
    simctl(&["bootstatus", udid, "-b"]).map(|_| ())
}

pub fn shutdown(udid: &str) -> Result<(), String> {
    simctl(&["shutdown", udid]).map(|_| ())
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
    fn iphones_come_booted_first_then_newest_ios() {
        let json = r#"{"devices":{
            "com.apple.CoreSimulator.SimRuntime.iOS-18-3":[
                {"udid":"A","name":"iPhone 16","state":"Shutdown","isAvailable":true}],
            "com.apple.CoreSimulator.SimRuntime.iOS-26-1":[
                {"udid":"B","name":"iPhone 17 Pro","state":"Shutdown","isAvailable":true},
                {"udid":"C","name":"iPhone Air","state":"Shutdown","isAvailable":false}],
            "com.apple.CoreSimulator.SimRuntime.watchOS-11-0":[
                {"udid":"D","name":"Apple Watch","state":"Booted","isAvailable":true}],
            "com.apple.CoreSimulator.SimRuntime.iOS-17-5":[
                {"udid":"E","name":"iPhone 15","state":"Booted","isAvailable":true}]
        }}"#;
        let list = parse_list(json);
        let order: Vec<(&str, &str, bool)> = list
            .iter()
            .map(|sim| (sim.udid.as_str(), sim.runtime.as_str(), sim.booted))
            .collect();
        assert_eq!(
            order,
            vec![
                ("E", "iOS 17.5", true),
                ("B", "iOS 26.1", false),
                ("A", "iOS 18.3", false),
            ]
        );
        assert!(parse_list("not json").is_empty());
    }
}
