//! An agent's page in two tabs (Tucano's and Monocode's agent DM): "Conversa"
//! first — the person talks to that agent alone, the team room on it
//! (`room_focus`) — and "Configuração", the page as it was: how it runs,
//! its triggers, routines, memory, what it may use. Picking an agent in the
//! team column opens its conversation; "Conversa completa" opens the whole
//! one, with its tools.

use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::AgentPage;
use super::conversation::home_session;
use crate::agent::AgentAvatarView;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Tabs};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentTab {
    Conversation,
    #[default]
    Settings,
}

impl AgentPage {
    /// The agent's page on its conversation.
    pub fn show_conversation(&mut self, agent_id: String, cx: &mut Context<Self>) {
        self.show(agent_id, cx);
        self.tab = AgentTab::Conversation;
        cx.notify();
    }

    pub(super) fn set_tab(&mut self, tab: AgentTab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            cx.notify();
        }
    }

    /// The agent's name, the tabs and, on the conversation, its whole one.
    fn tab_head(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = cx.theme().colors.clone();
        let agent = self
            .sidebar
            .read(cx)
            .agents()
            .iter()
            .find(|agent| agent.id == self.agent_id)
            .cloned()?;
        let page = cx.entity().downgrade();
        let tabs = Tabs::new(
            "agent-tabs",
            vec![
                cx.t("agents.tabs.conversation"),
                cx.t("agents.tabs.settings"),
            ],
            match self.tab {
                AgentTab::Conversation => 0,
                AgentTab::Settings => 1,
            },
        )
        .on_select(move |index, _, cx| {
            let tab = if index == 0 {
                AgentTab::Conversation
            } else {
                AgentTab::Settings
            };
            let _ = page.update(cx, |page, cx| page.set_tab(tab, cx));
        });
        let whole =
            (self.tab == AgentTab::Conversation && home_session(&agent).is_some()).then(|| {
                Button::new("agent-tab-whole")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .on_click(cx.listener(|page, _, _, cx| page.open_conversation(cx)))
                    .child(cx.t("agents.tabs.whole"))
            });
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(12.0))
                .px(px(24.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(colors.border.opacity(0.7))
                .child(AgentAvatarView::new(
                    SharedString::from("agent-tab-face"),
                    &agent.avatar,
                    None,
                    28.0,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.foreground)
                        .child(agent.name),
                )
                .children(whole)
                .child(div().flex_none().w(px(240.0)).child(tabs))
                .into_any_element(),
        )
    }

    /// The picked agent's page, under its tabs.
    pub(super) fn tabbed_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(head) = self.tab_head(cx) else {
            return self.settings_body(window, cx);
        };
        let body = match self.tab {
            AgentTab::Conversation => self.room_element(Some(self.agent_id.clone()), window, cx),
            AgentTab::Settings => self.settings_body(window, cx),
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(cx.theme().colors.background)
            .child(head)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}
