//! AT-SPI 2 over D-Bus: the accessibility bus, each app's windows and their
//! elements, and the elements' own actions (press, set text, insert text),
//! which reach an app without the pointer or focus, on X11 and on Wayland.
//! Every call has a short timeout: a hung app must not hang the walk.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const CALL_TIMEOUT: Duration = Duration::from_millis(400);
const TREE_BUDGET: Duration = Duration::from_secs(2);
const MAX_VISITED: usize = 6000;
const MAX_REFS: usize = super::super::MAX_ELEMENT_REFS;
const MAX_DEPTH: usize = 64;
const MAX_TEXT: usize = 48 * 1024;
const MAX_FIELD: usize = 120;
const MAX_TREES: usize = 32;

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const COMPONENT: &str = "org.a11y.atspi.Component";
const ACTION: &str = "org.a11y.atspi.Action";
const EDITABLE: &str = "org.a11y.atspi.EditableText";
const TEXT: &str = "org.a11y.atspi.Text";
const VALUE: &str = "org.a11y.atspi.Value";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

// ATSPI_STATE_* bits.
const STATE_ACTIVE: u32 = 1;
const STATE_BUSY: u32 = 3;
const STATE_CHECKED: u32 = 4;
const STATE_EDITABLE: u32 = 7;
const STATE_ENABLED: u32 = 8;
const STATE_EXPANDABLE: u32 = 9;
const STATE_EXPANDED: u32 = 10;
const STATE_FOCUSED: u32 = 12;
const STATE_SELECTED: u32 = 23;
const STATE_SHOWING: u32 = 25;
const STATE_CHECKABLE: u32 = 41;
const STATE_READ_ONLY: u32 = 43;

/// ATSPI_COORD_TYPE_SCREEN.
pub(super) const COORDS_SCREEN: u32 = 0;

/// An accessible object: its owner's bus name and its path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Node {
    pub bus: String,
    pub path: OwnedObjectPath,
}

/// A window an app exposes (a frame, a dialog), as AT-SPI sees it.
#[derive(Clone, Debug)]
pub(super) struct Frame {
    pub node: Node,
    pub pid: i32,
    pub app: String,
    pub title: String,
    /// Its extents in screen coordinates; on Wayland usually not the real
    /// place (toolkits do not know where the compositor put them).
    pub extents: Option<(i32, i32, i32, i32)>,
}

#[derive(Clone, Copy, Default)]
struct States(u64);

impl States {
    fn has(self, bit: u32) -> bool {
        self.0 & (1 << bit) != 0
    }
}

/// A walked tree: its elements by ref (1-based ref is index + 1).
struct Tree {
    pid: i32,
    window_id: u32,
    elements: Vec<Node>,
}

pub(super) struct WalkedTree {
    pub handle: u64,
    pub count: usize,
    pub text: String,
    pub truncated: bool,
    pub busy: bool,
}

pub(super) struct Atspi {
    connection: Mutex<Option<Connection>>,
    trees: Mutex<(u64, Vec<(u64, Tree)>)>,
}

fn session_call<R>(
    connection: &Connection,
    destination: &str,
    path: &str,
    interface: &str,
    method: &str,
    body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
) -> Option<R>
where
    R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
{
    let reply = connection
        .call_method(Some(destination), path, Some(interface), method, body)
        .ok()?;
    reply.body().deserialize::<R>().ok()
}

/// Whether accessibility is switched on for the session
/// (`org.a11y.Status.IsEnabled`: Chromium, Electron and Firefox only build
/// their tree when it is).
pub(super) fn status_enabled() -> Option<bool> {
    let session = Connection::session().ok()?;
    let (value,): (OwnedValue,) = session_call(
        &session,
        "org.a11y.Bus",
        "/org/a11y/bus",
        PROPERTIES,
        "Get",
        &("org.a11y.Status", "IsEnabled"),
    )?;
    bool::try_from(value).ok()
}

pub(super) fn set_status_enabled(enabled: bool) -> bool {
    let Ok(session) = Connection::session() else {
        return false;
    };
    session
        .call_method(
            Some("org.a11y.Bus"),
            "/org/a11y/bus",
            Some(PROPERTIES),
            "Set",
            &("org.a11y.Status", "IsEnabled", Value::from(enabled)),
        )
        .is_ok()
}

fn clean(text: &str) -> Option<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let mut out: String = collapsed.chars().take(MAX_FIELD).collect();
    if collapsed.chars().count() > MAX_FIELD {
        out.push('…');
    }
    Some(out.replace('"', "\\\""))
}

/// `push button` reads `pushButton`, as the Mac's roles read `button`.
fn short_role(role: &str) -> String {
    let mut out = String::new();
    for (index, word) in role.split_whitespace().enumerate() {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            if index == 0 {
                out.extend(first.to_lowercase());
            } else {
                out.extend(first.to_uppercase());
            }
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        "unknown".into()
    } else {
        out
    }
}

const INTERACTIVE: &[&str] = &[
    "push button",
    "toggle button",
    "check box",
    "radio button",
    "combo box",
    "entry",
    "password text",
    "text",
    "spin button",
    "slider",
    "link",
    "menu item",
    "check menu item",
    "radio menu item",
    "menu",
    "page tab",
    "list item",
    "tree item",
    "switch",
    "button",
];

const LABELLED: &[&str] = &[
    "label",
    "static",
    "heading",
    "image",
    "table cell",
    "list",
    "tree",
    "table",
    "dialog",
    "alert",
    "tool bar",
    "page tab list",
    "document web",
    "document frame",
    "paragraph",
    "section",
];

/// Controls a plain left click means pressing.
const PRESSABLE: &[&str] = &[
    "push button",
    "button",
    "toggle button",
    "check box",
    "radio button",
    "link",
    "menu item",
    "check menu item",
    "radio menu item",
    "page tab",
    "switch",
];

const ACTION_PRIORITY: &[&str] = &["click", "press", "activate", "jump", "toggle", "open"];

impl Atspi {
    pub fn new() -> Self {
        Self {
            connection: Mutex::new(None),
            trees: Mutex::new((0, Vec::new())),
        }
    }

    /// The accessibility bus (its address comes from `org.a11y.Bus`).
    fn bus(&self) -> Option<Connection> {
        let mut slot = self.connection.lock().ok()?;
        if let Some(connection) = slot.as_ref() {
            return Some(connection.clone());
        }
        let session = Connection::session().ok()?;
        let (address,): (String,) = session_call(
            &session,
            "org.a11y.Bus",
            "/org/a11y/bus",
            "org.a11y.Bus",
            "GetAddress",
            &(),
        )?;
        let connection = zbus::blocking::connection::Builder::address(address.as_str())
            .ok()?
            .method_timeout(CALL_TIMEOUT)
            .build()
            .ok()?;
        *slot = Some(connection.clone());
        Some(connection)
    }

    fn forget_bus(&self) {
        if let Ok(mut slot) = self.connection.lock() {
            *slot = None;
        }
    }

    pub fn available(&self) -> bool {
        self.bus().is_some()
    }

    fn call<R>(
        &self,
        bus: &Connection,
        node: &Node,
        interface: &str,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> Option<R>
    where
        R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
    {
        session_call(bus, &node.bus, node.path.as_str(), interface, method, body)
    }

    fn property(
        &self,
        bus: &Connection,
        node: &Node,
        interface: &str,
        name: &str,
    ) -> Option<OwnedValue> {
        let (value,): (OwnedValue,) =
            self.call(bus, node, PROPERTIES, "Get", &(interface, name))?;
        Some(value)
    }

    fn text_property(&self, bus: &Connection, node: &Node, name: &str) -> Option<String> {
        String::try_from(self.property(bus, node, ACCESSIBLE, name)?).ok()
    }

    fn children(&self, bus: &Connection, node: &Node) -> Vec<Node> {
        self.call::<(Vec<(String, OwnedObjectPath)>,)>(bus, node, ACCESSIBLE, "GetChildren", &())
            .map(|(children,)| {
                children
                    .into_iter()
                    .filter(|(_, path)| path.as_str() != "/org/a11y/atspi/null")
                    .map(|(bus, path)| Node { bus, path })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn role(&self, bus: &Connection, node: &Node) -> String {
        self.call::<(String,)>(bus, node, ACCESSIBLE, "GetRoleName", &())
            .map(|(role,)| role)
            .unwrap_or_default()
    }

    fn states(&self, bus: &Connection, node: &Node) -> States {
        self.call::<(Vec<u32>,)>(bus, node, ACCESSIBLE, "GetState", &())
            .map(|(words,)| {
                States(
                    u64::from(words.first().copied().unwrap_or(0))
                        | (u64::from(words.get(1).copied().unwrap_or(0)) << 32),
                )
            })
            .unwrap_or_default()
    }

    fn interfaces(&self, bus: &Connection, node: &Node) -> Vec<String> {
        self.call::<(Vec<String>,)>(bus, node, ACCESSIBLE, "GetInterfaces", &())
            .map(|(interfaces,)| interfaces)
            .unwrap_or_default()
    }

    fn extents(&self, bus: &Connection, node: &Node, coords: u32) -> Option<(i32, i32, i32, i32)> {
        let (extents,): ((i32, i32, i32, i32),) =
            self.call(bus, node, COMPONENT, "GetExtents", &(coords,))?;
        (extents.2 > 0 && extents.3 > 0).then_some(extents)
    }

    fn actions(&self, bus: &Connection, node: &Node) -> Vec<String> {
        self.call::<(Vec<(String, String, String)>,)>(bus, node, ACTION, "GetActions", &())
            // GTK names them `Click`, Qt and Chromium `click`/`press`.
            .map(|(actions,)| {
                actions
                    .into_iter()
                    .map(|(name, _, _)| name.to_lowercase())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn pid_of(&self, bus: &Connection, name: &str) -> Option<i32> {
        let (pid,): (u32,) = session_call(
            bus,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            &(name,),
        )?;
        i32::try_from(pid).ok()
    }

    /// Every app's windows, with their process.
    pub fn frames(&self) -> Vec<Frame> {
        let Some(bus) = self.bus() else {
            return Vec::new();
        };
        let root = Node {
            bus: "org.a11y.atspi.Registry".into(),
            path: OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root").expect("valid path"),
        };
        let apps = self.children(&bus, &root);
        if apps.is_empty()
            && self
                .call::<(i32,)>(&bus, &root, PROPERTIES, "Get", &(ACCESSIBLE, "ChildCount"))
                .is_none()
        {
            // The bus went away (session restarted): connect again next time.
            self.forget_bus();
            return Vec::new();
        }
        let mut frames = Vec::new();
        for app in apps {
            let Some(pid) = self.pid_of(&bus, &app.bus) else {
                continue;
            };
            let app_name = self.text_property(&bus, &app, "Name").unwrap_or_default();
            for window in self.children(&bus, &app) {
                let role = self.role(&bus, &window);
                if !matches!(
                    role.as_str(),
                    "frame" | "window" | "dialog" | "file chooser" | "alert"
                ) {
                    continue;
                }
                let states = self.states(&bus, &window);
                if !states.has(STATE_SHOWING) {
                    continue;
                }
                frames.push(Frame {
                    title: self
                        .text_property(&bus, &window, "Name")
                        .unwrap_or_default(),
                    extents: self.extents(&bus, &window, COORDS_SCREEN),
                    node: window,
                    pid,
                    app: app_name.clone(),
                });
            }
        }
        frames
    }

    /// Whether a window is the active one (the one keys and the portal's
    /// pointer reach on Wayland, where DCC cannot raise it).
    pub fn is_active(&self, frame: &Node) -> bool {
        self.bus()
            .is_some_and(|bus| self.states(&bus, frame).has(STATE_ACTIVE))
    }

    /// Walks a window into `e<ref> role "name" … @x,y wxh` lines. `origin`
    /// is the window's screen position when element boxes can be placed
    /// on the screenshot (X11); `None` lists elements without boxes.
    pub fn walk(
        &self,
        frame: &Node,
        pid: i32,
        window_id: u32,
        origin: Option<(f64, f64, f64, f64)>,
    ) -> Option<WalkedTree> {
        let bus = self.bus()?;
        let mut walk = Walk {
            atspi: self,
            bus: &bus,
            origin,
            deadline: Instant::now() + TREE_BUDGET,
            visited: 0,
            elements: Vec::new(),
            text: String::new(),
            truncated: false,
            busy: false,
        };
        walk.element(frame, 0, 0, None);
        // Popups and menus of the same app are windows of their own.
        if !walk.truncated {
            let app = Node {
                bus: frame.bus.clone(),
                path: OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root")
                    .expect("valid path"),
            };
            for window in self.children(&bus, &app) {
                if &window == frame {
                    continue;
                }
                let role = self.role(&bus, &window);
                if !matches!(role.as_str(), "window" | "menu" | "popup menu" | "tool tip") {
                    continue;
                }
                if !self.states(&bus, &window).has(STATE_SHOWING) {
                    continue;
                }
                walk.text.push_str("open menu:\n");
                walk.element(&window, 0, 1, None);
                if walk.truncated {
                    break;
                }
            }
        }
        let (count, text, truncated, busy) =
            (walk.elements.len(), walk.text, walk.truncated, walk.busy);
        let elements = walk.elements;
        let mut trees = self.trees.lock().ok()?;
        trees.0 += 1;
        let handle = trees.0;
        trees.1.push((
            handle,
            Tree {
                pid,
                window_id,
                elements,
            },
        ));
        if trees.1.len() > MAX_TREES {
            trees.1.remove(0);
        }
        Some(WalkedTree {
            handle,
            count,
            text,
            truncated,
            busy,
        })
    }

    pub fn release(&self, handle: u64) {
        if let Ok(mut trees) = self.trees.lock() {
            trees.1.retain(|(known, _)| *known != handle);
        }
    }

    /// An element of a walked tree, if it still belongs to that window.
    pub fn element(
        &self,
        handle: u64,
        index: u32,
        pid: i32,
        window_id: u32,
    ) -> Result<Node, Fault> {
        let trees = self.trees.lock().map_err(|_| Fault::Stale)?;
        let (_, tree) = trees
            .1
            .iter()
            .find(|(known, _)| *known == handle)
            .ok_or(Fault::Stale)?;
        if tree.pid != pid || tree.window_id != window_id {
            return Err(Fault::Stale);
        }
        tree.elements
            .get(index as usize)
            .cloned()
            .ok_or(Fault::Invalid)
    }

    /// The element's box in screen (or window) coordinates.
    pub fn element_extents(&self, node: &Node, coords: u32) -> Result<(i32, i32, i32, i32), Fault> {
        let bus = self.bus().ok_or(Fault::Permission)?;
        if self.states(&bus, node).0 == 0 {
            return Err(Fault::Stale);
        }
        self.extents(&bus, node, coords).ok_or(Fault::Outside)
    }

    /// Presses what is under a screen point when it is a control a click
    /// means pressing (a button, a link, a check box, a tab, a menu item);
    /// `false` leaves the click to the pointer.
    pub fn press_at(&self, frame: &Node, x: f64, y: f64) -> bool {
        let Some(bus) = self.bus() else {
            return false;
        };
        let (x, y) = (x.round() as i32, y.round() as i32);
        let mut current = frame.clone();
        for _ in 0..MAX_DEPTH {
            let Some(((name, path),)) = self.call::<((String, OwnedObjectPath),)>(
                &bus,
                &current,
                COMPONENT,
                "GetAccessibleAtPoint",
                &(x, y, COORDS_SCREEN),
            ) else {
                break;
            };
            if path.as_str() == "/org/a11y/atspi/null" || name.is_empty() {
                break;
            }
            let child = Node { bus: name, path };
            if child == current {
                break;
            }
            current = child;
        }
        if &current == frame || !PRESSABLE.contains(&self.role(&bus, &current).as_str()) {
            return false;
        }
        let states = self.states(&bus, &current);
        if !states.has(STATE_ENABLED) || !states.has(STATE_SHOWING) {
            return false;
        }
        let actions = self.actions(&bus, &current);
        let Some(index) = ["click", "press", "toggle", "jump"]
            .iter()
            .find_map(|wanted| actions.iter().position(|name| name == wanted))
        else {
            return false;
        };
        self.call::<(bool,)>(&bus, &current, ACTION, "DoAction", &(index as i32,))
            .is_some_and(|(done,)| done)
    }

    /// Runs the element's own press/click action.
    pub fn press(&self, node: &Node) -> Result<bool, Fault> {
        let bus = self.bus().ok_or(Fault::Permission)?;
        let actions = self.actions(&bus, node);
        let Some(index) = ACTION_PRIORITY
            .iter()
            .find_map(|wanted| actions.iter().position(|name| name == wanted))
            .or_else(|| (!actions.is_empty()).then_some(0))
        else {
            return Ok(false);
        };
        let (done,): (bool,) = self
            .call(&bus, node, ACTION, "DoAction", &(index as i32,))
            .ok_or(Fault::Stale)?;
        Ok(done)
    }

    /// Replaces an editable element's text.
    pub fn set_text(&self, node: &Node, value: &str) -> Result<(), Fault> {
        let bus = self.bus().ok_or(Fault::Permission)?;
        let interfaces = self.interfaces(&bus, node);
        if interfaces.is_empty() {
            return Err(Fault::Stale);
        }
        if !interfaces.iter().any(|name| name == EDITABLE) {
            return Err(Fault::Unsupported);
        }
        let states = self.states(&bus, node);
        if states.has(STATE_READ_ONLY) || !states.has(STATE_ENABLED) {
            return Err(Fault::Unsupported);
        }
        let (done,): (bool,) = self
            .call(&bus, node, EDITABLE, "SetTextContents", &(value,))
            .ok_or(Fault::Event)?;
        if done {
            Ok(())
        } else {
            Err(Fault::Event)
        }
    }

    /// Inserts text at the caret of the window's focused editable element;
    /// `false` when no such element has focus.
    pub fn insert_at_caret(&self, frame: &Node, text: &str) -> bool {
        let Some(bus) = self.bus() else {
            return false;
        };
        let Some(focused) =
            self.focused_editable(&bus, frame, 0, Instant::now() + Duration::from_millis(800))
        else {
            return false;
        };
        let caret = self
            .property(&bus, &focused, TEXT, "CaretOffset")
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(-1);
        if caret < 0 {
            return false;
        }
        let length = text.chars().count() as i32;
        self.call::<(bool,)>(
            &bus,
            &focused,
            EDITABLE,
            "InsertText",
            &(caret, text, length),
        )
        .is_some_and(|(done,)| done)
    }

    fn focused_editable(
        &self,
        bus: &Connection,
        node: &Node,
        depth: usize,
        deadline: Instant,
    ) -> Option<Node> {
        if depth > MAX_DEPTH || Instant::now() > deadline {
            return None;
        }
        let states = self.states(bus, node);
        if !states.has(STATE_SHOWING) && depth > 0 {
            return None;
        }
        if states.has(STATE_FOCUSED) && states.has(STATE_EDITABLE) {
            let interfaces = self.interfaces(bus, node);
            return interfaces
                .iter()
                .any(|name| name == EDITABLE)
                .then(|| node.clone());
        }
        self.children(bus, node)
            .into_iter()
            .find_map(|child| self.focused_editable(bus, &child, depth + 1, deadline))
    }
}

/// Why an element could not be used, mapped to the shared layer's errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fault {
    Permission,
    Invalid,
    Stale,
    Outside,
    Unsupported,
    Event,
}

struct Walk<'a> {
    atspi: &'a Atspi,
    bus: &'a Connection,
    origin: Option<(f64, f64, f64, f64)>,
    deadline: Instant,
    visited: usize,
    elements: Vec<Node>,
    text: String,
    truncated: bool,
    busy: bool,
}

impl Walk<'_> {
    fn should_stop(&mut self) -> bool {
        if self.visited >= MAX_VISITED
            || self.elements.len() >= MAX_REFS
            || self.text.len() >= MAX_TEXT
            || Instant::now() > self.deadline
        {
            self.truncated = true;
        }
        self.truncated
    }

    fn element(&mut self, node: &Node, depth: usize, indent: usize, parent_name: Option<&str>) {
        if depth > MAX_DEPTH || self.should_stop() {
            return;
        }
        self.visited += 1;
        let (atspi, bus) = (self.atspi, self.bus);
        let states = atspi.states(bus, node);
        // Off-screen content (scrolled away, hidden tabs) is not reachable.
        if depth > 0 && !states.has(STATE_SHOWING) {
            return;
        }
        if states.has(STATE_BUSY) {
            self.busy = true;
        }
        let role = atspi.role(bus, node);
        let interfaces = atspi.interfaces(bus, node);
        let has = |name: &str| interfaces.iter().any(|known| known == name);
        let name = atspi
            .text_property(bus, node, "Name")
            .and_then(|name| clean(&name))
            .or_else(|| {
                atspi
                    .text_property(bus, node, "Description")
                    .and_then(|text| clean(&text))
            });
        let secure = role == "password text";
        let editable = has(EDITABLE) && states.has(STATE_EDITABLE) && !states.has(STATE_READ_ONLY);
        let value = if secure {
            None
        } else if has(TEXT) && (editable || role == "label" && name.is_none()) {
            atspi
                .call::<(String,)>(bus, node, TEXT, "GetText", &(0i32, -1i32))
                .and_then(|(text,)| clean(&text))
        } else if has(VALUE) {
            atspi
                .property(bus, node, VALUE, "CurrentValue")
                .and_then(|value| f64::try_from(value).ok())
                .map(|value| format!("{value}"))
        } else {
            None
        };
        let actions = if has(ACTION) && role != "label" {
            atspi.actions(bus, node)
        } else {
            Vec::new()
        };
        let duplicate = role == "label" && name.is_some() && name.as_deref() == parent_name;
        let interesting = !duplicate
            && (!actions.is_empty()
                || editable
                || INTERACTIVE.contains(&role.as_str())
                || (name.is_some() && LABELLED.contains(&role.as_str())));
        let mut child_indent = indent;
        let mut child_parent = parent_name.map(str::to_string);
        if interesting {
            self.elements.push(node.clone());
            let reference = self.elements.len();
            let mut line = "  ".repeat(indent);
            line.push_str(&format!("e{reference} {}", short_role(&role)));
            if secure {
                line.push_str(" (secure)");
            }
            if let Some(name) = &name {
                line.push_str(&format!(" \"{name}\""));
            }
            if states.has(STATE_CHECKABLE)
                || matches!(
                    role.as_str(),
                    "check box" | "radio button" | "toggle button" | "switch"
                )
            {
                line.push_str(if states.has(STATE_CHECKED) {
                    " checked"
                } else {
                    " unchecked"
                });
            } else if let Some(value) = value.filter(|value| Some(value) != name.as_ref()) {
                line.push_str(&format!(" value=\"{value}\""));
            }
            if states.has(STATE_FOCUSED) {
                line.push_str(" focused");
            }
            if !states.has(STATE_ENABLED) {
                line.push_str(" disabled");
            }
            if states.has(STATE_SELECTED) {
                line.push_str(" selected");
            }
            if states.has(STATE_EXPANDABLE) {
                line.push_str(if states.has(STATE_EXPANDED) {
                    " expanded"
                } else {
                    " collapsed"
                });
            }
            if editable {
                line.push_str(" editable");
            }
            if !actions.is_empty() {
                let mut short: Vec<&str> = actions
                    .iter()
                    .map(|action| match action.as_str() {
                        "click" | "press" | "activate" | "jump" | "open" => "press",
                        "toggle" => "toggle",
                        "expand or contract" | "expand" | "collapse" => "expand",
                        "showMenu" | "show menu" => "menu",
                        other => other,
                    })
                    .collect();
                short.dedup();
                line.push_str(&format!(" [{}]", short.join(",")));
            }
            if let Some((x, y, width, height)) = self.origin {
                if let Some((ex, ey, ew, eh)) = atspi.extents(bus, node, COORDS_SCREEN) {
                    let (lx, ly) = (f64::from(ex) - x, f64::from(ey) - y);
                    let inside = lx < width
                        && ly < height
                        && lx + f64::from(ew) > 0.0
                        && ly + f64::from(eh) > 0.0;
                    if inside {
                        line.push_str(&format!(" @{},{} {}x{}", lx.round(), ly.round(), ew, eh));
                    }
                }
            }
            line.push('\n');
            self.text.push_str(&line);
            child_indent = indent + 1;
            child_parent = name.clone();
        }
        for child in atspi.children(bus, node) {
            self.element(&child, depth + 1, child_indent, child_parent.as_deref());
            if self.truncated {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_and_text_read_like_the_macs() {
        assert_eq!(short_role("push button"), "pushButton");
        assert_eq!(short_role("frame"), "frame");
        assert_eq!(short_role(""), "unknown");
        assert_eq!(clean("  a\n  b \"c\" ").as_deref(), Some("a b \\\"c\\\""));
        assert_eq!(clean("   "), None);
        let long = "x".repeat(200);
        assert_eq!(clean(&long).unwrap().chars().count(), MAX_FIELD + 1);
    }

    #[test]
    fn states_read_both_words() {
        let states = States(1 << STATE_SHOWING | 1u64 << STATE_READ_ONLY);
        assert!(states.has(STATE_SHOWING));
        assert!(states.has(STATE_READ_ONLY));
        assert!(!states.has(STATE_FOCUSED));
    }

    #[test]
    fn unused_handles_are_stale() {
        let atspi = Atspi::new();
        assert_eq!(atspi.element(7, 0, 1, 1), Err(Fault::Stale));
    }
}
