//! New task, Pull Requests and Notes (`.dcc-sidebar-navigation`): a soft
//! rounded panel with 34px rows.

use gpui::{
    BoxShadow, ClickEvent, Context, Hsla, SharedString, Window, div, point, prelude::*, px,
};

use super::{Sidebar, SidebarEvent};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, tabular_nums};

/// The page a navigation row stands for, when it is the one on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavPage {
    NewTask,
    PullRequests,
}

pub use crate::ui::surface_edge;

/// `aria-current="page"`: `--dcc-shadow-card` plus a 1px inset edge.
fn current_shadow(edge: Hsla) -> Vec<BoxShadow> {
    crate::ui::card_shadow()
        .into_iter()
        .chain([BoxShadow {
            color: edge,
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
            inset: true,
        }])
        .collect()
}

struct Entry {
    id: &'static str,
    icon: &'static str,
    stroke: f32,
    icon_color: Option<Hsla>,
    label: SharedString,
    /// New task reads in full foreground; the others are muted until hovered.
    primary: bool,
    current: bool,
    count: usize,
    /// The count needs the person (amber).
    urgent: bool,
    event: SidebarEvent,
}

impl Sidebar {
    /// The reviewed pull requests with news, beside "Pull Requests".
    pub fn set_pr_news(&mut self, count: usize, cx: &mut Context<Self>) {
        if self.pr_news != count {
            self.pr_news = count;
            cx.notify();
        }
    }

    pub(super) fn pr_news(&self) -> usize {
        self.pr_news
    }
}

pub fn render(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let theme = cx.theme();
    let current = sidebar.current_page;
    let entries = [
        Entry {
            id: "nav-new-task",
            icon: "plus",
            stroke: 1.9,
            icon_color: Some(theme.colors.accent_calm),
            label: cx.t("sidebar.newWorkspace"),
            primary: true,
            current: current == Some(NavPage::NewTask),
            count: 0,
            urgent: false,
            event: SidebarEvent::NewTask,
        },
        Entry {
            id: "nav-pull-requests",
            icon: "git-pull-request",
            stroke: 1.9,
            icon_color: None,
            label: cx.t("sidebar.pullRequests"),
            primary: false,
            current: current == Some(NavPage::PullRequests),
            count: sidebar.pr_news,
            urgent: true,
            event: SidebarEvent::OpenPullRequests,
        },
        Entry {
            id: "nav-notes",
            icon: "sticky-note",
            stroke: 1.8,
            icon_color: Some(theme.colors.accent_idea),
            label: cx.t("notes.headingShort"),
            primary: false,
            current: false,
            count: sidebar.state.data.open_notes,
            urgent: false,
            event: SidebarEvent::OpenNotes,
        },
    ];
    let mut rows = Vec::new();
    for entry in entries {
        rows.push(row(entry, cx));
    }
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(4.0))
        .mt(px(4.0))
        .mx(px(8.0))
        .mb(px(12.0))
        .p(px(5.0))
        .border_1()
        .border_color(surface_edge(theme))
        .rounded(theme.radius.surface)
        .bg(theme.colors.background.opacity(0.45))
        .children(rows)
}

fn row(entry: Entry, cx: &mut Context<Sidebar>) -> gpui::AnyElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let hover = colors.accent.opacity(0.5);
    let text = if entry.primary || entry.current {
        colors.foreground
    } else {
        colors.muted_foreground
    };
    let hover_text = colors.foreground;
    let (count_bg, count_fg) = if entry.urgent {
        let amber = if theme.appearance == crate::theme::Appearance::Dark {
            crate::theme::tailwind::AMBER_400.hsla()
        } else {
            crate::theme::tailwind::AMBER_600.hsla()
        };
        (
            crate::theme::tailwind::AMBER_500.hsla().opacity(0.14),
            amber,
        )
    } else {
        (colors.foreground.opacity(0.05), text)
    };
    let count_id = format!("{}-count", entry.id);
    let event = entry.event;
    let icon = Icon::new(entry.icon).stroke(entry.stroke);
    let icon = match entry.icon_color {
        Some(color) => icon.color(color),
        None => icon,
    };
    div()
        .id(entry.id)
        .flex()
        .items_center()
        .gap(px(10.0))
        .h(px(34.0))
        .w_full()
        .px(px(10.0))
        .rounded(theme.radius.control)
        .cursor_pointer()
        .text_color(text)
        .when(entry.current, |row| {
            row.bg(colors.popover)
                .shadow(current_shadow(surface_edge(theme)))
        })
        .when(!entry.current, |row| {
            row.hover(move |style| style.bg(hover).text_color(hover_text))
        })
        .on_click(cx.listener(move |_, _: &ClickEvent, _: &mut Window, cx| cx.emit(event.clone())))
        .child(icon)
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(entry.label),
        )
        .when(entry.count > 0, |row| {
            row.child(
                div()
                    .debug_selector(move || count_id)
                    .ml_auto()
                    .px(px(6.0))
                    .rounded_full()
                    .bg(count_bg)
                    .text_color(count_fg)
                    .text_size(px(10.0))
                    .font_features(tabular_nums())
                    .child(entry.count.to_string()),
            )
        })
        .into_any_element()
}
