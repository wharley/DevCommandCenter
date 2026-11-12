//! Desktop computer use on Linux, behind the same `ComputerNative` trait as
//! the Mac's Objective-C bridge, with the same three steps per action:
//!
//! 1. **accessibility** — AT-SPI 2 over D-Bus: an element's own action,
//!    its text set or inserted at the caret; no pointer, no focus. Works on
//!    X11 and on Wayland.
//! 2. **background** — X11/XWayland windows only: XSendEvent to the window;
//!    the real pointer does not move and the window is not raised.
//! 3. **foreground** — X11: `_NET_ACTIVE_WINDOW`, then XTEST (the real
//!    pointer and keyboard), only when the agent asks (`foreground: true`).
//!    Native Wayland windows: the RemoteDesktop portal, after the person
//!    approves it, and only on the active window, since no app can raise
//!    another's window on Wayland. The agent is told so (`input_note`).
//!
//! Captures: X11 GetImage per window; native Wayland windows through the
//! Screenshot portal, which captures the whole screen.

mod atspi;
mod keys;
mod portal;
mod x11;

use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use self::atspi::{Atspi, Fault, Frame, Node};
use self::keys::Modifiers;
use self::x11::{XWindow, X11};
use super::{
    ComputerAccessKind, ComputerInputMethod, ComputerNative, ComputerTarget, NativeAt,
    NativeEffectError, NativeStatus, NativeTree, PointerButton, WebAccessibility,
};

/// Window ids of native Wayland windows (AT-SPI frames) have this bit; X11
/// window ids never reach it.
const WAYLAND_ID: u32 = 0x8000_0000;

thread_local! {
    static ACTION_POINT: Cell<Option<(f64, f64)>> = const { Cell::new(None) };
}

fn note_point(x: f64, y: f64) {
    ACTION_POINT.with(|point| point.set(Some((x, y))));
}

struct Linux {
    x11: Option<X11>,
    /// A Wayland compositor runs the session (DCC itself is on XWayland).
    wayland: bool,
    atspi: Atspi,
    remote: portal::RemoteInput,
    /// Native Wayland windows by the id DCC gave them.
    frames: Mutex<HashMap<u32, Frame>>,
    /// The last whole-screen capture's size (Wayland without XWayland).
    screen: Mutex<Option<(f64, f64)>>,
    /// Processes DCC switched `org.a11y.Status` on for.
    web_accessibility: Mutex<std::collections::HashSet<i32>>,
}

fn linux() -> &'static Linux {
    static LINUX: OnceLock<Linux> = OnceLock::new();
    LINUX.get_or_init(|| Linux {
        x11: X11::connect(),
        wayland: std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty())
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland"),
        atspi: Atspi::new(),
        remote: portal::RemoteInput::new(),
        frames: Mutex::new(HashMap::new()),
        screen: Mutex::new(None),
        web_accessibility: Mutex::new(std::collections::HashSet::new()),
    })
}

/// The id an app is allowed by: its X11 class or AT-SPI name, lowercased,
/// else its process name.
fn app_id(hint: &str, pid: i32) -> String {
    let hint = hint.trim();
    if !hint.is_empty() {
        return hint.to_lowercase();
    }
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|name| name.trim().to_lowercase())
        .unwrap_or_else(|_| format!("pid-{pid}"))
}

fn wayland_id(node: &Node) -> u32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    node.hash(&mut hasher);
    (hasher.finish() as u32) | WAYLAND_ID
}

fn fault(error: Fault) -> NativeEffectError {
    match error {
        Fault::Permission => NativeEffectError::Permission,
        Fault::Invalid => NativeEffectError::Invalid,
        Fault::Stale => NativeEffectError::ElementStale,
        Fault::Outside => NativeEffectError::ElementOutside,
        Fault::Unsupported => NativeEffectError::Unsupported,
        Fault::Event => NativeEffectError::Event,
    }
}

fn event(error: String) -> NativeEffectError {
    NativeEffectError::Other(format!("the input event could not be delivered: {error}"))
}

/// Where an action lands, resolved for this platform.
enum Place {
    X11(XWindow),
    Wayland(Frame),
}

const WAYLAND_INACTIVE: &str = "on Wayland DCC cannot bring a window to the front, and input by x/y or keys reaches only the active window; ask the user to bring the target window to the front, or act on an element ref";
const WAYLAND_NO_POINTER: &str = "on Wayland this element has no accessibility action and DCC cannot place the pointer on it; click its x/y on the screenshot instead";

impl Linux {
    fn targets(&self) -> Vec<ComputerTarget> {
        let mut targets: Vec<ComputerTarget> = self
            .x11
            .as_ref()
            .map(|x11| {
                x11.windows()
                    .into_iter()
                    .map(|window| ComputerTarget {
                        bundle_id: app_id(&window.class, window.pid),
                        name: if window.class.is_empty() {
                            app_id("", window.pid)
                        } else {
                            window.class.clone()
                        },
                        pid: window.pid,
                        window_id: window.id,
                        title: window.title,
                        x: window.x,
                        y: window.y,
                        width: window.width,
                        height: window.height,
                    })
                    .collect()
            })
            .unwrap_or_default();
        if self.wayland {
            // Native Wayland windows: the apps AT-SPI knows that have no X11
            // window. Their place is unknown, so they span the screen.
            let on_x11: Vec<i32> = targets.iter().map(|target| target.pid).collect();
            let frames: Vec<Frame> = self
                .atspi
                .frames()
                .into_iter()
                .filter(|frame| !on_x11.contains(&frame.pid))
                .collect();
            let (width, height) = self.screen_size(frames.first());
            let mut known = HashMap::new();
            for frame in frames {
                let id = wayland_id(&frame.node);
                targets.push(ComputerTarget {
                    bundle_id: app_id(&frame.app, frame.pid),
                    name: if frame.app.is_empty() {
                        app_id("", frame.pid)
                    } else {
                        frame.app.clone()
                    },
                    pid: frame.pid,
                    window_id: id,
                    title: frame.title.clone(),
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                });
                known.insert(id, frame);
            }
            if let Ok(mut frames) = self.frames.lock() {
                *frames = known;
            }
        }
        targets
    }

    fn screen_size(&self, frame: Option<&Frame>) -> (f64, f64) {
        if let Some(size) = self.x11.as_ref().and_then(X11::desktop_size) {
            return size;
        }
        if let Some(size) = self.screen.lock().ok().and_then(|size| *size) {
            return size;
        }
        frame
            .and_then(|frame| frame.extents)
            .map(|(_, _, width, height)| (f64::from(width), f64::from(height)))
            .unwrap_or((1920.0, 1080.0))
    }

    /// The target as it is now; gone, moved or resized is unavailable.
    fn place(&self, target: &ComputerTarget) -> Result<Place, NativeEffectError> {
        if target.window_id & WAYLAND_ID != 0 {
            let frame = self
                .frames
                .lock()
                .ok()
                .and_then(|frames| frames.get(&target.window_id).cloned())
                .filter(|frame| frame.pid == target.pid)
                .ok_or(NativeEffectError::TargetUnavailable)?;
            return Ok(Place::Wayland(frame));
        }
        let window = self
            .x11
            .as_ref()
            .and_then(|x11| x11.window(target.window_id))
            .filter(|window| {
                window.pid == target.pid
                    && (window.x - target.x).abs() < 0.5
                    && (window.y - target.y).abs() < 0.5
                    && (window.width - target.width).abs() < 0.5
                    && (window.height - target.height).abs() < 0.5
            })
            .ok_or(NativeEffectError::TargetUnavailable)?;
        Ok(Place::X11(window))
    }

    fn x11(&self) -> Result<&X11, NativeEffectError> {
        self.x11
            .as_ref()
            .ok_or(NativeEffectError::TargetUnavailable)
    }

    /// The AT-SPI window of an X11 window: same process, then same title,
    /// then the one whose box matches.
    fn frame_of(&self, window: &XWindow) -> Option<Node> {
        let frames: Vec<Frame> = self
            .atspi
            .frames()
            .into_iter()
            .filter(|frame| frame.pid == window.pid)
            .collect();
        if frames.len() == 1 {
            return frames.into_iter().next().map(|frame| frame.node);
        }
        frames
            .iter()
            .find(|frame| !window.title.is_empty() && frame.title == window.title)
            .or_else(|| {
                frames.iter().find(|frame| {
                    frame.extents.is_some_and(|(x, y, width, height)| {
                        (f64::from(x) - window.x).abs() < 2.0
                            && (f64::from(y) - window.y).abs() < 2.0
                            && (f64::from(width) - window.width).abs() < 2.0
                            && (f64::from(height) - window.height).abs() < 2.0
                    })
                })
            })
            .map(|frame| frame.node.clone())
    }

    fn require_active(&self, frame: &Frame) -> Result<(), NativeEffectError> {
        if self.atspi.is_active(&frame.node) {
            Ok(())
        } else {
            Err(NativeEffectError::Other(WAYLAND_INACTIVE.into()))
        }
    }

    fn element(
        &self,
        target: &ComputerTarget,
        handle: u64,
        index: u32,
    ) -> Result<Node, NativeEffectError> {
        self.atspi
            .element(handle, index, target.pid, target.window_id)
            .map_err(fault)
    }

    /// The center of an element's part inside an X11 window, window-local.
    fn element_center(
        &self,
        window: &XWindow,
        node: &Node,
    ) -> Result<(f64, f64), NativeEffectError> {
        let (x, y, width, height) = self
            .atspi
            .element_extents(node, atspi::COORDS_SCREEN)
            .map_err(fault)?;
        let left = (f64::from(x) - window.x).max(0.0);
        let top = (f64::from(y) - window.y).max(0.0);
        let right = (f64::from(x + width) - window.x).min(window.width);
        let bottom = (f64::from(y + height) - window.y).min(window.height);
        if right - left < 1.0 || bottom - top < 1.0 {
            return Err(NativeEffectError::ElementOutside);
        }
        Ok(((left + right) / 2.0, (top + bottom) / 2.0))
    }

    fn pointer_point(
        &self,
        place: &Place,
        target: &ComputerTarget,
        at: NativeAt,
    ) -> Result<(f64, f64), NativeEffectError> {
        match (at, place) {
            (NativeAt::Point { x, y }, _) => {
                if x < 0.0 || y < 0.0 || x >= target.width || y >= target.height {
                    return Err(NativeEffectError::OutsideTarget);
                }
                Ok((x, y))
            }
            (NativeAt::Element { handle, index }, Place::X11(window)) => {
                let node = self.element(target, handle, index)?;
                self.element_center(window, &node)
            }
            (NativeAt::Element { .. }, Place::Wayland(_)) => {
                Err(NativeEffectError::Other(WAYLAND_NO_POINTER.into()))
            }
        }
    }

    fn click(
        &self,
        target: &ComputerTarget,
        at: NativeAt,
        button: PointerButton,
        count: u8,
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        if !(1..=2).contains(&count) {
            return Err(NativeEffectError::Invalid);
        }
        let place = self.place(target)?;
        if let NativeAt::Element { handle, index } = at {
            if button == PointerButton::Left && count == 1 && !foreground {
                let node = self.element(target, handle, index)?;
                // Where it is, before pressing it may close its window.
                let center = match &place {
                    Place::X11(window) => self
                        .element_center(window, &node)
                        .ok()
                        .map(|(x, y)| (window.x + x, window.y + y)),
                    Place::Wayland(_) => None,
                };
                if self.atspi.press(&node).map_err(fault)? {
                    if let Some((x, y)) = center {
                        note_point(x, y);
                    }
                    return Ok(ComputerInputMethod::Accessibility);
                }
            }
        }
        // A plain click on a point over a button, link, check box…: its own
        // action, as a ref would (toolkits on XInput2, like GTK 3, ignore
        // synthesized core clicks).
        if let (NativeAt::Point { x, y }, Place::X11(window)) = (at, &place) {
            if button == PointerButton::Left && count == 1 && !foreground {
                let (sx, sy) = (window.x + x, window.y + y);
                if self
                    .frame_of(window)
                    .is_some_and(|frame| self.atspi.press_at(&frame, sx, sy))
                {
                    note_point(sx, sy);
                    return Ok(ComputerInputMethod::Accessibility);
                }
            }
        }
        let (x, y) = self.pointer_point(&place, target, at)?;
        let right = button == PointerButton::Right;
        match place {
            Place::X11(window) => {
                note_point(window.x + x, window.y + y);
                let x11 = self.x11()?;
                let code = if right {
                    x11::BUTTON_RIGHT
                } else {
                    x11::BUTTON_LEFT
                };
                if !foreground {
                    x11.send_click(&window, x, y, code, count).map_err(event)?;
                    return Ok(ComputerInputMethod::Background);
                }
                if !x11.activate(&window) {
                    return Err(NativeEffectError::Focus);
                }
                x11.fake_click(window.x + x, window.y + y, code, count)
                    .map_err(event)?;
                Ok(ComputerInputMethod::Foreground)
            }
            Place::Wayland(frame) => {
                self.require_active(&frame)?;
                note_point(x, y);
                let code = if right {
                    portal::BTN_RIGHT
                } else {
                    portal::BTN_LEFT
                };
                self.remote.click(x, y, code, count).map_err(event)?;
                Ok(ComputerInputMethod::Foreground)
            }
        }
    }

    fn scroll(
        &self,
        target: &ComputerTarget,
        at: NativeAt,
        delta_x: i32,
        delta_y: i32,
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        if delta_x == 0 && delta_y == 0 {
            return Err(NativeEffectError::Invalid);
        }
        let place = self.place(target)?;
        let (x, y) = self.pointer_point(&place, target, at)?;
        match place {
            Place::X11(window) => {
                note_point(window.x + x, window.y + y);
                let x11 = self.x11()?;
                if !foreground {
                    x11.send_scroll(&window, x, y, delta_x, delta_y)
                        .map_err(event)?;
                    return Ok(ComputerInputMethod::Background);
                }
                if !x11.activate(&window) {
                    return Err(NativeEffectError::Focus);
                }
                x11.fake_scroll(window.x + x, window.y + y, delta_x, delta_y)
                    .map_err(event)?;
                Ok(ComputerInputMethod::Foreground)
            }
            Place::Wayland(frame) => {
                self.require_active(&frame)?;
                note_point(x, y);
                self.remote.scroll(x, y, delta_x, delta_y).map_err(event)?;
                Ok(ComputerInputMethod::Foreground)
            }
        }
    }

    fn drag(
        &self,
        target: &ComputerTarget,
        from: (f64, f64),
        to: (f64, f64),
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        let place = self.place(target)?;
        for (x, y) in [from, to] {
            self.pointer_point(&place, target, NativeAt::Point { x, y })?;
        }
        match place {
            Place::X11(window) => {
                note_point(window.x + to.0, window.y + to.1);
                let x11 = self.x11()?;
                if !foreground {
                    x11.send_drag(&window, from, to).map_err(event)?;
                    return Ok(ComputerInputMethod::Background);
                }
                if !x11.activate(&window) {
                    return Err(NativeEffectError::Focus);
                }
                x11.fake_drag(
                    (window.x + from.0, window.y + from.1),
                    (window.x + to.0, window.y + to.1),
                )
                .map_err(event)?;
                Ok(ComputerInputMethod::Foreground)
            }
            Place::Wayland(frame) => {
                self.require_active(&frame)?;
                note_point(to.0, to.1);
                self.remote.drag(from, to).map_err(event)?;
                Ok(ComputerInputMethod::Foreground)
            }
        }
    }

    /// Keys typed into the target: X11 in the background unless asked,
    /// else the real keyboard; Wayland through the portal, on the active
    /// window only.
    fn keys(
        &self,
        target: &ComputerTarget,
        keysyms: &[u32],
        modifiers: Modifiers,
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        match self.place(target)? {
            Place::X11(window) => {
                let x11 = self.x11()?;
                if !foreground {
                    x11.send_keys(&window, keysyms, modifiers).map_err(event)?;
                    return Ok(ComputerInputMethod::Background);
                }
                if !x11.activate(&window) {
                    return Err(NativeEffectError::Focus);
                }
                x11.fake_keys(keysyms, modifiers).map_err(event)?;
                Ok(ComputerInputMethod::Foreground)
            }
            Place::Wayland(frame) => {
                self.require_active(&frame)?;
                self.remote
                    .keys(keysyms, &modifiers.keysyms())
                    .map_err(event)?;
                Ok(ComputerInputMethod::Foreground)
            }
        }
    }

    fn type_text(
        &self,
        target: &ComputerTarget,
        text: &str,
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        let place = self.place(target)?;
        // Plain text goes straight into the focused field when it is an
        // editable AT-SPI element: no keys at all. Newlines and tabs are
        // keys (submit, next field), so they are typed.
        if !foreground && !text.contains(['\n', '\r', '\t']) {
            let frame = match &place {
                Place::X11(window) => self.frame_of(window),
                Place::Wayland(frame) => Some(frame.node.clone()),
            };
            if frame.is_some_and(|frame| self.atspi.insert_at_caret(&frame, text)) {
                return Ok(ComputerInputMethod::Accessibility);
            }
        }
        let keysyms: Vec<u32> = text.chars().map(keys::keysym_for_char).collect();
        self.keys(target, &keysyms, Modifiers::default(), foreground)
    }

    fn input_note(&self) -> Option<String> {
        let x11 = "X11 windows: element refs, and plain clicks by x/y on a button, link, check box or tab, act through AT-SPI accessibility; typing goes into the focused text field through AT-SPI when it can. Other clicks, scroll, drag and keys go to the window in the background with XSendEvent, which toolkits on XInput2 (GTK 3, Qt 5 and later, Chromium) usually ignore — when nothing changed, retry with foreground true, which raises the window and moves the real pointer and keyboard (XTEST). COMMAND acts as Control.";
        let wayland = "Native Wayland windows: the screenshot is the whole screen and x/y are screen pixels; element refs act through AT-SPI accessibility in the background; clicks by x/y, scroll, drag, typing outside a text field and keys go through the desktop's RemoteDesktop portal, which the user approves once, moves the real pointer and reaches only the active window, because Wayland lets no app raise another's window.";
        Some(match (self.x11.is_some(), self.wayland) {
            (true, false) => format!("Linux (X11). {x11}"),
            (true, true) => format!("Linux (Wayland). Apps on XWayland: {x11} {wayland}"),
            (false, true) => format!("Linux (Wayland, no XWayland). {wayland}"),
            (false, false) => return None,
        })
    }
}

/// The trait's face on Linux; all state lives in `linux()`.
pub(super) struct PlatformComputerNative;

impl ComputerNative for PlatformComputerNative {
    fn status(&self) -> NativeStatus {
        let linux = linux();
        NativeStatus {
            accessibility: linux.atspi.available(),
            screen_recording: linux.x11.is_some() || linux.wayland,
            major_version: 0,
        }
    }
    fn supported(&self) -> bool {
        let linux = linux();
        linux.x11.is_some() || linux.wayland
    }
    fn input_note(&self) -> Option<String> {
        linux().input_note()
    }
    fn request_access(&self, kind: ComputerAccessKind) -> bool {
        let linux = linux();
        match kind {
            // Apps that build their tree only on demand (Chromium, Electron,
            // Firefox) read this; GTK and Qt apps are on the bus anyway.
            ComputerAccessKind::Accessibility => {
                atspi::set_status_enabled(true) || linux.atspi.available()
            }
            // X11 needs no permission; Wayland asks in the portal's dialog.
            ComputerAccessKind::ScreenRecording => {
                linux.x11.is_some() || portal::screenshot(true).is_ok()
            }
        }
    }
    fn targets(&self) -> Result<Vec<ComputerTarget>, String> {
        Ok(linux().targets())
    }
    fn capture(&self, target: &ComputerTarget) -> Result<Vec<u8>, String> {
        let linux = linux();
        match linux.place(target).map_err(|error| error.message())? {
            Place::X11(window) => linux
                .x11()
                .map_err(|error| error.message())?
                .capture(&window),
            Place::Wayland(_) => {
                let png = portal::screenshot(false)?;
                if let Some(size) = png_size(&png) {
                    if let Ok(mut screen) = linux.screen.lock() {
                        *screen = Some(size);
                    }
                }
                Ok(png)
            }
        }
    }
    fn accessibility_tree(&self, target: &ComputerTarget) -> Result<NativeTree, String> {
        let linux = linux();
        if !linux.atspi.available() {
            return Err(NativeEffectError::Permission.message());
        }
        let (frame, origin) = match linux.place(target).map_err(|error| error.message())? {
            Place::X11(window) => (
                linux.frame_of(&window),
                Some((window.x, window.y, window.width, window.height)),
            ),
            Place::Wayland(frame) => (Some(frame.node), None),
        };
        let Some(frame) = frame else {
            return Ok(NativeTree::default());
        };
        let Some(tree) = linux
            .atspi
            .walk(&frame, target.pid, target.window_id, origin)
        else {
            return Ok(NativeTree::default());
        };
        Ok(NativeTree {
            available: true,
            handle: tree.handle,
            count: tree.count,
            text: tree.text,
            truncated: tree.truncated,
            busy: tree.busy,
        })
    }
    fn release_tree(&self, handle: u64) {
        linux().atspi.release(handle);
    }
    fn take_action_point(&self) -> Option<(f64, f64)> {
        ACTION_POINT.with(|point| point.take())
    }
    /// Global on Linux (`org.a11y.Status.IsEnabled`), not per app: DCC
    /// switches it on for the first app that needs it and off after the
    /// last one it switched it on for; one a screen reader (or the person)
    /// switched on is left alone.
    fn set_web_accessibility(&self, pid: i32, enabled: bool) -> WebAccessibility {
        let Ok(mut owners) = linux().web_accessibility.lock() else {
            return WebAccessibility::Unsupported;
        };
        if !enabled {
            if owners.remove(&pid) && owners.is_empty() && !atspi::set_status_enabled(false) {
                return WebAccessibility::Unsupported;
            }
            return WebAccessibility::Changed;
        }
        match atspi::status_enabled() {
            None => WebAccessibility::Unsupported,
            Some(true) if owners.is_empty() => WebAccessibility::AlreadyEnabled,
            Some(true) => {
                owners.insert(pid);
                WebAccessibility::Changed
            }
            Some(false) if atspi::set_status_enabled(true) => {
                owners.insert(pid);
                WebAccessibility::Changed
            }
            Some(false) => WebAccessibility::Unsupported,
        }
    }
    fn click(
        &self,
        target: &ComputerTarget,
        at: NativeAt,
        button: PointerButton,
        click_count: u8,
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        linux().click(target, at, button, click_count, foreground)
    }
    fn scroll(
        &self,
        target: &ComputerTarget,
        at: NativeAt,
        delta_x: i32,
        delta_y: i32,
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        linux().scroll(target, at, delta_x, delta_y, foreground)
    }
    fn drag(
        &self,
        target: &ComputerTarget,
        from: (f64, f64),
        to: (f64, f64),
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        linux().drag(target, from, to, foreground)
    }
    fn set_value(
        &self,
        target: &ComputerTarget,
        handle: u64,
        index: u32,
        value: &str,
    ) -> Result<(), NativeEffectError> {
        let linux = linux();
        linux.place(target)?;
        let node = linux.element(target, handle, index)?;
        linux.atspi.set_text(&node, value).map_err(fault)
    }
    fn type_text(
        &self,
        target: &ComputerTarget,
        text: &str,
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        linux().type_text(target, text, foreground)
    }
    fn key(
        &self,
        target: &ComputerTarget,
        key_code: u16,
        flags: u64,
        foreground: bool,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        let keysym = keys::keysym_for_mac_key(key_code).ok_or(NativeEffectError::Invalid)?;
        linux().keys(target, &[keysym], Modifiers::from_flags(flags), foreground)
    }
}

/// A PNG's width and height, from its header.
fn png_size(bytes: &[u8]) -> Option<(f64, f64)> {
    if bytes.len() < 24 || &bytes[1..4] != b"PNG" {
        return None;
    }
    let read =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    Some((f64::from(read(16)), f64::from(read(20))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_headers_give_the_capture_size() {
        let png = x11::encode_png(3, 2, &[0; 3 * 2 * 4]).unwrap();
        assert_eq!(png_size(&png), Some((3.0, 2.0)));
        assert_eq!(png_size(b"nope"), None);
    }

    #[test]
    fn wayland_ids_never_collide_with_x11_ids() {
        let node = Node {
            bus: ":1.42".into(),
            path: zbus::zvariant::OwnedObjectPath::try_from("/org/a11y/atspi/accessible/7")
                .unwrap(),
        };
        assert_ne!(wayland_id(&node) & WAYLAND_ID, 0);
        assert_eq!(wayland_id(&node), wayland_id(&node.clone()));
    }

    #[test]
    fn app_ids_are_lowercased_hints() {
        assert_eq!(app_id("Gnome-text-editor", 1), "gnome-text-editor");
        assert!(!app_id("", std::process::id() as i32).is_empty());
    }

    /// A live desktop for the `live_*` tests (run with `--ignored` under an
    /// X server with a window manager and an AT-SPI bus): the window whose
    /// title is `DCC_LIVE_TITLE`, its tree, and the ref of a line matching.
    fn live_target() -> (ComputerTarget, NativeTree) {
        let title = std::env::var("DCC_LIVE_TITLE").expect("DCC_LIVE_TITLE");
        let native = PlatformComputerNative;
        assert!(native.supported(), "no X11 or Wayland session");
        let target = native
            .targets()
            .unwrap()
            .into_iter()
            .find(|target| target.title == title)
            .unwrap_or_else(|| panic!("no window titled {title}"));
        let png = native.capture(&target).expect("capture");
        assert_eq!(
            png_size(&png),
            Some((target.width, target.height)),
            "the capture is the window"
        );
        let tree = native.accessibility_tree(&target).expect("tree");
        eprintln!("{target:?}\n{}", tree.text);
        (target, tree)
    }

    fn live_ref(tree: &NativeTree, wanted: &[&str]) -> u32 {
        let line = tree
            .text
            .lines()
            .find(|line| wanted.iter().all(|part| line.contains(part)))
            .unwrap_or_else(|| panic!("no element with {wanted:?}"));
        let reference: u32 = line.trim_start()[1..]
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        reference - 1
    }

    fn live_entry(tree: &NativeTree) -> u32 {
        live_ref(tree, &["editable"])
    }

    #[test]
    #[ignore = "needs a live X11 desktop"]
    fn live_set_value_and_press_by_accessibility() {
        let (target, tree) = live_target();
        let native = PlatformComputerNative;
        native
            .set_value(&target, tree.handle, live_entry(&tree), "set-by-dcc")
            .expect("set value");
        let ok = live_ref(&tree, &["\"OK\""]);
        let at = NativeAt::Element {
            handle: tree.handle,
            index: ok,
        };
        let method = native
            .click(&target, at, PointerButton::Left, 1, false)
            .expect("press OK");
        assert_eq!(method, ComputerInputMethod::Accessibility);
        assert!(
            native.take_action_point().is_some(),
            "the cursor has a point"
        );
    }

    #[test]
    #[ignore = "needs a live X11 desktop"]
    fn live_type_at_the_caret() {
        let (target, tree) = live_target();
        let native = PlatformComputerNative;
        let method = native
            .type_text(&target, "caret-typed", false)
            .expect("type");
        assert_eq!(method, ComputerInputMethod::Accessibility);
        let ok = live_ref(&tree, &["\"OK\""]);
        let at = NativeAt::Element {
            handle: tree.handle,
            index: ok,
        };
        native
            .click(&target, at, PointerButton::Left, 1, false)
            .unwrap();
    }

    #[test]
    #[ignore = "needs a live X11 desktop"]
    fn live_keys_in_the_background() {
        let (target, _) = live_target();
        let method = PlatformComputerNative
            .type_text(&target, "bg-typed\n", false)
            .expect("type");
        assert_eq!(method, ComputerInputMethod::Background);
    }

    #[test]
    #[ignore = "needs a live X11 desktop"]
    fn live_click_by_point_presses_the_button() {
        let (target, tree) = live_target();
        let native = PlatformComputerNative;
        native
            .set_value(&target, tree.handle, live_entry(&tree), "clicked-by-point")
            .unwrap();
        let line = tree
            .text
            .lines()
            .find(|line| line.contains("\"OK\""))
            .unwrap();
        let at = line.rsplit('@').next().unwrap();
        let (place, size) = at.split_once(' ').unwrap();
        let (x, y) = place.split_once(',').unwrap();
        let (w, h) = size.split_once('x').unwrap();
        let center = |a: &str, b: &str| a.parse::<f64>().unwrap() + b.parse::<f64>().unwrap() / 2.0;
        let point = NativeAt::Point {
            x: center(x, w),
            y: center(y, h),
        };
        let method = native
            .click(&target, point, PointerButton::Left, 1, false)
            .expect("click");
        assert_eq!(method, ComputerInputMethod::Accessibility);
    }

    #[test]
    #[ignore = "needs a live X11 desktop"]
    fn live_keys_in_the_foreground() {
        let (target, _) = live_target();
        let method = PlatformComputerNative
            .type_text(&target, "fg-typed\n", true)
            .expect("type");
        assert_eq!(method, ComputerInputMethod::Foreground);
    }

    #[test]
    fn wheel_and_drag_steps() {
        assert_eq!(x11::wheel_clicks(0, 100), [(5, 3)]);
        assert_eq!(x11::wheel_clicks(-10, -1), [(4, 1), (6, 1)]);
        let steps = x11::drag_steps((0.0, 0.0), (12.0, 24.0));
        assert_eq!(steps.last(), Some(&(12.0, 24.0)));
        assert_eq!(steps.len(), 12);
    }
}
