//! Internal consistency.
//!
//! These rules compare the document against itself and never against an
//! outside authority. İkinciGöz does not know which citation format is correct
//! — it knows that one document should pick one and stay with it.
//!
//! The single exception is the amount rule, which is arithmetic: `50.000,00 TL
//! (Yüz Bin Türk Lirası)` is wrong in a way that needs no opinion.

use super::Context;
use crate::cdm::{BlockKind, SourceLocation};
use crate::finding::{Finding, FindingBuilder};
use crate::numwords::{from_words, parse_amount, to_words};
use crate::text::tr_lower;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn builder(id: &str) -> FindingBuilder {
    FindingBuilder::new(crate::finding::rule_info(id).expect("registered rule"))
}

/// `HMK m. 119`, `HMK m.119`, `HMK 119. madde`, `HMK madde 119`.
static CITATION: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"\b([A-ZÇĞİÖŞÜ]{2,8})\s*(?:m|md|mad)\s*\.\s*(\d{1,4})\b").unwrap(),
        Regex::new(r"\b([A-ZÇĞİÖŞÜ]{2,8})\s+(\d{1,4})\s*\.\s*madde").unwrap(),
        Regex::new(r"\b([A-ZÇĞİÖŞÜ]{2,8})\s+madde\s+(\d{1,4})\b").unwrap(),
    ]
});

/// `50.000,00 TL (Ellibin Türk Lirası)`.
static AMOUNT_WITH_WORDS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(\d{1,3}(?:\.\d{3})+(?:,\d{1,2})?|\d+(?:,\d{1,2})?)\s*(?:TL|TRY|₺|Türk\s+Lirası)?\s*\(\s*([^)]{3,160}?)\s*\)",
    )
    .unwrap()
});

static DATE_NUMERIC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\d{1,2})([./-])(\d{1,2})[./-](\d{4})\b").unwrap());

static DATE_WORDED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b\d{1,2}\s+(ocak|şubat|mart|nisan|mayıs|haziran|temmuz|ağustos|eylül|ekim|kasım|aralık)\s+\d{4}\b",
    )
    .unwrap()
});

static AMOUNT_TOKEN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(\d{1,3}(?:\.\d{3})+(?:,\d{1,2})?|\d+,\d{1,2})\s*(TL|TRY|₺|Türk\s+Lirası)\b")
        .unwrap()
});

pub fn run(ctx: &Context<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(amount_words(ctx));
    out.extend(citation_formats(ctx));
    out.extend(date_formats(ctx));
    out.extend(amount_formats(ctx));
    out.extend(term_spellings(ctx));
    out
}

/// The one rule here that checks a fact rather than a habit.
fn amount_words(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("CONSISTENCY_AMOUNT_WORDS");
    let mut out = Vec::new();

    for block in &ctx.document.blocks {
        for m in AMOUNT_WITH_WORDS.captures_iter(&block.text) {
            let digits = m.get(1).unwrap();
            let inner = m.get(2).unwrap();
            let Some(amount) = parse_amount(digits.as_str()) else {
                continue;
            };

            // Strip the currency words so only the number phrase is left. If
            // anything else survives, the parenthesis is not a written amount
            // and the rule stays out of it.
            let phrase = strip_currency(inner.as_str());
            let Some(written) = from_words(&phrase) else {
                continue;
            };
            if written == amount.major {
                continue;
            }
            let whole = m.get(0).unwrap();
            let start = block.text[..whole.start()].chars().count();
            let end = start + whole.as_str().chars().count();
            let mut f = b.at(
                &block.id,
                SourceLocation::span(block.source_location.block_index, start, end),
                0.95,
                format!(
                    "Rakamla {} yazılmış; yazıyla {} yazılmış.",
                    format_amount(amount.major),
                    format_amount(written)
                ),
                format!(
                    "Parantez içindeki yazılı tutar, rakamla belirtilen tutarla uyuşmuyor. \
                     {} tutarının yazılı karşılığı \u{201C}{}\u{201D} olmalıdır.",
                    format_amount(amount.major),
                    to_words(amount.major)
                ),
            );
            f.context = Some(crate::text::excerpt(&block.text, start, 50));
            out.push(f);
        }
    }
    out
}

fn strip_currency(s: &str) -> String {
    let lower = tr_lower(s);
    let cleaned = lower
        .replace("türk lirası", " ")
        .replace("türklirası", " ")
        .replace("lirası", " ")
        .replace("lira", " ")
        .replace("kuruş", " ")
        .replace("tl", " ")
        .replace("try", " ")
        .replace('₺', " ");
    cleaned.trim().to_string()
}

fn format_amount(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Where a surface form was first seen: block id, block index, char offset.
type Site = (String, usize, usize);
/// `(statute, article)` -> surface form -> first site.
type CitationIndex = BTreeMap<(String, String), BTreeMap<String, Site>>;

fn citation_formats(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("CONSISTENCY_CITATION_FORMAT");
    let mut out = Vec::new();
    let mut seen: CitationIndex = BTreeMap::new();

    for block in &ctx.document.blocks {
        for re in CITATION.iter() {
            for m in re.captures_iter(&block.text) {
                let whole = m.get(0).unwrap();
                let code = m.get(1).unwrap().as_str().to_string();
                let article = m.get(2).unwrap().as_str().to_string();
                let surface = collapse_spaces(whole.as_str());
                let start = block.text[..whole.start()].chars().count();
                seen.entry((code, article))
                    .or_default()
                    .entry(surface)
                    .or_insert((block.id.clone(), block.source_location.block_index, start));
            }
        }
    }

    for ((code, article), variants) in seen {
        if variants.len() < 2 {
            continue;
        }
        let names: Vec<&String> = variants.keys().collect();
        // Report on every form after the first, so the user sees each spelling.
        for (surface, (block_id, block_index, start)) in variants.iter().skip(1) {
            let mut f = b.at(
                block_id,
                SourceLocation::span(*block_index, *start, start + surface.chars().count()),
                0.6,
                format!(
                    "{code} {article}. madde atfı belgede {} farklı biçimde yazılmış: {}.",
                    variants.len(),
                    names
                        .iter()
                        .map(|n| format!("\u{201C}{n}\u{201D}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                "Aynı hükme yapılan atıflar belge boyunca aynı biçimde yazıldığında \
                 metin daha okunaklı olur. Hangi biçimin doğru olduğu konusunda \
                 İkinciGöz bir görüş bildirmez.",
            );
            f.context = None;
            out.push(f);
        }
    }
    out
}

fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(c);
    }
    out
}

fn date_formats(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("CONSISTENCY_DATE_FORMAT");
    let mut styles: BTreeSet<String> = BTreeSet::new();
    let mut first: FirstHit = None;

    for block in &ctx.document.blocks {
        let bi = block.source_location.block_index;
        for m in DATE_NUMERIC.captures_iter(&block.text) {
            // A quoted judgment writes dates its own way. Counting them as the
            // author's inconsistency asks them to edit a quotation.
            let at = block.text[..m.get(0).unwrap().start()].chars().count();
            if ctx.zones().is_verbatim(bi, at) {
                continue;
            }
            let sep = m.get(2).unwrap().as_str();
            styles.insert(match sep {
                "." => "gg.aa.yyyy".to_string(),
                "/" => "gg/aa/yyyy".to_string(),
                _ => "gg-aa-yyyy".to_string(),
            });
            record_first(&mut first, block, m.get(0).unwrap());
        }
        for m in DATE_WORDED.find_iter(&block.text) {
            let at = block.text[..m.start()].chars().count();
            if ctx.zones().is_verbatim(bi, at) {
                continue;
            }
            styles.insert("gg Ay yyyy".to_string());
            record_first(&mut first, block, m);
        }
    }

    if styles.len() < 2 {
        return Vec::new();
    }
    let (block_id, bi, start, len) = first.unwrap();
    let mut f = b.at(
        &block_id,
        SourceLocation::span(bi, start, start + len),
        0.6,
        format!(
            "Belgede {} farklı tarih biçimi kullanılmış: {}.",
            styles.len(),
            styles.iter().cloned().collect::<Vec<_>>().join(", ")
        ),
        "Tarihlerin belge boyunca tek bir biçimde yazılması, okuyucunun tarihleri \
         karşılaştırmasını kolaylaştırır.",
    );
    f.context = None;
    vec![f]
}

/// Block id, block index, char offset and char length of a first occurrence.
type FirstHit = Option<(String, usize, usize, usize)>;

fn record_first(slot: &mut FirstHit, block: &crate::cdm::Block, m: regex::Match<'_>) {
    if slot.is_some() {
        return;
    }
    let start = block.text[..m.start()].chars().count();
    *slot = Some((
        block.id.clone(),
        block.source_location.block_index,
        start,
        m.as_str().chars().count(),
    ));
}

fn amount_formats(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("CONSISTENCY_AMOUNT_FORMAT");
    let mut currencies: BTreeSet<String> = BTreeSet::new();
    let mut first: FirstHit = None;

    for block in &ctx.document.blocks {
        for m in AMOUNT_TOKEN.captures_iter(&block.text) {
            let unit = collapse_spaces(m.get(2).unwrap().as_str());
            currencies.insert(tr_lower(&unit));
            record_first(&mut first, block, m.get(0).unwrap());
        }
    }
    if currencies.len() < 2 {
        return Vec::new();
    }
    let (block_id, bi, start, len) = first.unwrap();
    let mut f = b.at(
        &block_id,
        SourceLocation::span(bi, start, start + len),
        0.6,
        format!(
            "Para birimi belgede {} farklı biçimde yazılmış: {}.",
            currencies.len(),
            currencies.iter().cloned().collect::<Vec<_>>().join(", ")
        ),
        "Tutarların belge boyunca tek bir biçimde yazılması tercih edilir.",
    );
    f.context = None;
    vec![f]
}

/// Words that are the same but for their Turkish diacritics.
///
/// `İstanbul`/`Istanbul` and `sözleşme`/`sozlesme` are the common shapes, and
/// both are caused by a keyboard, not by a decision. Requiring the ASCII-folded
/// forms to match keeps this from becoming an open-ended spell checker.
fn term_spellings(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("CONSISTENCY_TERM_SPELLING");
    let mut out = Vec::new();
    let mut groups: SpellingIndex = HashMap::new();

    for block in &ctx.document.blocks {
        if block.kind == BlockKind::TableCell {
            continue;
        }
        let chars: Vec<char> = block.text.chars().collect();
        for token in crate::text::tokenize(&block.text) {
            if token.text.chars().count() < 5 || token.text.chars().any(|c| c.is_ascii_digit()) {
                continue;
            }
            if in_dotted_path(&chars, &token) {
                continue;
            }
            let lower = tr_lower(&token.text);
            let folded = ascii_fold(&lower);
            if folded == lower {
                // No Turkish-specific letters at all; nothing to compare.
                continue;
            }
            let entry = groups.entry(folded).or_default().entry(lower).or_insert((
                0,
                block.id.clone(),
                block.source_location.block_index,
                token.start,
            ));
            entry.0 += 1;
        }
        // Also record the fully-folded spellings, so a word typed without
        // diacritics is compared against the properly spelled one.
        for token in crate::text::tokenize(&block.text) {
            if token.text.chars().count() < 5 || token.text.chars().any(|c| c.is_ascii_digit()) {
                continue;
            }
            if in_dotted_path(&chars, &token) {
                continue;
            }
            let lower = tr_lower(&token.text);
            let folded = ascii_fold(&lower);
            if folded != lower {
                continue;
            }
            if let Some(g) = groups.get_mut(&folded) {
                let entry = g.entry(lower).or_insert((
                    0,
                    block.id.clone(),
                    block.source_location.block_index,
                    token.start,
                ));
                entry.0 += 1;
            }
        }
    }

    for (_folded, variants) in groups {
        if variants.len() < 2 {
            continue;
        }
        let mut ordered: Vec<(&String, &SpellingSite)> = variants.iter().collect();
        ordered.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
        let dominant = ordered[0].0.clone();
        let names: Vec<String> = ordered
            .iter()
            .map(|(k, _)| format!("\u{201C}{k}\u{201D}"))
            .collect();
        for (form, (_count, block_id, bi, start)) in ordered.into_iter().skip(1) {
            let mut f = b.at(
                block_id,
                SourceLocation::span(*bi, *start, start + form.chars().count()),
                0.6,
                format!(
                    "Aynı kelime belgede farklı yazılmış: {}.",
                    names.join(" / ")
                ),
                format!(
                    "\u{201C}{dominant}\u{201D} biçimi belgede daha sık geçiyor. Fark yalnızca \
                     Türkçe karakterlerde olduğu için klavyeden kaynaklanmış olabilir."
                ),
            );
            f.context = None;
            out.push(f);
        }
    }
    out
}

/// Where a spelling was seen: occurrence count plus its first site.
type SpellingSite = (usize, String, usize, usize);
/// folded spelling -> actual spelling -> site.
type SpellingIndex = HashMap<String, BTreeMap<String, SpellingSite>>;

/// Is this token a component of a domain name, an e-mail address or a path?
///
/// `www.yuksekbas.av.tr` is deliberately written without Turkish letters, so
/// comparing it against `YÜKSEKBAŞ` reports a spelling inconsistency that the
/// author cannot fix — a domain name has no diacritics to restore.
fn in_dotted_path(chars: &[char], token: &crate::text::Token) -> bool {
    let before = token
        .start
        .checked_sub(1)
        .and_then(|i| chars.get(i))
        .copied();
    if matches!(before, Some('.') | Some('@') | Some('/')) {
        return true;
    }
    // `yuksekbas.av` — a dot with another word component right after it.
    match (chars.get(token.end), chars.get(token.end + 1)) {
        (Some('.'), Some(next)) if next.is_alphanumeric() => true,
        (Some('@'), _) => true,
        _ => false,
    }
}

fn ascii_fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' => 'i',
            'ö' => 'o',
            'ş' => 's',
            'ü' => 'u',
            'â' => 'a',
            'î' => 'i',
            'û' => 'u',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::rules::testing::*;

    const WORDS: &str = "CONSISTENCY_AMOUNT_WORDS";
    const CITE: &str = "CONSISTENCY_CITATION_FORMAT";
    const DATE: &str = "CONSISTENCY_DATE_FORMAT";
    const CURRENCY: &str = "CONSISTENCY_AMOUNT_FORMAT";
    const TERM: &str = "CONSISTENCY_TERM_SPELLING";

    #[test]
    fn a_written_amount_that_contradicts_the_figure_is_reported() {
        let f = lint(&["Alacak tutarı 50.000,00 TL (Yüz Bin Türk Lirası) olarak belirlenmiştir."]);
        let a = of(&f, WORDS);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].severity, crate::finding::Severity::Error);
        assert!(
            a[0].explanation.contains("elli bin"),
            "{}",
            a[0].explanation
        );
    }

    #[test]
    fn a_matching_written_amount_is_silent() {
        assert_silent(
            &["Alacak tutarı 50.000,00 TL (Ellibin Türk Lirası) olarak belirlenmiştir."],
            WORDS,
        );
        assert_silent(
            &["Alacak tutarı 50.000,00 TL (Elli Bin Türk Lirası) olarak belirlenmiştir."],
            WORDS,
        );
    }

    #[test]
    fn a_parenthesis_that_is_not_an_amount_is_left_alone() {
        assert_silent(
            &["Bedel 50.000,00 TL (KDV hariç) olarak kararlaştırılmıştır."],
            WORDS,
        );
    }

    #[test]
    fn mixed_citation_formats_are_reported_for_review() {
        let f = lint(&[
            "HMK m. 119 uyarınca dava dilekçesi sunulmuştur.",
            "Ayrıca HMK m.119 hükmü gereğince ekler eklenmiştir.",
        ]);
        let c = of(&f, CITE);
        assert_eq!(c.len(), 1, "{:?}", c);
        assert_eq!(c[0].severity, crate::finding::Severity::Review);
    }

    #[test]
    fn a_consistently_written_citation_is_silent() {
        assert_silent(
            &[
                "HMK m. 119 uyarınca dilekçe sunulmuştur.",
                "HMK m. 119 hükmü gereğince işlem yapılmıştır.",
            ],
            CITE,
        );
    }

    #[test]
    fn mixed_date_formats_are_reported_once() {
        let f = lint(&[
            "Sözleşme 01.02.2020 tarihinde imzalanmıştır.",
            "Fesih 15/03/2021 tarihinde bildirilmiştir.",
        ]);
        assert_eq!(of(&f, DATE).len(), 1);
    }

    #[test]
    fn a_date_inside_a_quoted_judgment_is_not_the_authors_inconsistency() {
        assert_silent(
            &[
                "Sözleşme 01.02.2020 tarihinde imzalanmış bulunmaktadır.",
                "Yargıtay, \"Taraflar arasında 22/09/2012 başlangıç tarihli kira sözleşmesi konusunda uyuşmazlık bulunmamaktadır.\" demiştir.",
            ],
            DATE,
        );
    }

    #[test]
    fn a_domain_name_is_not_a_misspelling_of_the_name_it_contains() {
        assert_silent(
            &[
                "Av. Raci Çetin YÜKSEKBAŞ tarafından dilekçe sunulmuş bulunmaktadır.",
                "İletişim için www.yuksekbas.av.tr adresine başvurulabilir bilgisi verilir.",
                "Ayrıca YÜKSEKBAŞ hukuk bürosu tarafından takip edilmektedir dosya.",
            ],
            TERM,
        );
    }

    #[test]
    fn a_single_date_format_is_silent() {
        assert_silent(
            &[
                "Sözleşme 01.02.2020 tarihinde imzalanmıştır.",
                "Fesih 15.03.2021 tarihinde bildirilmiştir.",
            ],
            DATE,
        );
    }

    #[test]
    fn mixed_currency_spellings_are_reported_once() {
        let f = lint(&[
            "Alacak tutarı 50.000,00 TL olarak belirlenmiştir.",
            "Ayrıca 1.500,00 Türk Lirası vekalet ücreti talep edilmektedir.",
        ]);
        assert_eq!(of(&f, CURRENCY).len(), 1);
    }

    #[test]
    fn a_word_typed_without_turkish_characters_is_reported() {
        let f = lint(&[
            "Taraflar arasındaki sözleşme feshedilmiştir ve bildirim yapılmıştır.",
            "İkinci sozlesme ise ayrıca imzalanmış ve dosyaya sunulmuştur.",
            "Üçüncü sözleşme henüz imzalanmamıştır ve beklemededir.",
        ]);
        let t = of(&f, TERM);
        assert_eq!(t.len(), 1, "{:?}", t);
        assert!(t[0].message.contains("sozlesme"));
    }

    #[test]
    fn consistently_spelled_words_are_silent() {
        assert_silent(
            &[
                "Taraflar arasındaki sözleşme feshedilmiştir ve bildirim yapılmıştır.",
                "İkinci sözleşme ise ayrıca imzalanmış ve dosyaya sunulmuştur.",
            ],
            TERM,
        );
    }
}
