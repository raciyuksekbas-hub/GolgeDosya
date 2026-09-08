//! What a full stop is doing.
//!
//! Before any spacing or punctuation rule can judge a `.`, it has to know what
//! that dot is for. Four of the roles below are not sentence punctuation at all,
//! and treating them as such is how `LL.M.` becomes `LL. M.` and how `Y..`
//! becomes `Y.` — both of which happened to a real petition.

use crate::text::tr_lower;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::ops::Range;

/// Abbreviations whose full stop belongs to the word.
///
/// Deliberately short and hand-written: every entry is a form that appears in
/// Turkish legal drafting, and the list is checked case-insensitively with
/// Turkish folding so `AV.` and `Av.` both match.
static ABBREVIATIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        // References into legislation and case law
        "m", "md", "mad", "f", "fk", "fkr", "bent", "b", "c", "e", "k", "t", "s", "sf", "sy", "bkz",
        "krş", "vd", "vb", "vs", "age", "agm", "par", "tar", "no", "nu", // Titles
        "av", "dr", "prof", "doç", "yrd", "öğr", "gör", "sn", "hz", "gen", "alb", "yzb",
        // Institutions and organisation forms
        "ltd", "şti", "aş", "tic", "san", "koop", "üni", "fak", "müd", "bşk", // General
        "örn", "ör", "yak", "yy", "çev", "der", "ed", "haz", "bkz",
    ]
    .into_iter()
    .collect()
});

/// The job a punctuation mark is doing at a given position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotRole {
    /// Ends a sentence. The only role the spacing rules may act on.
    SentenceEnd,
    /// Belongs to an abbreviation: `m.`, `Av.`, `vb.`
    Abbreviation,
    /// Internal to an initialism: `LL.M.`, `T.C.`, `A.Ş.`, `Ph.D.`
    Initialism,
    /// Part of an anonymised name in a quoted judgment: `Y..`, `Ö..`
    AnonymisedInitial,
    /// Separates the parts of a number or a date: `50.000`, `01.02.2020`
    NumberSeparator,
    /// The ordinal marker: `5. madde`
    Ordinal,
}

pub fn is_abbreviation(word: &str) -> bool {
    let folded = tr_lower(word.trim_matches(|c: char| !c.is_alphanumeric()));
    !folded.is_empty() && ABBREVIATIONS.contains(folded.as_str())
}

fn is_upper(c: char) -> bool {
    c.is_alphabetic() && crate::text::tr_upper(&c.to_string()) == c.to_string()
}

/// The span of the initialism token covering `index`, if there is one.
///
/// An initialism is two or more letter groups of at most three letters, each
/// closed by a dot, with nothing between them: `LL.M.`, `T.C.`, `Ph.D.`.
/// Requiring at least two groups is what keeps `Av. Raci` — a title followed by
/// a name — from being read as one token.
pub fn initialism_span(chars: &[char], index: usize) -> Option<Range<usize>> {
    // Walk left to the start of the token.
    let mut start = index;
    while start > 0 && (chars[start - 1].is_alphabetic() || chars[start - 1] == '.') {
        start -= 1;
    }
    // Walk right to its end.
    let mut end = index;
    while end < chars.len() && (chars[end].is_alphabetic() || chars[end] == '.') {
        end += 1;
    }
    if end <= start {
        return None;
    }

    // The token must be exactly a run of `letters.` groups.
    let mut groups = 0usize;
    let mut i = start;
    while i < end {
        let letters_start = i;
        while i < end && chars[i].is_alphabetic() {
            i += 1;
        }
        let letters = i - letters_start;
        if letters == 0 || letters > 3 {
            return None;
        }
        if !is_upper(chars[letters_start]) {
            return None;
        }
        if chars.get(i) != Some(&'.') || i >= end {
            return None; // a group not closed by a dot: not an initialism
        }
        i += 1;
        groups += 1;
    }
    if groups < 2 {
        return None;
    }
    Some(start..end)
}

/// The span of an anonymised initial covering `index`, if there is one.
///
/// Turkish judgments anonymise parties as `Y.. Ö..` — one or two capitals
/// followed by exactly two dots. The doubled dot is the convention, not a typo,
/// and collapsing it changes the text of a quoted judgment.
pub fn anonymised_initial_span(chars: &[char], index: usize) -> Option<Range<usize>> {
    // Find the run of dots containing or adjacent to `index`.
    let mut dot_start = index;
    while dot_start > 0 && chars[dot_start - 1] == '.' {
        dot_start -= 1;
    }
    let mut dot_end = index;
    while dot_end < chars.len() && chars[dot_end] == '.' {
        dot_end += 1;
    }
    // Exactly two dots. Three is an ellipsis and one is ordinary punctuation.
    if dot_end - dot_start != 2 {
        return None;
    }
    // One or two capitals immediately before them.
    let mut letters_start = dot_start;
    while letters_start > 0
        && chars[letters_start - 1].is_alphabetic()
        && dot_start - letters_start < 2
    {
        letters_start -= 1;
    }
    let letters = dot_start - letters_start;
    if letters == 0 || !chars[letters_start..dot_start].iter().all(|c| is_upper(*c)) {
        return None;
    }
    // And a boundary before that, so `ederiz..` cannot match on its last letter.
    if letters_start > 0 && chars[letters_start - 1].is_alphanumeric() {
        return None;
    }
    Some(letters_start..dot_end)
}

/// A short all-capitals token: `TCKN`, `HMK`, `TBK`, `VKN`.
fn is_acronym(word: &str) -> bool {
    let n = word.chars().count();
    (2..=5).contains(&n)
        && word.chars().all(|c| c.is_alphabetic())
        && crate::text::tr_upper(word) == word
}

/// What the punctuation mark at `index` is doing.
pub fn classify_dot(chars: &[char], index: usize) -> DotRole {
    if chars.get(index) != Some(&'.') {
        return DotRole::SentenceEnd;
    }
    if anonymised_initial_span(chars, index).is_some() {
        return DotRole::AnonymisedInitial;
    }
    if initialism_span(chars, index).is_some() {
        return DotRole::Initialism;
    }
    // `50.000` and `01.02.2020`: digits on both sides.
    let before_digit = index > 0 && chars[index - 1].is_ascii_digit();
    let after_digit = chars
        .get(index + 1)
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false);
    if before_digit && after_digit {
        return DotRole::NumberSeparator;
    }
    // The word in front of it.
    let mut start = index;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '\u{2019}') {
        start -= 1;
    }
    let word: String = chars[start..index].iter().collect();
    if word.chars().all(|c| c.is_ascii_digit()) && !word.is_empty() {
        return DotRole::Ordinal;
    }
    if is_abbreviation(&word) {
        return DotRole::Abbreviation;
    }
    // A short all-capitals token followed by a dot is an acronym being
    // abbreviated — `TCKN.`, `VKN.`, `HMK.` — not a sentence ending in a
    // shouted word. The length bound keeps `İSTANBUL.` out: a real word in
    // capitals is longer than any acronym in use here.
    if is_acronym(&word) {
        return DotRole::Abbreviation;
    }
    DotRole::SentenceEnd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn role(s: &str, needle: char, nth: usize) -> DotRole {
        let c = chars(s);
        let index = c
            .iter()
            .enumerate()
            .filter(|(_, ch)| **ch == needle)
            .map(|(i, _)| i)
            .nth(nth)
            .expect("index");
        classify_dot(&c, index)
    }

    #[test]
    fn an_initialism_is_recognised_and_its_dots_are_internal() {
        for token in ["LL.M.", "T.C.", "A.Ş.", "Ph.D."] {
            let text = format!("Av. Raci Çetin YÜKSEKBAŞ, {token} olarak");
            let c = chars(&text);
            let first_dot = text.find(token).unwrap();
            let dot_index = text[..first_dot].chars().count() + token.find('.').unwrap();
            assert_eq!(
                classify_dot(&c, dot_index),
                DotRole::Initialism,
                "{token} was not read as an initialism"
            );
        }
    }

    #[test]
    fn a_title_followed_by_a_name_is_not_an_initialism() {
        // `Av. Raci` and `Prof. Dr.` are separate tokens; a space breaks them.
        assert_eq!(role("Av. Raci Çetin", '.', 0), DotRole::Abbreviation);
        assert_eq!(
            role("Prof. Dr. Nurşen Mazıcı", '.', 0),
            DotRole::Abbreviation
        );
        assert_eq!(
            role("Prof. Dr. Nurşen Mazıcı", '.', 1),
            DotRole::Abbreviation
        );
    }

    #[test]
    fn anonymised_initials_from_a_judgment_are_recognised() {
        let text = "davacı, oğlu Y.. Ö..'ın konut ihtiyacı";
        assert_eq!(role(text, '.', 0), DotRole::AnonymisedInitial);
        assert_eq!(role(text, '.', 1), DotRole::AnonymisedInitial);
        assert_eq!(role(text, '.', 2), DotRole::AnonymisedInitial);
        assert_eq!(role(text, '.', 3), DotRole::AnonymisedInitial);
    }

    #[test]
    fn a_real_doubled_full_stop_is_still_a_sentence_end() {
        assert_eq!(
            role("Davanın kabulünü talep ederiz..", '.', 0),
            DotRole::SentenceEnd
        );
        assert_eq!(role("bu husus açıktır..", '.', 0), DotRole::SentenceEnd);
    }

    #[test]
    fn an_ellipsis_is_not_an_anonymised_initial() {
        let c = chars("bilmiyorum... dedi");
        assert!(anonymised_initial_span(&c, 11).is_none());
    }

    #[test]
    fn number_and_date_separators_are_recognised() {
        assert_eq!(
            role("Alacak 50.000,00 TL", '.', 0),
            DotRole::NumberSeparator
        );
        assert_eq!(
            role("Sözleşme 01.02.2020 tarihinde", '.', 0),
            DotRole::NumberSeparator
        );
    }

    #[test]
    fn an_ordinal_dot_is_recognised() {
        assert_eq!(role("HMK 119. maddede", '.', 0), DotRole::Ordinal);
    }

    #[test]
    fn a_legal_reference_abbreviation_is_recognised() {
        assert_eq!(role("HMK m. 119 uyarınca", '.', 0), DotRole::Abbreviation);
        assert_eq!(
            role("bkz. yukarıdaki açıklama", '.', 0),
            DotRole::Abbreviation
        );
    }

    #[test]
    fn a_short_acronym_before_a_dot_is_an_abbreviation() {
        // `(TCKN.: 47971525538)` — the dot abbreviates the acronym and the
        // colon introduces the value. Neither is doubled punctuation.
        assert_eq!(
            role("Nurşen MAZICI (TCKN.: 47971525538)", '.', 0),
            DotRole::Abbreviation
        );
        assert_eq!(role("HMK. 119 uyarınca", '.', 0), DotRole::Abbreviation);
    }

    #[test]
    fn a_long_capitalised_word_still_ends_a_sentence() {
        assert_eq!(
            role("Sarıyer / İSTANBUL. Sonraki cümle.", '.', 0),
            DotRole::SentenceEnd
        );
    }

    #[test]
    fn an_ordinary_sentence_end_is_classified_as_one() {
        assert_eq!(
            role("Dava açılmıştır. Davalı cevap verdi.", '.', 0),
            DotRole::SentenceEnd
        );
    }

    #[test]
    fn initialism_detection_does_not_run_off_the_end_of_the_text() {
        for text in ["LL.", ".", "..", "A.", "", "LL.M"] {
            let c = chars(text);
            for i in 0..c.len() {
                let _ = classify_dot(&c, i);
                let _ = initialism_span(&c, i);
                let _ = anonymised_initial_span(&c, i);
            }
        }
    }
}
