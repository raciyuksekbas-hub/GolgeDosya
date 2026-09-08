//! Container parsers.
//!
//! A parser's only job is to turn a byte stream into a [`Document`]. Nothing
//! downstream is allowed to learn which container a document came from, so any
//! rule written against the model works identically for DOCX and UDF.

pub mod archive;
pub mod docx;
pub mod udf;

use crate::cdm::{
    Block, BlockKind, Document, Numbering, ParagraphStyle, Run, SourceFormat, SourceLocation,
};
use crate::numbering::parse_marker;
use crate::text::normalize;

/// Hard limits applied to every container before it is expanded.
///
/// A malformed or hostile archive must fail cleanly rather than exhaust
/// memory, so decompression is bounded up front instead of being trusted.
pub mod limits {
    /// Largest accepted input file.
    pub const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;
    /// Largest total size the archive may expand to.
    pub const MAX_TOTAL_UNCOMPRESSED: u64 = 256 * 1024 * 1024;
    /// Largest single entry we will read out of an archive.
    pub const MAX_ENTRY_UNCOMPRESSED: u64 = 96 * 1024 * 1024;
    /// Largest compression ratio tolerated for one entry.
    pub const MAX_COMPRESSION_RATIO: u64 = 512;
    /// Largest number of entries examined.
    pub const MAX_ENTRIES: usize = 4096;
    /// Largest number of blocks produced. Beyond this the document is truncated.
    pub const MAX_BLOCKS: usize = 200_000;
    /// Deepest XML nesting accepted.
    pub const MAX_XML_DEPTH: usize = 256;
}

/// Why a document could not be read.
///
/// The variants carry a stage and a class, never a fragment of the document.
/// This is what user-facing messages and any diagnostic output are built from,
/// so document content cannot escape through an error path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The extension is not one we handle.
    UnsupportedFormat,
    /// The file is larger than [`limits::MAX_INPUT_BYTES`].
    TooLarge,
    /// The bytes are not a readable archive of the expected kind.
    NotAnArchive,
    /// A required part is missing from the archive.
    MissingPart(&'static str),
    /// The XML inside the archive is not well formed.
    MalformedXml,
    /// The archive declares or expands to more than the limits allow.
    ResourceLimit(&'static str),
    /// The archive read, but held no text at all.
    EmptyDocument,
    /// Reading the file from disk failed.
    Io,
}

impl ParseError {
    /// The short, complete sentence shown to the user. Never includes content.
    pub fn message_tr(&self) -> &'static str {
        match self {
            ParseError::UnsupportedFormat => "Bu dosya biçimi desteklenmiyor. DOCX veya UDF seçin.",
            ParseError::TooLarge => "Dosya çok büyük.",
            ParseError::NotAnArchive => "Belge okunamadı. Dosya bozuk görünüyor.",
            ParseError::MissingPart(_) => "Belge okunamadı. Dosyanın bir bölümü eksik.",
            ParseError::MalformedXml => "Belge okunamadı. Dosyanın içeriği bozuk.",
            ParseError::ResourceLimit(_) => "Belge okunamadı. Dosya beklenenden çok daha karmaşık.",
            ParseError::EmptyDocument => "Belgede incelenecek metin bulunamadı.",
            ParseError::Io => "Dosya açılamadı.",
        }
    }

    /// A stable, content-free label for diagnostics.
    pub fn code(&self) -> &'static str {
        match self {
            ParseError::UnsupportedFormat => "unsupported_format",
            ParseError::TooLarge => "too_large",
            ParseError::NotAnArchive => "not_an_archive",
            ParseError::MissingPart(_) => "missing_part",
            ParseError::MalformedXml => "malformed_xml",
            ParseError::ResourceLimit(_) => "resource_limit",
            ParseError::EmptyDocument => "empty_document",
            ParseError::Io => "io",
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for ParseError {}

/// Detect the format from the file name alone.
pub fn format_from_name(file_name: &str) -> Option<SourceFormat> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".docx") {
        Some(SourceFormat::Docx)
    } else if lower.ends_with(".udf") {
        Some(SourceFormat::Udf)
    } else {
        None
    }
}

/// Parse `bytes` as the format implied by `file_name`.
///
/// `file_name` is used for the extension and is recorded in the metadata as a
/// bare name. Directory paths are never accepted here.
pub fn parse(file_name: &str, bytes: &[u8]) -> Result<Document, ParseError> {
    if bytes.len() as u64 > limits::MAX_INPUT_BYTES {
        return Err(ParseError::TooLarge);
    }
    let base = base_name(file_name);
    let format = format_from_name(&base).ok_or(ParseError::UnsupportedFormat)?;
    let mut doc = match format {
        SourceFormat::Docx => docx::parse(bytes)?,
        SourceFormat::Udf => udf::parse(bytes)?,
    };
    doc.metadata.file_name = base;
    doc.metadata.byte_size = bytes.len() as u64;
    doc.recompute_metadata();
    if doc.blocks.iter().all(|b| b.text.trim().is_empty()) {
        return Err(ParseError::EmptyDocument);
    }
    Ok(doc)
}

/// Strip any directory component. Guards against a crafted name reaching the
/// model, and keeps paths out of the metadata by construction.
pub fn base_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Shared post-processing: assign ids, detect markers, derive outline levels.
///
/// Both parsers hand over raw blocks and let this decide what is a heading,
/// so the two containers cannot drift apart in how structure is recognised.
pub(crate) fn finalize_blocks(mut blocks: Vec<Block>) -> Vec<Block> {
    blocks.truncate(limits::MAX_BLOCKS);
    for (i, b) in blocks.iter_mut().enumerate() {
        b.id = format!("p{i}");
        b.source_location.block_index = i;
        b.normalized_text = normalize(&b.text);
        if let Some(m) = parse_marker(&b.text) {
            let chosen = m.primary();
            b.numbering = Some(m.to_numbering(chosen));
        }
        if is_heading(b) {
            b.kind = BlockKind::Heading;
        }
    }
    assign_hierarchy_levels(&mut blocks);
    blocks
}

/// A block counts as a heading when the container says so, or when its shape
/// is unmistakably one: short, numbered, and not ending in a full stop.
fn is_heading(b: &Block) -> bool {
    if b.kind == BlockKind::Heading {
        return true;
    }
    if b.kind == BlockKind::TableCell || b.kind == BlockKind::SectionBreak {
        return false;
    }
    if let Some(name) = &b.style.style_name {
        let l = name.to_ascii_lowercase();
        if l.starts_with("heading") || l.starts_with("başlık") || l.starts_with("baslik") {
            return true;
        }
    }
    let body = b.body_text().trim();
    if body.is_empty() || b.numbering.is_none() {
        return false;
    }
    // Headings are short and do not read as a sentence.
    let words = body.split_whitespace().count();
    if words > 12 {
        return false;
    }
    if body.ends_with('.') || body.ends_with(';') || body.ends_with(',') {
        return false;
    }
    // All-caps or fully bold reads as a heading; so does a lone marker.
    let all_bold = !b.runs.is_empty()
        && b.runs
            .iter()
            .filter(|r| !r.text.trim().is_empty())
            .all(|r| r.bold);
    let upper = crate::text::tr_upper(body);
    let all_caps = body.chars().any(|c| c.is_alphabetic()) && upper == body;
    all_bold || all_caps
}

/// Outline depth from the marker's shape.
///
/// Documents mix conventions freely, so depth is assigned by first appearance:
/// the first marker shape seen is depth 0, the next new shape depth 1, and so
/// on. This is stable within a document and never compares two shapes that the
/// author clearly meant to be different levels.
fn assign_hierarchy_levels(blocks: &mut [Block]) {
    let mut order: Vec<(crate::cdm::NumberingScheme, crate::cdm::Decoration)> = Vec::new();
    for b in blocks.iter() {
        if b.kind != BlockKind::Heading {
            continue;
        }
        if let Some(n) = &b.numbering {
            let key = (n.scheme, n.decoration);
            if !order.contains(&key) {
                order.push(key);
            }
        }
    }
    for b in blocks.iter_mut() {
        if b.kind != BlockKind::Heading {
            continue;
        }
        b.hierarchy_level = match &b.numbering {
            Some(n) => order
                .iter()
                .position(|k| *k == (n.scheme, n.decoration))
                .map(|i| i.min(u8::MAX as usize) as u8),
            None => Some(0),
        };
    }
}

/// Assemble a block from runs, computing the concatenated text and offsets.
pub(crate) fn block_from_runs(
    kind: BlockKind,
    mut runs: Vec<Run>,
    style: ParagraphStyle,
    container_path: Option<String>,
) -> Block {
    let mut text = String::new();
    let mut cursor = 0usize;
    for r in runs.iter_mut() {
        r.char_start = cursor;
        cursor += r.text.chars().count();
        text.push_str(&r.text);
    }
    Block {
        id: String::new(),
        kind,
        normalized_text: normalize(&text),
        text,
        runs,
        style,
        numbering: None::<Numbering>,
        hierarchy_level: None,
        source_location: SourceLocation {
            container_path,
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_detection_is_extension_based_and_case_insensitive() {
        assert_eq!(format_from_name("dilekce.DOCX"), Some(SourceFormat::Docx));
        assert_eq!(format_from_name("dilekce.udf"), Some(SourceFormat::Udf));
        assert_eq!(format_from_name("dilekce.pdf"), None);
    }

    #[test]
    fn base_name_drops_directories_so_paths_never_reach_the_model() {
        assert_eq!(base_name("/Users/x/Belgeler/dava.docx"), "dava.docx");
        assert_eq!(base_name("..\\..\\gizli\\dava.docx"), "dava.docx");
        assert_eq!(base_name("dava.docx"), "dava.docx");
    }

    #[test]
    fn parse_errors_never_carry_document_content() {
        for e in [
            ParseError::MalformedXml,
            ParseError::NotAnArchive,
            ParseError::ResourceLimit("x"),
            ParseError::MissingPart("word/document.xml"),
        ] {
            assert!(!e.message_tr().is_empty());
            assert!(!e.code().contains(' '));
        }
    }

    #[test]
    fn block_from_runs_lays_out_char_offsets_across_runs() {
        let b = block_from_runs(
            BlockKind::Paragraph,
            vec![
                Run {
                    text: "İkinci".into(),
                    ..Default::default()
                },
                Run {
                    text: "Göz".into(),
                    ..Default::default()
                },
            ],
            ParagraphStyle::default(),
            None,
        );
        assert_eq!(b.text, "İkinciGöz");
        assert_eq!(b.runs[1].char_start, 6);
    }
}
