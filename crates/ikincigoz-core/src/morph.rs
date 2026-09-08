//! A deliberately small Turkish morphology layer.
//!
//! The problem it solves is narrow and real. A user defines `mecur` ->
//! `kiralanan`; the document contains `mecuru`, `mecurun`, `mecurda`. A naive
//! replacement produces `kiralananu`, which is worse than no replacement at
//! all. This module carries the grammatical features across instead:
//!
//! ```text
//! mecuru   -> kiralananı
//! mecurun  -> kiralananın
//! mecurdan -> kiralanandan
//! ```
//!
//! It is not a general analyser and does not try to be. It generates the forms
//! it supports and matches against them, which means it can only ever be
//! confidently right or explicitly unsure — never confidently wrong. When a
//! form is ambiguous or falls outside the supported feature set, the caller is
//! told so and reports a review finding rather than offering a correction.

use crate::text::{is_vowel, tr_lower, tr_upper};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Number {
    Singular,
    Plural,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Possessive {
    None,
    First1Sg,
    Second2Sg,
    Third3Sg,
    First1Pl,
    Second2Pl,
    Third3Pl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Case {
    Nominative,
    Accusative,
    Dative,
    Locative,
    Ablative,
    Genitive,
    Instrumental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Features {
    pub number: Number,
    pub possessive: Possessive,
    pub case: Case,
}

impl Features {
    pub fn bare() -> Self {
        Features {
            number: Number::Singular,
            possessive: Possessive::None,
            case: Case::Nominative,
        }
    }
}

/// Every feature combination the layer supports, in a fixed order.
pub fn all_features() -> Vec<Features> {
    let mut out = Vec::with_capacity(98);
    for number in [Number::Singular, Number::Plural] {
        for possessive in [
            Possessive::None,
            Possessive::First1Sg,
            Possessive::Second2Sg,
            Possessive::Third3Sg,
            Possessive::First1Pl,
            Possessive::Second2Pl,
            Possessive::Third3Pl,
        ] {
            for case in [
                Case::Nominative,
                Case::Accusative,
                Case::Dative,
                Case::Locative,
                Case::Ablative,
                Case::Genitive,
                Case::Instrumental,
            ] {
                out.push(Features {
                    number,
                    possessive,
                    case,
                });
            }
        }
    }
    out
}

/// The last vowel of a word decides which suffix vowels follow it.
fn last_vowel(s: &str) -> Option<char> {
    s.chars().rev().find(|c| is_vowel(*c))
}

/// Two-way harmony: the `a`/`e` alternation.
fn wide_vowel(stem: &str) -> char {
    match last_vowel(stem) {
        Some('a') | Some('ı') | Some('o') | Some('u') => 'a',
        _ => 'e',
    }
}

/// Four-way harmony: the `ı`/`i`/`u`/`ü` alternation.
fn narrow_vowel(stem: &str) -> char {
    match last_vowel(stem) {
        Some('a') | Some('ı') => 'ı',
        Some('e') | Some('i') => 'i',
        Some('o') | Some('u') => 'u',
        Some('ö') | Some('ü') => 'ü',
        _ => 'i',
    }
}

fn ends_with_vowel(s: &str) -> bool {
    s.chars().next_back().map(is_vowel).unwrap_or(false)
}

/// `f s t k ç ş h p` — the voiceless consonants that harden a following `d`.
fn ends_voiceless(s: &str) -> bool {
    matches!(
        s.chars().next_back(),
        Some('f' | 's' | 't' | 'k' | 'ç' | 'ş' | 'h' | 'p')
    )
}

/// Final-consonant softening before a vowel: `kitap` -> `kitab(ı)`.
///
/// Whether a given word softens is lexical, not phonological: `sokak` ->
/// `sokağı` but `hukuk` -> `hukuku`. Generation therefore refuses to pick, and
/// callers treat such a stem as ambiguous.
fn softened(stem: &str) -> Option<String> {
    let last = stem.chars().next_back()?;
    let soft = match last {
        'p' => 'b',
        'ç' => 'c',
        't' => 'd',
        'k' => {
            // `nk` hardens to `ng`, everything else to `ğ`.
            if stem.chars().rev().nth(1) == Some('n') {
                'g'
            } else {
                'ğ'
            }
        }
        _ => return None,
    };
    let mut out: String = stem.chars().take(stem.chars().count() - 1).collect();
    out.push(soft);
    Some(out)
}

/// True when attaching a vowel-initial suffix to this stem is ambiguous.
pub fn softening_is_ambiguous(stem: &str) -> bool {
    softened(stem).is_some()
}

fn substitute(template: &str, stem: &str) -> String {
    let a = wide_vowel(stem);
    let i = narrow_vowel(stem);
    template
        .chars()
        .map(|c| match c {
            'A' => a,
            'I' => i,
            other => other,
        })
        .collect()
}

/// Build the surface form for `lemma` under `features`.
///
/// `soften` chooses the stem variant for vowel-initial suffixes; callers that
/// cannot know which is right should generate both and treat a disagreement as
/// ambiguity.
pub fn inflect_with(lemma: &str, features: Features, soften: bool) -> String {
    let lemma = tr_lower(lemma.trim());
    let mut word = lemma.clone();

    let vowel_initial_next = |w: &str, f: Features| -> bool {
        // Does the very next suffix begin with a vowel?
        if f.number == Number::Plural {
            return false; // `-lAr`
        }
        match f.possessive {
            Possessive::None => {
                matches!(f.case, Case::Accusative | Case::Dative | Case::Genitive)
                    && !ends_with_vowel(w)
            }
            Possessive::Third3Pl => false, // `-lArI`
            _ => !ends_with_vowel(w),
        }
    };

    if soften && vowel_initial_next(&word, features) {
        if let Some(s) = softened(&word) {
            word = s;
        }
    }

    if features.number == Number::Plural {
        word.push_str(&substitute("lAr", &word));
    }

    let third_person_possessive = matches!(
        features.possessive,
        Possessive::Third3Sg | Possessive::Third3Pl
    );

    let poss = match features.possessive {
        Possessive::None => String::new(),
        Possessive::First1Sg => {
            if ends_with_vowel(&word) {
                "m".into()
            } else {
                substitute("Im", &word)
            }
        }
        Possessive::Second2Sg => {
            if ends_with_vowel(&word) {
                "n".into()
            } else {
                substitute("In", &word)
            }
        }
        Possessive::Third3Sg => {
            if ends_with_vowel(&word) {
                substitute("sI", &word)
            } else {
                substitute("I", &word)
            }
        }
        Possessive::First1Pl => {
            if ends_with_vowel(&word) {
                substitute("mIz", &word)
            } else {
                substitute("ImIz", &word)
            }
        }
        Possessive::Second2Pl => {
            if ends_with_vowel(&word) {
                substitute("nIz", &word)
            } else {
                substitute("InIz", &word)
            }
        }
        Possessive::Third3Pl => substitute("lArI", &word),
    };
    word.push_str(&poss);

    let case = match features.case {
        Case::Nominative => String::new(),
        Case::Accusative => {
            if third_person_possessive {
                substitute("nI", &word)
            } else if ends_with_vowel(&word) {
                substitute("yI", &word)
            } else {
                substitute("I", &word)
            }
        }
        Case::Dative => {
            if third_person_possessive {
                substitute("nA", &word)
            } else if ends_with_vowel(&word) {
                substitute("yA", &word)
            } else {
                substitute("A", &word)
            }
        }
        Case::Locative => {
            let d = if ends_voiceless(&word) { 't' } else { 'd' };
            let base = substitute(&format!("{d}A"), &word);
            if third_person_possessive {
                format!("n{base}")
            } else {
                base
            }
        }
        Case::Ablative => {
            let d = if ends_voiceless(&word) { 't' } else { 'd' };
            let base = substitute(&format!("{d}An"), &word);
            if third_person_possessive {
                format!("n{base}")
            } else {
                base
            }
        }
        Case::Genitive => {
            if third_person_possessive || ends_with_vowel(&word) {
                substitute("nIn", &word)
            } else {
                substitute("In", &word)
            }
        }
        Case::Instrumental => {
            if third_person_possessive || ends_with_vowel(&word) {
                substitute("ylA", &word)
            } else {
                substitute("lA", &word)
            }
        }
    };
    word.push_str(&case);
    word
}

/// Generate the plain form, without applying consonant softening.
pub fn inflect(lemma: &str, features: Features) -> String {
    inflect_with(lemma, features, false)
}

/// Which feature sets could have produced `surface` from `lemma`.
///
/// May return more than one: `mecuru` is both accusative and third-person
/// possessive, and no amount of local analysis can tell them apart.
pub fn analyze(surface: &str, lemma: &str) -> Vec<Features> {
    let target = tr_lower(surface.trim());
    all_features()
        .into_iter()
        .filter(|f| {
            inflect_with(lemma, *f, false) == target || inflect_with(lemma, *f, true) == target
        })
        .collect()
}

/// The outcome of trying to carry a form across from one lemma to another.
#[derive(Debug, Clone, PartialEq)]
pub enum Transfer {
    /// The form was understood and the target form is unambiguous.
    Safe {
        replacement: String,
        features: Features,
    },
    /// The form was recognised, but the target cannot be produced with
    /// confidence. Reported for review rather than corrected.
    Ambiguous { reason: AmbiguityReason },
    /// `surface` is not a form of `from` at all.
    NotAForm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbiguityReason {
    /// Several readings that would produce different target forms.
    MultipleReadings,
    /// The target stem ends in `p`, `ç`, `t` or `k`, where softening is
    /// lexically determined and cannot be derived.
    StemSoftening,
    /// The form is outside the supported feature set.
    Unsupported,
}

impl AmbiguityReason {
    pub fn message_tr(self) -> &'static str {
        match self {
            AmbiguityReason::MultipleReadings => {
                "Bu biçim birden çok şekilde çözümlenebiliyor; otomatik dönüştürme güvenli değil."
            }
            AmbiguityReason::StemSoftening => {
                "Hedef kelimenin son sesi yumuşama gösterebilir; doğru biçim kurallardan \
                 türetilemediği için otomatik dönüştürme yapılmadı."
            }
            AmbiguityReason::Unsupported => "Bu çekimli biçim desteklenen ekler arasında değil.",
        }
    }
}

/// Replace an inflected `surface` form of `from` with the matching form of `to`.
pub fn transfer(surface: &str, from: &str, to: &str) -> Transfer {
    let readings = analyze(surface, from);
    if readings.is_empty() {
        return Transfer::NotAForm;
    }

    // The target stem may or may not soften. When it could, both spellings are
    // defensible and neither can be derived, so decline.
    let needs_soften_decision = readings.iter().any(|f| {
        softening_is_ambiguous(&tr_lower(to))
            && inflect_with(to, *f, false) != inflect_with(to, *f, true)
    });
    if needs_soften_decision {
        return Transfer::Ambiguous {
            reason: AmbiguityReason::StemSoftening,
        };
    }

    // Several readings are fine as long as they agree on the output. `mecuru`
    // is accusative or possessive, but `kiralananı` either way.
    let mut outputs: Vec<String> = readings.iter().map(|f| inflect(to, *f)).collect();
    outputs.sort();
    outputs.dedup();
    if outputs.len() != 1 {
        return Transfer::Ambiguous {
            reason: AmbiguityReason::MultipleReadings,
        };
    }

    Transfer::Safe {
        replacement: match_capitalisation(surface, &outputs[0]),
        features: readings[0],
    }
}

/// Carry the source word's capitalisation onto the replacement.
fn match_capitalisation(surface: &str, replacement: &str) -> String {
    let trimmed = surface.trim();
    if trimmed.is_empty() {
        return replacement.to_string();
    }
    if tr_upper(trimmed) == trimmed && trimmed.chars().any(|c| c.is_alphabetic()) {
        return tr_upper(replacement);
    }
    let first = trimmed.chars().next().unwrap();
    if tr_upper(&first.to_string()) == first.to_string() {
        let mut out = tr_upper(&replacement.chars().take(1).collect::<String>());
        out.push_str(&replacement.chars().skip(1).collect::<String>());
        return out;
    }
    replacement.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(number: Number, possessive: Possessive, case: Case) -> Features {
        Features {
            number,
            possessive,
            case,
        }
    }

    #[test]
    fn the_worked_example_from_the_specification() {
        // The case this module exists for.
        for (surface, expected) in [
            ("mecur", "kiralanan"),
            ("mecuru", "kiralananı"),
            ("mecurun", "kiralananın"),
            ("mecura", "kiralanana"),
            ("mecurda", "kiralananda"),
            ("mecurdan", "kiralanandan"),
            ("mecurlar", "kiralananlar"),
        ] {
            match transfer(surface, "mecur", "kiralanan") {
                Transfer::Safe { replacement, .. } => {
                    assert_eq!(replacement, expected, "for {surface}")
                }
                other => panic!("{surface}: expected a safe transfer, got {other:?}"),
            }
        }
    }

    #[test]
    fn vowel_harmony_picks_the_right_suffix_vowel() {
        assert_eq!(
            inflect(
                "dava",
                f(Number::Singular, Possessive::None, Case::Locative)
            ),
            "davada"
        );
        assert_eq!(
            inflect("ev", f(Number::Singular, Possessive::None, Case::Locative)),
            "evde"
        );
        assert_eq!(
            inflect(
                "göz",
                f(Number::Singular, Possessive::None, Case::Accusative)
            ),
            "gözü"
        );
        assert_eq!(
            inflect(
                "yol",
                f(Number::Singular, Possessive::None, Case::Accusative)
            ),
            "yolu"
        );
        assert_eq!(
            inflect(
                "kız",
                f(Number::Singular, Possessive::None, Case::Accusative)
            ),
            "kızı"
        );
    }

    #[test]
    fn a_voiceless_final_consonant_hardens_the_suffix() {
        assert_eq!(
            inflect(
                "mahkeme",
                f(Number::Singular, Possessive::None, Case::Locative)
            ),
            "mahkemede"
        );
        assert_eq!(
            inflect(
                "kitap",
                f(Number::Singular, Possessive::None, Case::Locative)
            ),
            "kitapta"
        );
        assert_eq!(
            inflect(
                "dosya",
                f(Number::Singular, Possessive::None, Case::Ablative)
            ),
            "dosyadan"
        );
        assert_eq!(
            inflect(
                "hukuk",
                f(Number::Singular, Possessive::None, Case::Ablative)
            ),
            "hukuktan"
        );
    }

    #[test]
    fn a_vowel_final_stem_takes_a_buffer_consonant() {
        assert_eq!(
            inflect(
                "dava",
                f(Number::Singular, Possessive::None, Case::Accusative)
            ),
            "davayı"
        );
        assert_eq!(
            inflect("dava", f(Number::Singular, Possessive::None, Case::Dative)),
            "davaya"
        );
        assert_eq!(
            inflect(
                "dava",
                f(Number::Singular, Possessive::None, Case::Genitive)
            ),
            "davanın"
        );
        assert_eq!(
            inflect(
                "dava",
                f(Number::Singular, Possessive::None, Case::Instrumental)
            ),
            "davayla"
        );
    }

    #[test]
    fn third_person_possessive_inserts_the_n_buffer_before_a_case_suffix() {
        assert_eq!(
            inflect(
                "dava",
                f(Number::Singular, Possessive::Third3Sg, Case::Nominative)
            ),
            "davası"
        );
        assert_eq!(
            inflect(
                "dava",
                f(Number::Singular, Possessive::Third3Sg, Case::Accusative)
            ),
            "davasını"
        );
        assert_eq!(
            inflect(
                "dava",
                f(Number::Singular, Possessive::Third3Sg, Case::Locative)
            ),
            "davasında"
        );
        assert_eq!(
            inflect(
                "dava",
                f(Number::Singular, Possessive::Third3Sg, Case::Ablative)
            ),
            "davasından"
        );
    }

    #[test]
    fn plurals_combine_with_case() {
        assert_eq!(
            inflect(
                "dosya",
                f(Number::Plural, Possessive::None, Case::Nominative)
            ),
            "dosyalar"
        );
        assert_eq!(
            inflect("dosya", f(Number::Plural, Possessive::None, Case::Locative)),
            "dosyalarda"
        );
        assert_eq!(
            inflect(
                "dosya",
                f(Number::Plural, Possessive::None, Case::Accusative)
            ),
            "dosyaları"
        );
    }

    #[test]
    fn analysis_finds_every_reading_of_an_ambiguous_form() {
        // `mecuru` is accusative, and also third-person possessive.
        let readings = analyze("mecuru", "mecur");
        assert!(readings.len() >= 2, "got {readings:?}");
        assert!(readings.iter().any(|f| f.case == Case::Accusative));
        assert!(readings
            .iter()
            .any(|f| f.possessive == Possessive::Third3Sg));
    }

    #[test]
    fn ambiguous_readings_that_agree_on_the_output_are_still_safe() {
        // Both readings of `mecuru` produce `kiralananı`, so there is nothing
        // for the user to decide.
        assert!(matches!(
            transfer("mecuru", "mecur", "kiralanan"),
            Transfer::Safe { .. }
        ));
    }

    #[test]
    fn a_target_stem_that_might_soften_is_declined_rather_than_guessed() {
        // `sokak` -> `sokağı` but `hukuk` -> `hukuku`; the rule is lexical.
        match transfer("mecuru", "mecur", "sokak") {
            Transfer::Ambiguous { reason } => assert_eq!(reason, AmbiguityReason::StemSoftening),
            other => panic!("expected ambiguity, got {other:?}"),
        }
    }

    #[test]
    fn softened_source_forms_are_still_recognised() {
        // `kitabı` is `kitap` + accusative with softening.
        assert!(!analyze("kitabı", "kitap").is_empty());
        assert!(!analyze("kitapta", "kitap").is_empty());
    }

    #[test]
    fn an_unrelated_word_is_not_a_form_of_the_lemma() {
        assert_eq!(
            transfer("mahkeme", "mecur", "kiralanan"),
            Transfer::NotAForm
        );
        assert_eq!(
            transfer("mecurluk", "mecur", "kiralanan"),
            Transfer::NotAForm
        );
    }

    #[test]
    fn capitalisation_is_carried_across() {
        match transfer("Mecurun", "mecur", "kiralanan") {
            Transfer::Safe { replacement, .. } => assert_eq!(replacement, "Kiralananın"),
            other => panic!("{other:?}"),
        }
        match transfer("MECURDA", "mecur", "kiralanan") {
            Transfer::Safe { replacement, .. } => assert_eq!(replacement, "KİRALANANDA"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_bare_form_transfers_to_the_bare_form() {
        match transfer("mecur", "mecur", "kiralanan") {
            Transfer::Safe {
                replacement,
                features,
            } => {
                assert_eq!(replacement, "kiralanan");
                assert_eq!(features.case, Case::Nominative);
            }
            other => panic!("{other:?}"),
        }
    }
}
