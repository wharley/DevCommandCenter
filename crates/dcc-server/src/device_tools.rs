//! The agents' iOS Simulator (`dcc_device_*`, macOS): list the simulators,
//! open one (the native app boots it and shows it in its Device panel, so
//! the person watches), read it (a screenshot, its accessibility elements)
//! and use it (tap, swipe, type, buttons, launch an app, open a link,
//! install a build). Screenshots and the app operations run `xcrun simctl`
//! here; touches, keys and the accessibility tree go through the panel's
//! own helper, which the app hands over through [`DeviceHost`]. Coordinates
//! are the screenshot's pixels.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub type DeviceFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;

/// An opened simulator.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDevice {
    pub udid: String,
    pub name: String,
    pub runtime: String,
    /// The screen in pixels (the screenshot's size).
    pub width: u32,
    pub height: u32,
}

/// What the native app does for the agents' simulator tools.
pub trait DeviceHost: Send + Sync {
    /// Boots `udid` (or the panel's, or the most recently used) and shows it
    /// in the Device panel; resolves once it streams.
    fn open(&self, udid: Option<String>) -> DeviceFuture<OpenDevice>;
    /// A helper command (`touch`, `multiTouch`, `button`, `key`,
    /// `describe`) on the simulator the panel shows.
    fn command(&self, udid: String, command: Value) -> DeviceFuture<Value>;
    /// `text` typed on the simulator's keyboard.
    fn type_text(&self, udid: String, text: String) -> DeviceFuture<()>;
}

static HOST: OnceLock<Arc<dyn DeviceHost>> = OnceLock::new();

/// The native app's simulator, once it registers it (macOS).
pub fn register_device_host(host: Arc<dyn DeviceHost>) {
    let _ = HOST.set(host);
}

fn host() -> Result<&'static Arc<dyn DeviceHost>, String> {
    HOST.get()
        .ok_or_else(|| "the iOS Simulator is only available in the DCC app on a Mac".into())
}

/// The simulator each provider session opened, and its screen size.
fn opened() -> &'static Mutex<HashMap<String, OpenDevice>> {
    static OPENED: OnceLock<Mutex<HashMap<String, OpenDevice>>> = OnceLock::new();
    OPENED.get_or_init(Mutex::default)
}

fn session_device(session_id: &str) -> Result<OpenDevice, String> {
    opened()
        .lock()
        .ok()
        .and_then(|opened| opened.get(session_id).cloned())
        .ok_or_else(|| "no simulator is open for this session; call dcc_device_open first".into())
}

const NAMES: [&str; 11] = [
    "dcc_device_list",
    "dcc_device_open",
    "dcc_device_screenshot",
    "dcc_device_describe",
    "dcc_device_tap",
    "dcc_device_swipe",
    "dcc_device_type",
    "dcc_device_button",
    "dcc_device_launch",
    "dcc_device_open_url",
    "dcc_device_install",
];

pub fn is_device_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

/// How long a call may hold the request: booting takes a while.
pub fn timeout(name: &str) -> Duration {
    match name {
        "dcc_device_open" => Duration::from_secs(120),
        "dcc_device_install" => Duration::from_secs(180),
        _ => Duration::from_secs(30),
    }
}

/// The tools, listed only where the app offers a simulator.
pub fn tools() -> Vec<Value> {
    if HOST.get().is_none() {
        return Vec::new();
    }
    definitions()
}

/// Every tool's definition.
pub fn definitions() -> Vec<Value> {
    let none = json!({"type":"object","additionalProperties":false,"properties":{}});
    let read = json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false});
    let act = json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false});
    let point = |what: &str| json!({"type":"number","minimum":0,"description":what});
    vec![
        json!({"name":"dcc_device_list","description":"List the iOS Simulators on this Mac (name, iOS version, udid, whether it runs).","inputSchema":none,"annotations":read}),
        json!({"name":"dcc_device_open","description":"Open an iOS Simulator for this session: DCC boots it if needed and shows it to the person in its Device panel. Without udid it opens the one already shown or the most recently used. Returns the screen size in pixels; the other dcc_device_* tools act on it.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"udid":{"type":"string","pattern":"^[0-9A-Fa-f-]{36}$"}}},"annotations":act}),
        json!({"name":"dcc_device_screenshot","description":"A PNG of the open simulator's screen. Coordinates for dcc_device_tap and dcc_device_swipe are this image's pixels.","inputSchema":none,"annotations":read}),
        json!({"name":"dcc_device_describe","description":"The open simulator's accessibility elements (role, label, value) with their centers in screenshot pixels: tap an element by its label or its center. Right after an app opens, call it again if it comes back empty.","inputSchema":none,"annotations":read}),
        json!({"name":"dcc_device_tap","description":"Tap the open simulator at x,y (screenshot pixels), or on the first accessibility element whose label matches.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"x":point("pixels from the left"),"y":point("pixels from the top"),"label":{"type":"string","minLength":1,"maxLength":200}}},"annotations":act}),
        json!({"name":"dcc_device_swipe","description":"Drag one finger on the open simulator from (fromX,fromY) to (toX,toY), in screenshot pixels: scroll, swipe between pages, pull to refresh.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"fromX":point("start"),"fromY":point("start"),"toX":point("end"),"toY":point("end"),"durationMs":{"type":"integer","minimum":50,"maximum":3000}},"required":["fromX","fromY","toX","toY"]},"annotations":act}),
        json!({"name":"dcc_device_type","description":"Type text on the open simulator's keyboard into the focused field (US layout: letters, digits, punctuation, spaces, newlines).","inputSchema":{"type":"object","additionalProperties":false,"properties":{"text":{"type":"string","minLength":1,"maxLength":2000}},"required":["text"]},"annotations":act}),
        json!({"name":"dcc_device_button","description":"Press a hardware button on the open simulator.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string","enum":["home","lock","app_switcher","siri","side_button"]}},"required":["name"]},"annotations":act}),
        json!({"name":"dcc_device_launch","description":"Launch an installed app on the open simulator by its bundle id.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"bundleId":{"type":"string","pattern":"^[A-Za-z0-9.-]{1,255}$"}},"required":["bundleId"]},"annotations":act}),
        json!({"name":"dcc_device_open_url","description":"Open a URL on the open simulator: a web page in Safari, an app's deep link, or an Expo development URL (exp://).","inputSchema":{"type":"object","additionalProperties":false,"properties":{"url":{"type":"string","minLength":3,"maxLength":2048}},"required":["url"]},"annotations":act}),
        json!({"name":"dcc_device_install","description":"Install a simulator build (.app) on the open simulator, e.g. Xcode's or `expo run:ios` output.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"appPath":{"type":"string","minLength":5,"maxLength":4096}},"required":["appPath"]},"annotations":act}),
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenArgs {
    #[serde(default)]
    udid: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TapArgs {
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SwipeArgs {
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TextArgs {
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ButtonArgs {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LaunchArgs {
    bundle_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UrlArgs {
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallArgs {
    app_path: String,
}

fn parse<T: for<'de> Deserialize<'de>>(arguments: &Option<Value>) -> Option<T> {
    serde_json::from_value(arguments.clone().unwrap_or_else(|| json!({}))).ok()
}

/// The arguments fit the tool's schema (before anything runs).
pub fn call_is_well_formed(name: &str, arguments: &Option<Value>) -> bool {
    let finite = |value: f64| value.is_finite() && value >= 0.0;
    match name {
        "dcc_device_list" | "dcc_device_screenshot" | "dcc_device_describe" => arguments
            .as_ref()
            .is_none_or(|value| value.as_object().is_some_and(|o| o.is_empty())),
        "dcc_device_open" => parse::<OpenArgs>(arguments).is_some_and(|args| {
            args.udid.is_none_or(|udid| {
                udid.len() == 36 && udid.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
            })
        }),
        "dcc_device_tap" => parse::<TapArgs>(arguments).is_some_and(|args| {
            match (args.x, args.y, args.label.as_deref()) {
                (Some(x), Some(y), None) => finite(x) && finite(y),
                (None, None, Some(label)) => !label.trim().is_empty() && label.len() <= 200,
                _ => false,
            }
        }),
        "dcc_device_swipe" => parse::<SwipeArgs>(arguments).is_some_and(|args| {
            [args.from_x, args.from_y, args.to_x, args.to_y]
                .into_iter()
                .all(finite)
                && args.duration_ms.is_none_or(|ms| (50..=3000).contains(&ms))
        }),
        "dcc_device_type" => parse::<TextArgs>(arguments)
            .is_some_and(|args| !args.text.is_empty() && args.text.chars().count() <= 2000),
        "dcc_device_button" => parse::<ButtonArgs>(arguments).is_some_and(|args| {
            matches!(
                args.name.as_str(),
                "home" | "lock" | "app_switcher" | "siri" | "side_button"
            )
        }),
        "dcc_device_launch" => parse::<LaunchArgs>(arguments).is_some_and(|args| {
            !args.bundle_id.is_empty()
                && args.bundle_id.len() <= 255
                && args
                    .bundle_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        }),
        "dcc_device_open_url" => parse::<UrlArgs>(arguments).is_some_and(|args| {
            (3..=2048).contains(&args.url.len())
                && args.url.contains(':')
                && !args.url.starts_with('-')
        }),
        "dcc_device_install" => parse::<InstallArgs>(arguments).is_some_and(|args| {
            args.app_path.starts_with('/') && args.app_path.trim_end_matches('/').ends_with(".app")
        }),
        _ => false,
    }
}

fn text(message: impl Into<String>) -> Value {
    json!({"content":[{"type":"text","text":message.into()}]})
}

fn error(message: impl Into<String>) -> Value {
    json!({"content":[{"type":"text","text":message.into()}],"isError":true})
}

fn simctl(args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("xcrun")
        .arg("simctl")
        .args(args)
        .output()
        .map_err(|error| format!("xcrun: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
}

/// The simulators, from `simctl` (iOS only, the running ones first).
fn list() -> Result<Value, String> {
    let raw = simctl(&["list", "devices", "available", "-j"])?;
    let listing: Value = serde_json::from_slice(&raw).map_err(|error| error.to_string())?;
    let mut devices = Vec::new();
    for (runtime, entries) in listing["devices"].as_object().into_iter().flatten() {
        let Some(version) = runtime
            .rsplit('.')
            .next()
            .and_then(|key| key.strip_prefix("iOS-"))
        else {
            continue;
        };
        for entry in entries.as_array().into_iter().flatten() {
            devices.push(json!({
                "udid": entry["udid"],
                "name": entry["name"],
                "ios": version.replace('-', "."),
                "booted": entry["state"] == "Booted",
            }));
        }
    }
    devices.sort_by_key(|device| device["booted"] != true);
    Ok(json!({"devices": devices}))
}

/// One accessibility element for the agent: what it is and where to tap.
#[derive(Debug, PartialEq, Serialize)]
pub struct Element {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Center, in screenshot pixels.
    pub x: u32,
    pub y: u32,
}

/// The helper's tree (frames in points) as a flat list in pixels; the root
/// (the application) gives the points-to-pixels scale.
pub fn elements(tree: &Value, width_px: u32) -> Vec<Element> {
    let root_width = tree
        .as_array()
        .and_then(|roots| roots.first())
        .and_then(|root| root["frame"]["width"].as_f64())
        .filter(|width| *width > 0.0);
    let scale = root_width.map_or(1.0, |points| width_px as f64 / points);
    let mut out = Vec::new();
    let mut stack: Vec<&Value> = tree.as_array().into_iter().flatten().rev().collect();
    while let Some(node) = stack.pop() {
        let words = |key: &str| {
            node[key]
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        let (label, value) = (words("AXLabel"), words("AXValue"));
        let frame = &node["frame"];
        let rect = (
            frame["x"].as_f64(),
            frame["y"].as_f64(),
            frame["width"].as_f64(),
            frame["height"].as_f64(),
        );
        if let (true, (Some(x), Some(y), Some(w), Some(h))) =
            (label.is_some() || value.is_some(), rect)
        {
            if w <= 0.0 || h <= 0.0 {
                if let Some(children) = node["children"].as_array() {
                    stack.extend(children.iter().rev());
                }
                continue;
            }
            out.push(Element {
                role: words("role_description").unwrap_or_else(|| "element".into()),
                label,
                value,
                x: ((x + w / 2.0) * scale).round() as u32,
                y: ((y + h / 2.0) * scale).round() as u32,
            });
            if out.len() >= 300 {
                break;
            }
        }
        if let Some(children) = node["children"].as_array() {
            stack.extend(children.iter().rev());
        }
    }
    out
}

/// A point in screenshot pixels as 0..1 of the screen.
fn unit(device: &OpenDevice, x: f64, y: f64) -> (f64, f64) {
    (
        (x / device.width.max(1) as f64).clamp(0.0, 1.0),
        (y / device.height.max(1) as f64).clamp(0.0, 1.0),
    )
}

async fn describe(host: &dyn DeviceHost, device: &OpenDevice) -> Result<Vec<Element>, String> {
    // Right after an app opens the tree may not be there yet.
    let mut last = String::new();
    for attempt in 0..3 {
        match host
            .command(device.udid.clone(), json!({"op": "describe"}))
            .await
        {
            Ok(tree) => return Ok(elements(&tree, device.width)),
            Err(error) => last = error,
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_millis(600)).await;
        }
    }
    Err(last)
}

async fn touch(
    host: &dyn DeviceHost,
    device: &OpenDevice,
    phase: &str,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let (x, y) = unit(device, x, y);
    host.command(
        device.udid.clone(),
        json!({"op":"touch","phase":phase,"x":x,"y":y}),
    )
    .await
    .map(|_| ())
}

/// Runs a well-formed `dcc_device_*` call for `session_id`; the MCP result.
pub async fn dispatch(session_id: &str, name: &str, arguments: Option<Value>) -> Value {
    let result = match host() {
        Ok(host) => run(host.as_ref(), session_id, name, arguments).await,
        Err(message) => Err(message),
    };
    result.unwrap_or_else(error)
}

async fn run(
    host: &dyn DeviceHost,
    session_id: &str,
    name: &str,
    arguments: Option<Value>,
) -> Result<Value, String> {
    if name == "dcc_device_list" {
        let devices = blocking(list).await?;
        return Ok(
            json!({"content":[{"type":"text","text":devices.to_string()}],"structuredContent":devices}),
        );
    }
    if name == "dcc_device_open" {
        let args = parse::<OpenArgs>(&arguments).ok_or("invalid arguments")?;
        let device = host.open(args.udid).await?;
        if let Ok(mut opened) = opened().lock() {
            opened.insert(session_id.to_string(), device.clone());
        }
        let summary = format!(
            "{} ({}) is open and shown to the person. Its screen is {}x{} pixels; use dcc_device_screenshot to see it.",
            device.name, device.runtime, device.width, device.height
        );
        return Ok(json!({"content":[{"type":"text","text":summary}],"structuredContent":device}));
    }
    let device = session_device(session_id)?;
    let udid = device.udid.clone();
    match name {
        "dcc_device_screenshot" => {
            let png = blocking(move || {
                let path = std::env::temp_dir().join(format!("dcc-device-{udid}.png"));
                let path_text = path.to_string_lossy().into_owned();
                simctl(&["io", &udid, "screenshot", "--type=png", &path_text])?;
                let png = std::fs::read(&path).map_err(|error| error.to_string());
                let _ = std::fs::remove_file(&path);
                png
            })
            .await?;
            Ok(json!({"content":[
                {"type":"text","text":format!("{} screen, {}x{} pixels.", device.name, device.width, device.height)},
                {"type":"image","data":base64::engine::general_purpose::STANDARD.encode(png),"mimeType":"image/png"}
            ]}))
        }
        "dcc_device_describe" => {
            let elements = describe(host, &device).await?;
            let value =
                json!({"width": device.width, "height": device.height, "elements": elements});
            Ok(
                json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value}),
            )
        }
        "dcc_device_tap" => {
            let args = parse::<TapArgs>(&arguments).ok_or("invalid arguments")?;
            let (x, y, what) = match (args.x, args.y, args.label) {
                (Some(x), Some(y), _) => (x, y, format!("at {x:.0},{y:.0}")),
                (_, _, Some(label)) => {
                    let wanted = label.to_lowercase();
                    let elements = describe(host, &device).await?;
                    let found = elements
                        .iter()
                        .find(|element| {
                            element
                                .label
                                .as_deref()
                                .is_some_and(|l| l.to_lowercase() == wanted)
                        })
                        .or_else(|| {
                            elements.iter().find(|element| {
                                element
                                    .label
                                    .as_deref()
                                    .is_some_and(|l| l.to_lowercase().contains(&wanted))
                            })
                        })
                        .ok_or_else(|| {
                            format!("no element labelled \"{label}\"; call dcc_device_describe")
                        })?;
                    (
                        found.x as f64,
                        found.y as f64,
                        format!("\"{label}\" at {},{}", found.x, found.y),
                    )
                }
                _ => return Err("tap needs x and y, or a label".into()),
            };
            touch(host, &device, "begin", x, y).await?;
            tokio::time::sleep(Duration::from_millis(60)).await;
            touch(host, &device, "end", x, y).await?;
            Ok(text(format!("Tapped {what}.")))
        }
        "dcc_device_swipe" => {
            let args = parse::<SwipeArgs>(&arguments).ok_or("invalid arguments")?;
            let duration = Duration::from_millis(args.duration_ms.unwrap_or(300));
            let steps = 12u32;
            touch(host, &device, "begin", args.from_x, args.from_y).await?;
            for step in 1..=steps {
                let t = step as f64 / steps as f64;
                tokio::time::sleep(duration / steps).await;
                let x = args.from_x + (args.to_x - args.from_x) * t;
                let y = args.from_y + (args.to_y - args.from_y) * t;
                touch(host, &device, "move", x, y).await?;
            }
            touch(host, &device, "end", args.to_x, args.to_y).await?;
            Ok(text("Swiped."))
        }
        "dcc_device_type" => {
            let args = parse::<TextArgs>(&arguments).ok_or("invalid arguments")?;
            host.type_text(udid, args.text).await?;
            Ok(text("Typed."))
        }
        "dcc_device_button" => {
            let args = parse::<ButtonArgs>(&arguments).ok_or("invalid arguments")?;
            host.command(udid, json!({"op":"button","name":args.name}))
                .await?;
            Ok(text(format!("Pressed {}.", args.name)))
        }
        "dcc_device_launch" => {
            let args = parse::<LaunchArgs>(&arguments).ok_or("invalid arguments")?;
            let bundle = args.bundle_id.clone();
            blocking(move || simctl(&["launch", &udid, &bundle])).await?;
            Ok(text(format!("Launched {}.", args.bundle_id)))
        }
        "dcc_device_open_url" => {
            let args = parse::<UrlArgs>(&arguments).ok_or("invalid arguments")?;
            let url = args.url.clone();
            blocking(move || simctl(&["openurl", &udid, &url])).await?;
            Ok(text(format!("Opened {}.", args.url)))
        }
        "dcc_device_install" => {
            let args = parse::<InstallArgs>(&arguments).ok_or("invalid arguments")?;
            let path = args.app_path.clone();
            blocking(move || simctl(&["install", &udid, &path])).await?;
            Ok(text(format!("Installed {}.", args.app_path)))
        }
        _ => Err("unknown device tool".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_checked_before_anything_runs() {
        let args = |value: Value| Some(value);
        assert!(call_is_well_formed("dcc_device_list", &None));
        assert!(!call_is_well_formed(
            "dcc_device_list",
            &args(json!({"x": 1}))
        ));
        assert!(call_is_well_formed("dcc_device_open", &args(json!({}))));
        assert!(call_is_well_formed(
            "dcc_device_open",
            &args(json!({"udid": "47B9DDB6-8CF6-4866-9A60-B4D1F6EC0B4D"}))
        ));
        assert!(!call_is_well_formed(
            "dcc_device_open",
            &args(json!({"udid": "; rm -rf"}))
        ));
        assert!(call_is_well_formed(
            "dcc_device_tap",
            &args(json!({"x": 10, "y": 20}))
        ));
        assert!(call_is_well_formed(
            "dcc_device_tap",
            &args(json!({"label": "Ajustes"}))
        ));
        assert!(!call_is_well_formed(
            "dcc_device_tap",
            &args(json!({"x": 10}))
        ));
        assert!(!call_is_well_formed(
            "dcc_device_tap",
            &args(json!({"x": -1, "y": 2}))
        ));
        assert!(!call_is_well_formed(
            "dcc_device_button",
            &args(json!({"name": "reboot"}))
        ));
        assert!(!call_is_well_formed(
            "dcc_device_launch",
            &args(json!({"bundleId": "a b"}))
        ));
        assert!(!call_is_well_formed(
            "dcc_device_open_url",
            &args(json!({"url": "--set"}))
        ));
        assert!(call_is_well_formed(
            "dcc_device_install",
            &args(json!({"appPath": "/tmp/build/Feedr.app"}))
        ));
        assert!(!call_is_well_formed(
            "dcc_device_install",
            &args(json!({"appPath": "Feedr.app"}))
        ));
        assert!(!call_is_well_formed(
            "dcc_device_swipe",
            &args(json!({"fromX": 1, "fromY": 2, "toX": 3}))
        ));
        assert!(!call_is_well_formed("dcc_device_shell", &None));
    }

    #[test]
    fn the_tree_flattens_to_taps_in_pixels() {
        let tree = json!([{
            "role_description": "application",
            "AXLabel": "Ajustes",
            "frame": {"x": 0, "y": 0, "width": 402, "height": 874},
            "children": [
                {"role_description": "heading", "AXLabel": "Ajustes", "frame": {"x": 16, "y": 104, "width": 120, "height": 41}, "children": []},
                {"role_description": "button", "AXLabel": "", "frame": {"x": 0, "y": 0, "width": 10, "height": 10}},
                {"role_description": "search field", "AXValue": "Buscar", "frame": {"x": 16, "y": 160, "width": 370, "height": 36}}
            ]
        }]);
        let found = elements(&tree, 1206);
        assert_eq!(
            found.len(),
            3,
            "the unlabelled button is left out: {found:?}"
        );
        assert_eq!(found[1].label.as_deref(), Some("Ajustes"));
        assert_eq!((found[1].x, found[1].y), (228, 374), "points × 3");
        assert_eq!(found[2].value.as_deref(), Some("Buscar"));
    }

    #[test]
    fn every_definition_is_closed_and_listed_only_with_the_app() {
        assert!(HOST.get().is_none());
        assert!(tools().is_empty(), "no simulator outside the app");
        let names: Vec<_> = definitions()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, NAMES);
        for tool in definitions() {
            assert_eq!(tool["inputSchema"]["additionalProperties"], false, "{tool}");
        }
    }

    /// The app's side, faked: what the tools ask it for.
    #[derive(Default)]
    struct Fake {
        commands: Mutex<Vec<Value>>,
        typed: Mutex<Vec<String>>,
    }

    impl DeviceHost for Fake {
        fn open(&self, udid: Option<String>) -> DeviceFuture<OpenDevice> {
            Box::pin(async move {
                Ok(OpenDevice {
                    udid: udid.unwrap_or_else(|| "47B9DDB6-8CF6-4866-9A60-B4D1F6EC0B4D".into()),
                    name: "iPhone 16 Pro".into(),
                    runtime: "iOS 18.1".into(),
                    width: 1206,
                    height: 2622,
                })
            })
        }

        fn command(&self, _udid: String, command: Value) -> DeviceFuture<Value> {
            let tree = command["op"] == "describe";
            self.commands.lock().unwrap().push(command);
            Box::pin(async move {
                Ok(if tree {
                    json!([{"role_description":"application","frame":{"x":0,"y":0,"width":402,"height":874},
                        "children":[{"role_description":"button","AXLabel":"Ajustes","frame":{"x":300,"y":400,"width":60,"height":60}}]}])
                } else {
                    Value::Null
                })
            })
        }

        fn type_text(&self, _udid: String, text: String) -> DeviceFuture<()> {
            self.typed.lock().unwrap().push(text);
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn an_agent_opens_then_taps_by_label_and_types() {
        let fake = Fake::default();
        let session = "session-device-test";
        let before = run(
            &fake,
            session,
            "dcc_device_tap",
            Some(json!({"x": 1, "y": 1})),
        )
        .await;
        assert!(
            before.unwrap_err().contains("dcc_device_open"),
            "open comes first"
        );

        let opened = run(&fake, session, "dcc_device_open", None).await.unwrap();
        assert_eq!(opened["structuredContent"]["width"], 1206);

        let tapped = run(
            &fake,
            session,
            "dcc_device_tap",
            Some(json!({"label": "ajustes"})),
        )
        .await
        .unwrap();
        assert!(
            tapped["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("990,1290"),
            "{tapped}"
        );
        let commands = fake.commands.lock().unwrap().clone();
        let touches: Vec<_> = commands.iter().filter(|c| c["op"] == "touch").collect();
        assert_eq!(touches.len(), 2, "down and up");
        assert_eq!(touches[0]["phase"], "begin");
        let x = touches[0]["x"].as_f64().unwrap();
        assert!((x - 990.0 / 1206.0).abs() < 1e-6, "0..1 of the screen");

        run(
            &fake,
            session,
            "dcc_device_type",
            Some(json!({"text": "Olá"})),
        )
        .await
        .unwrap();
        assert_eq!(fake.typed.lock().unwrap().as_slice(), ["Olá"]);
        let missing = run(
            &fake,
            session,
            "dcc_device_tap",
            Some(json!({"label": "Nada"})),
        )
        .await;
        assert!(missing.unwrap_err().contains("dcc_device_describe"));
    }
}
