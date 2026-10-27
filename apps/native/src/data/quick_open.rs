//! `rankQuickOpenFiles` (`file-quick-open.tsx`): the checkout's tracked
//! files for a query, best first, 60 at most. A literal path or file name
//! wins; then a path that contains the query; then a fuzzy subsequence,
//! scored up for runs and for letters inside the file name.

/// `MAX_RESULTS`
pub const MAX_RESULTS: usize = 60;

pub fn basename(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

pub fn dirname(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// `fuzzyScore`: −1 when the query is not a subsequence of the path.
fn fuzzy_score(path: &str, query: &str, basename_start: usize) -> i64 {
    let hay: Vec<char> = path.chars().flat_map(char::to_lowercase).collect();
    let mut score = 0;
    let mut from = 0;
    let mut previous: Option<usize> = None;
    for letter in query.chars().flat_map(char::to_lowercase) {
        let Some(found) = hay[from.min(hay.len())..]
            .iter()
            .position(|candidate| *candidate == letter)
            .map(|offset| from + offset)
        else {
            return -1;
        };
        score += 1;
        // A consecutive run.
        if previous.is_some_and(|previous| found == previous + 1) {
            score += 4;
        }
        // Inside the file name.
        if found >= basename_start {
            score += 3;
        }
        previous = Some(found);
        from = found + 1;
    }
    score
}

/// The paths to show for `query` (already trimmed), in order.
pub fn rank(paths: &[String], query: &str) -> Vec<String> {
    if query.is_empty() {
        return paths.iter().take(MAX_RESULTS).cloned().collect();
    }
    let wanted = query.to_lowercase();
    let mut scored: Vec<(&String, i64)> = paths
        .iter()
        .filter_map(|path| {
            let lower = path.to_lowercase();
            let score = if lower == wanted {
                10_000
            } else if basename(&lower) == wanted {
                9_000
            } else if lower.contains(&wanted) {
                8_000
            } else {
                let start = match path.rsplit_once('/') {
                    Some((dir, _)) => dir.chars().count() + 1,
                    None => 0,
                };
                fuzzy_score(path, query, start)
            };
            (score >= 0).then_some((path, score))
        })
        .collect();
    // Stable, like `Array.prototype.sort`: equal paths keep the listing's order.
    scored.sort_by(|(a, a_score), (b, b_score)| {
        b_score
            .cmp(a_score)
            .then(a.chars().count().cmp(&b.chars().count()))
    });
    scored
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(path, _)| path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|path| path.to_string()).collect()
    }

    #[test]
    fn a_literal_match_comes_first() {
        let files = paths(&[
            "src/deep/config/loader.rs",
            "docs/config.md",
            "config.rs",
            "src/config.rs",
        ]);
        let ranked = rank(&files, "src/config.rs");
        assert_eq!(ranked[0], "src/config.rs");
        // Same file name: exact name beats containing it.
        let ranked = rank(&files, "config.rs");
        assert_eq!(ranked[0], "config.rs");
        assert_eq!(ranked[1], "src/config.rs");
    }

    #[test]
    fn fuzzy_rewards_runs_inside_the_file_name() {
        let files = paths(&["src/main.rs", "shell/mod.rs", "src/screens/shell/mod.rs"]);
        // "shmod": no literal match; the shorter path wins the tie.
        let ranked = rank(&files, "shmod");
        assert_eq!(ranked, ["shell/mod.rs", "src/screens/shell/mod.rs"]);
        assert!(rank(&files, "zzz").is_empty());
        assert!(fuzzy_score("ab/cd", "dc", 3) < 0);
        assert_eq!(fuzzy_score("ab/cd", "cd", 3), 1 + 3 + 1 + 4 + 3);
    }

    #[test]
    fn an_empty_query_lists_the_first_sixty() {
        let files: Vec<String> = (0..100).map(|index| format!("f{index}.rs")).collect();
        let ranked = rank(&files, "");
        assert_eq!(ranked.len(), MAX_RESULTS);
        assert_eq!(ranked[0], "f0.rs");
        assert_eq!(basename("a/b/c.rs"), "c.rs");
        assert_eq!(dirname("a/b/c.rs"), "a/b");
        assert_eq!(dirname("c.rs"), "");
    }
}
