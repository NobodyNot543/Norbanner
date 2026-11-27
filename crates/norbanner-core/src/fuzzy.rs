//! Fuzzy matching for the launcher: every query character must appear in
//! order in the candidate. Consecutive runs, word starts and an early first
//! match score higher, so "vsc" ranks "Visual Studio Code" above "Avast
//! Secure Browser".

/// Lowercases and strips accents so "configuracion" finds "Configuración".
#[must_use]
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// Score of `query` against `candidate` (both already [`fold`]ed), or
/// `None` when it does not match. Higher is better.
#[must_use]
pub fn score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let text: Vec<char> = candidate.chars().collect();
    let mut total = 0i32;
    let mut position = 0usize;
    let mut previous: Option<usize> = None;
    let mut first: Option<usize> = None;
    for wanted in query.chars().filter(|c| !c.is_whitespace()) {
        let found = (position..text.len()).find(|&i| text[i] == wanted)?;
        first.get_or_insert(found);
        let word_start = found == 0 || !text[found - 1].is_alphanumeric();
        total += 10;
        if word_start {
            total += 25;
        }
        if previous.is_some_and(|p| p + 1 == found) {
            total += 20;
        } else if let Some(p) = previous {
            total -= ((found - p) as i32).min(15);
        }
        previous = Some(found);
        position = found + 1;
    }
    if candidate.starts_with(query) {
        total += 60;
    } else if candidate.contains(query) {
        total += 30;
    }
    total -= first.unwrap_or(0).min(20) as i32;
    // Shorter names are more likely what was meant.
    total -= (text.len() as i32 / 8).min(10);
    Some(total)
}

/// The candidates matching `query`, best first. `key` gives each one's
/// folded search text.
pub fn rank<'a, T>(query: &str, items: &'a [T], key: impl Fn(&T) -> &str) -> Vec<(&'a T, i32)> {
    let query = fold(query.trim());
    let mut hits: Vec<(&T, i32)> = items
        .iter()
        .filter_map(|item| score(&query, key(item)).map(|s| (item, s)))
        .collect();
    hits.sort_by_key(|hit| std::cmp::Reverse(hit.1));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_characters_in_order() {
        assert!(score("vsc", "visual studio code").is_some());
        assert!(score("csv", "visual studio code").is_none());
    }

    #[test]
    fn prefers_word_starts_and_prefixes() {
        let code = score("vsc", "visual studio code").unwrap_or_default();
        let other = score("vsc", "avast secure browser").unwrap_or_default();
        assert!(code > other);
        let prefix = score("fire", "firefox").unwrap_or_default();
        let inside = score("fire", "bonfire maker").unwrap_or_default();
        assert!(prefix > inside);
    }

    #[test]
    fn folds_accents() {
        assert_eq!(fold("Configuración"), "configuracion");
    }
}
