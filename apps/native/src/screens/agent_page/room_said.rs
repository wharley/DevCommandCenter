//! What the person's message is made of, split once when the row is built
//! (`room_rows`): the words, the images and links that go under them, and
//! where the words name an agent, so a frame only draws it.

use std::ops::Range;

use crate::data::agent_mentions::MentionNames;
use crate::data::message_links::{MessageLink, split_images, split_links};
use crate::data::team_room::{RoomLine, Speaker};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Said {
    pub words: String,
    pub images: Vec<String>,
    pub links: Vec<MessageLink>,
    /// `@Agent` in `words`: its bytes and the mascot's color.
    pub mentions: Vec<(Range<usize>, u32)>,
}

/// Whether a line is the person's own words, not a turn passed, a routine's
/// or a source's line.
pub(crate) fn is_message(line: &RoomLine) -> bool {
    line.speaker == Speaker::Person
        && dcc_core::domain::team_room::handoff_heading(&line.text).is_none()
        && !line
            .text
            .starts_with(dcc_core::domain::agent_routine::ROUTINE_MARK)
        && !line
            .text
            .starts_with(dcc_core::domain::agent_source::SOURCE_MARK)
}

impl Said {
    pub(crate) fn new(text: &str, names: &MentionNames) -> Self {
        // Images and links go under the words.
        let (text, images) = split_images(text);
        let (words, links) = split_links(&text);
        let mentions = names.find(&words);
        Self {
            words,
            images,
            links,
            mentions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_keep_the_name_and_know_where_it_is() {
        let names = MentionNames::new([("Revisor", "violet")], |_| 7);
        let said = Said::new("@Revisor veja https://x.dev/a e @/tmp/shot.png", &names);
        assert!(said.words.starts_with("@Revisor veja"));
        assert_eq!(said.mentions, [(0..8, 7)]);
        assert_eq!(said.images, ["/tmp/shot.png"]);
        assert_eq!(said.links.len(), 1);
        assert!(Said::new("sem ninguém", &names).mentions.is_empty());
    }
}
