//! X11 (and XWayland) windows: the inventory (EWMH), captures (GetImage),
//! input in the background (XSendEvent to the window: the pointer does not
//! move and the window stays where it is) and in the foreground
//! (`_NET_ACTIVE_WINDOW`, then XTEST: the real pointer and keyboard).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ButtonPressEvent, ClientMessageEvent, ConnectionExt as _, EventMask, ImageFormat,
    KeyButMask, KeyPressEvent, MapState, Motion, MotionNotifyEvent, Window, BUTTON_PRESS_EVENT,
    BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, MOTION_NOTIFY_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use x11rb::CURRENT_TIME;

use super::keys::Modifiers;

const MAX_DIMENSION: u32 = 4096;
const ACTIVATE_WAIT: Duration = Duration::from_millis(800);

/// A top-level client window, in root (screen) coordinates.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct XWindow {
    pub id: u32,
    pub pid: i32,
    /// `WM_CLASS`'s class (`Gnome-text-editor`, `firefox`, `Code`).
    pub class: String,
    pub title: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

struct Atoms {
    client_list: u32,
    client_list_stacking: u32,
    active_window: u32,
    wm_pid: u32,
    wm_name: u32,
    utf8_string: u32,
    window_type: u32,
    type_normal: u32,
    type_dialog: u32,
}

pub(super) struct X11 {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
    min_keycode: u8,
    max_keycode: u8,
    /// One input sequence at a time: a remapped scratch key must not be
    /// seen by another sequence.
    input: Mutex<()>,
}

/// Mouse buttons as X11 numbers them.
pub(super) const BUTTON_LEFT: u8 = 1;
pub(super) const BUTTON_RIGHT: u8 = 3;

fn atom(conn: &RustConnection, name: &str) -> Option<u32> {
    Some(
        conn.intern_atom(false, name.as_bytes())
            .ok()?
            .reply()
            .ok()?
            .atom,
    )
}

impl X11 {
    /// The display in `DISPLAY` (XWayland's on a Wayland session).
    pub fn connect() -> Option<Self> {
        if std::env::var_os("DISPLAY").is_none_or(|display| display.is_empty()) {
            return None;
        }
        let (conn, screen) = x11rb::connect(None).ok()?;
        let setup = conn.setup();
        let root = setup.roots.get(screen)?.root;
        let (min_keycode, max_keycode) = (setup.min_keycode, setup.max_keycode);
        // XTEST must be there for the foreground path; without it the
        // display is still usable in the background.
        let atoms = Atoms {
            client_list: atom(&conn, "_NET_CLIENT_LIST")?,
            client_list_stacking: atom(&conn, "_NET_CLIENT_LIST_STACKING")?,
            active_window: atom(&conn, "_NET_ACTIVE_WINDOW")?,
            wm_pid: atom(&conn, "_NET_WM_PID")?,
            wm_name: atom(&conn, "_NET_WM_NAME")?,
            utf8_string: atom(&conn, "UTF8_STRING")?,
            window_type: atom(&conn, "_NET_WM_WINDOW_TYPE")?,
            type_normal: atom(&conn, "_NET_WM_WINDOW_TYPE_NORMAL")?,
            type_dialog: atom(&conn, "_NET_WM_WINDOW_TYPE_DIALOG")?,
        };
        Some(Self {
            conn,
            root,
            atoms,
            min_keycode,
            max_keycode,
            input: Mutex::new(()),
        })
    }

    /// The root window's size: the whole desktop.
    pub fn desktop_size(&self) -> Option<(f64, f64)> {
        let geometry = self.conn.get_geometry(self.root).ok()?.reply().ok()?;
        Some((f64::from(geometry.width), f64::from(geometry.height)))
    }

    fn property32(&self, window: Window, property: u32, kind: impl Into<u32>) -> Vec<u32> {
        self.conn
            .get_property(false, window, property, kind.into(), 0, 4096)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().map(|values| values.collect()))
            .unwrap_or_default()
    }

    fn property_text(&self, window: Window, property: u32, kind: impl Into<u32>) -> Option<String> {
        let reply = self
            .conn
            .get_property(false, window, property, kind.into(), 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        (!reply.value.is_empty()).then(|| String::from_utf8_lossy(&reply.value).into_owned())
    }

    fn describe(&self, id: Window) -> Option<XWindow> {
        let attributes = self.conn.get_window_attributes(id).ok()?.reply().ok()?;
        if attributes.map_state != MapState::VIEWABLE {
            return None;
        }
        let types = self.property32(id, self.atoms.window_type, AtomEnum::ATOM);
        if !types.is_empty()
            && !types
                .iter()
                .any(|kind| *kind == self.atoms.type_normal || *kind == self.atoms.type_dialog)
        {
            return None;
        }
        let pid = *self
            .property32(id, self.atoms.wm_pid, AtomEnum::CARDINAL)
            .first()?;
        let geometry = self.conn.get_geometry(id).ok()?.reply().ok()?;
        if geometry.width == 0 || geometry.height == 0 {
            return None;
        }
        let origin = self
            .conn
            .translate_coordinates(id, self.root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        let class = self
            .property_text(id, u32::from(AtomEnum::WM_CLASS), AtomEnum::STRING)
            .and_then(|value| {
                value
                    .split('\0')
                    .filter(|part| !part.is_empty())
                    .last()
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let title = self
            .property_text(id, self.atoms.wm_name, self.atoms.utf8_string)
            .or_else(|| self.property_text(id, u32::from(AtomEnum::WM_NAME), AtomEnum::STRING))
            .unwrap_or_default();
        Some(XWindow {
            id,
            pid: i32::try_from(pid).ok()?,
            class,
            title,
            x: f64::from(origin.dst_x),
            y: f64::from(origin.dst_y),
            width: f64::from(geometry.width),
            height: f64::from(geometry.height),
        })
    }

    /// The managed windows, bottom to top.
    pub fn windows(&self) -> Vec<XWindow> {
        let mut ids = self.property32(self.root, self.atoms.client_list_stacking, AtomEnum::WINDOW);
        if ids.is_empty() {
            ids = self.property32(self.root, self.atoms.client_list, AtomEnum::WINDOW);
        }
        ids.into_iter().filter_map(|id| self.describe(id)).collect()
    }

    pub fn window(&self, id: u32) -> Option<XWindow> {
        self.describe(id)
    }

    /// The window's pixels as a PNG.
    pub fn capture(&self, window: &XWindow) -> Result<Vec<u8>, String> {
        let (width, height) = (window.width as u32, window.height as u32);
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err("the target window is too large to capture".into());
        }
        let image = self
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                window.id,
                0,
                0,
                width as u16,
                height as u16,
                !0,
            )
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|_| {
                "the X server could not read the window (is it minimized or off screen?)"
                    .to_string()
            })?;
        let pixels = (width * height) as usize;
        if image.data.len() < pixels * 4 {
            return Err(format!(
                "unsupported window depth {} (expected 24 or 32 bits)",
                image.depth
            ));
        }
        // ZPixmap at 24/32 bits is B, G, R, X in memory on every common server.
        let mut rgba = Vec::with_capacity(pixels * 4);
        for pixel in image.data.chunks_exact(4).take(pixels) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
        encode_png(width, height, &rgba)
    }

    /// The deepest child under a window-local point, and the point in it.
    fn child_at(&self, window: Window, x: i16, y: i16) -> (Window, i16, i16) {
        let (mut current, mut local) = (window, (x, y));
        for _ in 0..16 {
            let Some(reply) = self
                .conn
                .translate_coordinates(window, current, x, y)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
            else {
                break;
            };
            local = (reply.dst_x, reply.dst_y);
            if reply.child == x11rb::NONE {
                break;
            }
            current = reply.child;
        }
        (current, local.0, local.1)
    }

    fn button_event(
        &self,
        kind: u8,
        target: &XWindow,
        x: f64,
        y: f64,
        button: u8,
        state: u16,
    ) -> Result<(), String> {
        let (lx, ly) = (x as i16, y as i16);
        let (event, ex, ey) = self.child_at(target.id, lx, ly);
        let press = ButtonPressEvent {
            response_type: kind,
            detail: button,
            sequence: 0,
            time: CURRENT_TIME,
            root: self.root,
            event,
            child: x11rb::NONE,
            root_x: (target.x + x) as i16,
            root_y: (target.y + y) as i16,
            event_x: ex,
            event_y: ey,
            state: KeyButMask::from(state),
            same_screen: true,
        };
        let mask = if kind == BUTTON_PRESS_EVENT {
            EventMask::BUTTON_PRESS
        } else {
            EventMask::BUTTON_RELEASE
        };
        self.conn
            .send_event(true, event, mask, press)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn motion_event(&self, target: &XWindow, x: f64, y: f64, state: u16) -> Result<(), String> {
        let (event, ex, ey) = self.child_at(target.id, x as i16, y as i16);
        let motion = MotionNotifyEvent {
            response_type: MOTION_NOTIFY_EVENT,
            detail: Motion::NORMAL,
            sequence: 0,
            time: CURRENT_TIME,
            root: self.root,
            event,
            child: x11rb::NONE,
            root_x: (target.x + x) as i16,
            root_y: (target.y + y) as i16,
            event_x: ex,
            event_y: ey,
            state: KeyButMask::from(state),
            same_screen: true,
        };
        self.conn
            .send_event(true, event, EventMask::POINTER_MOTION, motion)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn flush(&self) -> Result<(), String> {
        self.conn.flush().map_err(|error| error.to_string())
    }

    /// A click delivered to the window: the pointer does not move.
    pub fn send_click(
        &self,
        target: &XWindow,
        x: f64,
        y: f64,
        button: u8,
        count: u8,
    ) -> Result<(), String> {
        let _input = self.input.lock().map_err(|_| "input lock poisoned")?;
        self.motion_event(target, x, y, 0)?;
        let held = if button == BUTTON_RIGHT {
            1 << 10
        } else {
            1 << 8
        };
        for click in 0..count {
            self.button_event(BUTTON_PRESS_EVENT, target, x, y, button, 0)?;
            self.button_event(BUTTON_RELEASE_EVENT, target, x, y, button, held)?;
            self.flush()?;
            if click + 1 < count {
                std::thread::sleep(Duration::from_millis(60));
            }
        }
        Ok(())
    }

    /// Wheel clicks (buttons 4–7) delivered to the window.
    pub fn send_scroll(
        &self,
        target: &XWindow,
        x: f64,
        y: f64,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), String> {
        let _input = self.input.lock().map_err(|_| "input lock poisoned")?;
        for (button, clicks) in wheel_clicks(delta_x, delta_y) {
            for _ in 0..clicks {
                self.button_event(BUTTON_PRESS_EVENT, target, x, y, button, 0)?;
                self.button_event(BUTTON_RELEASE_EVENT, target, x, y, button, 0)?;
            }
        }
        self.flush()
    }

    /// Press, move in steps, release — all delivered to the window.
    pub fn send_drag(
        &self,
        target: &XWindow,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), String> {
        let _input = self.input.lock().map_err(|_| "input lock poisoned")?;
        self.motion_event(target, from.0, from.1, 0)?;
        self.button_event(BUTTON_PRESS_EVENT, target, from.0, from.1, BUTTON_LEFT, 0)?;
        self.flush()?;
        for (x, y) in drag_steps(from, to) {
            self.motion_event(target, x, y, 1 << 8)?;
            self.flush()?;
            std::thread::sleep(Duration::from_millis(12));
        }
        self.button_event(
            BUTTON_RELEASE_EVENT,
            target,
            to.0,
            to.1,
            BUTTON_LEFT,
            1 << 8,
        )?;
        self.flush()
    }

    /// The keyboard map: keysyms per keycode and the keysyms.
    fn keyboard_map(&self) -> Option<(u8, Vec<u32>)> {
        let count = self.max_keycode - self.min_keycode + 1;
        let reply = self
            .conn
            .get_keyboard_mapping(self.min_keycode, count)
            .ok()?
            .reply()
            .ok()?;
        Some((reply.keysyms_per_keycode, reply.keysyms))
    }

    /// The keycode that types `keysym`, and whether it needs Shift; else a
    /// spare keycode remapped to it (restored by `release_scratch`).
    fn keycode_for(&self, keysym: u32) -> Option<(u8, bool, bool)> {
        let (per, syms) = self.keyboard_map()?;
        let per = usize::from(per.max(1));
        for (index, chunk) in syms.chunks(per).enumerate() {
            let code = self.min_keycode + index as u8;
            if chunk.first() == Some(&keysym) {
                return Some((code, false, false));
            }
            if chunk.get(1) == Some(&keysym) {
                return Some((code, true, false));
            }
        }
        // A keycode with nothing on it, borrowed for this one key.
        let spare = syms
            .chunks(per)
            .enumerate()
            .rev()
            .find(|(_, chunk)| chunk.iter().all(|sym| *sym == 0))
            .map(|(index, _)| self.min_keycode + index as u8)?;
        let mut mapping = vec![0u32; per];
        mapping[0] = keysym;
        if per > 1 {
            mapping[1] = keysym;
        }
        self.conn
            .change_keyboard_mapping(1, spare, per as u8, &mapping)
            .ok()?;
        self.conn.flush().ok()?;
        // Clients read the new map (MappingNotify) before the key.
        std::thread::sleep(Duration::from_millis(30));
        Some((spare, false, true))
    }

    fn release_scratch(&self, code: u8) {
        let per = self.keyboard_map().map(|(per, _)| per.max(1)).unwrap_or(1);
        let empty = vec![0u32; usize::from(per)];
        let _ = self.conn.change_keyboard_mapping(1, code, per, &empty);
        let _ = self.conn.flush();
    }

    fn key_event(&self, kind: u8, window: &XWindow, code: u8, state: u16) -> Result<(), String> {
        let event = KeyPressEvent {
            response_type: kind,
            detail: code,
            sequence: 0,
            time: CURRENT_TIME,
            root: self.root,
            event: window.id,
            child: x11rb::NONE,
            root_x: 0,
            root_y: 0,
            event_x: 0,
            event_y: 0,
            state: KeyButMask::from(state),
            same_screen: true,
        };
        let mask = if kind == KEY_PRESS_EVENT {
            EventMask::KEY_PRESS
        } else {
            EventMask::KEY_RELEASE
        };
        self.conn
            .send_event(true, window.id, mask, event)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Keysyms delivered to the window, each with `modifiers` held.
    pub fn send_keys(
        &self,
        window: &XWindow,
        keysyms: &[u32],
        modifiers: Modifiers,
    ) -> Result<(), String> {
        let _input = self.input.lock().map_err(|_| "input lock poisoned")?;
        for keysym in keysyms {
            let (code, shift, scratch) = self
                .keycode_for(*keysym)
                .ok_or("no key on this keyboard types that character")?;
            let mut state = modifiers.x11_state();
            if shift {
                state |= 1;
            }
            let sent = self
                .key_event(KEY_PRESS_EVENT, window, code, state)
                .and_then(|()| self.key_event(KEY_RELEASE_EVENT, window, code, state))
                .and_then(|()| self.flush());
            if scratch {
                std::thread::sleep(Duration::from_millis(10));
                self.release_scratch(code);
            }
            sent?;
        }
        Ok(())
    }

    /// Asks the window manager to raise and focus the window, and waits for
    /// it to say so.
    pub fn activate(&self, window: &XWindow) -> bool {
        let message = ClientMessageEvent::new(
            32,
            window.id,
            self.atoms.active_window,
            [2, CURRENT_TIME, 0, 0, 0],
        );
        if self
            .conn
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                message,
            )
            .is_err()
            || self.conn.flush().is_err()
        {
            return false;
        }
        let deadline = Instant::now() + ACTIVATE_WAIT;
        while Instant::now() < deadline {
            let active = self.property32(self.root, self.atoms.active_window, AtomEnum::WINDOW);
            if active.first() == Some(&window.id) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        false
    }

    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) -> Result<(), String> {
        self.conn
            .xtest_fake_input(kind, detail, CURRENT_TIME, self.root, x, y, 0)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// The real pointer: moves to a screen point and clicks there.
    pub fn fake_click(&self, x: f64, y: f64, button: u8, count: u8) -> Result<(), String> {
        let _input = self.input.lock().map_err(|_| "input lock poisoned")?;
        self.fake(MOTION_NOTIFY_EVENT, 0, x as i16, y as i16)?;
        self.flush()?;
        std::thread::sleep(Duration::from_millis(40));
        for click in 0..count {
            self.fake(BUTTON_PRESS_EVENT, button, 0, 0)?;
            self.fake(BUTTON_RELEASE_EVENT, button, 0, 0)?;
            self.flush()?;
            if click + 1 < count {
                std::thread::sleep(Duration::from_millis(60));
            }
        }
        Ok(())
    }

    pub fn fake_scroll(&self, x: f64, y: f64, delta_x: i32, delta_y: i32) -> Result<(), String> {
        let _input = self.input.lock().map_err(|_| "input lock poisoned")?;
        self.fake(MOTION_NOTIFY_EVENT, 0, x as i16, y as i16)?;
        for (button, clicks) in wheel_clicks(delta_x, delta_y) {
            for _ in 0..clicks {
                self.fake(BUTTON_PRESS_EVENT, button, 0, 0)?;
                self.fake(BUTTON_RELEASE_EVENT, button, 0, 0)?;
            }
        }
        self.flush()
    }

    pub fn fake_drag(&self, from: (f64, f64), to: (f64, f64)) -> Result<(), String> {
        let _input = self.input.lock().map_err(|_| "input lock poisoned")?;
        self.fake(MOTION_NOTIFY_EVENT, 0, from.0 as i16, from.1 as i16)?;
        self.fake(BUTTON_PRESS_EVENT, BUTTON_LEFT, 0, 0)?;
        self.flush()?;
        for (x, y) in drag_steps(from, to) {
            self.fake(MOTION_NOTIFY_EVENT, 0, x as i16, y as i16)?;
            self.flush()?;
            std::thread::sleep(Duration::from_millis(12));
        }
        self.fake(BUTTON_RELEASE_EVENT, BUTTON_LEFT, 0, 0)?;
        self.flush()
    }

    /// The real keyboard: modifiers down, each key, modifiers up.
    pub fn fake_keys(&self, keysyms: &[u32], modifiers: Modifiers) -> Result<(), String> {
        let _input = self.input.lock().map_err(|_| "input lock poisoned")?;
        let mut held = Vec::new();
        for modifier in modifiers.keysyms() {
            if let Some((code, _, _)) = self.keycode_for(modifier) {
                self.fake(KEY_PRESS_EVENT, code, 0, 0)?;
                held.push(code);
            }
        }
        let mut result = Ok(());
        for keysym in keysyms {
            let Some((code, shift, scratch)) = self.keycode_for(*keysym) else {
                result = Err("no key on this keyboard types that character".to_string());
                break;
            };
            let shift_code = if shift {
                self.keycode_for(super::keys::SHIFT_L)
                    .map(|(code, _, _)| code)
            } else {
                None
            };
            if let Some(shift) = shift_code {
                self.fake(KEY_PRESS_EVENT, shift, 0, 0)?;
            }
            self.fake(KEY_PRESS_EVENT, code, 0, 0)?;
            self.fake(KEY_RELEASE_EVENT, code, 0, 0)?;
            if let Some(shift) = shift_code {
                self.fake(KEY_RELEASE_EVENT, shift, 0, 0)?;
            }
            self.flush()?;
            if scratch {
                std::thread::sleep(Duration::from_millis(10));
                self.release_scratch(code);
            }
        }
        for code in held.into_iter().rev() {
            self.fake(KEY_RELEASE_EVENT, code, 0, 0)?;
        }
        self.flush()?;
        result
    }
}

/// Wheel buttons and clicks for a pixel delta (about 40 px a notch, at
/// least one, at most 60): 4 up, 5 down, 6 left, 7 right.
pub(super) fn wheel_clicks(delta_x: i32, delta_y: i32) -> Vec<(u8, u32)> {
    let clicks = |delta: i32| (delta.unsigned_abs().div_ceil(40)).clamp(1, 60);
    let mut out = Vec::new();
    if delta_y != 0 {
        out.push((if delta_y > 0 { 5 } else { 4 }, clicks(delta_y)));
    }
    if delta_x != 0 {
        out.push((if delta_x > 0 { 7 } else { 6 }, clicks(delta_x)));
    }
    out
}

/// The points a drag passes through after `from`, ending at `to`.
pub(super) fn drag_steps(from: (f64, f64), to: (f64, f64)) -> Vec<(f64, f64)> {
    const STEPS: usize = 12;
    (1..=STEPS)
        .map(|step| {
            let t = step as f64 / STEPS as f64;
            (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
        })
        .collect()
}

pub(super) fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(rgba)
            .map_err(|error| error.to_string())?;
    }
    Ok(out)
}
