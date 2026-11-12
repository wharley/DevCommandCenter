//! "Duplicar" on an agent's page: "Novo agente" filled with the agent as it
//! is — role, extra instructions, model, projects, access, who it may call,
//! account and limit — under "Dev (cópia)" and the next color, so a second
//! Dev on another model is a name and a model away. Its conversations,
//! memory, sources and routines stay the original's.

use dcc_core::domain::agent::AGENT_AVATAR_COLORS;
use gpui::{Context, Window};

use super::AgentPage;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};

/// The copy, not saved yet: a new agent of the person's.
pub(crate) fn duplicate_record(agent: &AgentRecord, name: String) -> AgentRecord {
    let mut copy = agent.clone();
    copy.id = String::new();
    copy.name = name;
    // A built-in agent's copy is the person's own, its role kept as written.
    copy.preset = None;
    copy.sessions = Vec::new();
    let at = AGENT_AVATAR_COLORS
        .iter()
        .position(|color| *color == agent.avatar.color)
        .map_or(0, |at| at + 1);
    copy.avatar.color = AGENT_AVATAR_COLORS[at % AGENT_AVATAR_COLORS.len()].to_string();
    copy
}

impl AgentPage {
    pub(crate) fn open_duplicate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(data) = self.sidebar.read(cx).agent_page(&self.agent_id) else {
            return;
        };
        let name = cx
            .t_with(
                "agents.page.copyName",
                &Args::default().with("name", data.agent.name.clone()),
            )
            .chars()
            .take(60)
            .collect();
        self.edit(duplicate_record(&data.agent, name), window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::agent_page::editor_role::blank_agent;

    #[test]
    fn a_copy_keeps_how_the_agent_works_but_not_who_it_is() {
        let mut dev = blank_agent();
        dev.id = "dev".into();
        dev.name = "Dev".into();
        dev.preset = Some("reviewer".into());
        dev.role = "Escreve código.".into();
        dev.extra_instructions = "Rode os testes.".into();
        dev.model = Some("astra".into());
        dev.projects = vec!["checkout".into()];
        dev.may_call = vec!["rev".into()];
        dev.avatar.color = "pink".into();
        let copy = duplicate_record(&dev, "Dev (cópia)".into());
        assert!(copy.id.is_empty());
        assert_eq!(copy.name, "Dev (cópia)");
        assert_eq!(copy.preset, None);
        assert!(copy.sessions.is_empty());
        assert_eq!(copy.role, dev.role);
        assert_eq!(copy.extra_instructions, dev.extra_instructions);
        assert_eq!(copy.model, dev.model);
        assert_eq!(copy.projects, dev.projects);
        assert_eq!(copy.may_call, dev.may_call);
        // The next color, around the end: told apart in the room.
        assert_eq!(copy.avatar.color, "blue");
        assert_eq!(copy.avatar.shape, dev.avatar.shape);
    }
}
