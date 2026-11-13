//! How the changes are laid out: a compact row per file (the default; a
//! click opens the diff on the stage), the same rows under their folders,
//! or a card per file with its diff inside. One button goes round them.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    #[default]
    List,
    Tree,
    Cards,
}

impl Layout {
    /// The layout the button goes to next.
    pub fn next(self) -> Self {
        match self {
            Layout::List => Layout::Tree,
            Layout::Tree => Layout::Cards,
            Layout::Cards => Layout::List,
        }
    }

    /// The button shows where it goes: its icon and its tooltip's key.
    pub fn button(self) -> (&'static str, &'static str) {
        match self.next() {
            Layout::List => ("list", "inspector.changes.listView"),
            Layout::Tree => ("list-tree", "inspector.changes.treeView"),
            Layout::Cards => ("file-diff", "inspector.changes.cardsView"),
        }
    }

    /// A row per file, not a card.
    pub fn rows(self) -> bool {
        self != Layout::Cards
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_goes_round_the_three_layouts_from_the_list() {
        assert_eq!(Layout::default(), Layout::List);
        assert_eq!(Layout::List.next().next().next(), Layout::List);
        assert_eq!(Layout::List.button().0, "list-tree");
        assert!(Layout::Tree.rows() && !Layout::Cards.rows());
    }
}
