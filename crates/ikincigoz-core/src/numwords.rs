//! Turkish number words, in both directions.
//!
//! Legal documents write an amount twice — `50.000,00 TL (Ellibin Türk
//! Lirası)` — and the two disagreeing is a serious, silent error. Comparing
//! them needs a real conversion, not string matching, because the same amount
//! is written `Elli Bin`, `ELLİBİN` and `elli bin` with equal frequency.
//!
//! Everything here is a closed set of rules over a fixed vocabulary. There is
//! no dictionary file and nothing is learned.

use crate::text::tr_lower;

const ONES: [&str; 10] = [
    "", "bir", "iki", "üç", "dört", "beş", "altı", "yedi", "sekiz", "dokuz",
];
const TENS: [&str; 10] = [
    "", "on", "yirmi", "otuz", "kırk", "elli", "altmış", "yetmiş", "seksen", "doksan",
];
/// Scale words, from the smallest group upwards.
const SCALES: [&str; 7] = [
    "",
    "bin",
    "milyon",
    "milyar",
    "trilyon",
    "katrilyon",
    "kentilyon",
];

/// Largest amount handled. Beyond this the rule declines rather than guesses.
pub const MAX_AMOUNT: u64 = 999_999_999_999_999_999;

/// Write a non-negative integer in Turkish words, space separated.
///
/// ```text
/// 0        -> sıfır
/// 100      -> yüz            (never "bir yüz")
/// 1_000    -> bin            (never "bir bin")
/// 1_000_000 -> bir milyon    ("milyon" alone is not an amount)
/// ```
pub fn to_words(n: u64) -> String {
    if n == 0 {
        return "sıfır".to_string();
    }
    if n > MAX_AMOUNT {
        return String::new();
    }

    // Split into groups of three, least significant first.
    let mut groups: Vec<u64> = Vec::new();
    let mut rest = n;
    while rest > 0 {
        groups.push(rest % 1000);
        rest /= 1000;
    }

    let mut parts: Vec<String> = Vec::new();
    for (i, &g) in groups.iter().enumerate().rev() {
        if g == 0 {
            continue;
        }
        let scale = SCALES[i];
        // `bin` stands alone; `bir bin` is not Turkish. Every larger scale does
        // take `bir`.
        let words = if i == 1 && g == 1 {
            String::new()
        } else {
            group_to_words(g)
        };
        if !words.is_empty() {
            parts.push(words);
        }
        if !scale.is_empty() {
            parts.push(scale.to_string());
        }
    }
    parts.join(" ")
}

fn group_to_words(g: u64) -> String {
    debug_assert!(g < 1000);
    let mut parts: Vec<&str> = Vec::new();
    let hundreds = (g / 100) as usize;
    let tens = ((g % 100) / 10) as usize;
    let ones = (g % 10) as usize;

    if hundreds > 0 {
        // `yüz` stands alone for exactly one hundred.
        if hundreds > 1 {
            parts.push(ONES[hundreds]);
        }
        parts.push("yüz");
    }
    if tens > 0 {
        parts.push(TENS[tens]);
    }
    if ones > 0 {
        parts.push(ONES[ones]);
    }
    parts.join(" ")
}

/// Every recognised number word, longest first so greedy matching is correct
/// on run-together spellings like `ellibin`.
fn vocabulary() -> Vec<(&'static str, u64)> {
    let mut v: Vec<(&'static str, u64)> = Vec::new();
    for (i, w) in ONES.iter().enumerate().skip(1) {
        v.push((w, i as u64));
    }
    for (i, w) in TENS.iter().enumerate().skip(1) {
        v.push((w, i as u64 * 10));
    }
    v.push(("yüz", 100));
    v.push(("bin", 1_000));
    v.push(("milyon", 1_000_000));
    v.push(("milyar", 1_000_000_000));
    v.push(("trilyon", 1_000_000_000_000));
    v.push(("sıfır", 0));
    v.sort_by_key(|(w, _)| std::cmp::Reverse(w.chars().count()));
    v
}

/// Read a Turkish number phrase back into a value.
///
/// Returns `None` when anything in the phrase is not a number word, which is
/// the signal to stay quiet rather than report a mismatch we do not understand.
pub fn from_words(phrase: &str) -> Option<u64> {
    // Spacing is not reliable, so compare on letters alone.
    let compact: String = tr_lower(phrase)
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect();
    if compact.is_empty() {
        return None;
    }
    let vocab = vocabulary();

    let mut total: u64 = 0;
    let mut current: u64 = 0;
    let mut rest = compact.as_str();
    let mut matched_any = false;

    while !rest.is_empty() {
        // An unknown word means the phrase is not a number; refuse all of it.
        let (word, value) = *vocab.iter().find(|(w, _)| rest.starts_with(*w))?;
        rest = &rest[word.len()..];
        matched_any = true;

        match value {
            0 => {}
            100 => current = current.max(1) * 100,
            v if v >= 1_000 => {
                total = total.checked_add(current.max(1).checked_mul(v)?)?;
                current = 0;
            }
            v => current = current.checked_add(v)?,
        }
    }
    if !matched_any {
        return None;
    }
    total.checked_add(current)
}

/// A currency amount split into whole units and hundredths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amount {
    pub major: u64,
    pub minor: u8,
}

/// Parse `50.000,00`, `50000`, `1.234.567,89` into an amount.
///
/// Turkish uses `.` for thousands and `,` for the decimal, the opposite of the
/// English convention, so a digit group of exactly three after a dot is a
/// separator and never a fraction.
pub fn parse_amount(s: &str) -> Option<Amount> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if t.is_empty() || !t.chars().next()?.is_ascii_digit() {
        return None;
    }
    let (int_part, frac_part) = match t.split_once(',') {
        Some((a, b)) => (a, Some(b)),
        None => (t.as_str(), None),
    };
    // Thousands separators must be exactly three digits apart to count.
    let digits: String = if int_part.contains('.') {
        let groups: Vec<&str> = int_part.split('.').collect();
        if groups.len() < 2 || groups[0].is_empty() || groups[0].len() > 3 {
            return None;
        }
        if groups[1..].iter().any(|g| g.len() != 3) {
            return None;
        }
        groups.concat()
    } else {
        int_part.to_string()
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) || digits.len() > 18 {
        return None;
    }
    let major = digits.parse::<u64>().ok()?;
    let minor = match frac_part {
        None => 0u8,
        Some(f) => {
            if f.is_empty() || f.len() > 2 || !f.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let padded = if f.len() == 1 {
                format!("{f}0")
            } else {
                f.to_string()
            };
            padded.parse::<u8>().ok()?
        }
    };
    Some(Amount { major, minor })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_numbers_are_written_out() {
        assert_eq!(to_words(0), "sıfır");
        assert_eq!(to_words(1), "bir");
        assert_eq!(to_words(15), "on beş");
        assert_eq!(to_words(42), "kırk iki");
        assert_eq!(to_words(99), "doksan dokuz");
    }

    #[test]
    fn one_hundred_and_one_thousand_stand_alone() {
        assert_eq!(to_words(100), "yüz");
        assert_eq!(to_words(1_000), "bin");
        assert_eq!(to_words(200), "iki yüz");
        assert_eq!(to_words(2_000), "iki bin");
    }

    #[test]
    fn one_million_takes_bir_unlike_one_thousand() {
        assert_eq!(to_words(1_000_000), "bir milyon");
        assert_eq!(to_words(1_000_000_000), "bir milyar");
    }

    #[test]
    fn the_amounts_that_appear_in_petitions() {
        assert_eq!(to_words(50_000), "elli bin");
        assert_eq!(to_words(1_000_000), "bir milyon");
        assert_eq!(to_words(250_000), "iki yüz elli bin");
        assert_eq!(
            to_words(1_234_567),
            "bir milyon iki yüz otuz dört bin beş yüz altmış yedi"
        );
    }

    #[test]
    fn words_round_trip_back_to_the_number() {
        for n in [
            0u64,
            1,
            15,
            100,
            101,
            1_000,
            1_001,
            50_000,
            250_000,
            1_234_567,
            999_999_999,
        ] {
            assert_eq!(from_words(&to_words(n)), Some(n), "failed for {n}");
        }
    }

    #[test]
    fn run_together_and_capitalised_spellings_read_the_same() {
        assert_eq!(from_words("ellibin"), Some(50_000));
        assert_eq!(from_words("Elli Bin"), Some(50_000));
        assert_eq!(from_words("ELLİBİN"), Some(50_000));
        assert_eq!(from_words("elli  bin"), Some(50_000));
    }

    #[test]
    fn a_phrase_containing_a_non_number_word_is_refused() {
        assert_eq!(from_words("elli bin lira"), None);
        assert_eq!(from_words("yaklaşık elli bin"), None);
        assert_eq!(from_words(""), None);
    }

    #[test]
    fn turkish_amount_notation_is_parsed_the_turkish_way() {
        assert_eq!(
            parse_amount("50.000,00"),
            Some(Amount {
                major: 50_000,
                minor: 0
            })
        );
        assert_eq!(
            parse_amount("50.000,50"),
            Some(Amount {
                major: 50_000,
                minor: 50
            })
        );
        assert_eq!(
            parse_amount("1.000.000"),
            Some(Amount {
                major: 1_000_000,
                minor: 0
            })
        );
        assert_eq!(
            parse_amount("1234"),
            Some(Amount {
                major: 1234,
                minor: 0
            })
        );
        assert_eq!(
            parse_amount("50.000,5"),
            Some(Amount {
                major: 50_000,
                minor: 50
            })
        );
    }

    #[test]
    fn a_malformed_thousands_group_is_refused_rather_than_guessed() {
        assert_eq!(parse_amount("50.00"), None);
        assert_eq!(parse_amount("1.2345"), None);
        assert_eq!(parse_amount("abc"), None);
    }

    #[test]
    fn conversion_declines_amounts_beyond_its_range() {
        assert_eq!(to_words(u64::MAX), "");
    }
}
