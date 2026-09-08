//! Numbering and hierarchy rules.
//!
//! The hard part is not spotting `A B D`; it is deciding what counts as a
//! sequence. Documents nest numbering freely and reuse the same marker shape at
//! several depths, so the rules maintain an outline stack: a marker shape's
//! depth is the order in which the shape first appears, and seeing a shallower
//! marker closes every deeper sequence. Without that, `A. 1. 2. B. 1.` reports
//! a phantom backtrack at the second `1.`.

use super::Context;
use crate::cdm::{Block, BlockKind, Decoration, NumberingScheme, SourceLocation};
use crate::finding::{Finding, FindingBuilder};
use crate::numbering::{parse_marker, value_letter, value_roman, Alphabet, Candidate, Marker};
use std::collections::HashMap;

fn builder(id: &str) -> FindingBuilder {
    FindingBuilder::new(crate::finding::rule_info(id).expect("registered rule"))
}

/// The shape of a marker, independent of its value. Two markers belong to the
/// same sequence only when their shapes match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Shape {
    decoration: Decoration,
    class: Class,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Class {
    Digit,
    Upper,
    Lower,
}

fn shape_of(m: &Marker) -> Shape {
    let class = if m.core.chars().all(|c| c.is_ascii_digit()) {
        Class::Digit
    } else if crate::text::tr_upper(&m.core) == m.core {
        Class::Upper
    } else {
        Class::Lower
    };
    Shape {
        decoration: m.decoration,
        class,
    }
}

/// One marker found in the document, with the block it came from.
struct Entry<'a> {
    block: &'a Block,
    marker: Marker,
    shape: Shape,
}

/// How a whole shape group is being read.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Reading {
    scheme: NumberingScheme,
    alphabet: Option<Alphabet>,
}

pub fn run(ctx: &Context<'_>) -> Vec<Finding> {
    let entries = collect(ctx);
    if entries.is_empty() {
        return Vec::new();
    }
    let readings = choose_readings(&entries);
    let mut out = sequence_findings(ctx, &entries, &readings);
    out.extend(orphan_findings(&entries));
    out.extend(empty_sections(ctx));
    out
}

fn collect<'a>(ctx: &'a Context<'a>) -> Vec<Entry<'a>> {
    ctx.document
        .blocks
        .iter()
        .filter(|b| b.kind != BlockKind::TableCell)
        .filter_map(|b| {
            let marker = parse_marker(&b.text)?;
            let shape = shape_of(&marker);
            Some(Entry {
                block: b,
                marker,
                shape,
            })
        })
        .collect()
}

/// Decide, per shape group, which alphabet and which scheme to read it under.
///
/// Reading `A B C D` as Turkish would invent a missing `Ç`, and reading
/// `A B C Ç D` as Latin would leave `Ç` unreadable. The rule that settles it:
/// a group is Turkish only when it actually uses a Turkish-only letter.
fn choose_readings(entries: &[Entry<'_>]) -> HashMap<Shape, Reading> {
    const TURKISH_ONLY: [char; 6] = ['Ç', 'Ğ', 'İ', 'Ö', 'Ş', 'Ü'];
    let mut by_shape: HashMap<Shape, Vec<&Entry<'_>>> = HashMap::new();
    for e in entries {
        by_shape.entry(e.shape).or_default().push(e);
    }

    let mut out = HashMap::new();
    for (shape, group) in by_shape {
        if shape.class == Class::Digit {
            out.insert(
                shape,
                Reading {
                    scheme: NumberingScheme::Decimal,
                    alphabet: None,
                },
            );
            continue;
        }
        let upper = shape.class == Class::Upper;
        // A multi-letter member can only be a Roman numeral, and a group does
        // not mix conventions, so the whole group is Roman.
        let any_multi = group.iter().any(|e| e.marker.core.chars().count() > 1);
        if any_multi {
            out.insert(
                shape,
                Reading {
                    scheme: if upper {
                        NumberingScheme::UpperRoman
                    } else {
                        NumberingScheme::LowerRoman
                    },
                    alphabet: None,
                },
            );
            continue;
        }
        let uses_turkish_only = group.iter().any(|e| {
            let c = crate::text::tr_upper(&e.marker.core)
                .chars()
                .next()
                .unwrap_or(' ');
            TURKISH_ONLY.contains(&c)
        });
        out.insert(
            shape,
            Reading {
                scheme: if upper {
                    NumberingScheme::UpperLatin
                } else {
                    NumberingScheme::LowerLatin
                },
                alphabet: Some(if uses_turkish_only {
                    Alphabet::Turkish
                } else {
                    Alphabet::Latin
                }),
            },
        );
    }
    out
}

fn value_of(e: &Entry<'_>, reading: Reading) -> Option<Candidate> {
    e.marker.candidate_for(reading.scheme, reading.alphabet)
}

/// Render the value that should have appeared, for the message.
fn render_expected(value: u32, reading: Reading, decoration: Decoration) -> String {
    let core = match reading.scheme {
        NumberingScheme::Decimal => value.to_string(),
        NumberingScheme::UpperLatin | NumberingScheme::LowerLatin => {
            let upper = reading.scheme == NumberingScheme::UpperLatin;
            match value_letter(value, reading.alphabet.unwrap_or(Alphabet::Latin), upper) {
                Some(c) => c.to_string(),
                None => return format!("{value}."),
            }
        }
        NumberingScheme::UpperRoman | NumberingScheme::LowerRoman => {
            match value_roman(value, reading.scheme == NumberingScheme::UpperRoman) {
                Some(s) => s,
                None => return format!("{value}."),
            }
        }
    };
    decorate(&core, decoration)
}

fn decorate(core: &str, d: Decoration) -> String {
    match d {
        Decoration::Dot => format!("{core}."),
        Decoration::Paren => format!("{core})"),
        Decoration::Bracketed => format!("({core})"),
        Decoration::Dash => format!("{core} -"),
    }
}

/// Does this block end the section a numbered list belongs to?
///
/// A petition's closing list starts again at `1.` under `SONUÇ VE İSTEM`, which
/// is a label row rather than a numbered heading. Without treating that as a
/// boundary the restart reads as a backtrack from `11.`, which is how a
/// correctly drafted petition acquired a phantom error.
fn starts_a_new_section(ctx: &Context<'_>, block: &Block) -> bool {
    let bi = block.source_location.block_index;
    if ctx.zones().is_layout_block(bi) {
        return true;
    }
    block.kind == BlockKind::Heading && block.numbering.is_none() && !block.is_empty()
}

fn sequence_findings(
    ctx: &Context<'_>,
    entries: &[Entry<'_>],
    readings: &HashMap<Shape, Reading>,
) -> Vec<Finding> {
    let gap = builder("SEC_SEQ_GAP");
    let dup = builder("SEC_SEQ_DUPLICATE");
    let back = builder("SEC_SEQ_BACKTRACK");
    let mut out = Vec::new();

    // Depth of a shape = order of first appearance. This is what makes nesting
    // work without the document having to declare its own outline scheme.
    let mut depth_of: HashMap<Shape, usize> = HashMap::new();
    for e in entries {
        let next = depth_of.len();
        depth_of.entry(e.shape).or_insert(next);
    }

    // Last value seen at each depth. Seeing depth d resets everything deeper,
    // because a new parent starts its children over.
    let mut last: HashMap<usize, u32> = HashMap::new();
    // Walk the document, not just the markers, so a section boundary lying
    // between two markers can close the list that was open across it.
    let mut cursor = 0usize;

    for e in entries {
        let here = e.block.source_location.block_index;
        let scanned = cursor.min(ctx.document.blocks.len())..here.min(ctx.document.blocks.len());
        if ctx.document.blocks[scanned]
            .iter()
            .any(|b| starts_a_new_section(ctx, b))
        {
            last.clear();
        }
        cursor = here;

        let Some(&reading) = readings.get(&e.shape) else {
            continue;
        };
        let Some(candidate) = value_of(e, reading) else {
            continue;
        };
        let depth = depth_of[&e.shape];
        last.retain(|&d, _| d <= depth);

        let previous = last.insert(depth, candidate.value);
        let Some(previous) = previous else { continue };

        let loc = SourceLocation::span(
            e.block.source_location.block_index,
            0,
            e.marker.raw.chars().count(),
        );
        let seen = e.marker.raw.clone();

        if candidate.value == previous {
            let mut f = dup.at(
                &e.block.id,
                loc,
                0.95,
                format!("{seen} sıra numarası bir önceki maddede de kullanılmış."),
                "Aynı düzeydeki iki madde aynı sıra numarasını taşıyor.",
            );
            f.context = Some(crate::text::excerpt(&e.block.text, 0, 50));
            out.push(f);
        } else if candidate.value < previous {
            let mut f = back.at(
                &e.block.id,
                loc,
                0.93,
                format!(
                    "{} sonrasında {} bekleniyordu; {} bulundu.",
                    render_expected(previous, reading, e.shape.decoration),
                    render_expected(previous + 1, reading, e.shape.decoration),
                    seen
                ),
                "Sıra numaraları artan düzende ilerlemiyor; numaralandırma geriye dönüyor.",
            );
            f.context = Some(crate::text::excerpt(&e.block.text, 0, 50));
            out.push(f);
        } else if candidate.value > previous + 1 {
            let missing: Vec<String> = ((previous + 1)..candidate.value)
                .take(8)
                .map(|v| render_expected(v, reading, e.shape.decoration))
                .collect();
            let mut f = gap.at(
                &e.block.id,
                loc,
                0.95,
                format!(
                    "{} başlığından sonra {} bekleniyordu; {} bulundu.",
                    render_expected(previous, reading, e.shape.decoration),
                    missing.join(", "),
                    seen
                ),
                format!(
                    "Aynı düzeydeki maddeler {} ile sıralanıyor ve arada atlanan sıra var.",
                    reading
                        .alphabet
                        .map(|a| a.label())
                        .unwrap_or(reading.scheme.label())
                ),
            );
            f.context = Some(crate::text::excerpt(&e.block.text, 0, 50));
            out.push(f);
        }
    }
    out
}

/// A deeper marker appearing before the first marker of its parent level.
///
/// Sequence grouping derives depth from the order shapes first appear, which is
/// exactly right for detecting gaps but makes this question unanswerable: under
/// that scheme the first marker in the document is always the shallowest, so
/// nothing can ever be an orphan.
///
/// Orphan detection therefore uses the conventional Turkish outline ladder
/// instead, and only fires when the parent level is genuinely present somewhere
/// in the document. A document that numbers with `1.` and nothing else has no
/// parent level to be missing, and stays silent.
fn ladder_rank(shape: Shape) -> Option<u8> {
    // Roman, then capitals, then digits, then lower-case letters — the order
    // Turkish petitions almost always nest in.
    let class_rank = match shape.class {
        Class::Upper => 0u8,
        Class::Digit => 2,
        Class::Lower => 3,
    };
    // Within one class, `1.` encloses `1)` encloses `(1)`.
    let decoration_rank = match shape.decoration {
        Decoration::Dot => 0u8,
        Decoration::Dash => 1,
        Decoration::Paren => 2,
        Decoration::Bracketed => 3,
    };
    Some(class_rank * 4 + decoration_rank)
}

fn orphan_findings(entries: &[Entry<'_>]) -> Vec<Finding> {
    let b = builder("SEC_HIERARCHY_ORPHAN");
    let mut out = Vec::new();

    // Rank every shape present, then compress to consecutive levels so that a
    // document using only `A.` and `a)` has a parent/child pair rather than a
    // gap in the ladder.
    let mut ranks: Vec<u8> = entries
        .iter()
        .filter_map(|e| ladder_rank(e.shape))
        .collect();
    ranks.sort_unstable();
    ranks.dedup();
    if ranks.len() < 2 {
        return out;
    }
    let level_of = |shape: Shape| -> Option<usize> {
        ladder_rank(shape).and_then(|r| ranks.iter().position(|x| *x == r))
    };

    let mut seen_level: Vec<bool> = vec![false; ranks.len()];
    for e in entries {
        let Some(level) = level_of(e.shape) else {
            continue;
        };
        if level > 0 && !seen_level[level - 1] {
            let mut f = b.at(
                &e.block.id,
                SourceLocation::span(
                    e.block.source_location.block_index,
                    0,
                    e.marker.raw.chars().count(),
                ),
                0.7,
                format!(
                    "{} maddesi, kendisini kapsayan bir üst başlık açılmadan başlıyor.",
                    e.marker.raw
                ),
                "Belgede bu düzeydeki maddeler başka bir düzeyin altında yer alıyor; \
                 ancak burada üst düzey henüz açılmamış.",
            );
            f.context = Some(crate::text::excerpt(&e.block.text, 0, 50));
            out.push(f);
        }
        seen_level[level] = true;
    }
    out
}

/// A heading with nothing underneath it before the next heading of the same or
/// a shallower level.
fn empty_sections(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("SEC_EMPTY_SECTION");
    let blocks = &ctx.document.blocks;
    let mut out = Vec::new();

    for (i, block) in blocks.iter().enumerate() {
        if block.kind != BlockKind::Heading {
            continue;
        }
        let level = block.hierarchy_level.unwrap_or(0);
        let mut has_content = false;
        for next in blocks.iter().skip(i + 1) {
            if next.kind == BlockKind::Heading && next.hierarchy_level.unwrap_or(0) <= level {
                break;
            }
            if !next.is_empty() {
                has_content = true;
                break;
            }
        }
        if has_content {
            continue;
        }
        let mut f = b.at(
            &block.id,
            SourceLocation::block(block.source_location.block_index),
            0.8,
            format!(
                "\u{201C}{}\u{201D} başlığının altında metin yok.",
                block.body_text().trim()
            ),
            "Başlık açılmış; ancak bir sonraki başlığa kadar hiç içerik yazılmamış.",
        );
        f.context = Some(crate::text::excerpt(&block.text, 0, 50));
        out.push(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::rules::testing::*;

    const GAP: &str = "SEC_SEQ_GAP";
    const DUP: &str = "SEC_SEQ_DUPLICATE";
    const BACK: &str = "SEC_SEQ_BACKTRACK";
    const ORPHAN: &str = "SEC_HIERARCHY_ORPHAN";
    const EMPTY: &str = "SEC_EMPTY_SECTION";

    /// Body text for a section. Deliberately does not begin with anything that
    /// could itself be read as a numbering marker.
    fn body(n: &str) -> String {
        format!("Bu bölümde ({n} altında) yer alan açıklamalar burada bulunmaktadır.")
    }

    /// Interleave headings with a body paragraph so nothing is an empty section.
    fn outline(markers: &[&str]) -> Vec<String> {
        let mut v = Vec::new();
        for m in markers {
            v.push(format!("{m} Başlık"));
            v.push(body(m));
        }
        v
    }

    fn lint_outline(markers: &[&str]) -> Vec<crate::finding::Finding> {
        let owned = outline(markers);
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        lint(&refs)
    }

    #[test]
    fn a_clean_letter_outline_is_silent() {
        let f = lint_outline(&["A.", "B.", "C.", "D."]);
        assert!(of(&f, GAP).is_empty(), "{:?}", of(&f, GAP));
        assert!(of(&f, BACK).is_empty());
        assert!(of(&f, DUP).is_empty());
    }

    #[test]
    fn a_gap_in_a_letter_outline_is_reported() {
        let f = lint_outline(&["A.", "B.", "D."]);
        let g = of(&f, GAP);
        assert_eq!(g.len(), 1);
        assert!(
            g[0].message.contains("C."),
            "message was {:?}",
            g[0].message
        );
    }

    #[test]
    fn a_turkish_letter_outline_expects_c_cedilla_in_its_place() {
        // The group uses Ş, a Turkish-only letter, so it is read as Turkish and
        // the missing item between C and D is Ç, not nothing.
        let f = lint_outline(&["A.", "B.", "C.", "D.", "Ş."]);
        let g = of(&f, GAP);
        assert!(!g.is_empty());
        assert!(
            g[0].message.contains("Ç."),
            "message was {:?}",
            g[0].message
        );
    }

    #[test]
    fn a_latin_outline_that_skips_c_cedilla_is_not_a_gap() {
        // The most common convention. Reading this as Turkish would invent Ç.
        assert_silent(
            &outline(&["A.", "B.", "C.", "D.", "E."])
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            GAP,
        );
    }

    #[test]
    fn a_duplicate_number_is_reported() {
        let f = lint_outline(&["A.", "B.", "B.", "C."]);
        assert_eq!(of(&f, DUP).len(), 1);
    }

    #[test]
    fn a_backtrack_is_reported() {
        let f = lint_outline(&["1.", "2.", "3.", "5.", "4."]);
        assert_eq!(of(&f, BACK).len(), 1);
        assert_eq!(of(&f, GAP).len(), 1);
    }

    #[test]
    fn nested_numbering_restarts_without_a_false_backtrack() {
        // `1.` under A, then `1.` again under B. A flat reading would call the
        // second `1.` a backtrack.
        let f = lint_outline(&["A.", "1.", "2.", "B.", "1.", "2."]);
        assert!(of(&f, BACK).is_empty(), "{:?}", of(&f, BACK));
        assert!(of(&f, DUP).is_empty(), "{:?}", of(&f, DUP));
        assert!(of(&f, GAP).is_empty(), "{:?}", of(&f, GAP));
    }

    #[test]
    fn different_decorations_are_different_sequences() {
        // `1)` items do not continue the `1.` items.
        let f = lint_outline(&["1.", "2.", "1)", "2)"]);
        assert!(of(&f, BACK).is_empty(), "{:?}", of(&f, BACK));
    }

    #[test]
    fn roman_numerals_are_read_as_roman_when_the_group_has_a_multi_letter_member() {
        let f = lint_outline(&["I.", "II.", "III.", "V."]);
        let g = of(&f, GAP);
        assert_eq!(g.len(), 1);
        assert!(
            g[0].message.contains("IV."),
            "message was {:?}",
            g[0].message
        );
    }

    #[test]
    fn lowercase_letters_form_their_own_sequence() {
        let f = lint_outline(&["a.", "b.", "d."]);
        assert_eq!(of(&f, GAP).len(), 1);
    }

    #[test]
    fn a_sub_item_before_its_parent_level_is_an_orphan() {
        // A `1.` item appears before the first `A.` heading, even though the
        // document does use `A.` headings as the enclosing level.
        let owned = outline(&["A.", "1.", "2.", "B."]);
        let mut refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        refs.insert(0, "1. Üst başlığı olmayan madde");
        let f = lint(&refs);
        assert!(!of(&f, ORPHAN).is_empty(), "expected an orphan finding");
    }

    #[test]
    fn a_document_with_only_one_numbering_level_has_no_orphans() {
        let f = lint_outline(&["1.", "2.", "3."]);
        assert!(of(&f, ORPHAN).is_empty(), "{:?}", of(&f, ORPHAN));
    }

    #[test]
    fn a_properly_nested_outline_has_no_orphans() {
        let f = lint_outline(&["A.", "1.", "2.", "B.", "1."]);
        assert!(of(&f, ORPHAN).is_empty(), "{:?}", of(&f, ORPHAN));
    }

    #[test]
    fn a_heading_with_no_content_is_reported() {
        let f = lint(&[
            "A. BİRİNCİ BAŞLIK",
            "B. İKİNCİ BAŞLIK",
            "İkinci başlığın altındaki açıklamalar burada yer alır.",
        ]);
        let e = of(&f, EMPTY);
        assert_eq!(e.len(), 1);
        assert!(e[0].message.contains("BİRİNCİ BAŞLIK"));
    }

    #[test]
    fn a_heading_with_content_is_not_an_empty_section() {
        assert_silent(
            &[
                "A. BİRİNCİ BAŞLIK",
                "Başlığın altındaki açıklamalar burada yer alır.",
                "B. İKİNCİ BAŞLIK",
                "İçerik.",
            ],
            EMPTY,
        );
    }

    #[test]
    fn a_list_restarting_after_a_section_label_is_not_a_backtrack() {
        // The closing list of a petition starts again at 1. under a label row.
        let f = lint(&[
            "10. Onuncu maddeye ilişkin açıklamalar burada yer almaktadır.",
            "11. On birinci maddeye ilişkin açıklamalar burada yer almaktadır.",
            "SONUÇ VE İSTEM\t\t\t: Yukarıda açıklanan nedenlerle",
            "1. Davamızın kabulü ile,",
            "2. Yargılama giderlerinin davalıya yükletilmesine,",
        ]);
        assert!(of(&f, BACK).is_empty(), "{:?}", of(&f, BACK));
        assert!(of(&f, DUP).is_empty(), "{:?}", of(&f, DUP));
    }

    #[test]
    fn a_restart_without_any_section_boundary_is_still_a_backtrack() {
        let f = lint(&[
            "10. Onuncu maddeye ilişkin açıklamalar burada yer almaktadır.",
            "11. On birinci maddeye ilişkin açıklamalar burada yer almaktadır.",
            "1. Yeniden başlayan madde burada yer almaktadır.",
        ]);
        assert_eq!(of(&f, BACK).len(), 1);
    }

    #[test]
    fn dates_and_amounts_in_prose_do_not_start_a_sequence() {
        assert_silent(
            &[
                "Sözleşme 2019 yılında imzalanmış ve 2021 yılında feshedilmiştir.",
                "Bedel 50.000 TL olarak kararlaştırılmıştır.",
            ],
            GAP,
        );
    }
}
