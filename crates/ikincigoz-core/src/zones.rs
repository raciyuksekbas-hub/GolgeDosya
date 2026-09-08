//! Where in a document a rule is allowed to have an opinion.
//!
//! A petition is not uniform prose. Three regions behave differently and a rule
//! that ignores the difference is wrong most of the time it fires:
//!
//! * **Layout** — the label/value column at the head of every petition:
//!   `DAVACI\t\t\t:\tProf. Dr. …`. The tabs are the alignment. Reporting them as
//!   stray whitespace, and worse replacing them with spaces, destroys the
//!   column.
//! * **Verbatim** — text quoted from a judgment. It is reproduced, not written,
//!   so "improving" its punctuation falsifies the quotation.
//! * **Prose** — everything else, where the punctuation and spacing rules mean
//!   what they say.
//!
//! Detection is structural. No label name is hard-coded: `DAVACI` is recognised
//! because of the shape of the line, not because it is on a list.

use crate::cdm::Document;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Prose,
    Layout,
    Verbatim,
}

/// A tab whose job is alignment rather than spacing, and why we think so.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralTab {
    pub range: Range<usize>,
    pub reason: TabReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabReason {
    /// The line begins with it: indentation, or the continuation of a column.
    Indentation,
    /// It sits next to the colon that separates a label from its value.
    ColumnSeparator,
    /// It follows the item marker: `1.\tMetin`, `A.\tBAŞLIK`, `7/b.\tMetin`.
    AfterMarker,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlockZones {
    /// The block is part of the label/value column.
    pub layout: bool,
    pub structural_tabs: Vec<StructuralTab>,
    /// Char ranges reproduced from another text.
    pub verbatim: Vec<Range<usize>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZoneMap {
    blocks: Vec<BlockZones>,
}

/// How far into a line an outline label may reach — `7/b.` is the longest shape
/// seen in practice, and a longer prefix without a space is not a label.
const MAX_MARKER_PREFIX: usize = 8;
/// A label is short. Beyond this it is a sentence that happens to contain a tab.
const MAX_LABEL_CHARS: usize = 48;
const MAX_LABEL_WORDS: usize = 6;

impl ZoneMap {
    pub fn build(document: &Document) -> Self {
        let mut blocks: Vec<BlockZones> = document
            .blocks
            .iter()
            .map(|b| {
                let structural_tabs = structural_tabs(b);
                BlockZones {
                    layout: structural_tabs
                        .iter()
                        .any(|t| t.reason != TabReason::AfterMarker),
                    structural_tabs,
                    verbatim: Vec::new(),
                }
            })
            .collect();

        mark_verbatim(document, &mut blocks);
        ZoneMap { blocks }
    }

    pub fn is_layout_block(&self, block_index: usize) -> bool {
        self.blocks
            .get(block_index)
            .map(|b| b.layout)
            .unwrap_or(false)
    }

    pub fn is_structural_tab(&self, block_index: usize, char_offset: usize) -> bool {
        self.blocks
            .get(block_index)
            .map(|b| {
                b.structural_tabs
                    .iter()
                    .any(|t| t.range.contains(&char_offset))
            })
            .unwrap_or(false)
    }

    pub fn is_verbatim(&self, block_index: usize, char_offset: usize) -> bool {
        self.blocks
            .get(block_index)
            .map(|b| b.verbatim.iter().any(|r| r.contains(&char_offset)))
            .unwrap_or(false)
    }

    /// True when any part of `[start, end)` is quoted.
    pub fn range_is_verbatim(&self, block_index: usize, start: usize, end: usize) -> bool {
        self.blocks
            .get(block_index)
            .map(|b| {
                b.verbatim
                    .iter()
                    .any(|r| r.start < end.max(start + 1) && start < r.end)
            })
            .unwrap_or(false)
    }

    pub fn zone_at(&self, block_index: usize, char_offset: usize) -> Zone {
        if self.is_verbatim(block_index, char_offset) {
            Zone::Verbatim
        } else if self.is_layout_block(block_index) {
            Zone::Layout
        } else {
            Zone::Prose
        }
    }

    pub fn block(&self, block_index: usize) -> Option<&BlockZones> {
        self.blocks.get(block_index)
    }
}

/// Classify every run of tabs in a block.
fn structural_tabs(block: &crate::cdm::Block) -> Vec<StructuralTab> {
    let chars: Vec<char> = block.text.chars().collect();
    let marker_end = block.numbering.as_ref().map(|n| n.text_start).unwrap_or(0);
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] != '\t' {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i] == '\t' {
            i += 1;
        }
        let range = start..i;

        // Only whitespace before it: this is indentation, or the continuation
        // line of a column whose label sat on an earlier row.
        if chars[..start].iter().all(|c| c.is_whitespace()) {
            out.push(StructuralTab {
                range,
                reason: TabReason::Indentation,
            });
            continue;
        }
        // Next to the colon that separates a label from its value. Either side
        // counts: `DAVACI\t\t\t:` and `KONU:\tdeğer` are the same column.
        if touches_label_colon(&chars, start, i) {
            out.push(StructuralTab {
                range,
                reason: TabReason::ColumnSeparator,
            });
            continue;
        }
        // Immediately after the item marker.
        if start == marker_end && marker_end > 0 {
            out.push(StructuralTab {
                range,
                reason: TabReason::AfterMarker,
            });
            continue;
        }
        // An outline label the numbering parser declined, such as `7/b.`: a
        // short run of non-space characters at the head of the line.
        if start <= MAX_MARKER_PREFIX && !chars[..start].iter().any(|c| c.is_whitespace()) {
            out.push(StructuralTab {
                range,
                reason: TabReason::AfterMarker,
            });
            continue;
        }
    }
    out
}

/// Does the tab run at `[start, end)` sit against the colon of a label row?
fn touches_label_colon(chars: &[char], start: usize, end: usize) -> bool {
    // Look right, past any further spacing, for the colon.
    let mut r = end;
    while r < chars.len() && (chars[r] == ' ' || chars[r] == '\t') {
        r += 1;
    }
    if chars.get(r) == Some(&':') && is_label(&chars[..start]) {
        return true;
    }
    // Look left: `KONU:\tdeğer`, and the trailing tab of `DAVACI\t\t\t:\t`.
    let mut l = start;
    while l > 0 && (chars[l - 1] == ' ' || chars[l - 1] == '\t') {
        l -= 1;
    }
    if l > 0 && chars[l - 1] == ':' && is_label(&chars[..l - 1]) {
        return true;
    }
    false
}

/// Is this prefix a field label rather than a sentence?
///
/// Short, no sentence punctuation, and either upper case throughout or a
/// handful of words. That is the shape of `DAVACI`, `HUKUKİ NEDENLER`,
/// `DAVA DEĞERİ` and `SONUÇ VE İSTEM` without naming any of them.
fn is_label(prefix: &[char]) -> bool {
    let text: String = prefix.iter().collect();
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_LABEL_CHARS {
        return false;
    }
    if trimmed.split_whitespace().count() > MAX_LABEL_WORDS {
        return false;
    }
    if trimmed.contains(['.', ',', ';', '!', '?']) {
        return false;
    }
    // Upper case is the strongest signal, and the one real petitions use.
    let upper = crate::text::tr_upper(trimmed);
    upper == trimmed && trimmed.chars().any(|c| c.is_alphabetic())
}

/// Mark quoted stretches, which may open in one paragraph and close in another.
///
/// Both quote conventions appear in Turkish legal writing and they are tracked
/// separately: an apostrophe-heavy language makes the straight single quote
/// useless as a delimiter, so only `"` and the typographic pair are used.
fn mark_verbatim(document: &Document, blocks: &mut [BlockZones]) {
    #[derive(Clone, Copy)]
    struct Open {
        block: usize,
        offset: usize,
    }
    let mut open_typographic: Option<Open> = None;
    let mut open_straight: Option<Open> = None;

    let close = |open: Open, block: usize, offset: usize, blocks: &mut [BlockZones]| {
        if open.block == block {
            blocks[block].verbatim.push(open.offset..offset + 1);
            return;
        }
        // The quotation spans paragraphs: mark the tail of the opening block,
        // every block in between, and the head of the closing one.
        let opening_len = document.blocks[open.block].char_len();
        blocks[open.block].verbatim.push(open.offset..opening_len);
        for (middle, zones) in blocks
            .iter_mut()
            .enumerate()
            .take(block)
            .skip(open.block + 1)
        {
            zones.verbatim.push(0..document.blocks[middle].char_len());
        }
        blocks[block].verbatim.push(0..offset + 1);
    };

    for (bi, b) in document.blocks.iter().enumerate() {
        for (i, c) in b.text.chars().enumerate() {
            match c {
                '\u{201C}' | '\u{00AB}' => {
                    if open_typographic.is_none() {
                        open_typographic = Some(Open {
                            block: bi,
                            offset: i,
                        });
                    }
                }
                '\u{201D}' | '\u{00BB}' => {
                    if let Some(open) = open_typographic.take() {
                        close(open, bi, i, blocks);
                    }
                }
                '"' => match open_straight.take() {
                    Some(open) => close(open, bi, i, blocks),
                    None => {
                        open_straight = Some(Open {
                            block: bi,
                            offset: i,
                        })
                    }
                },
                _ => {}
            }
        }
    }
    // An unterminated quotation is reported by TYPO_UNCLOSED_PAIR. It is not
    // treated as verbatim, because the region has no end and marking the rest
    // of the document as quoted would silence every later rule.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdm::{Block, BlockKind, ParagraphStyle, Run, SourceFormat, SourceLocation};
    use crate::parser::finalize_blocks;

    fn doc(lines: &[&str]) -> Document {
        let blocks: Vec<Block> = lines
            .iter()
            .map(|t| Block {
                id: String::new(),
                kind: BlockKind::Paragraph,
                text: (*t).to_string(),
                normalized_text: String::new(),
                runs: vec![Run {
                    text: (*t).to_string(),
                    ..Default::default()
                }],
                style: ParagraphStyle::default(),
                numbering: None,
                hierarchy_level: None,
                source_location: SourceLocation::default(),
            })
            .collect();
        Document::new(SourceFormat::Udf, finalize_blocks(blocks))
    }

    fn zones(lines: &[&str]) -> (Document, ZoneMap) {
        let d = doc(lines);
        let z = ZoneMap::build(&d);
        (d, z)
    }

    #[test]
    fn a_label_value_row_is_layout_and_its_tabs_are_structural() {
        let (_, z) = zones(&["DAVACI\t\t\t:\tProf. Dr. Nurşen MAZICI"]);
        assert!(z.is_layout_block(0));
        // Every tab in the row, including the one after the colon.
        for offset in [6, 7, 8, 10] {
            assert!(
                z.is_structural_tab(0, offset),
                "tab at {offset} was not structural"
            );
        }
    }

    #[test]
    fn every_field_label_shape_in_a_petition_is_recognised() {
        for line in [
            "DAVACI\t\t\t:\tdeğer",
            "VEKİLİ\t\t\t: \tdeğer",
            "KONU\t\t\t:\tdeğer",
            "DAVA DEĞERİ\t:\tdeğer",
            "AÇIKLAMALAR \t:",
            "HUKUKİ NEDENLER\t\t\t\t: TBK, TMK",
            "SONUÇ VE İSTEM\t\t\t\t\t: Yukarıda açıklanan",
        ] {
            let (_, z) = zones(&[line]);
            assert!(z.is_layout_block(0), "not layout: {line:?}");
        }
    }

    #[test]
    fn a_continuation_line_that_starts_with_tabs_is_layout() {
        let (_, z) = zones(&["\t\t\t\t\tZekeriyaköy Mah. Şair Nazım Sk."]);
        assert!(z.is_layout_block(0));
        assert!(z.is_structural_tab(0, 0));
    }

    #[test]
    fn a_tab_after_the_item_marker_is_structural_but_not_layout() {
        let (_, z) = zones(&["1.\tMüvekkil Prof. Nurşen Mazıcı beyanda bulunmuştur."]);
        assert!(z.is_structural_tab(0, 2));
        // The paragraph itself is prose: only its marker separator is special.
        assert!(!z.is_layout_block(0));
    }

    #[test]
    fn an_outline_label_the_numbering_parser_declines_still_protects_its_tab() {
        // `7/b.` is not a marker shape the numbering parser accepts, but the
        // tab after it is plainly structural.
        let (_, z) = zones(&["7/b.\tTemizliği ve Düzeni Kolaydır: 1+1 planındaki daire"]);
        assert!(z.is_structural_tab(0, 4));
    }

    #[test]
    fn a_tab_in_the_middle_of_a_sentence_is_not_structural() {
        let (_, z) = zones(&["Davacı bu nedenle\tmahkemeye başvurmuştur."]);
        assert!(!z.is_layout_block(0));
        assert!(!z.is_structural_tab(0, 17));
    }

    #[test]
    fn a_long_sentence_before_a_colon_is_not_a_label() {
        let line = "Mahkemenizce gerek görülmesi halinde yapılacak keşif ve bilirkişi incelemesi\t: yapılacaktır";
        let (_, z) = zones(&[line]);
        assert!(!z.is_layout_block(0));
    }

    #[test]
    fn a_quotation_inside_one_paragraph_is_verbatim() {
        let line = "Yargıtay, \"davacı, oğlu Y.. Ö..'ın konut ihtiyacı nedeniyle dava açmıştır.\" şeklinde karar vermiştir.";
        let (_, z) = zones(&[line]);
        let quote_start = line.chars().position(|c| c == '"').unwrap();
        assert!(z.is_verbatim(0, quote_start + 5));
        assert!(!z.is_verbatim(0, 2), "the lead-in is not quoted");
        assert!(
            !z.is_verbatim(0, line.chars().count() - 3),
            "the tail is not quoted"
        );
    }

    #[test]
    fn a_quotation_spanning_paragraphs_covers_the_paragraphs_between() {
        let (_, z) = zones(&[
            "Yargıtay şöyle demiştir: \u{201C}Taraflar arasında",
            "uyuşmazlık bulunmamaktadır ve dava süresindedir",
            "bu nedenle hüküm kurulmuştur.\u{201D} Bu görüşe katılıyoruz.",
        ]);
        assert!(z.is_verbatim(0, 30));
        assert!(
            z.is_verbatim(1, 5),
            "the middle paragraph is inside the quotation"
        );
        assert!(z.is_verbatim(2, 5));
        assert!(
            !z.is_verbatim(2, 40),
            "the comment after the quotation is not quoted"
        );
    }

    #[test]
    fn an_unterminated_quotation_does_not_silence_the_rest_of_the_document() {
        let (_, z) = zones(&[
            "Yargıtay şöyle demiştir: \u{201C}Taraflar arasında uyuşmazlık yoktur",
            "Bu paragraf alıntı değildir ve denetlenmelidir.",
        ]);
        assert!(!z.is_verbatim(1, 5));
    }

    #[test]
    fn ordinary_prose_is_prose() {
        let (_, z) = zones(&["Davacı, davalıya karşı alacak davası açmıştır."]);
        assert_eq!(z.zone_at(0, 5), Zone::Prose);
    }
}
