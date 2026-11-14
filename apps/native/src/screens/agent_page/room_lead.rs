//! "Responde por padrão" in the room's head: the agent that answers a
//! message nobody is named in, when nothing else says who (a review's
//! words go to the Reviewer, and whoever was just talking goes on). Set per
//! room — Geral and each project's — and kept by the desktop, so the phone
//! routes the same way. None set: DCC's own pick, as before.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::popover::{Popover, PopoverAlign};
use crate::ui::{Icon, Toaster};

/// The room's lead as the app knows it.
#[derive(Default)]
pub(crate) struct RoomLead {
    /// The agent set for the room on screen.
    pub(crate) agent_id: Option<String>,
    /// The room it was read for (`Some(None)`: Geral); none: not read yet.
    read_for: Option<Option<String>>,
    pub(crate) open: bool,
}

impl TeamRoom {
    /// While the room draws: the lead of the room on screen, read once per
    /// room from DCC's database (a test sets its own).
    pub(super) fn sync_lead(&mut self) {
        if self.lead.read_for.as_ref() == Some(&self.room) {
            return;
        }
        self.lead.read_for = Some(self.room.clone());
        self.lead.agent_id = None;
        if self.db_path.as_os_str().is_empty() {
            return;
        }
        self.lead.agent_id = dcc_infra::db::SqliteSessionRepo::open_read_only(&self.db_path)
            .and_then(|repo| repo.room_lead(self.room.as_deref()))
            .ok()
            .flatten();
    }

    /// Sets who answers by default here; none: DCC's own pick.
    pub(crate) fn set_lead(&mut self, agent_id: Option<String>, cx: &mut Context<Self>) {
        self.lead.open = false;
        self.lead.agent_id = agent_id.clone();
        self.live.quiet = true;
        cx.notify();
        let room = self.room.clone();
        let Some(api) = (!cfg!(test))
            .then(crate::data::desktop_api::DesktopApi::load)
            .flatten()
        else {
            return;
        };
        cx.spawn(async move |_, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move { api.set_room_lead(room.as_deref(), agent_id.as_deref()) })
                .await;
            if let Err(error) = saved {
                let _ = cx.update(|cx| Toaster::error(error.to_string(), cx));
            }
        })
        .detach();
    }

    /// The control in the room's head, with two agents or more.
    pub(super) fn lead_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let agents = self.list.model().agents.clone();
        if agents.len() < 2 {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let current = self
            .lead
            .agent_id
            .as_ref()
            .and_then(|id| agents.iter().find(|agent| &agent.id == id));
        let label: SharedString = match current {
            Some(agent) => cx.t_with(
                "agents.room.lead.set",
                &Args::default().with("agent", agent.name.clone()),
            ),
            None => cx.t("agents.room.lead.auto"),
        };
        let trigger = div()
            .debug_selector(|| "room-lead".into())
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(3.0))
            .rounded_full()
            .border_1()
            .border_color(colors.border)
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .cursor_pointer()
            .hover(|chip| chip.text_color(colors.foreground))
            .child(label)
            .child(Icon::new("chevron-down").size(11.0));
        let option = |index: usize,
                      id: Option<String>,
                      name: SharedString,
                      picked: bool,
                      cx: &mut Context<Self>| {
            div()
                .id(SharedString::from(format!("room-lead-option-{index}")))
                .debug_selector(move || format!("room-lead-option-{index}"))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .text_size(px(12.5))
                .cursor_pointer()
                .hover(|row| row.bg(colors.accent))
                .when(picked, |row| row.font_weight(FontWeight::MEDIUM))
                .on_click(cx.listener(move |room, _, _, cx| room.set_lead(id.clone(), cx)))
                .child(name)
                .children(picked.then(|| Icon::new("check").size(13.0)))
        };
        let mut rows = vec![option(
            0,
            None,
            cx.t("agents.room.lead.autoOption"),
            current.is_none(),
            cx,
        )];
        for (index, agent) in agents.iter().enumerate() {
            let picked = current.is_some_and(|current| current.id == agent.id);
            rows.push(option(
                index + 1,
                Some(agent.id.clone()),
                agent.name.clone().into(),
                picked,
                cx,
            ));
        }
        let list = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(8.0))
                    .py(px(6.0))
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("agents.room.lead.hint")),
            )
            .children(rows)
            .into_any_element();
        let room = cx.entity().downgrade();
        Some(
            Popover::new(
                "room-lead-menu",
                self.lead.open,
                trigger,
                move |_, _| list,
                move |open, _, cx| {
                    let _ = room.update(cx, |room, cx| {
                        room.lead.open = open;
                        room.live.quiet = true;
                        cx.notify();
                    });
                },
            )
            .align(PopoverAlign::End)
            .width(260.0)
            .padding(4.0)
            .into_any_element(),
        )
    }
}

impl crate::data::desktop_api::DesktopApi {
    pub fn set_room_lead(
        &self,
        room: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<(), crate::data::desktop_api::ApiError> {
        self.post(
            "/api/v1/rooms/lead",
            &serde_json::json!({ "room": room, "agentId": agent_id }),
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use dcc_core::domain::team_room::RouteReason;
    use gpui::{Modifiers, TestAppContext};

    use super::super::room_list_tests::{dev, room_with_lines};
    use crate::data::agents::AgentState;
    use crate::data::loader::SidebarData;

    #[gpui::test]
    fn the_person_picks_who_answers_when_nobody_is_named(cx: &mut TestAppContext) {
        let (page, room, cx) = room_with_lines(cx, Vec::new(), AgentState::Idle);
        // One agent: nothing to choose.
        assert!(cx.debug_bounds("room-lead").is_none());
        let mut suporte = dev(AgentState::Idle);
        suporte.id = "sup".into();
        suporte.name = "Suporte".into();
        page.update(cx, |page, cx| {
            page.sidebar.update(cx, |sidebar, cx| {
                sidebar.apply_for_tests(
                    SidebarData {
                        agents: vec![dev(AgentState::Idle), suporte],
                        ..SidebarData::default()
                    },
                    cx,
                )
            });
        });
        cx.run_until_parked();
        let who = |cx: &mut gpui::VisualTestContext| {
            room.read_with(cx, |room, cx| {
                room.pick("e o deploy de hoje?", cx)
                    .map(|(agent, reason)| (agent.id, reason))
            })
        };
        // DCC's own pick: the first agent.
        assert_eq!(who(cx), Some(("dev".into(), RouteReason::Default)));

        let click = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
            let bounds = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} on screen"));
            cx.simulate_click(bounds.center(), Modifiers::none());
            cx.run_until_parked();
        };
        click(cx, "room-lead");
        // "Automático", then each agent.
        click(cx, "room-lead-option-2");
        assert_eq!(
            room.read_with(cx, |room, _| room.lead.agent_id.clone())
                .as_deref(),
            Some("sup")
        );
        assert_eq!(who(cx), Some(("sup".into(), RouteReason::Default)));
        // Naming someone still wins.
        let named = room.read_with(cx, |room, cx| {
            room.pick("@Dev olha isso", cx)
                .map(|(agent, reason)| (agent.id, reason))
        });
        assert_eq!(named, Some(("dev".into(), RouteReason::Mention)));

        // Back to automatic.
        click(cx, "room-lead");
        click(cx, "room-lead-option-0");
        assert_eq!(who(cx), Some(("dev".into(), RouteReason::Default)));
    }
}
