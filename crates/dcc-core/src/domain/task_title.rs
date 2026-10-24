//! A new task's name from its first prompt (`task-title.ts`): the first
//! meaningful line, without composer commands, list markers or the polite
//! opening ("Preciso que você…"), up to eight words or 56 characters.
//! Deterministic: no model writes it.

const MAX_TASK_TITLE_LENGTH: usize = 56;
const MAX_TASK_TITLE_WORDS: usize = 8;
const AUTOMATIC_TASK_TITLES: [&str; 2] = ["new task", "nova tarefa"];

/// `isAutomaticTaskTitle`: empty, or a localized placeholder.
pub fn is_automatic_task_title(title: Option<&str>) -> bool {
    let normalized = title
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    normalized.is_empty() || AUTOMATIC_TASK_TITLES.contains(&normalized.as_str())
}

/// Drops ```fenced``` blocks.
fn without_fences(prompt: &str) -> String {
    let mut out = String::new();
    let mut rest = prompt;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let Some(end) = after.find("```") else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push(' ');
        rest = &after[end + 3..];
    }
    out.push_str(rest);
    out
}

/// `^\/(?:plan|compact|review|spec)\b\s*`, any case.
fn without_command(line: &str) -> &str {
    let Some(rest) = line.strip_prefix('/') else {
        return line;
    };
    for command in ["plan", "compact", "review", "spec"] {
        if rest.len() >= command.len() && rest[..command.len()].eq_ignore_ascii_case(command) {
            let after = &rest[command.len()..];
            if after
                .chars()
                .next()
                .is_none_or(|next| !next.is_alphanumeric() && next != '_')
            {
                return after.trim_start();
            }
        }
    }
    line
}

/// `^[-*#>\d.)\s]+`
fn without_markers(line: &str) -> &str {
    line.trim_start_matches(|character: char| {
        matches!(character, '-' | '*' | '#' | '>' | '.' | ')')
            || character.is_ascii_digit()
            || character.is_whitespace()
    })
}

/// Case-insensitive word prefix: each word matches, separated by spaces.
fn strip_words<'a>(candidate: &'a str, words: &[&str]) -> Option<&'a str> {
    let mut rest = candidate;
    for (index, word) in words.iter().enumerate() {
        let alternatives: Vec<&str> = word.split('|').collect();
        let matched = alternatives.iter().find(|alternative| {
            rest.get(..alternative.len())
                .is_some_and(|head| head.to_lowercase() == alternative.to_lowercase())
        })?;
        rest = &rest[matched.len()..];
        if index + 1 < words.len() {
            let trimmed = rest.trim_start();
            if trimmed.len() == rest.len() {
                return None;
            }
            rest = trimmed;
        }
    }
    Some(rest)
}

/// `REQUEST_PREFIXES`, the first that matches.
fn without_request_prefix(candidate: &str) -> String {
    // Prefixes followed by whitespace (`\s+`).
    let spaced: [&[&str]; 7] = [
        &["preciso", "que", "você|voce"],
        &["quero", "que", "você|voce"],
        &["você|voce", "pode"],
        &["pode"],
        &["i", "need", "you", "to"],
        &["i", "want", "you", "to"],
        &["can|could|would", "you"],
    ];
    // Prefixes followed by `[,\s:]*`.
    let loose: [&[&str]; 2] = [&["por", "favor"], &["please"]];
    let mut ordered: Vec<(&[&str], bool)> = Vec::new();
    ordered.push((loose[0], true));
    ordered.extend(spaced[..4].iter().map(|words| (*words, false)));
    ordered.push((loose[1], true));
    ordered.extend(spaced[4..].iter().map(|words| (*words, false)));
    for (words, loose) in ordered {
        let Some(rest) = strip_words(candidate, words) else {
            continue;
        };
        let rest = if loose {
            rest.trim_start_matches(|character: char| {
                character == ',' || character == ':' || character.is_whitespace()
            })
        } else {
            let trimmed = rest.trim_start();
            if trimmed.len() == rest.len() {
                continue;
            }
            trimmed
        };
        let stripped = rest.trim();
        if stripped != candidate {
            return stripped.to_string();
        }
    }
    candidate.to_string()
}

fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

/// `deriveTaskTitle`
pub fn derive_task_title(prompt: &str, fallback: &str) -> String {
    let unfenced = without_fences(prompt);
    let line = unfenced
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let candidate = without_markers(without_command(line))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let candidate = without_request_prefix(&candidate);
    // The first sentence: up to a `.`, `!` or `?` followed by whitespace.
    let mut end = candidate.len();
    let mut previous = None;
    for (index, character) in candidate.char_indices() {
        if character.is_whitespace() && matches!(previous, Some('.' | '!' | '?')) {
            end = index;
            break;
        }
        previous = Some(character);
    }
    let sentence = candidate[..end]
        .trim_end_matches(['.', '!', '?', ',', ';', ':'])
        .trim();
    let words: Vec<&str> = sentence.split_whitespace().collect();
    let mut title = words
        .iter()
        .take(MAX_TASK_TITLE_WORDS)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    // JavaScript measures UTF-16 units; titles here are text, so chars.
    if title.chars().count() > MAX_TASK_TITLE_LENGTH {
        let cut: String = title.chars().take(MAX_TASK_TITLE_LENGTH - 1).collect();
        title = format!("{}…", cut.trim_end());
    } else if words.len() > MAX_TASK_TITLE_WORDS {
        title.push('…');
    }
    capitalize(if title.is_empty() { fallback } else { &title })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_conversational_portuguese_boilerplate() {
        assert_eq!(
            derive_task_title(
                "Preciso que você corrija o login do checkout",
                "Nova tarefa"
            ),
            "Corrija o login do checkout"
        );
        assert_eq!(
            derive_task_title("Por favor, revise o PR", "Nova tarefa"),
            "Revise o PR"
        );
        assert_eq!(
            derive_task_title("Can you fix the build? Then ship it.", "Nova tarefa"),
            "Fix the build"
        );
        assert_eq!(
            derive_task_title("Podemos ver isso", "Nova tarefa"),
            "Podemos ver isso"
        );
    }

    #[test]
    fn uses_the_first_meaningful_line_and_strips_composer_commands() {
        assert_eq!(
            derive_task_title(
                "\n/plan integrar pedidos ao Melhor Envio\nCom segurança",
                "Nova tarefa"
            ),
            "Integrar pedidos ao Melhor Envio"
        );
        assert_eq!(
            derive_task_title("```\ncode\n```\n- 1) ajustar o menu.", "Nova tarefa"),
            "Ajustar o menu"
        );
        assert_eq!(
            derive_task_title("/planejar x", "Nova tarefa"),
            "/planejar x"
        );
        assert_eq!(derive_task_title("   ", "Nova tarefa"), "Nova tarefa");
    }

    #[test]
    fn keeps_sidebar_titles_compact() {
        let title = derive_task_title(
            "Analise cuidadosamente toda a arquitetura existente e implemente uma solução robusta para autenticação compartilhada",
            "Nova tarefa",
        );
        assert!(title.ends_with('…'), "{title}");
        assert!(title.chars().count() <= 57, "{title}");
        let long_words = derive_task_title(&"palavra ".repeat(3), "Nova tarefa");
        assert_eq!(long_words, "Palavra palavra palavra");
        let over = derive_task_title(
            "Eixo-com-um-nome-muito-comprido-que-passa-de-cinquenta-e-seis-letras",
            "x",
        );
        assert_eq!(over.chars().count(), 56);
    }

    #[test]
    fn recognizes_localized_provisional_task_titles() {
        assert!(is_automatic_task_title(Some("Nova tarefa")));
        assert!(is_automatic_task_title(Some("  NEW   TASK ")));
        assert!(is_automatic_task_title(None));
        assert!(!is_automatic_task_title(Some("Corrigir autenticação")));
    }

    /// What the desktop's `deriveTaskTitle` answers for the same prompts
    /// (generated from `task-title.ts`).
    #[test]
    fn matches_the_desktop_title_for_the_same_prompts() {
        let cases = [
            ("Preciso que você corrija o login do checkout", "Corrija o login do checkout"),
            ("\n/plan integrar pedidos ao Melhor Envio\nCom segurança", "Integrar pedidos ao Melhor Envio"),
            ("Analise cuidadosamente toda a arquitetura existente e implemente uma solução robusta para autenticação compartilhada", "Analise cuidadosamente toda a arquitetura existente e i…"),
            ("Por favor, revise o PR", "Revise o PR"),
            ("Can you fix the build? Then ship it.", "Fix the build"),
            ("Podemos ver isso", "Podemos ver isso"),
            ("```\ncode\n```\n- 1) ajustar o menu.", "Ajustar o menu"),
            ("/planejar x", "/planejar x"),
            ("   ", "Nova tarefa"),
            ("Você pode olhar o arquivo src/app.rs?", "Olhar o arquivo src/app.rs"),
            ("quero que voce faça X: e Y", "Faça X: e Y"),
            ("PLEASE: deploy now!!!", "Deploy now"),
            ("# Título\ncorpo", "Título"),
            ("1. primeiro passo. segundo", "Primeiro passo"),
            ("/Review o diff", "O diff"),
            ("i need you to rename the module", "Rename the module"),
            ("Eixo-com-um-nome-muito-comprido-que-passa-de-cinquenta-e-seis-letras", "Eixo-com-um-nome-muito-comprido-que-passa-de-cinquenta-…"),
            ("olá mundo... de novo", "Olá mundo"),
            ("/spec\nx", "Nova tarefa"),
            ("   - * > texto", "Texto"),
        ];
        for (prompt, expected) in cases {
            assert_eq!(
                derive_task_title(prompt, "Nova tarefa"),
                expected,
                "{prompt:?}"
            );
        }
    }
}
