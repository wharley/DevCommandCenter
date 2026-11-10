//! An answer's markdown parsed once, by its line (after the Monocode's
//! `markdown/cache.rs`): a frame looks a line up by its key without
//! hashing or formatting anything; when the room's rows are rebuilt, a line
//! whose text changed or that left the room drops its entry. Bounded, the
//! least recently drawn going first, so scrolling a long room back up never
//! keeps the whole of it.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use crate::markdown::{Block, parse};

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
    pub(crate) fn get(&mut self, key: &str, text: &str) -> Rc<Vec<Block>> {
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
        let blocks = Rc::new(parse(text));
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
        let first = cache.get("a", "**oi**");
        let again = cache.get("a", "**oi**");
        assert!(Rc::ptr_eq(&first, &again));
        assert_eq!(cache.parses, 1);

        // Same length, other text: the rebuild drops it.
        cache.retain([("a", "**ou**")]);
        cache.get("a", "**ou**");
        assert_eq!(cache.parses, 2);

        // Unchanged: kept across rebuilds.
        cache.retain([("a", "**ou**")]);
        cache.get("a", "**ou**");
        assert_eq!(cache.parses, 2);
    }

    #[test]
    fn lines_that_leave_the_room_are_dropped_and_the_cache_is_bounded() {
        let mut cache = LineMarkdown::default();
        cache.get("a", "um");
        cache.get("b", "dois");
        cache.retain([("b", "dois")]);
        assert_eq!(cache.len(), 1);

        for index in 0..KEPT + 40 {
            cache.get(&format!("line-{index}"), "texto");
        }
        assert_eq!(cache.len(), KEPT);
        // The latest drawn stay; the oldest went first.
        let parses = cache.parses;
        cache.get(&format!("line-{}", KEPT + 39), "texto");
        assert_eq!(cache.parses, parses);
        cache.get("line-0", "texto");
        assert_eq!(cache.parses, parses + 1);
    }
}
