/// Whether `id` is a valid content id: a lowercase letter, then lowercase letters, digits or
/// underscores, like `captain_hale`. Ids are typed in the CLI, so they're kept plain.
pub fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A misspelt word and the nearest one known, as a "did you mean" offers it (U6c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub wrong: String,
    pub right: String,
}

impl Suggestion {
    pub fn new(wrong: &str, right: &str) -> Suggestion {
        Suggestion {
            wrong: wrong.to_owned(),
            right: right.to_owned(),
        }
    }
}

/// The candidate closest to `word`, if it's close enough to be a likely typo: for "did you
/// mean …?" hints. Ties go to the earliest candidate, so hints are deterministic. Close
/// enough means at most one edit per three letters, and always at least one; swapping two
/// neighbouring letters counts as one edit.
pub fn suggest<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let allowed = (word.chars().count() / 3).max(1);
    candidates
        .into_iter()
        .map(|candidate| (strsim::damerau_levenshtein(word, candidate), candidate))
        .filter(|(edits, _)| *edits <= allowed)
        .min_by_key(|(edits, _)| *edits)
        .map(|(_, candidate)| candidate)
}

/// "a" or "an", for an id in a sentence: "a captain", "an initiate". Ids are lowercase, so
/// a leading vowel is all that matters.
pub fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn articles_suit_the_word() {
        assert_eq!(
            (article("captain"), article("initiate"), article("acolyte")),
            ("a", "an", "an")
        );
    }

    #[test]
    fn ids_are_lowercase_letters_digits_and_underscores_starting_with_a_letter() {
        for good in ["vex", "captain_hale", "vex2", "a"] {
            assert!(is_valid_id(good), "{good:?} should be valid");
        }
        for bad in ["", "Vex", "2vex", "_vex", "vex-2", "vex hale", "véx"] {
            assert!(!is_valid_id(bad), "{bad:?} should be invalid");
        }
    }

    #[test]
    fn suggests_the_closest_candidate() {
        assert_eq!(
            suggest("alignmnet", ["name", "alignment"]),
            Some("alignment")
        );
        assert_eq!(suggest("vx", ["player", "vex"]), Some("vex"));
        assert_eq!(suggest("stael", ["steal", "help_stranger"]), Some("steal"));
    }

    #[test]
    fn suggests_nothing_when_no_candidate_is_close() {
        assert_eq!(suggest("zzz", ["vex"]), None);
        assert_eq!(suggest("vex", []), None);
    }

    #[test]
    fn allows_about_one_edit_per_three_letters() {
        assert_eq!(suggest("abcdef", ["abcdxy"]), Some("abcdxy")); // 2 edits in 6 letters
        assert_eq!(suggest("abcdef", ["abcxyz"]), None); // 3 edits is too many
        assert_eq!(suggest("ab", ["ax"]), Some("ax")); // always at least 1
    }

    #[test]
    fn breaks_ties_by_candidate_order() {
        assert_eq!(suggest("ab", ["ac", "ad"]), Some("ac"));
        assert_eq!(suggest("ab", ["ad", "ac"]), Some("ad"));
    }
}
