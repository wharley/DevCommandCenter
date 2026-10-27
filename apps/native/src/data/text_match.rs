//! The desktop's in-dialog searches (`matchesHelpTopic`, the Settings
//! navigation): accents and case ignored (`normalize("NFD")` without the
//! combining marks), and every word of the query has to appear.

/// The text without accents, in lowercase. Covers the Latin letters the
/// catalogs use (Portuguese and English); other letters are only lowercased.
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|letter| match letter {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            other => other,
        })
        .collect()
}

/// Every whitespace-separated word of `query` is in `haystack`; an empty
/// query matches everything.
pub fn matches_all_words(haystack: &str, query: &str) -> bool {
    let haystack = fold(haystack);
    fold(query)
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_and_case_do_not_matter() {
        assert_eq!(fold("Configurações ÁGUA"), "configuracoes agua");
        assert!(matches_all_words("Revisão do código", "revisao"));
        assert!(matches_all_words("Revisão do código", "CODIGO rev"));
        assert!(!matches_all_words("Revisão do código", "revisao terminal"));
        assert!(matches_all_words("anything", "   "));
    }
}
