//! A question asked in the room, answered one decision at a time
//! (Tucano's ask card): which step shows, what was picked for each — one of
//! the agent's options or "Outra coisa" with the person's words — and the
//! answers sent once every decision has one. Kept per request while the
//! room lives, so leaving and coming back finds the picks where they were.
//! No drawing here (`room_question`).

use dcc_core::ports::provider::{ProviderUserInputAnswer, ProviderUserInputQuestion};

/// What was picked for one decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    /// One of the agent's options, by its label.
    Option(String),
    /// "Outra coisa": the words are in its field.
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QuestionDraft {
    /// The decision on screen; `total` is the review.
    pub step: usize,
    pub picks: Vec<Option<Pick>>,
}

impl QuestionDraft {
    pub fn new(total: usize) -> Self {
        Self {
            step: 0,
            picks: vec![None; total],
        }
    }

    pub fn total(&self) -> usize {
        self.picks.len()
    }

    /// The review shows: every decision went by.
    pub fn reviewing(&self) -> bool {
        self.total() > 1 && self.step >= self.total()
    }

    pub fn pick(&mut self, index: usize, pick: Pick) {
        if let Some(slot) = self.picks.get_mut(index) {
            *slot = Some(pick);
        }
    }

    /// Decision `index`'s answer: the option's label, or the words written
    /// for "Outra coisa" (`other`); none while it has neither.
    pub fn answer(&self, index: usize, other: &str) -> Option<String> {
        match self.picks.get(index)? {
            Some(Pick::Option(label)) => Some(label.clone()),
            Some(Pick::Other) => Some(other.trim().to_string()).filter(|text| !text.is_empty()),
            None => None,
        }
    }

    /// "Próxima": on to the next decision, or to the review after the
    /// last one (a single decision sends from its own step).
    pub fn next(&mut self) {
        if self.step < self.total() {
            self.step += 1;
        }
    }

    pub fn back(&mut self) {
        self.step = self.step.saturating_sub(1);
    }

    /// What goes to the agent, keyed by each question's words as the
    /// conversation's card sends them; none while a decision lacks one.
    pub fn answers(
        &self,
        questions: &[ProviderUserInputQuestion],
        other: impl Fn(usize) -> String,
    ) -> Option<Vec<ProviderUserInputAnswer>> {
        questions
            .iter()
            .enumerate()
            .map(|(index, question)| {
                Some(ProviderUserInputAnswer {
                    question: question.question.clone(),
                    answer: self.answer(index, &other(index))?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(text: &str) -> ProviderUserInputQuestion {
        ProviderUserInputQuestion {
            id: text.into(),
            header: String::new(),
            question: text.into(),
            options: Vec::new(),
        }
    }

    #[test]
    fn each_decision_is_answered_then_reviewed() {
        let questions = [question("Qual ambiente?"), question("Posso publicar?")];
        let mut draft = QuestionDraft::new(2);
        assert_eq!(draft.answers(&questions, |_| String::new()), None);
        draft.pick(0, Pick::Option("staging".into()));
        draft.next();
        assert!(!draft.reviewing());
        draft.pick(1, Pick::Other);
        // "Outra coisa" with nothing written is no answer yet.
        assert_eq!(draft.answer(1, "  "), None);
        draft.next();
        assert!(draft.reviewing());
        let answers = draft
            .answers(&questions, |index| {
                if index == 1 { " só amanhã " } else { "" }.to_string()
            })
            .unwrap();
        assert_eq!(answers[0].answer, "staging");
        assert_eq!(answers[1].question, "Posso publicar?");
        assert_eq!(answers[1].answer, "só amanhã");
        draft.back();
        assert_eq!(draft.step, 1);
    }

    #[test]
    fn a_single_decision_has_no_review() {
        let mut draft = QuestionDraft::new(1);
        draft.pick(0, Pick::Option("sim".into()));
        draft.next();
        assert!(!draft.reviewing());
        draft.back();
        draft.back();
        assert_eq!(draft.step, 0);
    }
}
