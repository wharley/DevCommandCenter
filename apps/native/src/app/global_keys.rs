//! Global keys while this app hosts the backend (the desktop registers them
//! through `tauri-plugin-global-shortcut`, on the same `global-hotkey`). On
//! macOS they are registered on the main thread: requests from any thread
//! wait here for the UI's loop. A press of the quick composer's key toggles
//! it through its stream, as the desktop's key does; the Appshots key
//! captures the window in front (`data::appshot_key`).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::App;

pub enum Request {
    Register(HotKey, std::sync::mpsc::Sender<Result<(), String>>),
    Unregister(HotKey, std::sync::mpsc::Sender<Result<(), String>>),
}

type Channel = (
    UnboundedSender<Request>,
    Mutex<Option<UnboundedReceiver<Request>>>,
);

fn channel() -> &'static Channel {
    static CHANNEL: OnceLock<Channel> = OnceLock::new();
    CHANNEL.get_or_init(|| {
        let (send, receive) = unbounded();
        (send, Mutex::new(Some(receive)))
    })
}

/// The quick composer's key, as `global-hotkey` numbers it (0: none).
static QUICK_COMPOSER: AtomicU32 = AtomicU32::new(0);

pub fn set_quick_composer(key: Option<&HotKey>) {
    QUICK_COMPOSER.store(key.map_or(0, HotKey::id), Ordering::SeqCst);
}

/// The Appshots key, numbered the same way (0: none).
static APPSHOTS: AtomicU32 = AtomicU32::new(0);

pub fn set_appshots(key: Option<&HotKey>) {
    APPSHOTS.store(key.map_or(0, HotKey::id), Ordering::SeqCst);
}

/// Asks the main thread; blocks until it answered.
pub fn request(
    make: impl FnOnce(std::sync::mpsc::Sender<Result<(), String>>) -> Request,
) -> Result<(), String> {
    let (reply, answer) = std::sync::mpsc::channel();
    channel()
        .0
        .unbounded_send(make(reply))
        .map_err(|_| "unavailable".to_string())?;
    answer.recv().map_err(|_| "unavailable".to_string())?
}

/// Starts answering the requests on the main thread.
pub fn serve(cx: &mut App) {
    let Some(mut requests) = channel().1.lock().ok().and_then(|mut slot| slot.take()) else {
        return;
    };
    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(error) => {
            eprintln!("[dcc-native] global keys unavailable: {error}");
            return;
        }
    };
    GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
        if event.state != HotKeyState::Pressed || event.id == 0 {
            return;
        }
        if event.id == QUICK_COMPOSER.load(Ordering::SeqCst) {
            let _ = dcc_server::http_api::native_quick_composer().toggle();
        } else if event.id == APPSHOTS.load(Ordering::SeqCst) {
            crate::data::appshot_key::capture_front();
        }
    }));
    cx.spawn(async move |_| {
        while let Some(request) = requests.next().await {
            match request {
                Request::Register(key, reply) => {
                    let _ = reply.send(manager.register(key).map_err(|error| error.to_string()));
                }
                Request::Unregister(key, reply) => {
                    let _ = reply.send(manager.unregister(key).map_err(|error| error.to_string()));
                }
            }
        }
    })
    .detach();
}
