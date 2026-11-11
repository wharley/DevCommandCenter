//! Writing to an agent while it works (Tucano's way): a message to a
//! conversation with a turn open, one waiting on the person, or one that
//! already holds a queued message of the person's is queued for when that
//! turn ends instead of sent into it. The desktop's room and the phone's
//! decide it the same way.

/// Whether a message to a conversation waits for its turn: one runs, it
/// waits on the person, or a message is queued there already (they go in
/// order).
pub fn must_wait(running: bool, needs_you: bool, queued_ahead: bool) -> bool {
    running || needs_you || queued_ahead
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_idle_conversation_with_nothing_queued_takes_it_now() {
        assert!(!must_wait(false, false, false));
        assert!(must_wait(true, false, false));
        assert!(must_wait(false, true, false));
        assert!(must_wait(false, false, true));
    }
}
