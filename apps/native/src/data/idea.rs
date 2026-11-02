//! A Researcher's idea (`idea-status.ts`, `research-api.ts`): its task is a
//! folder in `~/dcc-ideias`; DCC reads how far the research got from the
//! fixed headings of its `IDEIA.md` (the agent never says so itself), and
//! the idea leaves either published (a GitHub repository, the folder moved
//! next to the projects, a project of its own) or discarded (task,
//! conversation, records and folder). The parse is tolerant: what it does
//! not recognise leaves the milestone open.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::data::desktop_api::{ApiError, DesktopApi};

pub const IDEA_FILE: &str = "IDEIA.md";
/// `IDEA_MILESTONES`, each `agents.research.milestones.<key>`.
pub const MILESTONES: [&str; 5] = ["problem", "audience", "references", "hypotheses", "name"];
const MIN_REFERENCES: usize = 3;

/// Whether a task's folder is an idea in progress (every idea is made in
/// `~/dcc-ideias`; one published moves out of it).
pub fn is_idea_root(path: &str) -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    Path::new(path).starts_with(Path::new(&home).join(dcc_core::domain::idea::IDEAS_DIR_NAME))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdeaStatus {
    /// `# Ideia: …`; none while it is still empty.
    pub title: Option<String>,
    /// Filled, in `MILESTONES` order.
    pub milestones: [bool; 5],
    /// `seguir`, `pivotar` or `descartar`.
    pub verdict: Option<String>,
    pub problem: String,
    pub audience: String,
    pub mvp: String,
    /// The candidates, the favourite (★) first.
    pub names: Vec<String>,
    pub favorite_name: Option<String>,
    pub has_architecture: bool,
    pub has_roadmap: bool,
}

impl IdeaStatus {
    pub fn filled_count(&self) -> usize {
        self.milestones.iter().filter(|filled| **filled).count()
    }

    /// Every milestone filled and the verdict "seguir": the card opens by itself.
    pub fn ready(&self) -> bool {
        self.filled_count() == MILESTONES.len() && self.verdict.as_deref() == Some("seguir")
    }
}

/// Lower case without accents ("Referências" → "referencias").
fn fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            'ñ' | 'Ñ' => 'n',
            other => other.to_ascii_lowercase(),
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Each `## ` section's body, by its folded heading.
fn sections(markdown: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut current: Option<(String, Vec<&str>)> = None;
    for line in markdown.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if let Some((name, body)) = current.take() {
                found.push((name, body.join("\n").trim().to_string()));
            }
            current = Some((fold(heading.trim_end_matches('#')), Vec::new()));
        } else if let Some((_, body)) = current.as_mut() {
            body.push(line);
        }
    }
    if let Some((name, body)) = current {
        found.push((name, body.join("\n").trim().to_string()));
    }
    found
}

fn list_items(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            ["- ", "* ", "+ "]
                .iter()
                .find_map(|bullet| line.strip_prefix(bullet))
                .map(|item| item.trim().to_string())
        })
        .filter(|item| !item.is_empty())
        .collect()
}

/// A section's text without `<!-- -->` comments, on one line.
fn plain(body: &str) -> String {
    let mut text = String::new();
    let mut rest = body;
    while let Some(start) = rest.find("<!--") {
        text.push_str(&rest[..start]);
        rest = rest[start..]
            .find("-->")
            .map_or("", |end| &rest[start + end + 3..]);
    }
    text.push_str(rest);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_markup(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '*' | '_' | '`'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// `parseIdeaStatus`
pub fn parse(markdown: &str) -> IdeaStatus {
    let found = sections(markdown);
    let section = |name: &str| {
        let wanted = fold(name);
        found
            .iter()
            .find(|(heading, _)| *heading == wanted)
            .map(|(_, body)| body.as_str())
            .unwrap_or("")
    };
    let title = markdown.lines().find_map(|line| {
        let rest = line.strip_prefix("# ")?.trim_start();
        let (word, value) = rest.split_once(':')?;
        (fold(word) == "ideia")
            .then(|| strip_markup(value))
            .filter(|value| !value.is_empty())
    });
    let problem = plain(section("Problema"));
    let audience = plain(section("Para quem"));
    let references = list_items(section("Referências"))
        .iter()
        .filter(|item| item.contains("http://") || item.contains("https://"))
        .count();
    let hypotheses: Vec<String> = list_items(section("Hipóteses"))
        .into_iter()
        .filter_map(|item| {
            let state = item
                .strip_prefix('[')?
                .split_once(']')?
                .0
                .trim()
                .to_lowercase();
            ["aberta", "confirmada", "derrubada"]
                .contains(&state.as_str())
                .then_some(state)
        })
        .collect();
    let (mut names, mut favorite_name) = (Vec::<String>::new(), None);
    for item in list_items(section("Nome")) {
        let favorite = item.starts_with('★');
        let item = item.trim_start_matches('★').trim();
        let cut = [" — ", " – ", " - "]
            .iter()
            .filter_map(|separator| item.find(separator))
            .min()
            .unwrap_or(item.len());
        let name = strip_markup(&item[..cut]);
        if name.is_empty() {
            continue;
        }
        if favorite && favorite_name.is_none() {
            favorite_name = Some(name.clone());
            names.insert(0, name);
        } else if !names.contains(&name) {
            names.push(name);
        }
    }
    let verdict_line = strip_markup(
        section("Veredito")
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or(""),
    )
    .to_lowercase();
    let verdict = ["seguir", "pivotar", "descartar"]
        .into_iter()
        .find(|candidate| verdict_line.starts_with(candidate))
        .map(str::to_string);
    IdeaStatus {
        title,
        milestones: [
            !problem.is_empty(),
            !audience.is_empty(),
            references >= MIN_REFERENCES,
            !hypotheses.is_empty() && hypotheses.iter().all(|state| state != "aberta"),
            favorite_name.is_some(),
        ],
        verdict,
        problem,
        audience,
        mvp: plain(section("MVP")),
        names,
        favorite_name,
        has_architecture: !plain(section("Arquitetura e stack")).is_empty(),
        has_roadmap: section("Roadmap")
            .lines()
            .any(|line| line.starts_with("### ") && line.len() > 4),
    }
}

/// A GitHub repository name, which is also the project's folder name.
pub fn is_valid_repository_name(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && name != "."
        && name != ".."
}

/// `repositoryNameFromCandidate`: "Salte Saúde" → "salte-saude".
pub fn repository_name(candidate: &str) -> String {
    let mut name = String::new();
    for c in fold(candidate).chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            name.push(c);
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    name.trim_matches('-').chars().take(100).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Private,
    Public,
}

/// `PublishIdeaInput`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publish {
    pub root_path: String,
    pub name: String,
    pub destination: String,
    pub visibility: Visibility,
    /// Public only: `docs/IDEIA.md` stays local, out of every commit.
    pub keep_idea_local: bool,
}

/// `PublishedIdea`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Published {
    pub root_path: String,
    pub repository_url: Option<String>,
}

impl DesktopApi {
    /// `research_publish_idea`: the repository, the move, the project.
    pub fn publish_idea(&self, publish: &Publish) -> Result<Published, ApiError> {
        Self::decoded(self.post(
            "/api/v1/research/ideas/publish",
            &json!({
                "rootPath": publish.root_path,
                "name": publish.name,
                "destination": publish.destination,
                "visibility": publish.visibility,
                "keepIdeaLocal": publish.keep_idea_local && publish.visibility == Visibility::Public,
            }),
        )?)
    }

    /// `research_discard_idea`: nothing of the idea is kept.
    pub fn discard_idea(&self, root_path: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/research/ideas/discard",
            &json!({ "rootPath": root_path }),
        )
        .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDEA: &str = "# Ideia: **Salte** Saúde\n\n## Problema\n<!-- dica -->\nAgendar exame é lento.\n\n## Para quem\nClínicas pequenas.\n\n## Referências\n- https://a.dev\n- https://b.dev\n- sem link\n- https://c.dev\n\n## Hipóteses\n- [confirmada] pagam por agenda\n- [derrubada] usam papel\n\n## Nome\n- Agenda Já — curto\n- ★ Salte Saúde - favorito\n\n## Veredito\n**Seguir** com o MVP\n\n## MVP\nAgenda online.\n\n## Arquitetura e stack\nRust.\n\n## Roadmap\n### Fase 1\n";

    #[test]
    fn a_complete_idea_reads_ready_with_its_favourite_first() {
        let status = parse(IDEA);
        assert_eq!(status.title.as_deref(), Some("Salte Saúde"));
        assert_eq!(status.milestones, [true; 5]);
        assert_eq!(status.verdict.as_deref(), Some("seguir"));
        assert_eq!(status.problem, "Agendar exame é lento.");
        assert_eq!(status.names, ["Salte Saúde", "Agenda Já"]);
        assert_eq!(status.favorite_name.as_deref(), Some("Salte Saúde"));
        assert!(status.has_architecture && status.has_roadmap && status.ready());
    }

    #[test]
    fn an_open_hypothesis_or_too_few_references_keep_the_milestone_open() {
        let status = parse(
            "## Referências\n- https://a.dev\n\n## Hipóteses\n- [aberta] x\n- [confirmada] y\n",
        );
        assert_eq!(status.milestones, [false, false, false, false, false]);
        assert_eq!(status.filled_count(), 0);
        assert!(!parse("").ready());
    }

    #[test]
    fn a_candidate_becomes_a_repository_name() {
        assert_eq!(repository_name("Salte Saúde"), "salte-saude");
        assert_eq!(repository_name("  Ação & Reação!  "), "acao-reacao");
        assert!(is_valid_repository_name("salte-saude"));
        assert!(!is_valid_repository_name("com espaço"));
        assert!(!is_valid_repository_name(".."));
        assert!(!is_valid_repository_name(""));
    }

    #[test]
    fn only_a_folder_in_the_ideas_dir_is_an_idea() {
        let home = std::env::var("HOME").unwrap();
        assert!(is_idea_root(&format!("{home}/dcc-ideias/ideia-3")));
        assert!(!is_idea_root(&format!("{home}/projetos/app")));
    }
}
