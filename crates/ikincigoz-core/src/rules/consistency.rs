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
        if i > 0 && (s.len() - i).is_multiple_of(3) {
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
                 GölgeDosya bir görüş bildirmez.",
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
///
/// GölgeDosya field item 36 ("terim tutarsız olduğunda seçenekli düzeltme
/// öneremez mi?"): the rule only reported, and its choice of reference form was
/// "most frequent, ties by byte order" — on a tie or a partial-diacritic
/// majority it pointed AT the correct spelling and said "daha sık geçiyor".
/// Now:
///  * the reference is the form with the most Turkish letters, then the most
///    frequent; a true tie has no reference and no fix;
///  * every occurrence of another form is a finding at its own span;
///  * a fix ("“sözleşme” olarak değiştirilir") is attached only when the
///    variant is a pure diacritic-fold of the reference AND differs in ç/ğ/ö/ş/ü.
///    A difference in ı/i alone can be two words (`sınır`/`sinir`): reported,
///    never fixed. Fixes are applied only when the user ticks them, to a copy.
fn term_spellings(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("CONSISTENCY_TERM_SPELLING");
    let mut out = Vec::new();

    // 1) Every candidate token, in document order.
    let mut seen: Vec<(String, String, Occurrence)> = Vec::new();
    for block in &ctx.document.blocks {
        if block.kind == BlockKind::TableCell {
            continue;
        }
        let bi = block.source_location.block_index;
        let chars: Vec<char> = block.text.chars().collect();
        for token in crate::text::tokenize(&block.text) {
            let len = token.text.chars().count();
            if len < 5 || token.text.chars().any(|c| c.is_ascii_digit()) {
                continue;
            }
            if in_dotted_path(&chars, &token) {
                continue;
            }
            // A quotation reproduces someone else's text; it is not ours to fix.
            if ctx
                .zones()
                .range_is_verbatim(bi, token.start, token.start + len)
            {
                continue;
            }
            let lower = tr_lower(&token.text);
            let folded = ascii_fold(&lower);
            seen.push((
                folded,
                lower,
                Occurrence {
                    block_id: block.id.clone(),
                    block_index: bi,
                    start: token.start,
                    len,
                    original: token.text.to_string(),
                },
            ));
        }
    }

    // 2) Group by folded spelling. Only a word that is written WITH Turkish
    //    letters somewhere can be misspelled without them; a word that never
    //    has any is not compared. Collecting first makes this independent of
    //    which spelling comes first in the document.
    let mut groups: HashMap<String, BTreeMap<String, Vec<Occurrence>>> = HashMap::new();
    let has_turkish: BTreeSet<&String> = seen
        .iter()
        .filter(|(folded, lower, _)| folded != lower)
        .map(|(folded, _, _)| folded)
        .collect();
    for (folded, lower, occ) in &seen {
        if has_turkish.contains(folded) {
            groups
                .entry(folded.clone())
                .or_default()
                .entry(lower.clone())
                .or_default()
                .push(occ.clone());
        }
    }

    // 3) Reference form, findings, fixes.
    let mut keys: Vec<&String> = groups.keys().collect();
    keys.sort();
    for key in keys {
        let variants = &groups[key];
        if variants.len() < 2 {
            continue;
        }
        let mut ranked: Vec<(&String, &Vec<Occurrence>)> = variants.iter().collect();
        ranked.sort_by(|a, b| {
            turkish_letters(b.0)
                .cmp(&turkish_letters(a.0))
                .then(b.1.len().cmp(&a.1.len()))
        });
        let reference = ranked[0].0.clone();
        let tie = turkish_letters(ranked[1].0) == turkish_letters(&reference)
            && ranked[1].1.len() == ranked[0].1.len();
        let names: Vec<String> = ranked
            .iter()
            .map(|(k, v)| format!("\u{201C}{k}\u{201D} ({})", v.len()))
            .collect();
        for (form, occurrences) in ranked.iter().skip(1) {
            let fixable = !tie && diacritic_fix_applies(form, &reference);
            for occ in occurrences.iter() {
                let explanation = if tie {
                    "Yazımlar eşit sayıda geçiyor ve hangisinin doğru olduğu belgeden \
                     anlaşılamıyor. Fark yalnızca Türkçe karakterlerde."
                        .to_string()
                } else if fixable {
                    format!(
                        "\u{201C}{reference}\u{201D} Türkçe karakterleriyle yazılmış biçimdir. \
                         Fark yalnızca Türkçe karakterlerde olduğu için klavyeden kaynaklanmış olabilir."
                    )
                } else {
                    "Fark yalnızca ı/i gibi harflerde; bunlar iki ayrı kelime de olabilir \
                     (ör. \u{201C}sınır\u{201D} / \u{201C}sinir\u{201D}). Kontrol edin."
                        .to_string()
                };
                let mut f = b.at(
                    &occ.block_id,
                    SourceLocation::span(occ.block_index, occ.start, occ.start + occ.len),
                    0.6,
                    format!(
                        "Aynı kelime belgede farklı yazılmış: {}.",
                        names.join(" / ")
                    ),
                    explanation,
                );
                f.context = None;
                if fixable {
                    let replacement = super::ortho::carry_case(&occ.original, &reference);
                    f.fix = Some(crate::finding::Fix {
                        block_id: occ.block_id.clone(),
                        char_start: occ.start,
                        char_end: occ.start + occ.len,
                        original: occ.original.clone(),
                        replacement,
                        description: format!("\u{201C}{reference}\u{201D} olarak değiştirilir"),
                    });
                }
                out.push(f);
            }
        }
    }
    out
}

/// Where one spelling was seen.
#[derive(Clone)]
struct Occurrence {
    block_id: String,
    block_index: usize,
    start: usize,
    len: usize,
    original: String,
}

/// Letters a Turkish keyboard adds; the form that has them was typed on purpose.
fn turkish_letters(s: &str) -> usize {
    s.chars().filter(|c| "çğıöşüâîû".contains(*c)).count()
}

/// Can `variant` safely become `reference`? Only when every differing letter is
/// the reference's diacritic folded away, and at least one of them is ç/ğ/ö/ş/ü.
/// ı/i (and â/a, î/i, û/u) alone can separate two real words.
fn diacritic_fix_applies(variant: &str, reference: &str) -> bool {
    let v: Vec<char> = variant.chars().collect();
    let r: Vec<char> = reference.chars().collect();
    if v.len() != r.len() {
        return false;
    }
    let mut decisive = false;
    for (a, b) in v.iter().zip(&r) {
        if a == b {
            continue;
        }
        if ascii_fold(&b.to_string()) != a.to_string() {
            return false;
        }
        if "çğöşü".contains(*b) {
            decisive = true;
        }
    }
    decisive
}

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

    fn span(f: &crate::finding::Finding) -> (String, usize, usize) {
        (
            f.block_id.clone(),
            f.location.char_start.unwrap(),
            f.location.char_end.unwrap(),
        )
    }

    #[test]
    fn the_misspelling_is_flagged_and_offered_the_correct_form() {
        // Tie in frequency: the old rule flagged the CORRECT "sözleşme" (p0)
        // and said it was "more frequent".
        let f = lint(&[
            "Taraflar arasındaki sözleşme feshedilmiştir.",
            "Ancak sozlesme hükümleri uygulanmamıştır.",
        ]);
        let t = of(&f, TERM);
        assert_eq!(t.len(), 1, "{t:?}");
        assert_eq!(span(&t[0]), ("p1".into(), 6, 14));
        let fix = t[0].fix.as_ref().expect("düzeltme önerisi");
        assert_eq!(
            (fix.original.as_str(), fix.replacement.as_str()),
            ("sozlesme", "sözleşme")
        );
        assert!(!t[0].explanation.contains("daha sık"));
    }

    #[test]
    fn a_partial_diacritic_majority_does_not_win_over_the_correct_form() {
        let f = lint(&[
            "Taraflar arasındaki sözleşme feshedilmiştir.",
            "Ancak sözlesme hükümleri uygulanmamıştır.",
            "Bu sözlesme de ayrıca geçersizdir.",
        ]);
        let t = of(&f, TERM);
        assert!(t.iter().all(|x| x.block_id != "p0"), "{t:?}");
        assert_eq!(t.len(), 2);
        assert!(t
            .iter()
            .all(|x| x.fix.as_ref().unwrap().replacement == "sözleşme"));
    }

    #[test]
    fn every_occurrence_gets_its_own_finding_and_keeps_its_case() {
        let f = lint(&[
            "Taraflar arasındaki sözleşme feshedilmiştir.",
            "Sozlesme hükümleri uygulanmamıştır.",
            "Ayrıca sozlesme ekleri eksiktir.",
            "SOZLESME başlıklı belge de sunulmuştur.",
        ]);
        let t = of(&f, TERM);
        let replacements: Vec<&str> = t
            .iter()
            .map(|x| x.fix.as_ref().unwrap().replacement.as_str())
            .collect();
        assert_eq!(replacements, ["Sözleşme", "sözleşme", "SÖZLEŞME"]);
    }

    #[test]
    fn a_true_tie_has_no_reference_and_no_fix() {
        let f = lint(&[
            "Taraflar arasındaki sözlesme feshedilmiştir.",
            "Ancak sozleşme hükümleri uygulanmamıştır.",
        ]);
        let t = of(&f, TERM);
        assert_eq!(t.len(), 1);
        assert!(t[0].fix.is_none());
        assert!(t[0].explanation.contains("anlaşılamıyor"));
        assert!(!t[0].explanation.contains("daha sık"));
    }

    #[test]
    fn i_and_dotless_i_alone_are_never_fixed() {
        // "sınır" ve "sinir" iki ayrı kelime: "sinir hastalığı" düzeltilmemeli.
        let f = lint(&[
            "Parselin sınır çizgisi belirlenmiştir.",
            "Komşu sınır tespit edilmiştir.",
            "Davacının sinir hastalığı raporla sabittir.",
        ]);
        assert!(of(&f, TERM).iter().all(|x| x.fix.is_none()));
    }

    #[test]
    fn the_order_of_spellings_in_the_document_does_not_matter() {
        // Eskiden harfsiz yazım, doğru yazımdan ÖNCE geçerse hiç kaydedilmiyordu.
        let f = lint(&[
            "Ancak sozlesme hükümleri uygulanmamıştır.",
            "Taraflar arasındaki sözleşme feshedilmiştir.",
            "Üçüncü sözleşme henüz imzalanmamıştır.",
        ]);
        let t = of(&f, TERM);
        assert_eq!(t.len(), 1);
        assert_eq!(span(&t[0]), ("p0".into(), 6, 14));
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
