//! Draft residue: the marks a document carries while it is still being written.
//!
//! These are the findings the product exists for. A placeholder that reaches
//! the court is not a style problem.

use super::Context;
use crate::cdm::SourceLocation;
use crate::finding::{Finding, FindingBuilder};
use crate::text::{tr_lower, tr_upper};
use once_cell::sync::Lazy;
use regex::Regex;

fn builder(id: &str) -> FindingBuilder {
    FindingBuilder::new(crate::finding::rule_info(id).expect("registered rule"))
}

/// Placeholder shapes, each with what to tell the user it is.
static PLACEHOLDERS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    vec![
        (
            Regex::new(r"(?i)\b(TODO|FIXME|TBD)\b").unwrap(),
            "taslak notu",
        ),
        (Regex::new(r"X{3,}").unwrap(), "doldurulmamış yer tutucu"),
        (Regex::new(r"\?{3,}").unwrap(), "belirsiz bırakılmış bölüm"),
        (Regex::new(r"_{3,}").unwrap(), "doldurulmamış boşluk"),
        (Regex::new(r"\.{5,}").unwrap(), "doldurulmamış boşluk"),
    ]
});

/// `[BURAYA EKLE]`, `[TARİH]`, `<İSİM>` — a bracketed all-caps slot.
static BRACKET_SLOT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\[<]([^\]>\n]{1,40})[\]>]").unwrap());

/// A paragraph ending on one of these is a sentence that was never finished.
const CONNECTORS: &[&str] = &[
    "ve",
    "veya",
    "ile",
    "ancak",
    "fakat",
    "ama",
    "çünkü",
    "zira",
    "dolayısıyla",
    "nedeniyle",
    "sebebiyle",
    "ayrıca",
    "bununla",
    "buna",
    "şöyle",
    "yani",
    "ki",
    "gibi",
    "kadar",
    "üzere",
    "rağmen",
    "karşın",
    "göre",
];

pub fn run(ctx: &Context<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    let ph = builder("DRAFT_PLACEHOLDER");
    let dangling = builder("DRAFT_DANGLING_CONNECTOR");
    let empty = builder("DRAFT_EMPTY_NUMBERED");

    for block in &ctx.document.blocks {
        let bi = block.source_location.block_index;

        for (re, what) in PLACEHOLDERS.iter() {
            for m in re.find_iter(&block.text) {
                let start = block.text[..m.start()].chars().count();
                let end = start + m.as_str().chars().count();
                let mut f = ph.at(
                    &block.id,
                    SourceLocation::span(bi, start, end),
                    0.95,
                    format!(
                        "Belgede {what} kalmış: \u{201C}{}\u{201D}.",
                        m.as_str().trim()
                    ),
                    "Taslak hazırlanırken bırakılan işaretler, belge gönderilmeden \
                     önce doldurulmalı veya kaldırılmalıdır.",
                );
                f.context = Some(crate::text::excerpt(&block.text, start, 40));
                out.push(f);
            }
        }

        for m in BRACKET_SLOT.captures_iter(&block.text) {
            let whole = m.get(0).unwrap();
            let inner = m.get(1).unwrap().as_str();
            if !is_placeholder_slot(inner) {
                continue;
            }
            let start = block.text[..whole.start()].chars().count();
            let end = start + whole.as_str().chars().count();
            let mut f = ph.at(
                &block.id,
                SourceLocation::span(bi, start, end),
                0.9,
                format!(
                    "Doldurulmamış yer tutucu: \u{201C}{}\u{201D}.",
                    whole.as_str()
                ),
                "Köşeli veya açılı parantez içine büyük harfle yazılmış bu ifade, \
                 sonradan doldurulmak üzere bırakılmış bir yer tutucu görünümünde.",
            );
            f.context = Some(crate::text::excerpt(&block.text, start, 40));
            out.push(f);
        }

        let body = block.body_text().trim();
        if let (Some(numbering), true) = (block.numbering.as_ref(), body.is_empty()) {
            let mut f = empty.at(
                &block.id,
                SourceLocation::block(bi),
                0.9,
                format!(
                    "\u{201C}{}\u{201D} numarası verilmiş; ancak maddenin metni yazılmamış.",
                    numbering.raw
                ),
                "Numaralandırılmış bir madde içeriksiz bırakılmış.",
            );
            f.context = None;
            out.push(f);
        }

        if let Some(word) = trailing_connector(body) {
            let start = body.chars().count().saturating_sub(word.chars().count());
            let offset = block.numbering.as_ref().map(|n| n.text_start).unwrap_or(0);
            let mut f = dangling.at(
                &block.id,
                SourceLocation::span(bi, offset + start, offset + start + word.chars().count()),
                0.75,
                format!("Paragraf \u{201C}{word}\u{201D} bağlacıyla bitiyor."),
                "Bir bağlaçla biten paragraf, çoğunlukla tamamlanmamış bir cümlenin \
                 işaretidir.",
            );
            f.context = Some(crate::text::excerpt(&block.text, block.char_len(), 40));
            out.push(f);
        }
    }
    out
}

/// True for the inside of a bracket that reads as a slot to fill in.
fn is_placeholder_slot(inner: &str) -> bool {
    let trimmed = inner.trim();
    if trimmed.is_empty() || trimmed.chars().count() < 3 {
        return false;
    }
    // `[...]` marks an elision inside a quotation; `[1]` is a footnote marker.
    if trimmed
        .chars()
        .all(|c| c == '.' || c.is_ascii_digit() || c.is_whitespace())
    {
        return false;
    }
    if !trimmed.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    // The slot convention is upper case throughout.
    tr_upper(trimmed) == trimmed
}

/// The connector a paragraph ends on, if any.
fn trailing_connector(body: &str) -> Option<&'static str> {
    let trimmed = body.trim_end();
    // An item in a numbered request list ends with a comma on purpose: the
    // sentence carries on into the next item. `1. Davamızın KABULÜ ile,` is
    // correct drafting, and reporting it made a well-formed petition look
    // unfinished.
    if trimmed.ends_with(',') || trimmed.ends_with(';') {
        return None;
    }
    let cleaned = trimmed.trim_end_matches(|c: char| c.is_whitespace());
    // A paragraph that ends in a full stop is finished, whatever the last word.
    if cleaned.ends_with('.')
        || cleaned.ends_with(':')
        || cleaned.ends_with('?')
        || cleaned.ends_with('!')
    {
        return None;
    }
    let last = cleaned.rsplit(|c: char| c.is_whitespace()).next()?;
    let folded = tr_lower(last);
    CONNECTORS.iter().copied().find(|c| *c == folded)
}

#[cfg(test)]
mod tests {
    use crate::rules::testing::*;

    const PH: &str = "DRAFT_PLACEHOLDER";
    const DANGLE: &str = "DRAFT_DANGLING_CONNECTOR";
    const EMPTY: &str = "DRAFT_EMPTY_NUMBERED";

    #[test]
    fn common_placeholder_markers_are_reported() {
        assert_count(&["Duruşma tarihi XXX olarak belirlenmiştir."], PH, 1);
        assert_count(&["Tutar ??? TL olarak hesaplanmıştır."], PH, 1);
        assert_count(&["TODO: ekleri kontrol et."], PH, 1);
        assert_count(&["Tarih: ________"], PH, 1);
    }

    #[test]
    fn a_bracketed_all_caps_slot_is_reported() {
        assert_count(&["Müvekkil [İSİM] adına dilekçe sunulmuştur."], PH, 1);
        assert_count(&["Tarih [BURAYA EKLE] olarak yazılacaktır."], PH, 1);
    }

    #[test]
    fn an_elision_in_a_quotation_is_not_a_placeholder() {
        assert_silent(
            &["Raporda \u{201C}taşınmazın değeri [...] belirlenmiştir\u{201D} denilmektedir."],
            PH,
        );
    }

    #[test]
    fn a_footnote_marker_is_not_a_placeholder() {
        assert_silent(
            &["Bu görüş öğretide de kabul edilmektedir [1] ve uygulanmaktadır."],
            PH,
        );
    }

    #[test]
    fn a_lowercase_bracketed_reference_is_not_a_placeholder() {
        assert_silent(&["İlgili belge [ek-1] olarak sunulmuştur."], PH);
    }

    #[test]
    fn a_paragraph_ending_in_a_connector_is_reported() {
        assert_count(
            &["Davacı, davalıya karşı alacak davası açmış ve"],
            DANGLE,
            1,
        );
        assert_count(&["Bu husus önemlidir, ancak"], DANGLE, 1);
    }

    #[test]
    fn an_item_in_a_request_list_ending_in_a_comma_is_not_dangling() {
        assert_silent(&["1. Davamızın KABULÜ ile,"], DANGLE);
        assert_silent(
            &["2. Yargılama giderlerinin davalıya yükletilmesine,"],
            DANGLE,
        );
    }

    #[test]
    fn a_finished_paragraph_is_not_dangling() {
        assert_silent(
            &["Davacı, davalıya karşı alacak davası açmış ve talepte bulunmuştur."],
            DANGLE,
        );
    }

    #[test]
    fn a_heading_ending_in_a_noun_is_not_dangling() {
        assert_silent(&["A. USULE İLİŞKİN İTİRAZLAR"], DANGLE);
    }

    #[test]
    fn an_empty_numbered_item_is_reported() {
        let f = lint(&["1. Birinci madde metni.", "2.", "3. Üçüncü madde metni."]);
        assert_eq!(of(&f, EMPTY).len(), 1);
    }
}
