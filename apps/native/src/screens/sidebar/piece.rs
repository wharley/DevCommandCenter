//! Parts of the sidebar drawn from their last frame until the sidebar
//! changes: a task's row, a child task's row, the running pulse and an
//! agent's face. In GPUI a view that changes re-renders the views around it,
//! so a spinner or a pulse in the sidebar itself laid out every row again on
//! each of its frames; in its own piece it repaints only that piece, and the
//! other rows are reused (the Monocode's session marks do the same).
//!
//! A piece reads the sidebar as it draws and redraws whenever the sidebar is
//! notified. The shell draws the sidebar itself uncached (`shell::cached`): a
//! cached view that redraws ignores the caches inside it. While the Browser's page is up they are drawn fresh, like the
//! shell's cached views (`ui::occlusion`).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use gpui::{
    AnyElement, App, Context, Entity, IntoElement, Render, StyleRefinement, Subscription,
    WeakEntity, Window, div, prelude::*,
};

use super::{Sidebar, agents, child_task_row, row, tasks_header};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Piece {
    Row {
        task_id: String,
        compact: bool,
    },
    Child {
        task_id: String,
        child_id: String,
        last: bool,
    },
    RunningPulse,
    AgentFace {
        agent_id: String,
        /// The collapsed rail's (20px) rather than the list's (22px).
        rail: bool,
    },
}

impl Piece {
    fn render(&self, sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> AnyElement {
        match self {
            Piece::Row { task_id, compact } => row::render(sidebar, task_id, *compact, cx),
            Piece::Child {
                task_id,
                child_id,
                last,
            } => child_task_row::render(sidebar, task_id, child_id, *last, cx),
            Piece::RunningPulse => tasks_header::running_pulse(cx),
            Piece::AgentFace { agent_id, rail } => agents::face(sidebar, agent_id, *rail),
        }
    }
}

pub struct PieceView {
    sidebar: WeakEntity<Sidebar>,
    piece: Piece,
    _sidebar: Subscription,
    #[cfg(test)]
    renders: usize,
}

impl Render for PieceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        let piece = self.piece.clone();
        self.sidebar
            .update(cx, |sidebar, cx| piece.render(sidebar, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }
}

#[derive(Default)]
pub struct Pieces(RefCell<HashMap<Piece, Entity<PieceView>>>);

impl Pieces {
    /// `piece` laid out by `style` (a cached view is not measured).
    pub fn element(
        &self,
        piece: Piece,
        style: StyleRefinement,
        cx: &mut Context<Sidebar>,
    ) -> AnyElement {
        let known = self.0.borrow().get(&piece).cloned();
        let created = known.is_none();
        let view = known.unwrap_or_else(|| {
            let sidebar = cx.entity();
            let view = cx.new(|cx| PieceView {
                sidebar: sidebar.downgrade(),
                piece: piece.clone(),
                _sidebar: cx.observe(&sidebar, |_, _, cx| cx.notify()),
                #[cfg(test)]
                renders: 0,
            });
            self.0.borrow_mut().insert(piece, view.clone());
            view
        });
        // Its first frame draws it fresh: a view made during a frame is left
        // out of what its own render read, so reused from that frame on the
        // window would stop redrawing when it changes.
        if created || crate::ui::occlusion::page_open(cx) {
            view.into_any_element()
        } else {
            view.cached(style).into_any_element()
        }
    }

    /// Drops the rows no longer in the list.
    pub fn retain_rows(&self, listed: &HashSet<Piece>) {
        self.0.borrow_mut().retain(|piece, _| match piece {
            Piece::Row { .. } | Piece::Child { .. } => listed.contains(piece),
            Piece::RunningPulse | Piece::AgentFace { .. } => true,
        });
    }

    /// Redraws only these tasks' rows (a mascot step, a seconds tick).
    pub fn notify_rows(&self, task_ids: &HashSet<String>, cx: &mut App) {
        let views: Vec<Entity<PieceView>> = self
            .0
            .borrow()
            .iter()
            .filter(|(piece, _)| {
                matches!(piece, Piece::Row { task_id, .. } if task_ids.contains(task_id))
            })
            .map(|(_, view)| view.clone())
            .collect();
        for view in views {
            view.update(cx, |_, cx| cx.notify());
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, WindowAppearance};

    use super::*;
    use crate::data::loader::SidebarData;
    use crate::data::tasks::TaskSummary;
    use crate::theme::{Theme, ThemePreference};

    fn renders(sidebar: &Entity<Sidebar>, id: &str, cx: &mut gpui::VisualTestContext) -> usize {
        sidebar.read_with(cx, |sidebar, cx| {
            let pieces = sidebar.pieces.0.borrow();
            let piece = pieces
                .iter()
                .find(|(piece, _)| matches!(piece, Piece::Row { task_id, .. } if task_id == id));
            piece.map_or(0, |(_, view)| view.read(cx).renders)
        })
    }

    #[gpui::test]
    fn a_row_redraws_alone_and_all_redraw_when_the_sidebar_changes(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
            crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        });
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("seen.json");
        let (sidebar, cx) = cx.add_window_view(|_, cx| Sidebar::new(false, false, false, seen, cx));
        let data = |ids: &[&str]| SidebarData {
            tasks: ids.iter().map(|id| TaskSummary::test(id)).collect(),
            ..SidebarData::default()
        };
        cx.simulate_resize(gpui::size(gpui::px(320.0), gpui::px(900.0)));
        sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(data(&["w-a", "w-b"]), cx)
        });
        cx.run_until_parked();
        let step_a = |sidebar: &Entity<Sidebar>, cx: &mut gpui::VisualTestContext| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar
                    .pieces
                    .notify_rows(&HashSet::from(["w-a".to_string()]), cx)
            });
            cx.run_until_parked();
        };
        // Another row animates right after they appear (each settles its cache).
        step_a(&sidebar, cx);
        let (a, b) = (renders(&sidebar, "w-a", cx), renders(&sidebar, "w-b", cx));
        assert!(a > 0 && b > 0, "both rows drawn");

        // A step of one row's mascot: the other row is reused.
        step_a(&sidebar, cx);
        assert_eq!(renders(&sidebar, "w-a", cx), a + 1);
        assert_eq!(renders(&sidebar, "w-b", cx), b);

        // The sidebar changed: every row reads it again, reused ones included.
        sidebar.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        assert_eq!(renders(&sidebar, "w-a", cx), a + 2);
        assert_eq!(renders(&sidebar, "w-b", cx), b + 1);

        // A task gone from the list takes its row's piece along.
        sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(data(&["w-a"]), cx)
        });
        cx.run_until_parked();
        assert_eq!(renders(&sidebar, "w-b", cx), 0);
    }
}
