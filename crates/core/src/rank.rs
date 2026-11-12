//! How well a name matches what was typed, so the best match comes first: typing `develop` puts
//! `develop` and `origin/develop` before `feature/DCT-0555-sync-develop`. Every word must appear
//! (case-insensitively); the whole query then decides the tier. A word that does not appear as it
//! was typed may still match the starts of the name's words, in order, as other IDEs do:
//! `pfmopenbanking` finds `self/pfm-history-open-banking-sheet`, `lvm` `LoginViewModel` — after
//! every name that holds it as typed.

/// A lower score is a better match; `None` when some word is missing.
pub fn score(query: &str, hay: &str) -> Option<(u8, usize)> {
    let hay_l = hay.to_lowercase();
    let query = query.trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    if !words.iter().all(|w| word_matches(w, hay)) {
        return None;
    }
    // A word found only by the starts of the name's words: after every name that holds them all.
    if !words.iter().all(|w| hay_l.contains(w)) {
        return Some((6, hay.len()));
    }
    let last = hay_l.rsplit('/').next().unwrap_or(&hay_l);
    let segments: Vec<&str> = hay_l.split(['/', '-', '_', '.', ' ']).collect();
    let tier = if hay_l == query {
        0
    } else if last == query {
        1
    } else if hay_l.starts_with(query.as_str()) || last.starts_with(query.as_str()) {
        2
    } else if segments.contains(&query.as_str()) {
        3
    } else if segments.iter().any(|s| s.starts_with(query.as_str())) {
        4
    } else {
        5
    };
    Some((tier, hay.len()))
}

/// Whether the typed `word` is in `hay`: as typed (ignoring case), or — three letters or more —
/// as the starts of `hay`'s words, in order (`pfmob`: **pfm**-**o**pen-**b**anking).
pub fn word_matches(word: &str, hay: &str) -> bool {
    let word = word.to_lowercase();
    if hay.to_lowercase().contains(&word) {
        return true;
    }
    let typed: Vec<char> = word.chars().filter(|c| c.is_alphanumeric()).collect();
    typed.len() >= 3 && starts_match(&typed, &words_of(hay))
}

/// Where the typed `word` matched `hay`, as byte ranges of `hay` — what a row marks (#191): each place it is found
/// as typed (ignoring case), else the starts of the words it matched by (`pfmob`: `pfm`, `o`, `b`). None where
/// lower case changes `hay`'s length.
pub fn word_spans(word: &str, hay: &str) -> Vec<std::ops::Range<usize>> {
    let word = word.to_lowercase();
    let lower = hay.to_lowercase();
    if word.is_empty() || lower.len() != hay.len() {
        return Vec::new();
    }
    let found: Vec<_> = (lower.match_indices(&word))
        .map(|(at, w)| at..at + w.len())
        .collect();
    if !found.is_empty() {
        return found;
    }
    let typed: Vec<char> = word.chars().filter(|c| c.is_alphanumeric()).collect();
    if typed.len() < 3 {
        return Vec::new();
    }
    starts_spans(&typed, &words_at(hay)).unwrap_or_default()
}

/// A word of a name: each of its characters lower case, with the bytes of the name it came from.
type Placed = Vec<(std::ops::Range<usize>, char)>;

/// `hay`'s words, lower case: split at anything not a letter or digit, and where a lower-case
/// letter or a digit meets an upper-case one (`LoginViewModel`: login, view, model).
fn words_of(hay: &str) -> Vec<Vec<char>> {
    (words_at(hay).into_iter())
        .map(|w| w.into_iter().map(|(_, c)| c).collect())
        .collect()
}

/// `words_of`, each character with where it is in `hay`.
fn words_at(hay: &str) -> Vec<Placed> {
    let mut words: Vec<Placed> = Vec::new();
    let mut word: Placed = Vec::new();
    let mut before: Option<char> = None;
    for (at, c) in hay.char_indices() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            let hump =
                c.is_uppercase() && before.is_some_and(|b| b.is_lowercase() || b.is_ascii_digit());
            if hump && !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            let bytes = at..at + c.len_utf8();
            word.extend(c.to_lowercase().map(|l| (bytes.clone(), l)));
        }
        before = Some(c);
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The starts `starts_match` finds `typed` by, as ranges of the name: the first way it finds.
fn starts_spans(typed: &[char], words: &[Placed]) -> Option<Vec<std::ops::Range<usize>>> {
    if typed.is_empty() {
        return Some(Vec::new());
    }
    words.iter().enumerate().find_map(|(n, word)| {
        let shared = (word.iter().zip(typed))
            .take_while(|((_, a), b)| a == *b)
            .count();
        (1..=shared).rev().find_map(|k| {
            let mut rest = starts_spans(&typed[k..], &words[n + 1..])?;
            rest.insert(0, word[0].0.start..word[k - 1].0.end);
            Some(rest)
        })
    })
}

/// Whether `typed` is the start of one word, then of a later one, and so on, all of it used.
fn starts_match(typed: &[char], words: &[Vec<char>]) -> bool {
    if typed.is_empty() {
        return true;
    }
    words.iter().enumerate().any(|(n, word)| {
        // The longest start first: `open` before `o`.
        let shared = word.iter().zip(typed).take_while(|(a, b)| a == b).count();
        (1..=shared)
            .rev()
            .any(|k| starts_match(&typed[k..], &words[n + 1..]))
    })
}

/// `items` that match `query`, best first; equal scores keep their order.
pub fn rank<T>(
    items: impl IntoIterator<Item = T>,
    query: &str,
    hay: impl Fn(&T) -> &str,
) -> Vec<T> {
    let mut scored: Vec<((u8, usize), T)> = items
        .into_iter()
        .filter_map(|item| Some((score(query, hay(&item))?, item)))
        .collect();
    scored.sort_by_key(|(s, _)| *s);
    scored.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_itself_beats_names_that_contain_it() {
        let names = [
            "feature/DCT-0555-sync-develop",
            "origin/feature/ST-37965-analysis-of-develop-changes",
            "origin/develop",
            "develop-old",
            "develop",
            "feature/developer-tools",
        ];
        let ranked = rank(names, "develop", |n| n);
        assert_eq!(
            ranked,
            [
                "develop",
                "origin/develop",
                "develop-old",
                // The branch's own name starts with it.
                "feature/developer-tools",
                "feature/DCT-0555-sync-develop",
                "origin/feature/ST-37965-analysis-of-develop-changes",
            ]
        );
    }

    #[test]
    fn a_word_is_marked_where_it_matched() {
        assert_eq!(word_spans("LOG", "Refactor the login"), vec![13..16]);
        assert_eq!(
            word_spans("juha", "Juniper Hale"),
            vec![0..2, 8..10],
            "by its initials"
        );
        assert_eq!(
            word_spans("lvm", "LoginViewModel.kt"),
            vec![0..1, 5..6, 9..10]
        );
        assert!(word_spans("jh", "Juniper Hale").is_empty(), "too short");
        assert!(word_spans("xyz", "Juniper Hale").is_empty());
    }

    #[test]
    fn every_word_must_match() {
        assert!(score("pfm history", "self/pfm-responsive-02-history").is_some());
        assert!(score("pfm analytics", "self/pfm-responsive-02-history").is_none());
        assert_eq!(score("DEVELOP", "develop"), Some((0, 7)));
    }

    #[test]
    fn the_starts_of_its_words_find_a_name() {
        let branch = "self/pfm-history-open-banking-sheet";
        assert!(score("pfmopenbanking", branch).is_some());
        assert!(score("pfmob", branch).is_some());
        assert!(score("pfm-open", branch).is_some());
        assert!(score("historysheet pfm", branch).is_some());
        // In the name's order only, and every letter used.
        assert!(score("bankingpfm", branch).is_none());
        assert!(score("pfmopenx", branch).is_none());
        // Its humps are words too.
        assert!(score("lvm", "feature/login/LoginViewModel.kt").is_some());
        assert!(score("loginvm", "LoginViewModel.kt").is_some());
        // Two letters only as typed: they would find too much.
        assert!(score("ob", branch).is_none());
    }

    #[test]
    fn a_name_that_holds_it_as_typed_comes_first() {
        let names = [
            "self/pfm-history-open-banking-sheet",
            "feature/pfmopenbanking-v2",
        ];
        assert_eq!(
            rank(names, "pfmopenbanking", |n| n),
            [
                "feature/pfmopenbanking-v2",
                "self/pfm-history-open-banking-sheet"
            ]
        );
    }
}
