//! `usePacedText`: streamed prose is revealed at an even pace, so the bursts
//! each refresh brings in spread over ~250ms instead of landing in one paint,
//! with a block caret at the end while it streams.

use std::time::{Duration, Instant};

/// Seconds the reveal aims to need to catch up with what already arrived.
const CATCH_UP_SECONDS: f64 = 0.25;
const MIN_CHARS_PER_SECOND: f64 = 90.0;
const MAX_CHARS_PER_SECOND: f64 = 2400.0;
/// The text is re-laid out at most this often.
pub const COMMIT_INTERVAL: Duration = Duration::from_millis(40);
/// A half-written word is held back at most this many bytes.
const WORD_LOOKAHEAD: usize = 12;
/// Text already there when the row first shows is not replayed from the start.
const INITIAL_REPLAY: usize = 240;
/// Streamdown's block caret.
pub const CARET: &str = "▍";

fn ceil_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// `nextPacedLength`: the speed follows the backlog and the cut waits for
/// the end of the word in sight.
pub fn next_paced_length(shown: usize, target: &str, elapsed: Duration) -> usize {
    let backlog = target.len().saturating_sub(shown);
    if backlog == 0 {
        return target.len();
    }
    let rate =
        (backlog as f64 / CATCH_UP_SECONDS).clamp(MIN_CHARS_PER_SECOND, MAX_CHARS_PER_SECOND);
    let step = ((rate * elapsed.as_secs_f64()).round() as usize).max(1);
    let mut next = ceil_boundary(target, shown + step);
    if next < target.len() {
        let window = &target[next..ceil_boundary(target, next + WORD_LOOKAHEAD)];
        if let Some(space) = window.find(char::is_whitespace) {
            next += space;
        }
    }
    next
}

/// One streaming text's reveal.
#[derive(Debug)]
pub struct Paced {
    pub shown: usize,
    /// When the reveal last moved; `None` while it has caught up, so text that
    /// arrives after a pause starts from now instead of jumping.
    last_tick: Option<Instant>,
    snapshot: String,
}

impl Paced {
    pub fn new(target: &str) -> Self {
        Self {
            shown: floor_boundary(target, target.len().saturating_sub(INITIAL_REPLAY)),
            last_tick: None,
            snapshot: target.to_string(),
        }
    }

    /// Shows all of `target` at once (the window is in the back).
    pub fn reveal_all(&mut self, target: &str) {
        self.shown = target.len();
        self.last_tick = None;
        self.snapshot = target.to_string();
    }

    /// Moves the reveal for this frame; `true` while more is left to show.
    pub fn advance(&mut self, target: &str, now: Instant) -> bool {
        if target != self.snapshot {
            let kept = &self.snapshot[..self.shown.min(self.snapshot.len())];
            if !target.starts_with(kept) {
                // The provider replaced its snapshot: snap instead of replaying.
                self.shown = floor_boundary(target, self.shown);
            }
            self.snapshot = target.to_string();
        }
        if self.shown >= target.len() {
            self.shown = target.len();
            self.last_tick = None;
            return false;
        }
        match self.last_tick {
            None => self.last_tick = Some(now),
            Some(last) if now.duration_since(last) >= COMMIT_INTERVAL => {
                self.shown = next_paced_length(self.shown, target, now.duration_since(last));
                self.last_tick = Some(now);
            }
            Some(_) => {}
        }
        self.shown < target.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreads_a_burst_and_waits_for_the_word() {
        let target = "uma frase bem longa que chegou de uma vez só na tela";
        let next = next_paced_length(0, target, Duration::from_millis(40));
        assert!(next > 0 && next < target.len());
        assert!(target[next..].starts_with(' ') || next == target.len());
        assert_eq!(
            next_paced_length(target.len(), target, Duration::from_millis(40)),
            target.len()
        );
        // Multibyte text never cuts inside a character.
        let accents = "ação é útil ".repeat(20);
        let cut = next_paced_length(1, &accents, Duration::from_millis(16));
        assert!(accents.is_char_boundary(cut));
    }

    #[test]
    fn text_after_a_pause_starts_slowly_and_a_rewrite_snaps() {
        let start = Instant::now();
        let mut paced = Paced::new("");
        let text = "palavra ".repeat(40);
        assert!(paced.advance(&text, start));
        assert_eq!(paced.shown, 0, "the first frame only starts the clock");
        assert!(paced.advance(&text, start + Duration::from_millis(50)));
        assert!(paced.shown > 0 && paced.shown < text.len());
        let other = "outro texto";
        paced.advance(other, start + Duration::from_millis(60));
        assert!(paced.shown <= other.len());
        // An existing long reply only replays its tail.
        let long = "x".repeat(1000);
        assert_eq!(Paced::new(&long).shown, 760);
    }

    #[test]
    fn in_the_back_all_of_it_shows_and_no_step_is_left() {
        let start = Instant::now();
        let text = "palavra ".repeat(40);
        let mut paced = Paced::new("");
        paced.advance(&text, start);
        paced.reveal_all(&text);
        assert_eq!(paced.shown, text.len());
        assert!(!paced.advance(&text, start + Duration::from_millis(50)));
        // What arrives after it, back in front, is revealed from there.
        let more = format!("{text}e mais um pouco de texto que chegou depois");
        assert!(paced.advance(&more, start + Duration::from_millis(60)));
        assert_eq!(paced.shown, text.len());
    }
}
