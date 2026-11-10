//! Where a message names an agent with `@` (`@Revisor`, `@QA Mobile`), so
//! the room can draw the name as a pill in the agent's color. Matches the
//! way the room routes (`team_room::mentioned`: any case, no accents, the
//! name ends at a non-letter, the longest name first) and leaves alone what
//! only looks like one: an e-mail's `@`, a path's `@/tmp/x.png`.

use std::hash::{Hash, Hasher};
use std::ops::Range;

/// One agent's name as matched, and its mascot's color.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Name {
    folded: Vec<char>,
    rgb: u32,
}

/// The team's names, longest first, built once per roster.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct MentionNames {
    names: Vec<Name>,
}

/// Lowercase and without the accents the router ignores.
fn fold(c: char) -> impl Iterator<Item = char> {
    c.to_lowercase().map(|c| match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'ê' | 'è' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        other => other,
    })
}

/// A character that makes the `@` part of a longer word or path.
fn joins(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '/' | '\\' | '.' | '_' | '-' | '@')
}

impl MentionNames {
    /// From `(name, color)` of each agent; `rgb` turns a color name into
    /// its value.
    pub fn new<'a>(
        agents: impl IntoIterator<Item = (&'a str, &'a str)>,
        rgb: impl Fn(&str) -> u32,
    ) -> Self {
        let mut names: Vec<Name> = agents
            .into_iter()
            .filter_map(|(name, color)| {
                let folded: Vec<char> = name.trim().chars().flat_map(fold).collect();
                (!folded.is_empty()).then(|| Name {
                    folded,
                    rgb: rgb(color),
                })
            })
            .collect();
        names.sort_by_key(|name| std::cmp::Reverse(name.folded.len()));
        Self { names }
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Changes when the roster's names or colors do.
    pub fn signature(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    /// The bytes of `rest` that spell a name at its start, the longest.
    fn name_at(&self, rest: &str) -> Option<(usize, u32)> {
        self.names.iter().find_map(|name| {
            let mut wanted = name.folded.iter();
            let mut used = 0;
            let mut pending: Vec<char> = Vec::new();
            for c in rest.chars() {
                // A character may fold into more than one; all must fit.
                pending.clear();
                pending.extend(fold(c));
                for folded in &pending {
                    if wanted.next() != Some(folded) {
                        return None;
                    }
                }
                used += c.len_utf8();
                if wanted.len() == 0 {
                    break;
                }
            }
            if wanted.len() != 0 {
                return None;
            }
            // The name ends there (`@Revisor,` but not `@Revisores`).
            let ends = rest[used..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric());
            ends.then_some((used, name.rgb))
        })
    }

    /// Each `@Name` in `text`: its bytes, `@` included, and its color.
    pub fn find(&self, text: &str) -> Vec<(Range<usize>, u32)> {
        let mut found = Vec::new();
        if self.is_empty() {
            return found;
        }
        let mut from = 0;
        while let Some(offset) = text[from..].find('@') {
            let at = from + offset;
            from = at + 1;
            if text[..at].chars().next_back().is_some_and(joins) {
                continue;
            }
            if let Some((len, rgb)) = self.name_at(&text[at + 1..]) {
                found.push((at..at + 1 + len, rgb));
                from = at + 1 + len;
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> MentionNames {
        MentionNames::new(
            [
                ("Revisor", "violet"),
                ("QA Mobile", "green"),
                ("QA", "amber"),
                ("Pesquisadora Ágata", "pink"),
            ],
            |color| color.len() as u32,
        )
    }

    fn spans(text: &str) -> Vec<(&str, u32)> {
        names()
            .find(text)
            .into_iter()
            .map(|(range, rgb)| (&text[range], rgb))
            .collect()
    }

    #[test]
    fn a_name_is_found_in_any_case_without_accents() {
        assert_eq!(spans("oi @revisor, vê isso"), [("@revisor", 6)]);
        assert_eq!(spans("@Pesquisadora Agata?"), [("@Pesquisadora Agata", 4)]);
        assert_eq!(spans("(@Revisor)"), [("@Revisor", 6)]);
    }

    #[test]
    fn the_longest_name_wins_and_names_may_have_spaces() {
        assert_eq!(spans("@QA Mobile agora"), [("@QA Mobile", 5)]);
        assert_eq!(spans("@QA agora"), [("@QA", 5)]);
        assert_eq!(spans("@Revisor e @QA"), [("@Revisor", 6), ("@QA", 5)]);
    }

    #[test]
    fn an_unknown_name_or_a_longer_word_stays_text() {
        assert!(spans("@Ninguém aqui").is_empty());
        assert!(spans("@Revisores").is_empty());
        assert!(spans("sem arroba").is_empty());
        assert!(spans("@").is_empty());
    }

    #[test]
    fn an_email_or_a_path_is_not_a_mention() {
        assert!(spans("dev@Revisor.com").is_empty());
        assert!(spans("@/tmp/x.png").is_empty());
        assert!(spans("veja /src/@Revisor").is_empty());
        assert!(spans("a@@Revisor").is_empty());
    }

    #[test]
    fn a_roster_has_a_signature_that_follows_its_names() {
        let same = names();
        assert_eq!(names().signature(), same.signature());
        let other = MentionNames::new([("Revisor", "pink")], |color| color.len() as u32);
        assert_ne!(other.signature(), same.signature());
    }
}
