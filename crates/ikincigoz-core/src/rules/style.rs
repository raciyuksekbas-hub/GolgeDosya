//! Formatting outliers.
//!
//! The engine never asks "is this the right format?" — it has no opinion. It
//! asks "did this one paragraph break the pattern the document itself set?".
//! That question is answerable from the document alone, which is what makes
//! the answer trustworthy.
//!
//! Comparison is always between siblings: headings at one outline level are
//! compared with each other, body paragraphs with each other. A heading is
//! never compared with body text, because they are *supposed* to differ.

use super::Context;
use crate::cdm::{Alignment, Block, BlockKind, SourceLocation};
use crate::finding::{Finding, FindingBuilder};
use crate::profile::DocumentProfile;
use std::collections::HashMap;

fn builder(id: &str) -> FindingBuilder {
    FindingBuilder::new(crate::finding::rule_info(id).expect("registered rule"))
}

/// How many siblings a group needs before "the majority" means anything.
const MIN_GROUP: usize = 3;
/// Share of the group that must agree before a deviation is worth reporting.
///
/// Set so that three headings out of four constitute a house style: that is the
/// smallest group where a single odd one out is meaningful rather than a
/// coin toss.
const DOMINANCE: f32 = 0.7;
/// Font sizes within this many points are the same size for our purposes.
const SIZE_EPSILON: f32 = 0.26;

/// The formatting facts compared across siblings.
#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    font_family: Option<String>,
    font_size: Option<i32>,
    alignment: Option<Alignment>,
}

/// Summarise a block's formatting from its runs, weighted by text length.
///
/// Weighting matters: a heading with one stray bold space should not read as a
/// bold heading, and a paragraph with a bold case number in the middle is not
/// a bold paragraph.
fn snapshot(block: &Block) -> Snapshot {
    let runs: Vec<&crate::cdm::Run> = block
        .runs
        .iter()
        .filter(|r| !r.text.trim().is_empty())
        .collect();
    let weight = |pred: &dyn Fn(&crate::cdm::Run) -> bool| -> Option<bool> {
        if runs.is_empty() {
            return None;
        }
        let total: usize = runs.iter().map(|r| r.text.chars().count()).sum();
        if total == 0 {
            return None;
        }
        let hit: usize = runs
            .iter()
            .filter(|r| pred(r))
            .map(|r| r.text.chars().count())
            .sum();
        Some(hit * 2 > total)
    };

    let dominant_family = {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for r in &runs {
            if let Some(f) = r.font_family.as_deref() {
                *counts.entry(f).or_default() += r.text.chars().count();
            }
        }
        counts
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map(|(f, _)| f.to_string())
    };
    let dominant_size = {
        let mut counts: HashMap<i32, usize> = HashMap::new();
        for r in &runs {
            if let Some(s) = r.font_size {
                *counts.entry((s * 4.0).round() as i32).or_default() += r.text.chars().count();
            }
        }
        counts.into_iter().max_by_key(|(_, n)| *n).map(|(s, _)| s)
    };

    Snapshot {
        bold: weight(&|r| r.bold),
        italic: weight(&|r| r.italic),
        underline: weight(&|r| r.underline),
        font_family: dominant_family,
        font_size: dominant_size,
        alignment: block.style.alignment,
    }
}

/// One comparable attribute, so the group scan can be written once.
struct Attribute {
    label: &'static str,
    /// Extract the attribute as a comparable string, or `None` if unknown.
    get: fn(&Snapshot) -> Option<String>,
    /// How the value reads in a message, e.g. `kalın` for `true`.
    describe: fn(&str) -> String,
}

fn describe_bool_bold(v: &str) -> String {
    if v == "true" {
        "kalın".into()
    } else {
        "düz".into()
    }
}
fn describe_bool_italic(v: &str) -> String {
    if v == "true" {
        "eğik".into()
    } else {
        "düz".into()
    }
}
fn describe_bool_underline(v: &str) -> String {
    if v == "true" {
        "altı çizili".into()
    } else {
        "altı çizilmemiş".into()
    }
}
fn describe_plain(v: &str) -> String {
    v.to_string()
}
fn describe_size(v: &str) -> String {
    match v.parse::<i32>() {
        Ok(q) => format!("{} punto", q as f32 / 4.0),
        Err(_) => v.to_string(),
    }
}
fn describe_alignment(v: &str) -> String {
    match v {
        "Left" => "sola hizalı".into(),
        "Center" => "ortalanmış".into(),
        "Right" => "sağa hizalı".into(),
        "Justify" => "iki yana yaslı".into(),
        other => other.to_string(),
    }
}

fn attributes() -> Vec<Attribute> {
    vec![
        Attribute {
            label: "kalınlık",
            get: |s| s.bold.map(|v| v.to_string()),
            describe: describe_bool_bold,
        },
        Attribute {
            label: "eğiklik",
            get: |s| s.italic.map(|v| v.to_string()),
            describe: describe_bool_italic,
        },
        Attribute {
            label: "alt çizgi",
            get: |s| s.underline.map(|v| v.to_string()),
            describe: describe_bool_underline,
        },
        Attribute {
            label: "yazı tipi",
            get: |s| s.font_family.clone(),
            describe: describe_plain,
        },
        Attribute {
            label: "punto",
            get: |s| s.font_size.map(|v| v.to_string()),
            describe: describe_size,
        },
        Attribute {
            label: "hizalama",
            get: |s| s.alignment.map(|a| format!("{a:?}")),
            describe: describe_alignment,
        },
    ]
}

pub fn run(ctx: &Context<'_>) -> Vec<Finding> {
    let mut out = Vec::new();

    // Headings, grouped by outline level *and* by whether they are numbered.
    //
    // A petition opens with an unnumbered addressee line — centred, bold, and
    // structurally a heading — followed by numbered section headings that are
    // left-aligned. They are not siblings, and comparing them reports the
    // addressee line as an outlier in every well-formed document there is.
    let mut by_group: HashMap<(u8, bool), Vec<&Block>> = HashMap::new();
    for b in ctx
        .document
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Heading && !b.is_empty())
    {
        by_group
            .entry((b.hierarchy_level.unwrap_or(0), b.numbering.is_some()))
            .or_default()
            .push(b);
    }
    let mut keys: Vec<(u8, bool)> = by_group.keys().copied().collect();
    keys.sort();
    for key in keys {
        let group = &by_group[&key];
        out.extend(scan_group(
            group,
            "STYLE_HEADING_OUTLIER",
            &format!(
                "Aynı düzeydeki diğer {} başlık",
                group.len().saturating_sub(1)
            ),
        ));
    }

    // Body paragraphs.
    let body: Vec<&Block> = ctx
        .document
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Paragraph | BlockKind::ListItem) && !b.is_empty())
        // Very short paragraphs are captions, signatures and dates; their
        // formatting is legitimately its own thing.
        .filter(|b| b.text.split_whitespace().count() >= 8)
        .collect();
    out.extend(scan_group(
        &body,
        "STYLE_BODY_OUTLIER",
        "Belgedeki diğer paragraflar",
    ));

    if let Some(profile) = ctx.profile {
        out.extend(profile_deviations(&body, profile));
    }
    out
}

fn scan_group(group: &[&Block], rule_id: &str, peers: &str) -> Vec<Finding> {
    let b = builder(rule_id);
    let mut out = Vec::new();
    if group.len() < MIN_GROUP {
        return out;
    }
    let snaps: Vec<Snapshot> = group.iter().map(|bl| snapshot(bl)).collect();

    for attr in attributes() {
        let values: Vec<Option<String>> = snaps.iter().map(|s| (attr.get)(s)).collect();
        let known: Vec<&String> = values.iter().flatten().collect();
        if known.len() < MIN_GROUP {
            continue;
        }
        // The majority value, and how much of the group holds it.
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for v in &known {
            *counts.entry(v.as_str()).or_default() += 1;
        }
        let Some((&majority, &count)) = counts.iter().max_by_key(|(_, n)| **n) else {
            continue;
        };
        if (count as f32 / known.len() as f32) < DOMINANCE {
            continue; // no house style here; nothing to deviate from
        }
        // Exactly one deviation is a slip. Several is a second convention.
        let deviants: Vec<usize> = values
            .iter()
            .enumerate()
            .filter(|(_, v)| v.as_deref().map(|s| s != majority).unwrap_or(false))
            .map(|(i, _)| i)
            .collect();
        if deviants.is_empty() || deviants.len() > 1 {
            continue;
        }

        let i = deviants[0];
        let actual = values[i].as_deref().unwrap_or("");
        // Font sizes that differ only by rounding are not a deviation.
        if attr.label == "punto" {
            if let (Ok(a), Ok(m)) = (actual.parse::<f32>(), majority.parse::<f32>()) {
                if ((a - m) / 4.0).abs() < SIZE_EPSILON {
                    continue;
                }
            }
        }
        let block = group[i];
        let mut f = b.at(
            &block.id,
            SourceLocation::block(block.source_location.block_index),
            0.75,
            format!(
                "{} {} iken bu paragraf {}.",
                peers,
                (attr.describe)(majority),
                (attr.describe)(actual)
            ),
            format!(
                "Karşılaştırma yalnızca aynı türdeki öğeler arasında yapılır. \
                 Bu grupta {} öğenin {}'i aynı {} değerini taşıyor; yalnızca bu öğe ayrılıyor.",
                known.len(),
                count,
                attr.label
            ),
        );
        f.context = Some(crate::text::excerpt(&block.text, 0, 45));
        out.push(f);
    }
    out
}

fn profile_deviations(body: &[&Block], profile: &DocumentProfile) -> Vec<Finding> {
    let b = builder("STYLE_PROFILE_DEVIATION");
    let mut out = Vec::new();

    for block in body {
        let s = snapshot(block);
        let mut differences: Vec<String> = Vec::new();

        if let (Some(expected), Some(actual)) = (&profile.body.font_family, &s.font_family) {
            if expected != actual {
                differences.push(format!("yazı tipi {expected} yerine {actual}"));
            }
        }
        if let (Some(expected), Some(actual)) = (profile.body.font_size, s.font_size) {
            let actual = actual as f32 / 4.0;
            if (expected - actual).abs() > SIZE_EPSILON {
                differences.push(format!("punto {expected} yerine {actual}"));
            }
        }
        if let (Some(expected), Some(actual)) = (profile.body.alignment, s.alignment) {
            if expected != actual {
                differences.push(format!(
                    "hizalama {} yerine {}",
                    describe_alignment(&format!("{expected:?}")),
                    describe_alignment(&format!("{actual:?}"))
                ));
            }
        }
        if differences.is_empty() {
            continue;
        }
        let mut f = b.at(
            &block.id,
            SourceLocation::block(block.source_location.block_index),
            0.7,
            format!(
                "\u{201C}{}\u{201D} standardından sapma: {}.",
                profile.name,
                differences.join(", ")
            ),
            "Bu paragrafın biçimi, seçtiğiniz belge standardıyla uyuşmuyor. \
             Standart, sizin doğru kabul ettiğiniz bir belgeden türetilmiştir.",
        );
        f.context = Some(crate::text::excerpt(&block.text, 0, 45));
        out.push(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdm::{Document, ParagraphStyle, Run, SourceFormat};
    use crate::parser::finalize_blocks;
    use crate::rules::testing::{lint_doc, of};

    const HEADING: &str = "STYLE_HEADING_OUTLIER";
    const BODY: &str = "STYLE_BODY_OUTLIER";
    const PROFILE: &str = "STYLE_PROFILE_DEVIATION";

    fn heading(text: &str, bold: bool) -> Block {
        Block {
            id: String::new(),
            kind: BlockKind::Heading,
            text: text.into(),
            normalized_text: String::new(),
            runs: vec![Run {
                text: text.into(),
                bold,
                font_family: Some("Times New Roman".into()),
                font_size: Some(12.0),
                ..Default::default()
            }],
            style: ParagraphStyle::default(),
            numbering: None,
            hierarchy_level: None,
            source_location: Default::default(),
        }
    }

    fn para(font: &str, size: f32, align: Alignment) -> Block {
        let text = "Bu paragraf yeterince uzun olduğu için biçim karşılaştırmasına girer.";
        Block {
            id: String::new(),
            kind: BlockKind::Paragraph,
            text: text.into(),
            normalized_text: String::new(),
            runs: vec![Run {
                text: text.into(),
                font_family: Some(font.into()),
                font_size: Some(size),
                ..Default::default()
            }],
            style: ParagraphStyle {
                alignment: Some(align),
                ..Default::default()
            },
            numbering: None,
            hierarchy_level: None,
            source_location: Default::default(),
        }
    }

    fn doc_of(blocks: Vec<Block>) -> Document {
        Document::new(SourceFormat::Docx, finalize_blocks(blocks))
    }

    #[test]
    fn one_non_bold_heading_among_three_bold_ones_is_reported() {
        let d = doc_of(vec![
            heading("A. BİRİNCİ", true),
            heading("B. İKİNCİ", true),
            heading("C. ÜÇÜNCÜ", false),
            heading("D. DÖRDÜNCÜ", true),
        ]);
        let f = lint_doc(&d);
        let h = of(&f, HEADING);
        assert_eq!(h.len(), 1, "{:?}", h);
        assert_eq!(h[0].block_id, "p2");
        assert!(h[0].message.contains("düz"));
        assert!(
            h[0].message.starts_with('A'),
            "message should read as a sentence: {}",
            h[0].message
        );
    }

    #[test]
    fn uniformly_formatted_headings_are_silent() {
        let d = doc_of(vec![
            heading("A. BİRİNCİ", true),
            heading("B. İKİNCİ", true),
            heading("C. ÜÇÜNCÜ", true),
        ]);
        assert!(of(&lint_doc(&d), HEADING).is_empty());
    }

    #[test]
    fn a_group_too_small_to_have_a_majority_is_not_judged() {
        let d = doc_of(vec![
            heading("A. BİRİNCİ", true),
            heading("B. İKİNCİ", false),
        ]);
        assert!(of(&lint_doc(&d), HEADING).is_empty());
    }

    #[test]
    fn two_deviations_are_a_second_convention_not_a_slip() {
        let d = doc_of(vec![
            heading("A. BİRİNCİ", true),
            heading("B. İKİNCİ", true),
            heading("C. ÜÇÜNCÜ", false),
            heading("D. DÖRDÜNCÜ", false),
        ]);
        assert!(of(&lint_doc(&d), HEADING).is_empty());
    }

    #[test]
    fn an_unnumbered_title_line_is_not_compared_against_numbered_headings() {
        // The addressee line at the head of a petition is centred while the
        // section headings below it are left-aligned. That is correct drafting,
        // not an outlier.
        let mut title = heading("SAYIN ANKARA 3. ASLİYE HUKUK MAHKEMESİNE", true);
        title.style.alignment = Some(Alignment::Center);
        let mut blocks = vec![title];
        for m in ["A.", "B.", "C.", "D."] {
            let mut h = heading(&format!("{m} BAŞLIK"), true);
            h.style.alignment = Some(Alignment::Left);
            blocks.push(h);
        }
        let d = doc_of(blocks);
        assert!(
            of(&lint_doc(&d), HEADING).is_empty(),
            "{:?}",
            of(&lint_doc(&d), HEADING)
        );
    }

    #[test]
    fn a_body_paragraph_in_the_wrong_font_is_reported() {
        let mut blocks: Vec<Block> = (0..5)
            .map(|_| para("Times New Roman", 12.0, Alignment::Justify))
            .collect();
        blocks.insert(3, para("Arial", 12.0, Alignment::Justify));
        let d = doc_of(blocks);
        let f = of(&lint_doc(&d), BODY);
        assert_eq!(f.len(), 1, "{:?}", f);
        assert!(f[0].message.contains("Arial"));
    }

    #[test]
    fn headings_are_never_compared_against_body_text() {
        // Bold headings among non-bold paragraphs must not flag either group.
        let mut blocks: Vec<Block> = (0..5)
            .map(|_| para("Times New Roman", 12.0, Alignment::Justify))
            .collect();
        blocks.insert(0, heading("A. BİRİNCİ", true));
        blocks.insert(3, heading("B. İKİNCİ", true));
        let d = doc_of(blocks);
        let f = lint_doc(&d);
        assert!(of(&f, BODY).is_empty(), "{:?}", of(&f, BODY));
        assert!(of(&f, HEADING).is_empty());
    }

    #[test]
    fn a_deviation_from_a_saved_profile_is_reported() {
        let standard = doc_of(
            (0..8)
                .map(|_| para("Times New Roman", 12.0, Alignment::Justify))
                .collect(),
        );
        let profile = DocumentProfile::derive("Dava Dilekçesi", &standard);
        assert!(profile.is_usable());

        let mut blocks: Vec<Block> = (0..5)
            .map(|_| para("Arial", 12.0, Alignment::Justify))
            .collect();
        blocks.push(para("Arial", 12.0, Alignment::Justify));
        let target = doc_of(blocks);

        let dict = crate::dict::UserDictionary::default();
        let opts = crate::rules::LintOptions::default();
        let ctx = Context::new(&target, &dict, Some(&profile), &opts);
        let f = crate::rules::analyze(&ctx).findings;
        let p = of(&f, PROFILE);
        assert!(!p.is_empty());
        assert!(p[0].message.contains("Times New Roman"));
    }

    #[test]
    fn a_document_matching_its_profile_produces_no_deviations() {
        let standard = doc_of(
            (0..8)
                .map(|_| para("Times New Roman", 12.0, Alignment::Justify))
                .collect(),
        );
        let profile = DocumentProfile::derive("Dava Dilekçesi", &standard);
        let dict = crate::dict::UserDictionary::default();
        let opts = crate::rules::LintOptions::default();
        let ctx = Context::new(&standard, &dict, Some(&profile), &opts);
        assert!(of(&crate::rules::analyze(&ctx).findings, PROFILE).is_empty());
    }
}
