//! "Salas" at the top of the team's column (`docs/PLANO_SALA_POR_PROJETO`):
//! Geral, each project's room, and "Nova sala", which lists the projects
//! with no room yet; a pick makes its room and opens it.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::AgentPage;
use super::editor_projects::project_options;
use crate::data::agent_rooms;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::Icon;

impl AgentPage {
    /// Opens `project`'s room (none: the Geral room).
    pub(crate) fn open_room_of(&mut self, project: Option<String>, cx: &mut Context<Self>) {
        self.room_picker = false;
        self.open_room(None, cx);
        self.room_project = project;
        cx.notify();
    }

    /// "Nova sala": `project` gets its room, kept for next time.
    fn add_room(&mut self, project: String, cx: &mut Context<Self>) {
        if !self.rooms.contains(&project) {
            self.rooms.push(project.clone());
            agent_rooms::save(&self.rooms);
        }
        self.open_room_of(Some(project), cx);
    }

    /// The projects as `(id, name)`.
    fn project_names(&self, cx: &gpui::App) -> Vec<(String, String)> {
        project_options(self.sidebar.read(cx).projects())
    }

    fn room_line(
        &self,
        key: String,
        icon: &'static str,
        label: SharedString,
        project: Option<String>,
        unread: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let current = self.room_open
            && self.editor.is_none()
            && self.room_project.as_deref() == project.as_deref();
        let dot_key = key.clone();
        div()
            .id(SharedString::from(key.clone()))
            .debug_selector(move || key.clone())
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .h(px(34.0))
            .rounded(cx.theme().radius.control)
            .cursor_pointer()
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(colors.foreground)
            .when(current, |row| row.bg(colors.accent))
            .when(!current, |row| {
                row.hover(move |style| style.bg(colors.accent.opacity(0.5)))
            })
            .on_click(cx.listener(move |page, _, _, cx| page.open_room_of(project.clone(), cx)))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .size(px(30.0))
                    .items_center()
                    .justify_center()
                    .child(Icon::new(icon).size(16.0)),
            )
            .child(div().flex_1().min_w_0().truncate().child(label))
            // What is new in the room, as the room's own bar counts it.
            .when(unread > 0, |row| {
                row.child(
                    div()
                        .debug_selector(move || format!("{dot_key}-unread"))
                        .flex_none()
                        .min_w(px(18.0))
                        .h(px(18.0))
                        .px(px(5.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(crate::theme::tailwind::SKY_500.hsla())
                        .text_size(px(10.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(gpui::white())
                        .child(unread.to_string()),
                )
            })
            .into_any_element()
    }

    /// The column's rooms: Geral, the projects' rooms, "Nova sala".
    pub(super) fn rooms_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let projects = self.project_names(cx);
        let talked = agent_rooms::talked(self.sidebar.read(cx).agents());
        let listed = agent_rooms::rooms(&self.rooms, talked, &projects);
        let unread = self
            .room
            .as_ref()
            .map(|(room, _)| room.read(cx).unread_by_room(cx))
            .unwrap_or_default();
        let mut rows = vec![
            div()
                .px(px(8.0))
                .pb(px(4.0))
                .child(AgentPage::section_title(cx.t("agents.rooms.title"), cx))
                .into_any_element(),
            self.room_line(
                "team-room-row".into(),
                "messages-square",
                cx.t("agents.rooms.geral"),
                None,
                unread.get(&None).copied().unwrap_or(0),
                cx,
            ),
        ];
        for (index, (id, name)) in listed.iter().enumerate() {
            rows.push(self.room_line(
                format!("team-room-{index}"),
                "folder",
                name.clone().into(),
                Some(id.clone()),
                unread.get(&Some(id.clone())).copied().unwrap_or(0),
                cx,
            ));
        }
        rows.push(
            div()
                .id("team-new-room")
                .debug_selector(|| "team-new-room".into())
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(8.0))
                .h(px(32.0))
                .rounded(cx.theme().radius.control)
                .cursor_pointer()
                .text_size(px(12.5))
                .text_color(colors.muted_foreground)
                .hover(move |style| {
                    style
                        .bg(colors.accent.opacity(0.5))
                        .text_color(colors.foreground)
                })
                .on_click(cx.listener(|page, _, _, cx| {
                    page.room_picker = !page.room_picker;
                    cx.notify();
                }))
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .size(px(30.0))
                        .items_center()
                        .justify_center()
                        .child(Icon::new("plus").size(14.0)),
                )
                .child(cx.t("agents.rooms.new"))
                .into_any_element(),
        );
        if self.room_picker {
            let open: Vec<(String, String)> = projects
                .into_iter()
                .filter(|(id, _)| !listed.iter().any(|(kept, _)| kept == id))
                .collect();
            let mut picker = div()
                .debug_selector(|| "team-room-picker".into())
                .flex()
                .flex_col()
                .gap(px(2.0))
                .mx(px(8.0))
                .p(px(6.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.card)
                .child(
                    div()
                        .px(px(6.0))
                        .py(px(4.0))
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(if open.is_empty() {
                            cx.t("agents.rooms.none")
                        } else {
                            cx.t("agents.rooms.pick")
                        }),
                );
            for (index, (id, name)) in open.into_iter().enumerate() {
                picker = picker.child(
                    div()
                        .id(SharedString::from(format!("team-room-pick-{index}")))
                        .debug_selector(move || format!("team-room-pick-{index}"))
                        .px(px(6.0))
                        .py(px(5.0))
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .truncate()
                        .text_size(px(12.5))
                        .text_color(colors.foreground)
                        .hover(move |style| style.bg(colors.accent.opacity(0.6)))
                        .on_click(cx.listener(move |page, _, _, cx| page.add_room(id.clone(), cx)))
                        .child(name),
                );
            }
            rows.push(picker.into_any_element());
        }
        rows
    }
}
