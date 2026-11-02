//! The tools row at the bottom (`.dcc-sidebar-tools`): usage, skills,
//! feedback, settings and help, then `AppUpdateButton` while an update is
//! on offer.

use gpui::{AnyElement, Context, ElementId, SharedString, div, prelude::*, px};

use super::navigation::surface_edge;
use super::{Sidebar, SidebarEvent};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::tooltip::Side;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip};

/// Footer tools, in the desktop's order, with the icon each one uses.
pub const TOOLS: [(&str, &str, SidebarEvent); 5] = [
    ("tool-usage", "chart-column", SidebarEvent::OpenUsage),
    ("tool-skills", "sparkles", SidebarEvent::OpenSkills),
    (
        "tool-feedback",
        "message-square-more",
        SidebarEvent::OpenFeedback,
    ),
    ("tool-settings", "settings-2", SidebarEvent::OpenSettings),
    ("tool-help", "circle-question-mark", SidebarEvent::OpenHelp),
];

/// What each tool's tooltip says ("Skills" is not translated).
fn tool_label(id: &str, cx: &Context<Sidebar>) -> SharedString {
    match id {
        "tool-usage" => cx.t("usage.title"),
        "tool-skills" => "Skills".into(),
        "tool-feedback" => cx.t("feedback.title"),
        "tool-settings" => cx.t("sidebar.openSettings"),
        _ => cx.t("sidebar.openHelp"),
    }
}

/// A tool with its tooltip: above it in the footer, beside it in the rail.
pub fn tool_button(
    id: &'static str,
    icon: &'static str,
    event: SidebarEvent,
    size: ButtonSize,
    side: Side,
    sidebar: &Sidebar,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    // The skills found for the open task, on the Skills tool.
    let badge = (id == "tool-skills" && sidebar.skill_count > 0).then(|| {
        let ring = cx.theme().colors.sidebar;
        div()
            .absolute()
            .right(px(-4.0))
            .top(px(-4.0))
            .flex()
            .items_center()
            .justify_center()
            .h(px(16.0))
            .min_w(px(16.0))
            .px(px(4.0))
            .rounded_full()
            .border_1()
            .border_color(ring)
            .bg(crate::theme::tailwind::EMERALD_500.hsla())
            .text_size(px(9.0))
            .line_height(px(9.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(gpui::white())
            .child(sidebar.skill_count.to_string())
    });
    let button = Button::new(ElementId::from(id))
        .variant(ButtonVariant::Ghost)
        .size(size)
        .muted()
        .child(Icon::new(icon).stroke(1.85))
        .children(badge)
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(event.clone())));
    Tooltip::text(
        SharedString::from(format!("tip-{id}")),
        button,
        tool_label(id, cx),
    )
    .side(side)
    .into_any_element()
}

/// `AppUpdateButton`: sky, the download (a spinner while installing), the
/// versions in its tip; a click installs and restarts.
pub fn update_button(size: f32, side: Side, cx: &mut Context<Sidebar>) -> Option<AnyElement> {
    let updater = crate::app::updater::updater(cx)?;
    let (current, update, installing) = {
        let state = updater.read(cx);
        let update = state.available()?.clone();
        let installing = matches!(state.stage, crate::app::updater::Stage::Installing(_));
        (state.current.clone(), update, installing)
    };
    let (sky, hover) = (
        crate::theme::tailwind::SKY_500.hsla(),
        crate::theme::tailwind::SKY_400.hsla(),
    );
    let button = div()
        .id("sidebar-update")
        .debug_selector(|| "sidebar-update".into())
        .flex()
        .flex_shrink_0()
        .size(px(size))
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .bg(sky)
        .text_color(gpui::white())
        .when(!installing, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(move |_, _, cx| {
                    updater.update(cx, |updater, cx| updater.install(cx));
                })
        })
        .child(if installing {
            crate::ui::spinner("sidebar-update-busy", 16.0).into_any_element()
        } else {
            Icon::new("download")
                .size(16.0)
                .stroke(2.1)
                .color(gpui::white())
                .into_any_element()
        });
    let label = if installing {
        cx.t("updater.installing").to_string()
    } else {
        format!(
            "{} · {current} → {}",
            cx.t("updater.update"),
            update.version
        )
    };
    Some(
        Tooltip::text("tip-sidebar-update", button, label)
            .side(side)
            .into_any_element(),
    )
}

pub fn render(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let mut tools = Vec::new();
    for (id, icon, event) in TOOLS {
        tools.push(tool_button(
            id,
            icon,
            event,
            ButtonSize::Icon,
            Side::Top,
            sidebar,
            cx,
        ));
    }
    tools.extend(update_button(32.0, Side::Top, cx));
    let edge = surface_edge(cx.theme());
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_start()
        .gap(px(4.0))
        .mx(px(10.0))
        .px(px(4.0))
        .pt(px(8.0))
        .pb(px(12.0))
        .border_t_1()
        .border_color(edge)
        .children(tools)
}
