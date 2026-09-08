//! Attachments and internal cross-references.
//!
//! The distinction that makes this work: an `Ek-3` at the head of its own
//! paragraph *defines* attachment three, while an `Ek-3` inside a sentence
//! *refers* to it. Everything else follows from that.

use super::Context;
use crate::cdm::{BlockKind, SourceLocation};
use crate::finding::{Finding, FindingBuilder};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};

fn builder(id: &str) -> FindingBuilder {
    FindingBuilder::new(crate::finding::rule_info(id).expect("registered rule"))
}

/// `Ek-1`, `EK 2`, `Ek.3`. The capital E is required: `ek` is an ordinary
/// Turkish noun and matching it case-insensitively would fire constantly.
static ATTACHMENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\bEK[-\s.]?(\d{1,3})\b").unwrap());

/// An internal reference, which only counts when the sentence anchors it to
/// this document. Without the anchor, `HMK 119. maddede` — a reference to a
/// statute, not to this petition — would be reported as a broken link.
static INTERNAL_REF: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(yukarıda|aşağıda|işbu|bu dilekçenin|bu dilekçede|belgenin|yukarıdaki|aşağıdaki)[^.;:]{0,60}?\b(\d{1,3}|[A-ZÇĞİÖŞÜ])\s*\.\s*(madde|başlık|bölüm|bent|fıkra)",
    )
    .unwrap()
});

struct Occurrence {
    number: u32,
    block_index: usize,
    block_id: String,
    char_start: usize,
    char_end: usize,
    is_definition: bool,
}

/// Whether an occurrence at the head of a block is defining that attachment.
///
/// Three things have to hold, and each one rules out a sentence that merely
/// happens to start with an attachment number:
///
/// * it sits at the head of its block — `Ek-1 Vekaletname örneği`;
/// * it carries no case suffix — `Ek-3'te belirtildiği` is a reference;
/// * it is the only attachment named in that block — a list entry names one
///   attachment, so `Ek-1 ve Ek-2 dosyaya sunulmuştur` is a sentence.
fn is_definition(
    char_start: usize,
    char_end: usize,
    marker_end: usize,
    block_text: &str,
    occurrences_in_block: usize,
) -> bool {
    if char_start > marker_end + 1 || occurrences_in_block != 1 {
        return false;
    }
    !block_text
        .chars()
        .nth(char_end)
        .map(|c| c == '\'' || c == '\u{2019}' || c.is_alphabetic())
        .unwrap_or(false)
}

pub fn run(ctx: &Context<'_>) -> Vec<Finding> {
    let occurrences = collect(ctx);
    let mut out = attachment_findings(ctx, &occurrences);
    out.extend(section_references(ctx));
    out
}

fn collect(ctx: &Context<'_>) -> Vec<Occurrence> {
    let mut out = Vec::new();
    for block in &ctx.document.blocks {
        if block.kind == BlockKind::TableCell {
            continue;
        }
        let marker_end = block.numbering.as_ref().map(|n| n.text_start).unwrap_or(0);
        // Match on the upper-cased text so `Ek` and `EK` are both found, while
        // keeping offsets aligned by upper-casing char by char.
        let upper: String = block
            .text
            .chars()
            .map(|c| {
                let u = crate::text::tr_upper(&c.to_string());
                if u.chars().count() == 1 {
                    u.chars().next().unwrap()
                } else {
                    c
                }
            })
            .collect();
        let in_block = ATTACHMENT.captures_iter(&upper).count();
        for m in ATTACHMENT.captures_iter(&upper) {
            let whole = m.get(0).unwrap();
            let Some(number) = m.get(1).and_then(|g| g.as_str().parse::<u32>().ok()) else {
                continue;
            };
            let char_start = upper[..whole.start()].chars().count();
            let char_end = char_start + whole.as_str().chars().count();
            out.push(Occurrence {
                number,
                block_index: block.source_location.block_index,
                block_id: block.id.clone(),
                char_start,
                char_end,
                is_definition: is_definition(
                    char_start,
                    char_end,
                    marker_end,
                    &block.text,
                    in_block,
                ),
            });
        }
    }
    out
}

fn attachment_findings(ctx: &Context<'_>, occurrences: &[Occurrence]) -> Vec<Finding> {
    let gap = builder("REF_ATTACHMENT_SEQ_GAP");
    let missing = builder("REF_ATTACHMENT_MISSING");
    let unused = builder("REF_ATTACHMENT_UNUSED");
    let mut out = Vec::new();

    let defined: BTreeMap<u32, &Occurrence> = occurrences
        .iter()
        .filter(|o| o.is_definition)
        .map(|o| (o.number, o))
        .collect();
    if defined.is_empty() {
        // Without a list of attachments there is nothing to check a reference
        // against, and guessing would be worse than staying quiet.
        return out;
    }

    // Gaps in the attachment list itself.
    let numbers: Vec<u32> = defined.keys().copied().collect();
    for pair in numbers.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b <= a + 1 {
            continue;
        }
        let occ = defined[&b];
        let miss: Vec<String> = ((a + 1)..b).take(8).map(|n| format!("Ek-{n}")).collect();
        let mut f = gap.at(
            &occ.block_id,
            SourceLocation::span(occ.block_index, occ.char_start, occ.char_end),
            0.9,
            format!("Ek-{a} ile Ek-{b} arasında {} bulunmuyor.", miss.join(", ")),
            "Ek listesindeki numaralandırmada atlanan sıra var.",
        );
        f.context = None;
        out.push(f);
    }

    // References pointing at an attachment that was never listed.
    let mut reported: BTreeSet<(usize, u32)> = BTreeSet::new();
    for o in occurrences.iter().filter(|o| !o.is_definition) {
        if defined.contains_key(&o.number) || !reported.insert((o.block_index, o.number)) {
            continue;
        }
        let block = &ctx.document.blocks[o.block_index];
        let mut f = missing.at(
            &o.block_id,
            SourceLocation::span(o.block_index, o.char_start, o.char_end),
            0.9,
            format!(
                "Metinde Ek-{} anılıyor; ancak ek listesinde Ek-{} yok.",
                o.number, o.number
            ),
            "Belgede tanımlı ekler listelenmiş; metnin atıf yaptığı ek bu listede bulunmuyor.",
        );
        f.context = Some(crate::text::excerpt(&block.text, o.char_start, 40));
        out.push(f);
    }

    // Attachments listed but never mentioned. A judgement call, so review only.
    let referenced: BTreeSet<u32> = occurrences
        .iter()
        .filter(|o| !o.is_definition)
        .map(|o| o.number)
        .collect();
    for (number, occ) in &defined {
        if referenced.contains(number) {
            continue;
        }
        let mut f = unused.at(
            &occ.block_id,
            SourceLocation::span(occ.block_index, occ.char_start, occ.char_end),
            0.6,
            format!("Ek-{number} listede yer alıyor; ancak metin içinde anılmamış."),
            "Ek listesinde bulunan bir eke metnin gövdesinde atıf yapılmamış. \
             Bilinçli bir tercih olabilir.",
        );
        f.context = None;
        out.push(f);
    }
    out
}

fn section_references(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("REF_SECTION_BROKEN");
    let mut out = Vec::new();

    // Everything the document actually numbers, as raw marker cores.
    let existing: BTreeSet<String> = ctx
        .document
        .blocks
        .iter()
        .filter_map(|bl| bl.numbering.as_ref())
        .map(|n| {
            crate::text::tr_upper(
                n.raw
                    .trim_end_matches(['.', ')', ' '])
                    .trim_start_matches('('),
            )
        })
        .collect();
    if existing.is_empty() {
        return out;
    }

    for block in &ctx.document.blocks {
        for m in INTERNAL_REF.captures_iter(&block.text) {
            let target = m.get(2).unwrap();
            let key = crate::text::tr_upper(target.as_str());
            if existing.contains(&key) {
                continue;
            }
            let whole = m.get(0).unwrap();
            let start = block.text[..whole.start()].chars().count();
            let end = start + whole.as_str().chars().count();
            let kind = m.get(3).unwrap().as_str();
            let mut f = b.at(
                &block.id,
                SourceLocation::span(block.source_location.block_index, start, end),
                0.7,
                format!("Metin \u{201C}{}. {kind}\u{201D} bölümüne atıf yapıyor; ancak belgede bu numarada bir bölüm yok.", target.as_str()),
                "Belge içi atıf, belgede bulunmayan bir başlığa veya maddeye işaret ediyor.",
            );
            f.context = Some(crate::text::excerpt(&block.text, start, 45));
            out.push(f);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::rules::testing::*;

    const GAP: &str = "REF_ATTACHMENT_SEQ_GAP";
    const MISSING: &str = "REF_ATTACHMENT_MISSING";
    const UNUSED: &str = "REF_ATTACHMENT_UNUSED";
    const BROKEN: &str = "REF_SECTION_BROKEN";

    #[test]
    fn a_gap_in_the_attachment_list_is_reported() {
        let f = lint(&[
            "Ek-1 Vekaletname örneği",
            "Ek-2 Sözleşme örneği",
            "Ek-4 Fatura örneği",
            "Ek-1, Ek-2 ve Ek-4 sunulmuştur.",
        ]);
        let g = of(&f, GAP);
        assert_eq!(g.len(), 1);
        assert!(g[0].message.contains("Ek-3"));
    }

    #[test]
    fn a_complete_attachment_list_is_silent() {
        assert_silent(
            &[
                "Ek-1 Vekaletname",
                "Ek-2 Sözleşme",
                "Ek-1 ve Ek-2 dilekçe ekinde sunulmuştur.",
            ],
            GAP,
        );
    }

    #[test]
    fn a_reference_to_an_undefined_attachment_is_reported() {
        let f = lint(&[
            "Ek-1 Vekaletname örneği",
            "Ek-2 Sözleşme örneği",
            "Ek-3'te belirtildiği üzere ödeme yapılmıştır.",
        ]);
        let m = of(&f, MISSING);
        assert_eq!(m.len(), 1);
        assert!(m[0].message.contains("Ek-3"));
    }

    #[test]
    fn a_document_without_an_attachment_list_produces_no_attachment_findings() {
        let f = lint(&["Ek-3'te belirtildiği üzere ödeme yapılmıştır ve dosyaya sunulmuştur."]);
        assert!(of(&f, MISSING).is_empty());
        assert!(of(&f, GAP).is_empty());
    }

    #[test]
    fn an_attachment_never_mentioned_in_the_body_is_review_only() {
        let f = lint(&[
            "Ek-1 Vekaletname örneği",
            "Ek-2 Sözleşme örneği",
            "Ek-1'de sunulan vekaletname uyarınca işlem yapılmıştır.",
        ]);
        let u = of(&f, UNUSED);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].severity, crate::finding::Severity::Review);
    }

    #[test]
    fn a_sentence_naming_two_attachments_defines_neither() {
        // `Ek-1 ve Ek-2 dosyaya sunulmuştur.` starts with an attachment number
        // but is a sentence, so it must not be read as the attachment list.
        let f = lint(&["Ek-1 ve Ek-2 dosyaya sunulmuş bulunmaktadır."]);
        assert!(of(&f, MISSING).is_empty(), "{:?}", of(&f, MISSING));
        assert!(of(&f, UNUSED).is_empty(), "{:?}", of(&f, UNUSED));
    }

    #[test]
    fn the_word_ek_as_an_ordinary_noun_is_not_an_attachment() {
        assert_silent(
            &["Sözleşmeye ek olarak bir protokol imzalanmış ve taraflarca kabul edilmiştir."],
            MISSING,
        );
    }

    #[test]
    fn a_statute_article_reference_is_not_an_internal_reference() {
        assert_silent(
            &[
                "A. USULE İLİŞKİN İTİRAZLAR",
                "HMK 119. maddede sayılan hususlar dilekçede yer almaktadır.",
                "TBK 112. maddesi uyarınca tazminat talep edilmektedir.",
            ],
            BROKEN,
        );
    }

    #[test]
    fn an_anchored_reference_to_a_missing_section_is_reported() {
        let f = lint(&[
            "A. USULE İLİŞKİN İTİRAZLAR",
            "Bu başlık altındaki açıklamalar burada yer almaktadır.",
            "B. ESASA İLİŞKİN BEYANLAR",
            "Yukarıda 7. maddede açıklandığı üzere talep yerindedir.",
        ]);
        assert_eq!(of(&f, BROKEN).len(), 1);
    }

    #[test]
    fn an_anchored_reference_to_an_existing_section_is_silent() {
        assert_silent(
            &[
                "1. USULE İLİŞKİN İTİRAZLAR",
                "Bu başlık altındaki açıklamalar burada yer almaktadır.",
                "2. ESASA İLİŞKİN BEYANLAR",
                "Yukarıda 1. maddede açıklandığı üzere talep yerindedir.",
            ],
            BROKEN,
        );
    }
}
