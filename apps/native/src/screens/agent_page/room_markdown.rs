//! An answer's markdown parsed once, by its line (after the Monocode's
//! `markdown/cache.rs`): a frame looks a line up by its key without
//! hashing or formatting anything; when the room's rows are rebuilt, a line
//! whose text changed or that left the room drops its entry. Bounded, the
//! least recently drawn going first, so scrolling a long room back up never
//! keeps the whole of it.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use crate::data::agent_mentions::MentionNames;
use crate::markdown::{Block, mentions, parse};

/// Answers kept at most; a frame shows far fewer.
const KEPT: usize = 128;

struct Entry {
    len: usize,
    hash: u64,
    blocks: Rc<Vec<Block>>,
    /// The draw it was last used in, for the eviction.
    used: u64,
}

#[derive(Default)]
pub(crate) struct LineMarkdown {
    entries: HashMap<String, Entry>,
    draws: u64,
    /// The roster the mentions in the entries were marked for.
    roster: u64,
    /// Texts parsed, for the tests.
    #[cfg(test)]
    pub(crate) parses: usize,
}

fn text_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

impl LineMarkdown {
    /// The blocks of the line `key` saying `text`, parsed on the first draw.
    pub(crate) fn get(&mut self, key: &str, text: &str, names: &MentionNames) -> Rc<Vec<Block>> {
        self.draws += 1;
        let draws = self.draws;
        if let Some(entry) = self.entries.get_mut(key) {
            entry.used = draws;
            return entry.blocks.clone();
        }
        if self.entries.len() >= KEPT
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone())
        {
            self.entries.remove(&oldest);
        }
        #[cfg(test)]
        {
            self.parses += 1;
        }
        let mut blocks = parse(text);
        mentions::mark(&mut blocks, names);
        let blocks = Rc::new(blocks);
        self.entries.insert(
            key.to_string(),
            Entry {
                len: text.len(),
                hash: text_hash(text),
                blocks: blocks.clone(),
                used: draws,
            },
        );
        blocks
    }

    /// A renamed or recolored agent changes how every answer reads: the
    /// entries are parsed again on their next draw.
    pub(crate) fn roster(&mut self, names: &MentionNames) {
        let signature = names.signature();
        if self.roster != signature {
            self.roster = signature;
            self.entries.clear();
        }
    }

    /// Keeps only the lines still in the room, as they read now: a line
    /// that grew or was edited parses again on its next draw.
    pub(crate) fn retain<'a>(&mut self, lines: impl IntoIterator<Item = (&'a str, &'a str)>) {
        let now: HashMap<&str, &str> = lines.into_iter().collect();
        self.entries.retain(|key, entry| {
            now.get(key.as_str())
                .is_some_and(|text| text.len() == entry.len && (entry.hash == text_hash(text)))
        });
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_parses_once_and_again_only_when_its_text_changes() {
        let mut cache = LineMarkdown::default();
        let none = MentionNames::default();
        let first = cache.get("a", "**oi**", &none);
        let again = cache.get("a", "**oi**", &none);
        assert!(Rc::ptr_eq(&first, &again));
        assert_eq!(cache.parses, 1);

        // Same length, other text: the rebuild drops it.
        cache.retain([("a", "**ou**")]);
        cache.get("a", "**ou**", &none);
        assert_eq!(cache.parses, 2);

        // Unchanged: kept across rebuilds.
        cache.retain([("a", "**ou**")]);
        cache.get("a", "**ou**", &none);
        assert_eq!(cache.parses, 2);
    }

    #[test]
    fn a_new_roster_marks_the_mentions_again() {
        let mut cache = LineMarkdown::default();
        let team = MentionNames::new([("Revisor", "violet")], |_| 1);
        cache.roster(&team);
        cache.get("a", "oi @Revisor", &team);
        cache.roster(&team);
        cache.get("a", "oi @Revisor", &team);
        assert_eq!(cache.parses, 1, "the same roster keeps the entry");
        cache.roster(&MentionNames::default());
        cache.get("a", "oi @Revisor", &MentionNames::default());
        assert_eq!(cache.parses, 2);
    }

    #[test]
    fn lines_that_leave_the_room_are_dropped_and_the_cache_is_bounded() {
        let mut cache = LineMarkdown::default();
        let none = MentionNames::default();
        cache.get("a", "um", &none);
        cache.get("b", "dois", &none);
        cache.retain([("b", "dois")]);
        assert_eq!(cache.len(), 1);

        for index in 0..KEPT + 40 {
            cache.get(&format!("line-{index}"), "texto", &none);
        }
        assert_eq!(cache.len(), KEPT);
        // The latest drawn stay; the oldest went first.
        let parses = cache.parses;
        cache.get(&format!("line-{}", KEPT + 39), "texto", &none);
        assert_eq!(cache.parses, parses);
        cache.get("line-0", "texto", &none);
        assert_eq!(cache.parses, parses + 1);
    }
}
