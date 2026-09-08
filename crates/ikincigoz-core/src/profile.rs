//! Document standards.
//!
//! The user points at a document they consider correct and says "this is my
//! standard". İkinciGöz records the *reliable* formatting facts from it — the
//! ones that repeat often enough to be a deliberate house style rather than an
//! accident — and later reports deviations from them.
//!
//! Only formatting is recorded. No text, no party names, no file paths.

use crate::cdm::{Alignment, Block, BlockKind, Document};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How much of the source document agreed on a value before it was recorded.
pub const MIN_DOMINANCE: f32 = 0.7;
/// How many samples a value needs before it is trusted at all.
pub const MIN_SAMPLES: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BodyStandard {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub alignment: Option<Alignment>,
    pub line_spacing: Option<f32>,
    pub spacing_after: Option<f32>,
    pub first_line_indent: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HeadingStandard {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub bold: Option<bool>,
    pub alignment: Option<Alignment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentProfile {
    /// User-chosen name, e.g. `Dava Dilekçesi`.
    pub name: String,
    pub body: BodyStandard,
    /// One entry per outline depth that had enough samples.
    pub headings: HashMap<u8, HeadingStandard>,
    /// Marker shapes used at each depth, e.g. `0 -> "büyük harf + ."`.
    pub numbering_shapes: HashMap<u8, String>,
    /// Share of body runs that were bold. A house style with almost no bold
    /// makes a suddenly-bold paragraph meaningful.
    pub bold_share: f32,
    /// How many paragraphs the profile was derived from, for the settings UI.
    pub sample_blocks: usize,
}

impl DocumentProfile {
    /// Derive a profile from a document the user has vouched for.
    pub fn derive(name: &str, doc: &Document) -> Self {
        let body: Vec<&Block> = doc
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Paragraph || b.kind == BlockKind::ListItem)
            .filter(|b| !b.is_empty())
            .collect();

        let body_std = BodyStandard {
            font_family: dominant(
                body.iter()
                    .flat_map(|b| b.runs.iter())
                    .filter_map(|r| r.font_family.clone()),
            ),
            font_size: dominant_num(
                body.iter()
                    .flat_map(|b| b.runs.iter())
                    .filter_map(|r| r.font_size),
            ),
            alignment: dominant(body.iter().filter_map(|b| b.style.alignment)),
            line_spacing: dominant_num(body.iter().filter_map(|b| b.style.line_spacing)),
            spacing_after: dominant_num(body.iter().filter_map(|b| b.style.spacing_after)),
            first_line_indent: dominant_num(body.iter().filter_map(|b| b.style.first_line_indent)),
        };

        let mut headings: HashMap<u8, HeadingStandard> = HashMap::new();
        let mut shapes: HashMap<u8, String> = HashMap::new();
        let mut by_level: HashMap<u8, Vec<&Block>> = HashMap::new();
        for b in doc.blocks.iter().filter(|b| b.kind == BlockKind::Heading) {
            by_level
                .entry(b.hierarchy_level.unwrap_or(0))
                .or_default()
                .push(b);
        }
        for (level, blocks) in by_level {
            if blocks.len() < 2 {
                continue;
            }
            headings.insert(
                level,
                HeadingStandard {
                    font_family: dominant(
                        blocks
                            .iter()
                            .flat_map(|b| b.runs.iter())
                            .filter_map(|r| r.font_family.clone()),
                    ),
                    font_size: dominant_num(
                        blocks
                            .iter()
                            .flat_map(|b| b.runs.iter())
                            .filter_map(|r| r.font_size),
                    ),
                    bold: dominant(blocks.iter().flat_map(|b| b.runs.iter()).map(|r| r.bold)),
                    alignment: dominant(blocks.iter().filter_map(|b| b.style.alignment)),
                },
            );
            if let Some(shape) = dominant(blocks.iter().filter_map(|b| {
                b.numbering
                    .as_ref()
                    .map(|n| format!("{} + {:?}", n.scheme.label(), n.decoration))
            })) {
                shapes.insert(level, shape);
            }
        }

        let all_runs: Vec<&crate::cdm::Run> = body
            .iter()
            .flat_map(|b| b.runs.iter())
            .filter(|r| !r.text.trim().is_empty())
            .collect();
        let bold_share = if all_runs.is_empty() {
            0.0
        } else {
            all_runs.iter().filter(|r| r.bold).count() as f32 / all_runs.len() as f32
        };

        DocumentProfile {
            name: name.trim().to_string(),
            body: body_std,
            headings,
            numbering_shapes: shapes,
            bold_share,
            sample_blocks: body.len(),
        }
    }

    /// A profile built from too little material would generate noise, so it is
    /// refused rather than stored.
    pub fn is_usable(&self) -> bool {
        self.sample_blocks >= MIN_SAMPLES
            && (self.body.font_family.is_some()
                || self.body.font_size.is_some()
                || self.body.alignment.is_some())
    }
}

/// The most common value, but only when it clearly dominates.
///
/// Requiring dominance is what keeps a profile honest: a document that mixes
/// two fonts evenly has no house font, and recording one anyway would make
/// every later document wrong.
fn dominant<T, I>(values: I) -> Option<T>
where
    T: Clone + PartialEq,
    I: IntoIterator<Item = T>,
{
    let v: Vec<T> = values.into_iter().collect();
    if v.len() < MIN_SAMPLES {
        return None;
    }
    let mut best: Option<(T, usize)> = None;
    for candidate in v.iter() {
        let count = v.iter().filter(|x| *x == candidate).count();
        if best.as_ref().map(|(_, c)| count > *c).unwrap_or(true) {
            best = Some((candidate.clone(), count));
        }
    }
    let (value, count) = best?;
    if count as f32 / v.len() as f32 >= MIN_DOMINANCE {
        Some(value)
    } else {
        None
    }
}

/// Same as [`dominant`], but tolerant of float representation noise.
fn dominant_num<I: IntoIterator<Item = f32>>(values: I) -> Option<f32> {
    let quantised: Vec<i32> = values
        .into_iter()
        .map(|v| (v * 100.0).round() as i32)
        .collect();
    dominant(quantised).map(|q| q as f32 / 100.0)
}

/// All profiles the user has saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProfileStore {
    pub profiles: Vec<DocumentProfile>,
    /// Name of the profile applied to new analyses, if any.
    pub active: Option<String>,
}

impl ProfileStore {
    pub fn upsert(&mut self, profile: DocumentProfile) {
        match self.profiles.iter_mut().find(|p| p.name == profile.name) {
            Some(existing) => *existing = profile,
            None => self.profiles.push(profile),
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.profiles.retain(|p| p.name != name);
        if self.active.as_deref() == Some(name) {
            self.active = None;
        }
    }

    pub fn active_profile(&self) -> Option<&DocumentProfile> {
        let name = self.active.as_ref()?;
        self.profiles.iter().find(|p| &p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdm::{ParagraphStyle, Run, SourceFormat, SourceLocation};

    fn para(font: &str, size: f32, align: Alignment) -> Block {
        Block {
            id: String::new(),
            kind: BlockKind::Paragraph,
            text: "Bir paragraf metni.".into(),
            normalized_text: "bir paragraf metni.".into(),
            runs: vec![Run {
                text: "Bir paragraf metni.".into(),
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
            source_location: SourceLocation::default(),
        }
    }

    #[test]
    fn a_consistent_document_yields_its_house_style() {
        let blocks: Vec<Block> = (0..8)
            .map(|_| para("Times New Roman", 12.0, Alignment::Justify))
            .collect();
        let doc = Document::new(SourceFormat::Docx, blocks);
        let p = DocumentProfile::derive("Dava Dilekçesi", &doc);
        assert_eq!(p.body.font_family.as_deref(), Some("Times New Roman"));
        assert_eq!(p.body.font_size, Some(12.0));
        assert_eq!(p.body.alignment, Some(Alignment::Justify));
        assert!(p.is_usable());
    }

    #[test]
    fn a_document_that_mixes_fonts_evenly_declares_no_house_font() {
        let mut blocks: Vec<Block> = (0..4)
            .map(|_| para("Times New Roman", 12.0, Alignment::Justify))
            .collect();
        blocks.extend((0..4).map(|_| para("Arial", 12.0, Alignment::Justify)));
        let doc = Document::new(SourceFormat::Docx, blocks);
        let p = DocumentProfile::derive("Karışık", &doc);
        assert_eq!(p.body.font_family, None);
        // The size still dominates, so the profile is not worthless.
        assert_eq!(p.body.font_size, Some(12.0));
    }

    #[test]
    fn a_short_document_does_not_produce_a_usable_profile() {
        let doc = Document::new(
            SourceFormat::Docx,
            vec![para("Arial", 11.0, Alignment::Left)],
        );
        assert!(!DocumentProfile::derive("Kısa", &doc).is_usable());
    }

    #[test]
    fn profiles_round_trip_through_json() {
        let blocks: Vec<Block> = (0..6)
            .map(|_| para("Arial", 11.0, Alignment::Left))
            .collect();
        let doc = Document::new(SourceFormat::Docx, blocks);
        let p = DocumentProfile::derive("İhtarname", &doc);
        let back: DocumentProfile =
            serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn a_profile_records_no_document_text() {
        let blocks: Vec<Block> = (0..6)
            .map(|_| para("Arial", 11.0, Alignment::Left))
            .collect();
        let doc = Document::new(SourceFormat::Docx, blocks);
        let p = DocumentProfile::derive("Gizli", &doc);
        let json = serde_json::to_string(&p).unwrap();
        assert!(
            !json.contains("paragraf"),
            "profile leaked document text: {json}"
        );
    }

    #[test]
    fn the_store_replaces_a_profile_of_the_same_name() {
        let mut s = ProfileStore::default();
        let mk = |n: &str| DocumentProfile {
            name: n.into(),
            body: BodyStandard::default(),
            headings: HashMap::new(),
            numbering_shapes: HashMap::new(),
            bold_share: 0.0,
            sample_blocks: 9,
        };
        s.upsert(mk("A"));
        s.upsert(mk("A"));
        s.upsert(mk("B"));
        assert_eq!(s.profiles.len(), 2);
        s.active = Some("A".into());
        assert!(s.active_profile().is_some());
        s.remove("A");
        assert!(s.active_profile().is_none());
    }
}
