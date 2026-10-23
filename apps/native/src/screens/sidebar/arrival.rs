//! The arrival mark (`dcc-section-arrival` / `dcc-task-arrival`, 1.6s): a
//! row "N rodando" jumped to, or the "Em espera"/"Concluídos" header a task
//! just moved into, glows once so the move reads as "went there".

use std::collections::HashSet;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, BoxShadow, Context, Hsla, Styled, Task, point, prelude::*,
    px,
};

use super::Sidebar;
use super::state::HoldSection;
use crate::data::rail;

const LENGTH: Duration = Duration::from_millis(1600);

/// What glows now; `turn` restarts the mark on a second jump to the same row.
pub struct Flash<T> {
    pub target: T,
    pub turn: usize,
    _clear: Task<()>,
}

/// Which tasks each hold section had, to tell a move from a first load.
#[derive(Default)]
pub struct Membership {
    tasks: HashSet<String>,
    waiting: HashSet<String>,
    completed: HashSet<String>,
}

/// `ease-out` on the fading part: full until 35%, then gone at 100%.
fn strength(delta: f32) -> f32 {
    if delta <= 0.35 {
        return 1.0;
    }
    let t = ((delta - 0.35) / 0.65).clamp(0.0, 1.0);
    1.0 - (1.0 - (1.0 - t).powi(2))
}

/// Wraps a row card or a header in the mark while it plays.
pub fn mark<E: Styled + IntoElement + 'static>(
    element: E,
    key: String,
    turn: usize,
    foreground: Hsla,
) -> AnyElement {
    element
        .with_animation(
            gpui::ElementId::NamedInteger(gpui::SharedString::from(key), turn as u64),
            Animation::new(LENGTH),
            move |element, delta| {
                let alpha = strength(delta);
                element
                    .bg(foreground.opacity(0.09 * alpha))
                    .shadow(vec![BoxShadow {
                        color: foreground.opacity(0.18 * alpha),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(1.0),
                        inset: true,
                    }])
            },
        )
        .into_any_element()
}

impl Sidebar {
    fn clear_after(cx: &mut Context<Self>, clear: impl FnOnce(&mut Self) + 'static) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LENGTH).await;
            let _ = this.update(cx, |sidebar, cx| {
                clear(sidebar);
                cx.notify();
            });
        })
    }

    /// "N rodando" landed on this row.
    pub(super) fn flash_task(&mut self, task_id: String, cx: &mut Context<Self>) {
        let turn = self.task_flash.as_ref().map_or(0, |flash| flash.turn) + 1;
        let clear = Self::clear_after(cx, move |sidebar| {
            if sidebar
                .task_flash
                .as_ref()
                .is_some_and(|flash| flash.turn == turn)
            {
                sidebar.task_flash = None;
            }
        });
        self.task_flash = Some(Flash {
            target: task_id,
            turn,
            _clear: clear,
        });
    }

    /// Marks the hold section a known task just moved into.
    pub(super) fn notice_section_moves(&mut self, cx: &mut Context<Self>) {
        let split = rail::split(&self.state.data.tasks, self.state.now_ms);
        let next = Membership {
            tasks: self
                .state
                .data
                .tasks
                .iter()
                .map(|task| task.id.clone())
                .collect(),
            waiting: split.waiting.iter().map(|task| task.id.clone()).collect(),
            completed: split.completed.iter().map(|task| task.id.clone()).collect(),
        };
        let previous = self.membership.replace(next);
        let (Some(previous), Some(next)) = (previous, self.membership.as_ref()) else {
            return;
        };
        let moved = |now: &HashSet<String>, before: &HashSet<String>| {
            now.iter()
                .any(|id| previous.tasks.contains(id) && !before.contains(id))
        };
        let section = if moved(&next.waiting, &previous.waiting) {
            HoldSection::Waiting
        } else if moved(&next.completed, &previous.completed) {
            HoldSection::Completed
        } else {
            return;
        };
        let turn = self.section_flash.as_ref().map_or(0, |flash| flash.turn) + 1;
        let clear = Self::clear_after(cx, move |sidebar| {
            if sidebar
                .section_flash
                .as_ref()
                .is_some_and(|flash| flash.turn == turn)
            {
                sidebar.section_flash = None;
            }
        });
        self.section_flash = Some(Flash {
            target: section,
            turn,
            _clear: clear,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::strength;

    #[test]
    fn the_mark_holds_then_fades() {
        assert_eq!(strength(0.0), 1.0);
        assert_eq!(strength(0.35), 1.0);
        assert!(strength(0.6) < 1.0 && strength(0.6) > 0.0);
        assert_eq!(strength(1.0), 0.0);
    }
}
