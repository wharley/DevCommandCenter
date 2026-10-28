//! The balloons over the window: which are open and where (oldest first,
//! four at most), the ones that come back on entering a task, dragging one
//! (it stays inside the window), and their places kept on disk 250ms after
//! they move.

use std::time::Duration;

use gpui::{
    AnyElement, AppContext, Context, Focusable, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, SharedString, Window, div, prelude::*, px,
};

use super::balloon::{BalloonEvent, NoteBalloon};
use super::{NotesEvent, NotesWorkspace};
use crate::data::note_balloons::{
    BalloonScope, FloatingState, clamp, default_position, restore, write_positions,
};
use crate::data::notes::note_prompt;
use crate::i18n::{Args, Translate};

/// A balloon being dragged: where the pointer and the balloon started.
#[derive(Clone, Debug)]
pub struct Drag {
    pub id: String,
    pub start: Point<Pixels>,
    pub origin: (f32, f32),
}

/// `ref.current?.offsetWidth ?? 340`, `offsetHeight ?? 450`
const DEFAULT_SIZE: (f32, f32) = (340.0, 450.0);

impl NotesWorkspace {
    fn balloon_scope(&self) -> Option<BalloonScope> {
        self.scope.as_ref().map(|scope| BalloonScope {
            project_id: scope.project_id.clone(),
            workspace_id: scope.workspace_id.clone(),
        })
    }

    /// Every open balloon's place joins the saved ones.
    fn remember_places(&mut self) {
        for (id, state) in &self.floating {
            self.saved.insert(id.clone(), *state);
        }
    }

    /// The effect over `restoreOpenTaskNotes`: on the notes or the task changing.
    pub(super) fn restore_balloons(&mut self, cx: &mut Context<Self>) {
        if self.store.read(cx).loading {
            return;
        }
        let scope = self.balloon_scope();
        let changed = self.last_scope.as_ref() != Some(&scope);
        self.last_scope = Some(scope.clone());
        if changed {
            self.dismissed.clear();
            self.focus_note = None;
        }
        self.remember_places();
        let notes = self.store.read(cx).notes.clone();
        let next = restore(
            &notes,
            &self.floating,
            &self.saved,
            &self.dismissed,
            scope.as_ref(),
            changed,
            self.viewport_width,
        );
        if next != self.floating {
            self.floating = next;
            self.places_changed(cx);
        }
        self.sync_balloons(cx);
    }

    /// `show`'s place: where it was, else the next free default.
    pub(super) fn place_for(&mut self, id: &str, others: usize) -> FloatingState {
        self.remember_places();
        let mut state = self
            .saved
            .get(id)
            .copied()
            .unwrap_or_else(|| default_position(others, self.viewport_width));
        state.minimized = false;
        state
    }

    /// The balloon entities follow `floating`; each knows its place and why
    /// "Adicionar à conversa atual" may be off.
    pub(super) fn sync_balloons(&mut self, cx: &mut Context<Self>) {
        let open: Vec<String> = self.floating.iter().map(|(id, _)| id.clone()).collect();
        self.balloons.retain(|id, _| open.contains(id));
        for (id, state) in self.floating.clone() {
            let Some(note) = self
                .store
                .read(cx)
                .notes
                .iter()
                .find(|note| note.id == id)
                .cloned()
            else {
                continue;
            };
            let reason = match &self.scope {
                Some(scope)
                    if scope.workspace_id.is_some() && scope.project_id == note.project_id =>
                {
                    None
                }
                _ => Some(cx.t_with(
                    "notes.useRequiresProjectTask",
                    &Args::default().with("project", note.project_name.clone()),
                )),
            };
            if let Some((balloon, _)) = self.balloons.get(&id) {
                balloon.update(cx, |balloon, cx| {
                    balloon.state = state;
                    balloon.use_reason = reason;
                    cx.notify();
                });
            } else {
                self.pending_balloons.push((id.clone(), state, reason));
            }
        }
        cx.notify();
    }

    /// New balloons need the window for their fields.
    fn make_balloons(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (id, state, reason) in std::mem::take(&mut self.pending_balloons) {
            if self.balloons.contains_key(&id) || !self.floating.iter().any(|(open, _)| *open == id)
            {
                continue;
            }
            let Some(note) = self
                .store
                .read(cx)
                .notes
                .iter()
                .find(|note| note.id == id)
                .cloned()
            else {
                continue;
            };
            let store = self.store.clone();
            let balloon = cx.new(|cx| {
                let mut balloon = NoteBalloon::new(&note, store, state, window, cx);
                balloon.use_reason = reason;
                balloon
            });
            if self.focus_note.as_deref() == Some(id.as_str()) {
                let title = balloon.read(cx).title.clone();
                cx.defer_in(window, move |_, window, cx| {
                    window.focus(&title.read(cx).focus_handle(cx), cx);
                });
            }
            let events = cx.subscribe_in(&balloon, window, {
                let id = id.clone();
                move |workspace, _, event, window, cx| {
                    workspace.balloon_event(&id, event, window, cx)
                }
            });
            self.balloons.insert(id, (balloon, events));
        }
    }

    fn balloon_event(
        &mut self,
        id: &str,
        event: &BalloonEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            BalloonEvent::Grab(start) => {
                if let Some((_, state)) = self.floating.iter().find(|(open, _)| open == id) {
                    self.dragging = Some(Drag {
                        id: id.to_string(),
                        start: *start,
                        origin: (state.x, state.y),
                    });
                    cx.notify();
                }
            }
            BalloonEvent::Nudge(dx, dy) => {
                if let Some((_, state)) = self.floating.iter().find(|(open, _)| open == id).cloned()
                {
                    self.move_to(id, state.x + dx, state.y + dy, window, cx);
                }
            }
            BalloonEvent::Placed(placed) => {
                if let Some((_, state)) = self.floating.iter_mut().find(|(open, _)| open == id) {
                    *state = *placed;
                }
                self.places_changed(cx);
            }
            BalloonEvent::Close => self.close_balloon(id, cx),
            BalloonEvent::Delete => self.ask_delete(vec![id.to_string()], cx),
            BalloonEvent::Use => {
                if let Some(note) = self.store.read(cx).notes.iter().find(|note| note.id == id) {
                    cx.emit(NotesEvent::Use(note_prompt(note)));
                }
            }
            BalloonEvent::CreateTask => cx.emit(NotesEvent::CreateTask(id.to_string())),
        }
    }

    /// `place`: inside the window, by the balloon's painted size.
    fn move_to(&mut self, id: &str, x: f32, y: f32, window: &mut Window, cx: &mut Context<Self>) {
        let size = self
            .balloons
            .get(id)
            .and_then(|(balloon, _)| balloon.read(cx).size.get())
            .unwrap_or(DEFAULT_SIZE);
        let viewport = window.viewport_size();
        let (x, y) = clamp(x, y, size, (viewport.width.into(), viewport.height.into()));
        let Some((_, state)) = self.floating.iter_mut().find(|(open, _)| open == id) else {
            return;
        };
        if (state.x, state.y) == (x, y) {
            return;
        }
        state.x = x;
        state.y = y;
        let state = *state;
        if let Some((balloon, _)) = self.balloons.get(id) {
            balloon.update(cx, |balloon, cx| {
                balloon.state = state;
                cx.notify();
            });
        }
        self.places_changed(cx);
    }

    /// The places go to disk 250ms after the last move.
    pub(super) fn places_changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        let Some(path) = self.positions_path.clone() else {
            return;
        };
        self.persist = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let _ = this.update(cx, |workspace, cx| {
                let store = workspace.store.read(cx);
                if store.loading || store.error.is_some() {
                    return;
                }
                let alive: std::collections::HashSet<String> =
                    store.notes.iter().map(|note| note.id.clone()).collect();
                workspace.remember_places();
                workspace.saved.retain(|id, _| alive.contains(id));
                write_positions(&path, &workspace.saved);
            });
        }));
    }

    /// The balloons where they are, over a layer that follows a drag.
    pub(super) fn balloons_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.viewport_width = window.viewport_size().width.into();
        self.make_balloons(window, cx);
        // A smaller window pulls them back in.
        let ids: Vec<(String, FloatingState)> = self.floating.clone();
        for (id, state) in ids {
            self.move_to(&id, state.x, state.y, window, cx);
        }
        let mut layer: Vec<AnyElement> = self
            .floating
            .iter()
            .filter_map(|(id, state)| {
                let (balloon, _) = self.balloons.get(id)?;
                Some(
                    div()
                        .absolute()
                        .left(px(state.x))
                        .top(px(state.y))
                        .child(balloon.clone())
                        .into_any_element(),
                )
            })
            .collect();
        if self.dragging.is_some() {
            layer.push(
                div()
                    .id(SharedString::from("note-drag-layer"))
                    .debug_selector(|| "note-drag-layer".into())
                    .absolute()
                    .inset_0()
                    .cursor_grabbing()
                    .on_mouse_move(
                        cx.listener(|workspace, event: &MouseMoveEvent, window, cx| {
                            let Some(drag) = workspace.dragging.clone() else {
                                return;
                            };
                            let x = drag.origin.0 + f32::from(event.position.x - drag.start.x);
                            let y = drag.origin.1 + f32::from(event.position.y - drag.start.y);
                            workspace.move_to(&drag.id, x, y, window, cx);
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|workspace, _: &MouseUpEvent, _, cx| {
                            workspace.dragging = None;
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
            );
        }
        layer
    }
}
