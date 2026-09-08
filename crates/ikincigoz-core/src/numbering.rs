//! Detection of the numbering marker at the head of a block.
//!
//! Two ambiguities matter in Turkish legal drafting and both are handled by
//! carrying candidates forward rather than by guessing early:
//!
//! 1. `I.` is Roman one and also the ninth Latin letter.
//! 2. Outlines are lettered with either the 29-letter Turkish alphabet
//!    (`A B C Ç D`) or the 26-letter Latin one (`A B C D`). Reading `A B C D`
//!    as Turkish would report a phantom gap at `Ç`, so the alphabet is chosen
//!    per sibling group by whichever reading is self-consistent.

use crate::cdm::{Decoration, Numbering, NumberingScheme};
use crate::text::tr_lower;

/// The 29 letters of the Turkish alphabet, in order.
pub const TURKISH_UPPER: [char; 29] = [
    'A', 'B', 'C', 'Ç', 'D', 'E', 'F', 'G', 'Ğ', 'H', 'I', 'İ', 'J', 'K', 'L', 'M', 'N', 'O', 'Ö',
    'P', 'R', 'S', 'Ş', 'T', 'U', 'Ü', 'V', 'Y', 'Z',
];

pub const TURKISH_LOWER: [char; 29] = [
    'a', 'b', 'c', 'ç', 'd', 'e', 'f', 'g', 'ğ', 'h', 'ı', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'ö',
    'p', 'r', 's', 'ş', 't', 'u', 'ü', 'v', 'y', 'z',
];

/// Which lettering convention a sibling group is being read under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    Turkish,
    Latin,
}

impl Alphabet {
    pub fn label(self) -> &'static str {
        match self {
            Alphabet::Turkish => "Türk alfabesi",
            Alphabet::Latin => "Latin alfabesi",
        }
    }
}

/// 1-based position of a letter, or `None` when the letter is not in the
/// alphabet at all (`Q`, `W`, `X` under the Turkish reading).
pub fn letter_value(ch: char, alphabet: Alphabet) -> Option<u32> {
    let upper = crate::text::tr_upper(&ch.to_string()).chars().next()?;
    match alphabet {
        Alphabet::Turkish => TURKISH_UPPER
            .iter()
            .position(|&c| c == upper)
            .map(|i| i as u32 + 1),
        Alphabet::Latin => {
            if upper.is_ascii_uppercase() {
                Some(upper as u32 - 'A' as u32 + 1)
            } else {
                None
            }
        }
    }
}

/// Render an ordinal back to a letter, for "expected X here" messages.
pub fn value_letter(value: u32, alphabet: Alphabet, upper: bool) -> Option<char> {
    if value == 0 {
        return None;
    }
    let idx = (value - 1) as usize;
    match alphabet {
        Alphabet::Turkish => {
            if upper {
                TURKISH_UPPER.get(idx).copied()
            } else {
                TURKISH_LOWER.get(idx).copied()
            }
        }
        Alphabet::Latin => {
            if idx >= 26 {
                return None;
            }
            let base = if upper { b'A' } else { b'a' };
            Some((base + idx as u8) as char)
        }
    }
}

pub fn roman_value(s: &str) -> Option<u32> {
    let up = crate::text::tr_upper(s);
    if up.is_empty() || !up.chars().all(|c| "IVXLCDM".contains(c)) {
        return None;
    }
    let digit = |c: char| match c {
        'I' => 1,
        'V' => 5,
        'X' => 10,
        'L' => 50,
        'C' => 100,
        'D' => 500,
        'M' => 1000,
        _ => 0,
    };
    // Subtractive pairs (`IV`) make the running total dip, so accumulate signed.
    let chars: Vec<char> = up.chars().collect();
    let mut total: i64 = 0;
    for i in 0..chars.len() {
        let v = digit(chars[i]) as i64;
        let next = chars.get(i + 1).map(|&c| digit(c) as i64).unwrap_or(0);
        if v < next {
            total -= v;
        } else {
            total += v;
        }
    }
    if total <= 0 {
        None
    } else {
        Some(total as u32)
    }
}

pub fn value_roman(mut value: u32, upper: bool) -> Option<String> {
    if value == 0 || value > 3999 {
        return None;
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (v, s) in TABLE {
        while value >= v {
            out.push_str(s);
            value -= v;
        }
    }
    Some(if upper { out } else { tr_lower(&out) })
}

/// One possible reading of a marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub scheme: NumberingScheme,
    pub value: u32,
    pub alphabet: Option<Alphabet>,
}

/// A marker found at the head of a block, before a scheme has been chosen.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub raw: String,
    /// The marker body without its decoration, e.g. `B` from `(B)`.
    pub core: String,
    pub decoration: Decoration,
    /// Char offset just past the marker and the whitespace after it.
    pub text_start: usize,
    /// Every reading that is consistent with the marker's shape.
    pub candidates: Vec<Candidate>,
}

impl Marker {
    /// The reading to use when no sibling group is available to disambiguate.
    pub fn primary(&self) -> Candidate {
        self.candidates[0]
    }

    pub fn candidate_for(
        &self,
        scheme: NumberingScheme,
        alphabet: Option<Alphabet>,
    ) -> Option<Candidate> {
        self.candidates
            .iter()
            .copied()
            .find(|c| c.scheme == scheme && (alphabet.is_none() || c.alphabet == alphabet))
    }

    pub fn to_numbering(&self, chosen: Candidate) -> Numbering {
        Numbering {
            raw: self.raw.clone(),
            scheme: chosen.scheme,
            value: chosen.value,
            decoration: self.decoration,
            text_start: self.text_start,
        }
    }
}

/// Parse a numbering marker from the start of `text`.
///
/// Deliberately strict: a marker must be followed by whitespace and then real
/// content, so a sentence beginning `1990 yılında` is not read as item 1990.
pub fn parse_marker(text: &str) -> Option<Marker> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }

    let bracketed = chars[i] == '(' || chars[i] == '[';
    let open = i;
    let body_start = if bracketed { i + 1 } else { i };

    // Read the marker body: digits, or a short letter run.
    let mut j = body_start;
    while j < chars.len() && (chars[j].is_alphanumeric()) && j - body_start < 6 {
        j += 1;
    }
    if j == body_start {
        return None;
    }
    let core: String = chars[body_start..j].iter().collect();

    // A marker body is either all digits or all letters.
    let all_digits = core.chars().all(|c| c.is_ascii_digit());
    let all_letters = core.chars().all(|c| c.is_alphabetic());
    if !all_digits && !all_letters {
        return None;
    }
    // Letter markers are one letter, or a Roman numeral.
    if all_letters && core.chars().count() > 1 && roman_value(&core).is_none() {
        return None;
    }
    // A bare number longer than three digits is a year or an amount, not an item.
    if all_digits && core.len() > 3 {
        return None;
    }

    // Read the decoration.
    let (decoration, mut after) = if bracketed {
        match chars.get(j) {
            Some(')') | Some(']') => (Decoration::Bracketed, j + 1),
            _ => return None,
        }
    } else {
        match chars.get(j) {
            Some('.') => (Decoration::Dot, j + 1),
            Some(')') => (Decoration::Paren, j + 1),
            Some('-') | Some('\u{2013}') | Some('\u{2014}') => (Decoration::Dash, j + 1),
            // `1 - metin`: a dash decoration may be spaced away from the digit.
            Some(c) if c.is_whitespace() => {
                let mut k = j;
                while k < chars.len() && chars[k] == ' ' {
                    k += 1;
                }
                match chars.get(k) {
                    Some('-') | Some('\u{2013}') | Some('\u{2014}') => (Decoration::Dash, k + 1),
                    _ => return None,
                }
            }
            _ => return None,
        }
    };

    // `1.1.` and `A.1.` are sub-numbered; treat the whole run as the marker
    // and read the last component, which is what the sibling group compares.
    // Only the simple single-component form is supported for now, so bail out
    // rather than mis-reading it.
    if chars
        .get(after)
        .map(|c| c.is_alphanumeric())
        .unwrap_or(false)
        && decoration == Decoration::Dot
    {
        return None;
    }

    // Require whitespace, then content.
    let content_begins = after;
    let mut saw_space = false;
    while after < chars.len() && chars[after].is_whitespace() {
        saw_space = true;
        after += 1;
    }
    if !saw_space || after >= chars.len() {
        // A marker alone on its line is still a marker: an empty section is a
        // finding in its own right, not something to discard here.
        if content_begins >= chars.len() {
            after = chars.len();
        } else {
            return None;
        }
    }

    let raw: String = chars[open..content_begins].iter().collect();
    let candidates = candidates_for(&core, all_digits);
    if candidates.is_empty() {
        return None;
    }

    Some(Marker {
        raw,
        core,
        decoration,
        text_start: after,
        candidates,
    })
}

fn candidates_for(core: &str, all_digits: bool) -> Vec<Candidate> {
    let mut out = Vec::new();
    if all_digits {
        if let Ok(v) = core.parse::<u32>() {
            if v > 0 {
                out.push(Candidate {
                    scheme: NumberingScheme::Decimal,
                    value: v,
                    alphabet: None,
                });
            }
        }
        return out;
    }

    let is_upper = core
        .chars()
        .next()
        .map(|c| crate::text::tr_upper(&c.to_string()) == c.to_string())
        .unwrap_or(false);
    let single = core.chars().count() == 1;

    // Multi-letter bodies only ever read as Roman numerals.
    if !single {
        if let Some(v) = roman_value(core) {
            out.push(Candidate {
                scheme: if is_upper {
                    NumberingScheme::UpperRoman
                } else {
                    NumberingScheme::LowerRoman
                },
                value: v,
                alphabet: None,
            });
        }
        return out;
    }

    // Single letters read as a letter first; `I`, `V`, `X`, `C`, `D`, `L`, `M`
    // additionally read as Roman and stay ambiguous until the group decides.
    for alphabet in [Alphabet::Turkish, Alphabet::Latin] {
        if let Some(v) = letter_value(core.chars().next().unwrap(), alphabet) {
            out.push(Candidate {
                scheme: if is_upper {
                    NumberingScheme::UpperLatin
                } else {
                    NumberingScheme::LowerLatin
                },
                value: v,
                alphabet: Some(alphabet),
            });
        }
    }
    if let Some(v) = roman_value(core) {
        out.push(Candidate {
            scheme: if is_upper {
                NumberingScheme::UpperRoman
            } else {
                NumberingScheme::LowerRoman
            },
            value: v,
            alphabet: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(s: &str) -> Marker {
        parse_marker(s).unwrap_or_else(|| panic!("no marker in {s:?}"))
    }

    #[test]
    fn decimal_dot_marker() {
        let m = marker("1. Davanın konusu");
        assert_eq!(m.raw, "1.");
        assert_eq!(m.decoration, Decoration::Dot);
        assert_eq!(m.primary().scheme, NumberingScheme::Decimal);
        assert_eq!(m.primary().value, 1);
        assert_eq!(m.text_start, 3);
    }

    #[test]
    fn decorations_are_distinguished() {
        assert_eq!(marker("1) x").decoration, Decoration::Paren);
        assert_eq!(marker("(1) x").decoration, Decoration::Bracketed);
        assert_eq!(marker("1 - x").decoration, Decoration::Dash);
        assert_eq!(marker("A. x").decoration, Decoration::Dot);
    }

    #[test]
    fn turkish_letters_are_ordered_with_c_cedilla_after_c() {
        assert_eq!(letter_value('C', Alphabet::Turkish), Some(3));
        assert_eq!(letter_value('Ç', Alphabet::Turkish), Some(4));
        assert_eq!(letter_value('D', Alphabet::Turkish), Some(5));
        assert_eq!(letter_value('D', Alphabet::Latin), Some(4));
        assert_eq!(letter_value('Ç', Alphabet::Latin), None);
    }

    #[test]
    fn a_single_letter_stays_ambiguous_between_roman_and_latin() {
        let m = marker("I. Giriş");
        let schemes: Vec<_> = m.candidates.iter().map(|c| c.scheme).collect();
        assert!(schemes.contains(&NumberingScheme::UpperLatin));
        assert!(schemes.contains(&NumberingScheme::UpperRoman));
    }

    #[test]
    fn multi_letter_markers_are_roman_only() {
        let m = marker("III. Esasa ilişkin");
        assert_eq!(m.candidates.len(), 1);
        assert_eq!(m.primary().scheme, NumberingScheme::UpperRoman);
        assert_eq!(m.primary().value, 3);
    }

    #[test]
    fn roman_conversion_round_trips() {
        for v in [1u32, 4, 9, 14, 40, 90, 400, 1987] {
            let s = value_roman(v, true).unwrap();
            assert_eq!(roman_value(&s), Some(v), "{v} -> {s}");
        }
    }

    #[test]
    fn years_and_amounts_are_not_markers() {
        assert!(parse_marker("1990 yılında dava açıldı").is_none());
        assert!(parse_marker("2024. yılın en önemli").is_none());
        assert!(parse_marker("Davacı beyan etmiştir.").is_none());
    }

    #[test]
    fn sub_numbered_markers_are_declined_rather_than_misread() {
        assert!(parse_marker("1.1. Alt başlık").is_none());
    }

    #[test]
    fn a_marker_alone_on_its_line_is_still_a_marker() {
        let m = marker("B.");
        assert_eq!(m.core, "B");
        assert_eq!(m.text_start, 2);
    }

    #[test]
    fn a_marker_glued_to_its_text_is_rejected() {
        assert!(parse_marker("A.Başlık").is_none());
    }
}
