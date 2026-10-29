//! `/` commands (`DEFAULT_SLASH_COMMANDS` + `SlashCommandPlugin`): typing
//! `/` after a space or at the start opens them in the mention popup. DCC
//! runs `/clear` (empties the draft) and `/compact` (sends the provider's
//! compaction as its own turn); `/goal` is the provider's own, inserted for
//! the person to finish. A command needing a capability the provider lacks
//! is not offered. `/spec` stays out (mission spec is parked).

use std::ops::Range;

use dcc_core::domain::provider::ProviderDescriptor;

use crate::ui::text_area::CHIP;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Clear,
    Compact,
    Goal,
}

impl Command {
    pub const ALL: [Command; 3] = [Command::Clear, Command::Compact, Command::Goal];

    pub fn name(self) -> &'static str {
        match self {
            Command::Clear => "clear",
            Command::Compact => "compact",
            Command::Goal => "goal",
        }
    }

    /// The description's i18n key.
    pub fn description_key(self) -> &'static str {
        match self {
            Command::Clear => "composer.slashCommands.clear",
            Command::Compact => "native.slashCompact",
            Command::Goal => "composer.slashCommands.goal",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Command::Clear => "eraser",
            Command::Compact => "minimize-2",
            Command::Goal => "target",
        }
    }

    /// `runsWhenTyped`: the provider runs it sent as text, so it is
    /// highlighted at the start of the field.
    pub fn runs_when_typed(self) -> bool {
        matches!(self, Command::Compact | Command::Goal)
    }

    /// `availableSlashCommands`; `/compact` also needs a conversation that
    /// is not mid-turn, since it goes as a turn of its own.
    fn offered(self, provider: Option<&ProviderDescriptor>, idle_session: bool) -> bool {
        let capabilities = provider.map(|provider| &provider.capabilities);
        match self {
            Command::Clear => true,
            Command::Compact => {
                idle_session && capabilities.is_some_and(|caps| caps.supports_compaction_command)
            }
            Command::Goal => capabilities.is_some_and(|caps| caps.supports_native_goal),
        }
    }
}

pub fn available(provider: Option<&ProviderDescriptor>, idle_session: bool) -> Vec<Command> {
    Command::ALL
        .into_iter()
        .filter(|command| command.offered(provider, idle_session))
        .collect()
}

/// `filterCommands`: names starting with the query first, then containing it.
pub fn filter(commands: &[Command], query: &str) -> Vec<Command> {
    let query = query.to_lowercase();
    let (mut prefix, mut inside) = (Vec::new(), Vec::new());
    for &command in commands {
        if command.name().starts_with(&query) {
            prefix.push(command);
        } else if command.name().contains(&query) {
            inside.push(command);
        }
    }
    prefix.extend(inside);
    prefix
}

/// `useBasicTypeaheadTriggerMatch("/")` on the text before the caret: the
/// `/query` range and the query.
pub fn trigger(before_caret: &str) -> Option<(Range<usize>, String)> {
    let at = before_caret.rfind('/')?;
    let query = &before_caret[at + 1..];
    if query.chars().count() > 32
        || !query
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    let lead = before_caret[..at].chars().next_back();
    if !lead.is_none_or(|c| c.is_whitespace() || c == CHIP) {
        return None;
    }
    Some((at..before_caret.len(), query.to_string()))
}

/// A command the provider runs, typed at the very start: its range there.
pub fn leading(text: &str, names: &[&str]) -> Option<Range<usize>> {
    let rest = text.strip_prefix('/')?;
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    names.contains(&&rest[..end]).then_some(0..end + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trigger_needs_a_boundary_and_a_word() {
        assert_eq!(trigger("/"), Some((0..1, String::new())));
        assert_eq!(trigger("veja /go"), Some((5..8, "go".into())));
        assert_eq!(trigger("src/app"), None);
        assert_eq!(trigger("/goal termine"), None);
        assert_eq!(trigger("https://x"), None);
    }

    #[test]
    fn prefix_matches_come_first() {
        let all = Command::ALL;
        assert_eq!(filter(&all, ""), all.to_vec());
        assert_eq!(filter(&all, "c"), vec![Command::Clear, Command::Compact]);
        assert_eq!(filter(&all, "oa"), vec![Command::Goal]);
        assert!(filter(&all, "spec").is_empty());
    }

    #[test]
    fn only_a_leading_known_command_is_highlighted() {
        let names = ["goal", "compact"];
        assert_eq!(leading("/goal testes passando", &names), Some(0..5));
        assert_eq!(leading("/compact", &names), Some(0..8));
        assert_eq!(leading("/goals", &names), None);
        assert_eq!(leading("rode /goal", &names), None);
    }
}
