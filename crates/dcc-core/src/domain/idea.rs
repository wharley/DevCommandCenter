//! Ideas researched before a project exists.
//!
//! An idea is a small git repository under `~/dcc-ideias/`, registered as a
//! project but kept out of the project list. It starts empty: the person
//! describes it in the researcher's chat, and the researcher keeps its
//! `IDEIA.md`. Its folder name is provisional; the real name is chosen when it
//! is published (it becomes a real project). Otherwise it is discarded
//! (deleted).

use serde::{Deserialize, Serialize};
use specta::Type;

/// Folder under the person's home that holds the ideas in progress.
pub const IDEAS_DIR_NAME: &str = "dcc-ideias";
/// Provisional folder name of an idea: `ideia`, then `ideia-2`, `ideia-3`…
pub const IDEA_FOLDER_PREFIX: &str = "ideia";

/// The level-2 headings of `IDEIA.md`, in order. DCC reads the file by them,
/// so the researcher's role forbids renaming them.
pub const IDEA_HEADINGS: &[&str] = &[
    "Problema",
    "Para quem",
    "Referências",
    "Hipóteses",
    "Riscos",
    "Nome",
    "Veredito",
    "MVP",
    "Arquitetura e stack",
    "Roadmap",
    "Registro de decisões",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Idea {
    /// The idea's folder; also the id of its repository.
    pub root_path: String,
    pub created_at: String,
}

/// The sections of `IDEIA.md`, empty, below its title line.
pub fn idea_sections() -> String {
    IDEA_HEADINGS
        .iter()
        .map(|heading| format!("\n## {heading}\n"))
        .collect()
}

/// The first `IDEIA.md`, before the person has said anything: the researcher
/// rewrites the title line from their first message.
pub fn idea_template() -> String {
    format!("# Ideia\n{}", idea_sections())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_has_every_heading_in_order() {
        let text = idea_template();
        assert!(text.starts_with("# Ideia\n\n## Problema\n"));
        let mut from = 0;
        for heading in IDEA_HEADINGS {
            let at = text[from..]
                .find(&format!("\n## {heading}\n"))
                .unwrap_or_else(|| panic!("{heading}"));
            from += at + 1;
        }
    }
}
