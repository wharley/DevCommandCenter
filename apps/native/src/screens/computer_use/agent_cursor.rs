//! The agent's cursor on the screen: while a task holds computer use, a
//! small window the person clicks through shows the provider's robot and the
//! task's name at the point of the agent's last action, and walks there when
//! the agent acts somewhere else. One per task; it goes when the task's
//! grant ends ("Parar", turned off, or expired).

use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    App, AppContext, Bounds, Context, FontWeight, IntoElement, Render, Task, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, point,
    prelude::*, px, size,
};

use super::cursor_window;
use crate::agent::{AgentMascot, RobotState};
use crate::data::browser_opens::OpenRequest;
use crate::data::computer_use::{Activity, ComputerEvent};
use crate::data::desktop_api::DesktopApi;
use crate::screens::browser::approval::TitleOf;
use crate::theme::ActiveTheme;

const WIDTH: f32 = 220.0;
const HEIGHT: f32 = 44.0;
/// The walk to a new point: steps of one frame.
const GLIDE_STEPS: u32 = 12;
const GLIDE_STEP: Duration = Duration::from_millis(16);
/// A little past the grant's end before it is read again.
const EXPIRY_SLACK: Duration = Duration::from_millis(500);

/// What one cursor shows.
pub struct CursorView {
    provider: String,
    label: String,
    working: bool,
    moving: bool,
}

impl Render for CursorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let state = if self.working {
            RobotState::Working
        } else {
            RobotState::Idle
        };
        div()
            .size_full()
            .child(
                // The tip sits on the point the agent acted on.
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(10.0))
                    .rounded_full()
                    .border_2()
                    .border_color(colors.background)
                    .bg(colors.primary),
            )
            .child(
                div()
                    .absolute()
                    .top(px(10.0))
                    .left(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .max_w(px(WIDTH - 12.0))
                    .rounded_full()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.background.opacity(0.95))
                    .pl(px(3.0))
                    .pr(px(10.0))
                    .py(px(3.0))
                    .shadow_md()
                    .child(AgentMascot::new(Some(&self.provider), None, state).moving(self.moving))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .truncate()
                            .child(self.label.clone()),
                    ),
            )
    }
}

struct Cursor {
    window: WindowHandle<CursorView>,
    at: (f64, f64),
    _glide: Option<Task<()>>,
    _expiry: Option<Task<()>>,
}

pub struct AgentCursors {
    api: Option<DesktopApi>,
    title_of: TitleOf,
    cursors: HashMap<String, Cursor>,
}

impl AgentCursors {
    pub fn new(api: Option<DesktopApi>, title_of: TitleOf) -> Self {
        Self {
            api,
            title_of,
            cursors: HashMap::new(),
        }
    }

    /// What the desktop said about computer use.
    pub fn apply(&mut self, event: &ComputerEvent, cx: &mut Context<Self>) {
        match event {
            ComputerEvent::Activity(activity) => self.follow(activity, cx),
            ComputerEvent::PreviewCleared(session) => self.close(session, cx),
            _ => {}
        }
    }

    fn follow(&mut self, activity: &Activity, cx: &mut Context<Self>) {
        let working = activity.phase == "started";
        let Some(at) = activity.point() else {
            if let Some(cursor) = self.cursors.get(&activity.session_id) {
                let _ = cursor.window.update(cx, |view, _, cx| {
                    view.working = working;
                    cx.notify();
                });
            }
            return;
        };
        let session = activity.session_id.clone();
        if !self.cursors.contains_key(&session) {
            let label = self.label(activity, cx);
            let Some(window) = open(&activity.provider_id, label, at, cx) else {
                return;
            };
            self.cursors.insert(
                session.clone(),
                Cursor {
                    window,
                    at,
                    _glide: None,
                    _expiry: None,
                },
            );
        }
        let Some(cursor) = self.cursors.get_mut(&session) else {
            return;
        };
        let _ = cursor.window.update(cx, |view, _, cx| {
            view.working = working;
            cx.notify();
        });
        if cursor.at != at {
            let from = cursor.at;
            cursor.at = at;
            cursor._glide = Some(glide(cursor.window, from, at, cx));
        }
        if activity.phase != "started" {
            let expiry = watch_grant(self.api.clone(), session.clone(), cx);
            if let Some(cursor) = self.cursors.get_mut(&session) {
                cursor._expiry = Some(expiry);
            }
        }
    }

    /// The task's name, else the provider's.
    fn label(&self, activity: &Activity, cx: &App) -> String {
        let request = OpenRequest {
            workspace_id: activity.workspace_id.clone(),
            session_id: activity.session_id.clone(),
            ..Default::default()
        };
        (self.title_of)(&request, cx).unwrap_or_else(|| provider_name(&activity.provider_id))
    }

    fn close(&mut self, session: &str, cx: &mut Context<Self>) {
        if let Some(cursor) = self.cursors.remove(session) {
            let _ = cursor
                .window
                .update(cx, |_, window, _| window.remove_window());
        }
    }
}

/// Closes the cursor once the grant has ended.
fn watch_grant(
    api: Option<DesktopApi>,
    session: String,
    cx: &mut Context<AgentCursors>,
) -> Task<()> {
    cx.spawn(async move |this, cx| {
        let Some(api) = api else { return };
        loop {
            let asked = session.clone();
            let reader = api.clone();
            let grant = cx
                .background_executor()
                .spawn(async move { reader.computer_grant(&asked) })
                .await;
            match grant {
                Ok(grant) if grant.armed => {
                    cx.background_executor()
                        .timer(Duration::from_millis(grant.remaining_ms) + EXPIRY_SLACK)
                        .await;
                }
                Ok(_) => {
                    let _ = this.update(cx, |cursors, cx| cursors.close(&session, cx));
                    return;
                }
                Err(_) => return,
            }
        }
    })
}

fn provider_name(provider_id: &str) -> String {
    let mut chars = provider_id.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "DCC".to_string(),
    }
}

fn open(
    provider: &str,
    label: String,
    at: (f64, f64),
    cx: &mut App,
) -> Option<WindowHandle<CursorView>> {
    let bounds = Bounds {
        origin: point(px(at.0 as f32), px(at.1 as f32)),
        size: size(px(WIDTH), px(HEIGHT)),
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("com.devcommandcenter.app".into()),
        ..Default::default()
    };
    let provider = provider.to_string();
    cx.open_window(options, move |window, cx| {
        cursor_window::make_overlay(window);
        cursor_window::place(window, at.0, at.1);
        cx.new(|_| CursorView {
            provider,
            label,
            working: true,
            moving: false,
        })
    })
    .map_err(|error| eprintln!("[dcc-native] could not open the agent's cursor: {error:#}"))
    .ok()
}

/// Walks the cursor from `from` to `to` in a few frames, the robot stepping.
fn glide(
    window: WindowHandle<CursorView>,
    from: (f64, f64),
    to: (f64, f64),
    cx: &mut Context<AgentCursors>,
) -> Task<()> {
    cx.spawn(async move |_, cx| {
        for step in 1..=GLIDE_STEPS {
            let t = ease(step as f64 / GLIDE_STEPS as f64);
            let x = from.0 + (to.0 - from.0) * t;
            let y = from.1 + (to.1 - from.1) * t;
            let moving = step < GLIDE_STEPS && (step / 3) % 2 == 0;
            let placed = window.update(cx, |view, window, cx| {
                cursor_window::place(window, x, y);
                if view.moving != moving {
                    view.moving = moving;
                    cx.notify();
                }
            });
            if placed.is_err() {
                return;
            }
            cx.background_executor().timer(GLIDE_STEP).await;
        }
    })
}

/// Ease-out: fast at first, settling on the point.
fn ease(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_settles_on_the_point_and_names_the_provider_without_a_task() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!(ease(0.5) > 0.5);
        assert_eq!(provider_name("codex"), "Codex");
        assert_eq!(provider_name(""), "DCC");
    }
}
