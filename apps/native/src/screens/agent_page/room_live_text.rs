//! The answer of a turn at work as the live row draws it: revealed at an
//! even pace (the task conversation's `paced`) with the caret at its end,
//! and its markdown parsed at most every `PARSE_INTERVAL` (as Tucano does),
//! not every frame — kept here, apart from the settled answers' cache
//! (`room_markdown`), so a stream never pushes them out.

use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::data::agent_mentions::MentionNames;
use crate::markdown::{Block, mentions, parse};
use crate::screens::conversation::paced::{CARET, Paced};

/// How often the streamed markdown is parsed again.
pub(crate) const PARSE_INTERVAL: Duration = Duration::from_millis(140);

/// One turn's answer as it streams.
pub(crate) struct LiveText {
    pub turn_id: String,
    paced: Paced,
    /// The length of the text the blocks were parsed for.
    parsed: Option<usize>,
    parsed_at: Option<Instant>,
    blocks: Rc<Vec<Block>>,
    /// Parses done, for the tests.
    #[cfg(test)]
    pub parses: usize,
}

impl LiveText {
    pub(crate) fn new(turn_id: &str, text: &str) -> Self {
        Self {
            turn_id: turn_id.to_string(),
            paced: Paced::new(text),
            parsed: None,
            parsed_at: None,
            blocks: Rc::default(),
            #[cfg(test)]
            parses: 0,
        }
    }

    /// The blocks to draw at `now`, and whether more is left to reveal.
    /// `at_once` shows all that arrived (reduced motion, DCC in the back).
    pub(crate) fn frame(
        &mut self,
        text: &str,
        now: Instant,
        at_once: bool,
        names: &MentionNames,
    ) -> (Rc<Vec<Block>>, bool) {
        let more = if at_once {
            self.paced.reveal_all(text);
            false
        } else {
            self.paced.advance(text, now)
        };
        let shown = self.paced.shown.min(text.len());
        let due = self
            .parsed_at
            .is_none_or(|at| now.duration_since(at) >= PARSE_INTERVAL);
        if self.parsed != Some(shown) && due && text.is_char_boundary(shown) {
            let mut blocks = parse(&format!("{}{CARET}", &text[..shown]));
            mentions::mark(&mut blocks, names);
            self.blocks = Rc::new(blocks);
            self.parsed = Some(shown);
            self.parsed_at = Some(now);
            #[cfg(test)]
            {
                self.parses += 1;
            }
        }
        (self.blocks.clone(), more || self.parsed != Some(shown))
    }
}

/// "0:42", "1:24", "1:02:05": the time since the turn started.
pub(crate) fn clock_text(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (hours, minutes, seconds) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_minutes_and_seconds() {
        assert_eq!(clock_text(0), "0:00");
        assert_eq!(clock_text(84), "1:24");
        assert_eq!(clock_text(3725), "1:02:05");
        assert_eq!(clock_text(-3), "0:00");
    }

    #[test]
    fn the_stream_is_parsed_at_most_every_interval() {
        let names = MentionNames::default();
        let start = Instant::now();
        let mut live = LiveText::new("t1", "");
        let text = "palavra ".repeat(200);
        live.frame(&text, start, false, &names);
        assert_eq!(live.parses, 1, "the first frame parses what it shows");
        // Frames close together reveal more but parse once per interval.
        for step in 1..=6u32 {
            live.frame(
                &text,
                start + Duration::from_millis(20 * u64::from(step)),
                false,
                &names,
            );
        }
        assert_eq!(live.parses, 1);
        live.frame(&text, start + PARSE_INTERVAL, false, &names);
        assert_eq!(live.parses, 2);
        // All of it at once, then nothing left to parse.
        let at = start + PARSE_INTERVAL * 2;
        let (blocks, more) = live.frame(&text, at, true, &names);
        assert!(!blocks.is_empty());
        assert!(!more);
        let parses = live.parses;
        live.frame(&text, at + PARSE_INTERVAL, true, &names);
        assert_eq!(live.parses, parses);
    }
}
