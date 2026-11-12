//! The desktop portals on a Wayland session, where an app cannot read other
//! windows or inject input on its own: Screenshot for captures, and
//! RemoteDesktop (with a ScreenCast stream, which absolute pointer
//! positions need) for the pointer and keyboard. Both ask the person in the
//! compositor's own dialog; RemoteDesktop's answer is kept for this run
//! (`persist_mode` 1) so it is asked once. Portal input is always
//! foreground input: it moves the real pointer and goes to whatever window
//! is at that point, so callers only use it on the active window.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const DESKTOP: &str = "org.freedesktop.portal.Desktop";
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
const SCREENSHOT: &str = "org.freedesktop.portal.Screenshot";
const REMOTE: &str = "org.freedesktop.portal.RemoteDesktop";
const SCREENCAST: &str = "org.freedesktop.portal.ScreenCast";
const QUICK: Duration = Duration::from_secs(15);
/// The person reads and answers the compositor's dialog.
const ASKED: Duration = Duration::from_secs(120);

/// Linux input event codes for the pointer buttons.
pub(super) const BTN_LEFT: i32 = 0x110;
pub(super) const BTN_RIGHT: i32 = 0x111;

fn token() -> String {
    format!("dcc{}", rand::random::<u32>())
}

/// Calls a portal method that answers with a Request's `Response` signal
/// and waits for it. `call` gets the handle token to put in its options.
fn request(
    connection: &Connection,
    wait: Duration,
    call: impl FnOnce(&str) -> zbus::Result<zbus::message::Message>,
) -> Result<HashMap<String, OwnedValue>, String> {
    let sender = connection
        .unique_name()
        .ok_or("no D-Bus name")?
        .trim_start_matches(':')
        .replace('.', "_");
    let token = token();
    let path = format!("{DESKTOP_PATH}/request/{sender}/{token}");
    let proxy = zbus::blocking::Proxy::new_owned(
        connection.clone(),
        DESKTOP.to_string(),
        path,
        "org.freedesktop.portal.Request".to_string(),
    )
    .map_err(|error| error.to_string())?;
    let mut responses = proxy
        .receive_signal("Response")
        .map_err(|error| error.to_string())?;
    call(&token).map_err(|error| format!("the desktop portal refused: {error}"))?;
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(responses.next());
    });
    let message = received
        .recv_timeout(wait)
        .map_err(|_| "the desktop portal did not answer in time".to_string())?
        .ok_or("the desktop portal closed the request")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = message
        .body()
        .deserialize()
        .map_err(|error| error.to_string())?;
    match code {
        0 => Ok(results),
        1 => Err("the person declined the desktop portal's request".into()),
        _ => Err("the desktop portal could not complete the request".into()),
    }
}

fn text(value: &OwnedValue) -> Option<String> {
    match &**value {
        Value::Str(text) => Some(text.to_string()),
        Value::ObjectPath(path) => Some(path.to_string()),
        _ => None,
    }
}

/// A whole-screen capture as PNG bytes (the portal writes a file; it is
/// read and removed).
pub(super) fn screenshot(interactive: bool) -> Result<Vec<u8>, String> {
    let connection = Connection::session().map_err(|error| error.to_string())?;
    let results = request(
        &connection,
        if interactive { ASKED } else { QUICK },
        |token| {
            let options: HashMap<&str, Value> = HashMap::from([
                ("handle_token", Value::from(token)),
                ("interactive", Value::from(interactive)),
            ]);
            connection.call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(SCREENSHOT),
                "Screenshot",
                &("", options),
            )
        },
    )?;
    let uri = results
        .get("uri")
        .and_then(text)
        .ok_or("the screenshot portal returned no image")?;
    let path = uri
        .strip_prefix("file://")
        .map(|path| percent_decode(path))
        .ok_or("the screenshot portal returned an unreadable address")?;
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(&path);
    Ok(bytes)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A started RemoteDesktop session with the monitor stream it controls.
struct Session {
    connection: Connection,
    handle: OwnedObjectPath,
    /// Node id, and the stream's position and size in desktop coordinates.
    stream: Option<(u32, (i32, i32), (i32, i32))>,
}

/// RemoteDesktop input, started on first use.
pub(super) struct RemoteInput {
    session: Mutex<Option<Session>>,
    restore_token: Mutex<Option<String>>,
}

fn streams(value: &OwnedValue) -> Vec<(u32, (i32, i32), (i32, i32))> {
    let Value::Array(array) = &**value else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|item| {
            let Value::Structure(structure) = item else {
                return None;
            };
            let fields = structure.fields();
            let Value::U32(node) = fields.first()? else {
                return None;
            };
            let Value::Dict(properties) = fields.get(1)? else {
                return None;
            };
            let pair = |key: &str| -> Option<(i32, i32)> {
                let value: Value = properties.get::<&str, Value>(&key).ok()??;
                let Value::Structure(pair) = value else {
                    return None;
                };
                match pair.fields() {
                    [Value::I32(a), Value::I32(b)] => Some((*a, *b)),
                    _ => None,
                }
            };
            Some((*node, pair("position").unwrap_or((0, 0)), pair("size")?))
        })
        .collect()
}

impl RemoteInput {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            restore_token: Mutex::new(None),
        }
    }

    fn start(&self) -> Result<Session, String> {
        let connection = Connection::session().map_err(|error| error.to_string())?;
        let session_token = token();
        let created = request(&connection, QUICK, |token| {
            let options: HashMap<&str, Value> = HashMap::from([
                ("handle_token", Value::from(token)),
                ("session_handle_token", Value::from(session_token.as_str())),
            ]);
            connection.call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(REMOTE),
                "CreateSession",
                &(options,),
            )
        })?;
        let handle = created
            .get("session_handle")
            .and_then(text)
            .ok_or("the RemoteDesktop portal created no session")?;
        let handle = OwnedObjectPath::try_from(handle).map_err(|error| error.to_string())?;
        let session = ObjectPath::from(&handle);
        let restore = self
            .restore_token
            .lock()
            .ok()
            .and_then(|token| token.clone());
        request(&connection, QUICK, |token| {
            let mut options: HashMap<&str, Value> = HashMap::from([
                ("handle_token", Value::from(token)),
                // Keyboard and pointer.
                ("types", Value::from(3u32)),
                ("persist_mode", Value::from(1u32)),
            ]);
            if let Some(restore) = restore.as_deref() {
                options.insert("restore_token", Value::from(restore));
            }
            connection.call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(REMOTE),
                "SelectDevices",
                &(&session, options),
            )
        })?;
        // A monitor stream: absolute pointer positions are relative to it.
        let _ = request(&connection, QUICK, |token| {
            let options: HashMap<&str, Value> = HashMap::from([
                ("handle_token", Value::from(token)),
                ("types", Value::from(1u32)),
                ("multiple", Value::from(false)),
            ]);
            connection.call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(SCREENCAST),
                "SelectSources",
                &(&session, options),
            )
        });
        let started = request(&connection, ASKED, |token| {
            let options: HashMap<&str, Value> =
                HashMap::from([("handle_token", Value::from(token))]);
            connection.call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(REMOTE),
                "Start",
                &(&session, "", options),
            )
        })?;
        if let Some(token) = started.get("restore_token").and_then(text) {
            if let Ok(mut slot) = self.restore_token.lock() {
                *slot = Some(token);
            }
        }
        let stream = started
            .get("streams")
            .map(streams)
            .and_then(|streams| streams.into_iter().next());
        Ok(Session {
            connection,
            handle,
            stream,
        })
    }

    /// Runs `send` on the session, starting it (and asking the person)
    /// first if needed; a session the compositor ended is started again.
    fn with_session<T>(&self, send: impl Fn(&Session) -> zbus::Result<T>) -> Result<T, String> {
        let mut slot = self.session.lock().map_err(|_| "portal lock poisoned")?;
        if slot.is_none() {
            *slot = Some(self.start()?);
        }
        match send(slot.as_ref().expect("started")) {
            Ok(value) => Ok(value),
            Err(_) => {
                *slot = Some(self.start()?);
                send(slot.as_ref().expect("started")).map_err(|error| error.to_string())
            }
        }
    }

    fn notify(
        &self,
        session: &Session,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> zbus::Result<()> {
        session
            .connection
            .call_method(Some(DESKTOP), DESKTOP_PATH, Some(REMOTE), method, body)
            .map(|_| ())
    }

    fn move_to(&self, session: &Session, x: f64, y: f64) -> zbus::Result<()> {
        let Some((node, (sx, sy), (width, height))) = session.stream else {
            return Err(zbus::Error::Failure(
                "no screen stream to place the pointer on".into(),
            ));
        };
        let (lx, ly) = (x - f64::from(sx), y - f64::from(sy));
        if lx < 0.0 || ly < 0.0 || lx >= f64::from(width) || ly >= f64::from(height) {
            return Err(zbus::Error::Failure(
                "that point is outside the shared screen".into(),
            ));
        }
        let options: HashMap<&str, Value> = HashMap::new();
        self.notify(
            session,
            "NotifyPointerMotionAbsolute",
            &(&ObjectPath::from(&session.handle), options, node, lx, ly),
        )
    }

    fn button(&self, session: &Session, button: i32, pressed: bool) -> zbus::Result<()> {
        let options: HashMap<&str, Value> = HashMap::new();
        self.notify(
            session,
            "NotifyPointerButton",
            &(
                &ObjectPath::from(&session.handle),
                options,
                button,
                u32::from(pressed),
            ),
        )
    }

    /// The real pointer: to a desktop point, then the clicks.
    pub fn click(&self, x: f64, y: f64, button: i32, count: u8) -> Result<(), String> {
        self.with_session(|session| {
            self.move_to(session, x, y)?;
            std::thread::sleep(Duration::from_millis(40));
            for click in 0..count {
                self.button(session, button, true)?;
                self.button(session, button, false)?;
                if click + 1 < count {
                    std::thread::sleep(Duration::from_millis(60));
                }
            }
            Ok(())
        })
    }

    pub fn scroll(&self, x: f64, y: f64, delta_x: i32, delta_y: i32) -> Result<(), String> {
        self.with_session(|session| {
            self.move_to(session, x, y)?;
            let options: HashMap<&str, Value> = HashMap::from([("finish", Value::from(true))]);
            self.notify(
                session,
                "NotifyPointerAxis",
                &(
                    &ObjectPath::from(&session.handle),
                    options,
                    f64::from(delta_x),
                    f64::from(delta_y),
                ),
            )
        })
    }

    pub fn drag(&self, from: (f64, f64), to: (f64, f64)) -> Result<(), String> {
        self.with_session(|session| {
            self.move_to(session, from.0, from.1)?;
            self.button(session, BTN_LEFT, true)?;
            for (x, y) in super::x11::drag_steps(from, to) {
                self.move_to(session, x, y)?;
                std::thread::sleep(Duration::from_millis(12));
            }
            self.button(session, BTN_LEFT, false)
        })
    }

    /// Keysyms, each pressed and released with `held` down around them.
    pub fn keys(&self, keysyms: &[u32], held: &[u32]) -> Result<(), String> {
        self.with_session(|session| {
            let handle = ObjectPath::from(&session.handle);
            let key = |keysym: u32, pressed: bool| {
                let options: HashMap<&str, Value> = HashMap::new();
                self.notify(
                    session,
                    "NotifyKeyboardKeysym",
                    &(&handle, options, keysym as i32, u32::from(pressed)),
                )
            };
            for modifier in held {
                key(*modifier, true)?;
            }
            for keysym in keysyms {
                key(*keysym, true)?;
                key(*keysym, false)?;
            }
            for modifier in held.iter().rev() {
                key(*modifier, false)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_file_addresses_are_decoded() {
        assert_eq!(
            percent_decode("/home/w/Pictures/Screenshot%20from%202026.png"),
            "/home/w/Pictures/Screenshot from 2026.png"
        );
        assert_eq!(percent_decode("/tmp/a%2"), "/tmp/a%2");
    }
}
