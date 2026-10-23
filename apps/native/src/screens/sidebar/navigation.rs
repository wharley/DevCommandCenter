//! New task, Pull Requests and Notes (`.dcc-sidebar-navigation`): a soft
//! rounded panel with 34px rows.

use gpui::{
    BoxShadow, ClickEvent, Context, Hsla, SharedString, Window, div, point, prelude::*, px,
};

use super::{Sidebar, SidebarEvent};
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, Theme};
use crate::ui::{Icon, tabular_nums};

/// The page a navigation row stands for, when it is the one on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavPage {
    NewTask,
    PullRequests,
}

/// `--dcc-surface-edge`: the border at 80%.
pub fn surface_edge(theme: &Theme) -> Hsla {
    theme.colors.border.opacity(0.8)
}

/// `aria-current="page"`: `--dcc-shadow-card` plus a 1px inset edge.
fn current_shadow(edge: Hsla) -> Vec<BoxShadow> {
    let shadow = |alpha: f32, y: f32, blur: f32| BoxShadow {
        color: Hsla::black().opacity(alpha),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    };
    vec![
        shadow(0.025, 2.0, 3.0),
        shadow(0.04, 8.0, 24.0),
        BoxShadow {
            color: edge,
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
            inset: true,
        },
    ]
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
    event: SidebarEvent,
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
            count: 0,
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
                    .ml_auto()
                    .px(px(6.0))
                    .rounded_full()
                    .bg(colors.foreground.opacity(0.05))
                    .text_size(px(10.0))
                    .font_features(tabular_nums())
                    .child(entry.count.to_string()),
            )
        })
        .into_any_element()
}
