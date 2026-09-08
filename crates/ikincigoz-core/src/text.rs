//! Turkish-aware text utilities shared by every rule.
//!
//! Rust's `to_lowercase` maps `I` to `i`, which is wrong for Turkish: `I`
//! lowercases to `ı` and `İ` lowercases to `i`. Every case fold in this crate
//! goes through here so the two dotted/dotless pairs stay correct.

/// Turkish-correct lowercase.
pub fn tr_lower(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'I' => out.push('ı'),
            'İ' => out.push('i'),
            _ => out.extend(ch.to_lowercase()),
        }
    }
    out
}

/// Turkish-correct uppercase.
pub fn tr_upper(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'i' => out.push('İ'),
            'ı' => out.push('I'),
            _ => out.extend(ch.to_uppercase()),
        }
    }
    out
}

/// True for the letters that can appear inside a Turkish word.
pub fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '\'' || ch == '’'
}

/// True for Turkish vowels, in either case.
pub fn is_vowel(ch: char) -> bool {
    matches!(
        ch,
        'a' | 'e'
            | 'ı'
            | 'i'
            | 'o'
            | 'ö'
            | 'u'
            | 'ü'
            | 'A'
            | 'E'
            | 'I'
            | 'İ'
            | 'O'
            | 'Ö'
            | 'U'
            | 'Ü'
    )
}

/// Collapse the cosmetic differences that must not hide a duplicate:
/// case, whitespace runs, curly quotes, and the several dash characters.
pub fn normalize(s: &str) -> String {
    let folded = tr_lower(s);
    let mut out = String::with_capacity(folded.len());
    let mut pending_space = false;
    for ch in folded.chars() {
        let ch = canonical_char(ch);
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

/// Map the visually-equivalent Unicode variants onto one ASCII representative
/// so comparisons do not trip over a typographic quote or an en dash.
pub fn canonical_char(ch: char) -> char {
    match ch {
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{02BC}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{00AB}' | '\u{00BB}' => '"',
        '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{FE58}' | '\u{FE63}' | '\u{FF0D}' => '-',
        '\u{00A0}' | '\u{2007}' | '\u{202F}' | '\u{2009}' | '\u{2002}' | '\u{2003}' => ' ',
        _ => ch,
    }
}

/// A word together with its char span inside the source string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Split into word tokens, recording char spans so a finding can point at the
/// exact stretch of text that triggered it.
pub fn tokenize(s: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut start = 0usize;
    for (i, ch) in s.chars().enumerate() {
        if is_word_char(ch) {
            if current.is_empty() {
                start = i;
            }
            current.push(ch);
        } else if !current.is_empty() {
            out.push(Token {
                text: std::mem::take(&mut current),
                start,
                end: i,
            });
        }
    }
    if !current.is_empty() {
        let end = s.chars().count();
        out.push(Token {
            text: current,
            start,
            end,
        });
    }
    out
}

/// Split a paragraph into sentences with char spans.
///
/// Turkish legal text is full of abbreviations (`m.`, `md.`, `Av.`, `bkz.`)
/// and of ordinals written `1.`, so a naive split on `.` produces nonsense.
/// Both are excluded before a period is accepted as a sentence end.
pub fn split_sentences(s: &str) -> Vec<Token> {
    const ABBREVIATIONS: &[&str] = &[
        "m", "md", "mad", "bkz", "vb", "vs", "av", "dr", "prof", "doç", "sn", "no", "s", "sf",
        "age", "agm", "bkz", "krş", "yy", "tl", "bt", "hz", "gen", "alb", "yrd", "öğr", "gör",
        "üni", "fak", "c", "k", "e", "t", "ör", "yak", "yak",
    ];

    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '.' || ch == '!' || ch == '?' || ch == '…' {
            // Consume a run of terminators plus any closing quote/bracket.
            let mut end = i + 1;
            while end < chars.len()
                && (chars[end] == '.'
                    || chars[end] == '!'
                    || chars[end] == '?'
                    || chars[end] == '"'
                    || chars[end] == '\''
                    || chars[end] == '\u{201D}'
                    || chars[end] == ')')
            {
                end += 1;
            }
            let terminates = if ch == '.' {
                !is_abbreviation_dot(&chars, i, ABBREVIATIONS) && !is_ordinal_dot(&chars, i)
            } else {
                true
            };
            if terminates {
                let text: String = chars[start..end].iter().collect();
                if !text.trim().is_empty() {
                    out.push(Token { text, start, end });
                }
                // Skip the whitespace that separates sentences.
                let mut next = end;
                while next < chars.len() && chars[next].is_whitespace() {
                    next += 1;
                }
                start = next;
                i = next;
                continue;
            }
            i = end;
            continue;
        }
        i += 1;
    }

    if start < chars.len() {
        let text: String = chars[start..].iter().collect();
        if !text.trim().is_empty() {
            out.push(Token {
                text,
                start,
                end: chars.len(),
            });
        }
    }
    out
}

/// `HMK m. 119` - the dot after `m` closes an abbreviation, not a sentence.
fn is_abbreviation_dot(chars: &[char], dot: usize, abbrevs: &[&str]) -> bool {
    let mut begin = dot;
    while begin > 0 && is_word_char(chars[begin - 1]) {
        begin -= 1;
    }
    if begin == dot {
        return false;
    }
    let word: String = chars[begin..dot].iter().collect();
    let word = tr_lower(&word);
    // A single capital letter followed by a dot is an initial, not a sentence end.
    if word.chars().count() == 1 && chars[begin].is_alphabetic() && chars[begin].is_uppercase() {
        return true;
    }
    abbrevs.contains(&word.as_str())
}

/// `3. madde` - the dot belongs to the ordinal. Requires a lowercase word
/// after it, since `3. Madde` starting a new sentence is genuinely ambiguous
/// and we prefer not to split there either.
fn is_ordinal_dot(chars: &[char], dot: usize) -> bool {
    let mut begin = dot;
    while begin > 0 && chars[begin - 1].is_ascii_digit() {
        begin -= 1;
    }
    if begin == dot {
        return false;
    }
    // Reject when the digits are themselves preceded by a letter (`A1.`).
    if begin > 0 && chars[begin - 1].is_alphabetic() {
        return false;
    }
    let mut after = dot + 1;
    while after < chars.len() && chars[after] == ' ' {
        after += 1;
    }
    match chars.get(after) {
        Some(c) => c.is_alphabetic() && tr_lower(&c.to_string()) == c.to_string(),
        None => false,
    }
}

/// Token-set Jaccard similarity in `[0.0, 1.0]`.
pub fn jaccard(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let sa: HashSet<String> = tokenize(&normalize(a))
        .into_iter()
        .map(|t| t.text)
        .collect();
    let sb: HashSet<String> = tokenize(&normalize(b))
        .into_iter()
        .map(|t| t.text)
        .collect();
    if sa.is_empty() && sb.is_empty() {
        return 1.0;
    }
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Levenshtein distance normalised to `[0.0, 1.0]`, where 1.0 is identical.
///
/// Guarded by a length cap: the DP table is quadratic and paragraph pairs in a
/// long document are compared pairwise, so oversized inputs fall back to the
/// cheap token measure rather than stalling the analysis.
pub fn normalized_levenshtein(a: &str, b: &str) -> f32 {
    const MAX: usize = 2000;
    let av: Vec<char> = a.chars().collect();
    let bv: Vec<char> = b.chars().collect();
    if av.len() > MAX || bv.len() > MAX {
        return jaccard(a, b);
    }
    if av.is_empty() && bv.is_empty() {
        return 1.0;
    }
    let longest = av.len().max(bv.len()) as f32;
    let dist = levenshtein(&av, &bv) as f32;
    1.0 - (dist / longest)
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// A short, safe excerpt for display next to a finding.
///
/// Excerpts are shown in the UI only. They are never written to a log or a
/// report file, so document content cannot leak out through diagnostics.
pub fn excerpt(text: &str, center: usize, radius: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = center.saturating_sub(radius);
    let end = (center + radius).min(chars.len());
    let mut s = String::new();
    if start > 0 {
        s.push('…');
    }
    s.extend(&chars[start..end]);
    if end < chars.len() {
        s.push('…');
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_case_folding_keeps_dotted_and_dotless_i_apart() {
        assert_eq!(tr_lower("İKİNCİGÖZ"), "ikincigöz");
        assert_eq!(tr_lower("ISPARTA"), "ısparta");
        assert_eq!(tr_upper("ikincigöz"), "İKİNCİGÖZ");
        assert_eq!(tr_upper("ısparta"), "ISPARTA");
    }

    #[test]
    fn normalize_collapses_whitespace_case_and_quote_variants() {
        assert_eq!(normalize("  Ankara’da   BÜYÜK  "), "ankara'da büyük");
        assert_eq!(normalize("“alıntı”"), "\"alıntı\"");
        assert_eq!(normalize("a\u{00A0}b"), "a b");
    }

    #[test]
    fn tokenize_reports_char_spans() {
        let t = tokenize("Ek-3'te belirtildiği");
        assert_eq!(t[0].text, "Ek");
        assert_eq!(t[1].text, "3'te");
        assert_eq!(t[1].start, 3);
        assert_eq!(t[2].text, "belirtildiği");
    }

    #[test]
    fn sentence_split_does_not_break_on_legal_abbreviations() {
        let s = split_sentences("HMK m. 119 uyarınca dava açılmıştır. İkinci cümle budur.");
        assert_eq!(
            s.len(),
            2,
            "got {:?}",
            s.iter().map(|t| &t.text).collect::<Vec<_>>()
        );
        assert!(s[0].text.starts_with("HMK m. 119"));
    }

    #[test]
    fn sentence_split_does_not_break_on_ordinals() {
        let s = split_sentences("5. maddede yazılıdır. Sonraki cümle.");
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn sentence_split_handles_a_plain_pair() {
        let s = split_sentences("Birinci cümle. İkinci cümle.");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].text, "Birinci cümle.");
    }

    #[test]
    fn similarity_measures_are_bounded_and_ordered() {
        assert!((jaccard("a b c", "a b c") - 1.0).abs() < 1e-6);
        assert_eq!(jaccard("a b", "c d"), 0.0);
        assert!(normalized_levenshtein("mecur", "mecuru") > 0.8);
        assert!(normalized_levenshtein("mecur", "tamamen farklı") < 0.4);
    }

    #[test]
    fn excerpt_stays_short_and_marks_truncation() {
        let long = "kelime ".repeat(50);
        let e = excerpt(&long, 100, 20);
        assert!(e.starts_with('…') && e.ends_with('…'));
        assert!(e.chars().count() < 60);
    }
}
