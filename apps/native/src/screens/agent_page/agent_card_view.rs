//! What the agent card shows (`agent_card`), read when it opens: the mascot,
//! the name and what it does (as the `@` list words it), what it runs on
//! with the provider's mark and the account, the rooms it talks in, what it
//! does now (the team column's line, in its color) and "Mensagem" /
//! "Ver perfil".

use gpui::{AnyElement, App, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::agent_card::{CardHost, agent_of, close};
use super::team_state::{TeamFacts, Tone, state_line};
use crate::agent::{AgentAvatarView, Mood};
use crate::data::agents::{AgentRecord, AgentState};
use crate::i18n::Translate;
use crate::project::ProviderIcon;
use crate::screens::composer::Composer;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant};

const WIDTH: f32 = 288.0;

/// The rooms `agent` talks in, by name: Geral, then each project room it
/// has a conversation in, once, while the project exists.
pub(super) fn room_names(
    agent: &AgentRecord,
    projects: &[(String, String)],
    geral: SharedString,
) -> Vec<SharedString> {
    let talked = crate::data::agent_rooms::talked(std::slice::from_ref(agent));
    std::iter::once(geral)
        .chain(
            crate::data::agent_rooms::rooms(&[], talked, projects)
                .into_iter()
                .map(|(_, name)| name.into()),
        )
        .collect()
}

/// "Roda em": the provider, and its model and account; none when the agent
/// follows the composer.
fn runs_on(agent: &AgentRecord, composer: &Composer, cx: &App) -> (Option<String>, SharedString) {
    let provider = agent.provider_id.as_deref().and_then(|id| {
        composer
            .catalog
            .as_deref()?
            .iter()
            .find(|provider| provider.id.0 == id)
    });
    let Some(provider) = provider else {
        return (None, cx.t("agents.page.runsOnCurrent"));
    };
    let model = agent
        .model
        .as_deref()
        .and_then(|id| provider.models.iter().find(|model| model.id == id))
        .map_or_else(|| provider.label.clone(), |model| model.label.clone());
    let account = agent.account_id.as_deref().and_then(|id| {
        crate::data::account_selection::known()?
            .into_iter()
            .find(|account| account.id == id)
            .map(|account| account.label)
    });
    let text = match account {
        Some(account) => format!("{model} · {account}"),
        None => model,
    };
    (Some(provider.id.0.clone()), text.into())
}

/// The card's inside for `agent_id`; an agent gone shows nothing.
pub(super) fn card_body<T: CardHost>(
    host: &T,
    agent_id: &str,
    _window: &mut Window,
    cx: &mut Context<T>,
) -> AnyElement {
    let (sidebar, composer) = host.card_sources();
    let Some(agent) = agent_of(&sidebar, agent_id, cx) else {
        return div().into_any_element();
    };
    let colors = cx.theme().colors.clone();
    let (line, tone) = {
        let sidebar = sidebar.read(cx);
        let mood = sidebar
            .mood(&agent.id, cx)
            .unwrap_or_else(|| Mood::of(AgentState::Idle, false, false));
        state_line(&agent, mood, &TeamFacts::read(sidebar), cx)
    };
    let projects = super::editor_projects::project_options(sidebar.read(cx).projects());
    let rooms = room_names(&agent, &projects, cx.t("agents.rooms.geral"));
    let (provider, model) = runs_on(&agent, composer.read(cx), cx);
    let what = super::room_mentions::what_it_does(&agent, cx);
    let label = |text: SharedString| {
        div()
            .w(px(64.0))
            .flex_none()
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .child(text)
    };
    let fact = |label: AnyElement, value: AnyElement| {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(label)
            .child(value)
    };
    let (message, profile) = (agent.id.clone(), agent.id.clone());
    div()
        .w(px(WIDTH))
        .p(px(14.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .rounded(cx.theme().radius.lg)
        .border_1()
        .border_color(colors.border.opacity(0.7))
        .bg(colors.popover)
        .text_color(colors.popover_foreground)
        .shadow(crate::ui::card_shadow())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(AgentAvatarView::new(
                    SharedString::from(format!("agent-card-face-{}", agent.id)),
                    &agent.avatar,
                    None,
                    56.0,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .gap(px(2.0))
                        .child(
                            div()
                                .debug_selector(|| "agent-card-name".into())
                                .text_size(px(15.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(colors.foreground)
                                .child(agent.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .line_clamp(2)
                                .text_color(colors.muted_foreground)
                                .child(what),
                        ),
                ),
        )
        .child(
            div()
                .debug_selector(|| "agent-card-status".into())
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(match tone {
                    Tone::Urgent => crate::theme::tailwind::AMBER_600.hsla(),
                    Tone::Active => crate::theme::tailwind::GREEN_600.hsla(),
                    Tone::Quiet => colors.muted_foreground,
                })
                .child(line),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(fact(
                    label(cx.t("agents.card.runsOn")).into_any_element(),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .min_w_0()
                        .children(provider.map(|id| ProviderIcon::new(&id).size(13.0)))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(colors.foreground)
                                .child(model),
                        )
                        .into_any_element(),
                ))
                .child(fact(
                    label(cx.t("agents.card.rooms")).into_any_element(),
                    div()
                        .debug_selector(|| "agent-card-rooms".into())
                        .truncate()
                        .text_size(px(12.0))
                        .text_color(colors.foreground)
                        .child(rooms.join(" · "))
                        .into_any_element(),
                )),
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    div().debug_selector(|| "agent-card-message".into()).child(
                        Button::new("agent-card-message")
                            .variant(ButtonVariant::Default)
                            .size(ButtonSize::Sm)
                            .on_click(cx.listener(move |host, _, window, cx| {
                                close(host, cx);
                                host.card_message(&message, window, cx);
                            }))
                            .child(cx.t("agents.card.message")),
                    ),
                )
                .child(
                    div().debug_selector(|| "agent-card-profile".into()).child(
                        Button::new("agent-card-profile")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .on_click(cx.listener(move |host, _, _, cx| {
                                close(host, cx);
                                host.card_profile(&profile, cx);
                            }))
                            .child(cx.t("agents.card.profile")),
                    ),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agents::AgentSession;

    #[test]
    fn the_rooms_are_geral_then_each_project_it_talks_in_once() {
        let session = |room: Option<&str>| AgentSession {
            session_id: "s".into(),
            workspace_id: "w".into(),
            state: AgentState::Idle,
            completed_at: None,
            project_id: "dcc-agent-dev".into(),
            title: None,
            updated_at: String::new(),
            room: room.map(Into::into),
        };
        let agent = AgentRecord {
            sessions: vec![
                session(None),
                session(Some("checkout")),
                session(Some("checkout")),
                session(Some("gone")),
            ],
            ..super::super::editor_role::blank_agent()
        };
        let projects = [("checkout".to_string(), "Checkout".to_string())];
        assert_eq!(
            room_names(&agent, &projects, "Geral".into()),
            ["Geral", "Checkout"]
        );
    }
}
