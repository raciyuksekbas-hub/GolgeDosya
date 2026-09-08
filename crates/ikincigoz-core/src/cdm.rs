//! Canonical Document Model.
//!
//! Both the DOCX and the UDF parser produce this model, and the lint engine
//! only ever sees this model. No rule may know which container the document
//! came from.

use serde::{Deserialize, Serialize};

/// Where a piece of the model came from inside the original container.
///
/// Kept deliberately coarse-grained but stable: write-back resolves a finding
/// back to a concrete text node through these coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SourceLocation {
    /// Index of the owning block in `Document::blocks`.
    pub block_index: usize,
    /// Index of the run inside the block, when the location is run-precise.
    pub run_index: Option<usize>,
    /// Character offset (in `char`s, not bytes) inside the block's `text`.
    pub char_start: Option<usize>,
    /// Exclusive end offset, in `char`s.
    pub char_end: Option<usize>,
    /// Container-specific address, opaque to the lint engine.
    /// DOCX: `w:body` child ordinal. UDF: offset into the content blob.
    pub container_path: Option<String>,
}

impl SourceLocation {
    pub fn block(block_index: usize) -> Self {
        Self {
            block_index,
            ..Default::default()
        }
    }

    pub fn span(block_index: usize, char_start: usize, char_end: usize) -> Self {
        Self {
            block_index,
            run_index: None,
            char_start: Some(char_start),
            char_end: Some(char_end),
            container_path: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    Left,
    Center,
    Right,
    Justify,
}

/// Character-level formatting for a contiguous stretch of text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub font_family: Option<String>,
    /// Points. Stored as half-points/2 for DOCX, raw for UDF.
    pub font_size: Option<f32>,
    /// Hex RGB without the leading `#`, uppercased.
    pub color: Option<String>,
    /// Char offset of this run's start inside the owning block's `text`.
    pub char_start: usize,
    /// Container address of the text node backing this run, for write-back.
    pub container_path: Option<String>,
}

impl Run {
    pub fn char_end(&self) -> usize {
        self.char_start + self.text.chars().count()
    }
}

/// Paragraph-level formatting. All measurements are in points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ParagraphStyle {
    pub alignment: Option<Alignment>,
    pub indent_left: Option<f32>,
    pub indent_right: Option<f32>,
    pub first_line_indent: Option<f32>,
    pub spacing_before: Option<f32>,
    pub spacing_after: Option<f32>,
    /// Multiplier. 1.0 = single, 1.5 = one-and-a-half.
    pub line_spacing: Option<f32>,
    /// Named style from the container, e.g. `Heading1`.
    pub style_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Paragraph,
    Heading,
    ListItem,
    TableCell,
    SectionBreak,
}

/// The numbering marker detected at the head of a block, e.g. `A.` or `1)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Numbering {
    /// Raw marker text as it appears, e.g. `"B."`.
    pub raw: String,
    /// Which numbering alphabet the marker belongs to.
    pub scheme: NumberingScheme,
    /// Ordinal value: `A` -> 1, `iv` -> 4, `12` -> 12.
    pub value: u32,
    /// How the marker is decorated, so `1.` and `1)` are not treated as peers.
    pub decoration: Decoration,
    /// Char offset just past the marker and its trailing whitespace.
    pub text_start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberingScheme {
    Decimal,
    UpperLatin,
    LowerLatin,
    UpperRoman,
    LowerRoman,
}

impl NumberingScheme {
    pub fn label(self) -> &'static str {
        match self {
            NumberingScheme::Decimal => "rakam",
            NumberingScheme::UpperLatin => "büyük harf",
            NumberingScheme::LowerLatin => "küçük harf",
            NumberingScheme::UpperRoman => "büyük Roma rakamı",
            NumberingScheme::LowerRoman => "küçük Roma rakamı",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decoration {
    /// `1.`
    Dot,
    /// `1)`
    Paren,
    /// `(1)`
    Bracketed,
    /// `1 -`
    Dash,
}

/// One addressable unit of the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// Stable id used by findings, e.g. `p17`.
    pub id: String,
    pub kind: BlockKind,
    /// Concatenation of all run texts. The canonical text of the block.
    pub text: String,
    /// `text` after whitespace, quote and case normalisation. Used for
    /// duplicate detection so cosmetic differences do not hide a repeat.
    pub normalized_text: String,
    pub runs: Vec<Run>,
    pub style: ParagraphStyle,
    pub numbering: Option<Numbering>,
    /// Outline depth. 0 = top level. `None` when the block is not a heading.
    pub hierarchy_level: Option<u8>,
    pub source_location: SourceLocation,
}

impl Block {
    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// The block text with any numbering marker stripped off the front.
    pub fn body_text(&self) -> &str {
        match &self.numbering {
            Some(n) => {
                let byte = char_to_byte(&self.text, n.text_start);
                &self.text[byte..]
            }
            None => &self.text,
        }
    }

    /// Locate the run covering a char offset inside `text`.
    pub fn run_at(&self, char_offset: usize) -> Option<(usize, &Run)> {
        self.runs
            .iter()
            .enumerate()
            .find(|(_, r)| char_offset >= r.char_start && char_offset < r.char_end())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceFormat {
    Docx,
    Udf,
}

impl SourceFormat {
    pub fn label(self) -> &'static str {
        match self {
            SourceFormat::Docx => "DOCX",
            SourceFormat::Udf => "UDF",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DocumentMetadata {
    /// File name only. Never a full path: paths are not carried into the model
    /// so they cannot leak into findings, logs or reports.
    pub file_name: String,
    pub byte_size: u64,
    pub block_count: usize,
    pub word_count: usize,
    pub char_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub format: SourceFormat,
    pub metadata: DocumentMetadata,
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn new(format: SourceFormat, blocks: Vec<Block>) -> Self {
        let mut doc = Document {
            format,
            metadata: DocumentMetadata::default(),
            blocks,
        };
        doc.recompute_metadata();
        doc
    }

    pub fn recompute_metadata(&mut self) {
        self.metadata.block_count = self.blocks.len();
        self.metadata.char_count = self.blocks.iter().map(|b| b.char_len()).sum();
        self.metadata.word_count = self
            .blocks
            .iter()
            .map(|b| b.text.split_whitespace().count())
            .sum();
    }

    pub fn block(&self, id: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Full plain text, one block per line. Used by write-back verification.
    pub fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Convert a char offset into a byte offset for the given string.
pub fn char_to_byte(s: &str, char_offset: usize) -> usize {
    s.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// Extract the substring covering `[start, end)` measured in `char`s.
pub fn char_slice(s: &str, start: usize, end: usize) -> String {
    s.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_with(text: &str) -> Block {
        Block {
            id: "p0".into(),
            kind: BlockKind::Paragraph,
            text: text.into(),
            normalized_text: text.into(),
            runs: vec![],
            style: ParagraphStyle::default(),
            numbering: None,
            hierarchy_level: None,
            source_location: SourceLocation::block(0),
        }
    }

    #[test]
    fn char_offsets_are_codepoint_based_for_turkish_text() {
        let b = block_with("İkinciGöz şğüçö");
        assert_eq!(b.char_len(), 15);
        assert_eq!(char_slice(&b.text, 0, 9), "İkinciGöz");
    }

    #[test]
    fn char_to_byte_clamps_past_the_end() {
        assert_eq!(char_to_byte("İG", 99), "İG".len());
    }

    #[test]
    fn body_text_strips_the_numbering_marker() {
        let mut b = block_with("A. Usule ilişkin itirazlar");
        b.numbering = Some(Numbering {
            raw: "A.".into(),
            scheme: NumberingScheme::UpperLatin,
            value: 1,
            decoration: Decoration::Dot,
            text_start: 3,
        });
        assert_eq!(b.body_text(), "Usule ilişkin itirazlar");
    }

    #[test]
    fn run_at_finds_the_covering_run() {
        let mut b = block_with("abcdef");
        b.runs = vec![
            Run {
                text: "abc".into(),
                char_start: 0,
                ..Default::default()
            },
            Run {
                text: "def".into(),
                char_start: 3,
                ..Default::default()
            },
        ];
        assert_eq!(b.run_at(0).unwrap().0, 0);
        assert_eq!(b.run_at(4).unwrap().0, 1);
        assert!(b.run_at(9).is_none());
    }
}
