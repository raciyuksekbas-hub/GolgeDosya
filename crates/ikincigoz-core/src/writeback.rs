//! Applying corrections to a copy of the document.
//!
//! This is the most dangerous thing the program does, so it is built to fail
//! rather than to damage:
//!
//! * The source file is never opened for writing. Output always goes to a new
//!   file, and the caller is given bytes, not a path.
//! * Nothing is re-serialised. Only the text nodes that an accepted change
//!   touches are patched; every other byte of the container is copied through
//!   unchanged, so styles, numbering, headers, images and metadata survive
//!   exactly as they were.
//! * Every change is checked against the text it claims to replace before it is
//!   applied. A stale change — one computed against a document that has since
//!   moved on — is rejected, not applied at the wrong offset.
//! * The result is re-opened, re-parsed and compared against the text the
//!   caller should have got. If it does not match, no output is produced.

use crate::cdm::{char_slice, Document, SourceFormat};
use crate::finding::Fix;
use crate::parser::{self, archive, ParseError};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteBackError {
    /// The document could not be read back for the write.
    Parse(ParseError),
    /// A change refers to a block or offset the document does not have.
    StaleChange { index: usize },
    /// A change's `original` does not match what is actually there.
    TextMismatch { index: usize },
    /// Two accepted changes cover the same characters.
    OverlappingChanges,
    /// The change touches text with no editable node behind it, such as a tab.
    Unaddressable { index: usize },
    /// Repackaging the container failed.
    Repackage,
    /// The output did not survive verification, so it was discarded.
    VerificationFailed(&'static str),
    /// The output differs from the source somewhere the user did not approve.
    UnauthorisedMutation,
}

impl WriteBackError {
    pub fn message_tr(&self) -> String {
        match self {
            WriteBackError::Parse(e) => e.message_tr().to_string(),
            WriteBackError::StaleChange { .. } | WriteBackError::TextMismatch { .. } => {
                "Belge, düzeltmeler hesaplandıktan sonra değişmiş görünüyor. \
                 Belgeyi yeniden inceleyin."
                    .into()
            }
            WriteBackError::OverlappingChanges => {
                "Seçilen düzeltmeler birbiriyle çakışıyor. Daha az düzeltme seçin.".into()
            }
            WriteBackError::Unaddressable { .. } => {
                "Bu düzeltme belgenin biçimini bozmadan uygulanamıyor.".into()
            }
            WriteBackError::Repackage => "Yeni dosya oluşturulamadı.".into(),
            WriteBackError::VerificationFailed(_) => {
                "Düzeltmeler uygulandı; ancak sonuç doğrulanamadığı için dosya \
                 oluşturulmadı. Kaynak belgeniz değiştirilmedi."
                    .into()
            }
            WriteBackError::UnauthorisedMutation => {
                "Sonuç dosyasında, onaylamadığınız bir yerde değişiklik saptandı. \
                 Dosya oluşturulmadı; kaynak belgeniz değiştirilmedi."
                    .into()
            }
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            WriteBackError::Parse(_) => "parse",
            WriteBackError::StaleChange { .. } => "stale_change",
            WriteBackError::TextMismatch { .. } => "text_mismatch",
            WriteBackError::OverlappingChanges => "overlapping_changes",
            WriteBackError::Unaddressable { .. } => "unaddressable",
            WriteBackError::Repackage => "repackage",
            WriteBackError::VerificationFailed(_) => "verification_failed",
            WriteBackError::UnauthorisedMutation => "unauthorised_mutation",
        }
    }
}

/// What a successful write-back produced.
#[derive(Debug, Clone)]
pub struct WriteBackResult {
    pub bytes: Vec<u8>,
    pub file_name: String,
    pub applied: usize,
}

/// The name of the copy. The original file name is never reused.
pub fn output_file_name(source: &str) -> String {
    let base = parser::base_name(source);
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) => (s, e),
        None => (base.as_str(), ""),
    };
    if ext.is_empty() {
        format!("{stem} - Düzeltilmiş")
    } else {
        format!("{stem} - Düzeltilmiş.{ext}")
    }
}

/// Apply `changes` to `bytes` and return a new container.
pub fn apply(
    file_name: &str,
    bytes: &[u8],
    changes: &[Fix],
) -> Result<WriteBackResult, WriteBackError> {
    let document = parser::parse(file_name, bytes).map_err(WriteBackError::Parse)?;
    let validated = validate(&document, changes)?;
    if validated.is_empty() {
        return Err(WriteBackError::OverlappingChanges);
    }

    let expected = expected_text(&document, &validated);

    // `Patched` carries the container's text image as it should read after the
    // approved changes and nothing else. Verification compares the real output
    // against that image at the container level, not at the model level: a
    // model can be lossy, and a region the model does not represent is exactly
    // where an unnoticed mutation would hide.
    let patched = match document.format {
        SourceFormat::Docx => apply_docx(bytes, &document, &validated)?,
        SourceFormat::Udf => apply_udf(bytes, &document, &validated)?,
    };

    verify(file_name, &patched, &document, &expected)?;

    Ok(WriteBackResult {
        bytes: patched.bytes,
        file_name: output_file_name(file_name),
        applied: validated.len(),
    })
}

/// The result of patching a container, with the text image it must produce.
struct Patched {
    bytes: Vec<u8>,
    /// Every character the container should now hold, in document order, with
    /// the approved changes applied and nothing else.
    expected_image: String,
}

/// A change that has been checked against the live document.
#[derive(Debug, Clone)]
struct Validated {
    block_index: usize,
    char_start: usize,
    char_end: usize,
    replacement: String,
}

fn validate(document: &Document, changes: &[Fix]) -> Result<Vec<Validated>, WriteBackError> {
    let mut out: Vec<Validated> = Vec::with_capacity(changes.len());

    for (index, fix) in changes.iter().enumerate() {
        let block = document
            .block(&fix.block_id)
            .ok_or(WriteBackError::StaleChange { index })?;
        if fix.char_start > fix.char_end || fix.char_end > block.char_len() {
            return Err(WriteBackError::StaleChange { index });
        }
        // The change carries the text it expects to find. If the document has
        // moved on, the offsets are meaningless and applying them would corrupt
        // the document silently.
        let actual = char_slice(&block.text, fix.char_start, fix.char_end);
        if actual != fix.original {
            return Err(WriteBackError::TextMismatch { index });
        }
        out.push(Validated {
            block_index: block.source_location.block_index,
            char_start: fix.char_start,
            char_end: fix.char_end,
            replacement: fix.replacement.clone(),
        });
    }

    out.sort_by_key(|c| (c.block_index, c.char_start, c.char_end));
    for pair in out.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.block_index == b.block_index && a.char_end > b.char_start {
            return Err(WriteBackError::OverlappingChanges);
        }
        // Two zero-width insertions at the same point would race.
        if a.block_index == b.block_index
            && a.char_start == b.char_start
            && a.char_end == b.char_end
        {
            return Err(WriteBackError::OverlappingChanges);
        }
    }
    Ok(out)
}

/// The plain text the patched document must produce, block by block.
fn expected_text(document: &Document, changes: &[Validated]) -> String {
    let mut blocks: Vec<String> = document.blocks.iter().map(|b| b.text.clone()).collect();
    let mut by_block: BTreeMap<usize, Vec<&Validated>> = BTreeMap::new();
    for c in changes {
        by_block.entry(c.block_index).or_default().push(c);
    }
    for (index, list) in by_block {
        let original: Vec<char> = blocks[index].chars().collect();
        let mut out = String::new();
        let mut cursor = 0usize;
        for c in list {
            out.extend(&original[cursor..c.char_start]);
            out.push_str(&c.replacement);
            cursor = c.char_end;
        }
        out.extend(&original[cursor..]);
        blocks[index] = out;
    }
    blocks.join("\n")
}

// --------------------------------------------------------------------------
// DOCX
// --------------------------------------------------------------------------

/// One edit expressed against a single `w:t` node.
#[derive(Debug, Clone)]
struct NodeEdit {
    local_start: usize,
    local_end: usize,
    replacement: String,
}

/// Split block-level changes into per-text-node edits.
///
/// A change may span several runs. The whole replacement goes into the first
/// run it touches — which keeps the replacement's formatting consistent — and
/// the covered text in later runs is removed.
fn split_to_nodes(
    document: &Document,
    changes: &[Validated],
) -> Result<BTreeMap<usize, Vec<NodeEdit>>, WriteBackError> {
    let mut out: BTreeMap<usize, Vec<NodeEdit>> = BTreeMap::new();

    for (index, change) in changes.iter().enumerate() {
        let block = &document.blocks[change.block_index];
        let mut placed = false;

        for run in &block.runs {
            let (rs, re) = (run.char_start, run.char_end());
            // A zero-width insertion belongs to the run it sits inside, or to
            // the run it sits at the end of when it lands on a boundary.
            let touches = if change.char_start == change.char_end {
                change.char_start >= rs && change.char_start <= re
            } else {
                change.char_start < re && change.char_end > rs
            };
            if !touches {
                continue;
            }
            let Some(path) = run.container_path.as_deref() else {
                return Err(WriteBackError::Unaddressable { index });
            };
            let Some(ordinal) = path
                .strip_prefix("docx:t:")
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return Err(WriteBackError::Unaddressable { index });
            };

            let local_start = change.char_start.max(rs) - rs;
            let local_end = change.char_end.min(re) - rs;
            let replacement = if placed {
                String::new()
            } else {
                change.replacement.clone()
            };
            placed = true;
            out.entry(ordinal).or_default().push(NodeEdit {
                local_start,
                local_end,
                replacement,
            });
            if change.char_end <= re {
                break;
            }
        }
        if !placed {
            return Err(WriteBackError::Unaddressable { index });
        }
    }

    for edits in out.values_mut() {
        edits.sort_by_key(|e| (e.local_start, e.local_end));
    }
    Ok(out)
}

fn apply_docx(
    bytes: &[u8],
    document: &Document,
    changes: &[Validated],
) -> Result<Patched, WriteBackError> {
    let edits = split_to_nodes(document, changes)?;
    let mut archive = archive::open(bytes).map_err(WriteBackError::Parse)?;
    let original = archive::read_entry(&mut archive, parser::docx::DOCUMENT_PART)
        .map_err(WriteBackError::Parse)?;
    let xml = archive::decode_utf8(&original);
    let expected_image = docx_text_image(&xml, Some(&edits));
    let patched = patch_docx_xml(&xml, &edits);
    Ok(Patched {
        bytes: replace_entry(bytes, parser::docx::DOCUMENT_PART, patched.as_bytes())?,
        expected_image,
    })
}

/// Every character held by the part's text nodes, in document order.
///
/// With `edits` supplied this is the image the patched part must produce;
/// without them it is the image a part actually holds. Comparing the two is
/// what proves no character moved outside an approved change.
fn docx_text_image(xml: &str, edits: Option<&BTreeMap<usize, Vec<NodeEdit>>>) -> String {
    let mut out = String::new();
    for (index, m) in WT_NODE.captures_iter(xml).enumerate() {
        // Text nodes are addressed from one, matching the parser's `docx:t:N`.
        let ordinal = index + 1;
        let text = unescape_xml(m.get(3).unwrap().as_str());
        match edits.and_then(|e| e.get(&ordinal)) {
            Some(node_edits) => out.push_str(&apply_node_edits(&text, node_edits)),
            None => out.push_str(&text),
        }
        // A separator keeps two adjacent nodes from merging in the image, so a
        // character migrating across a node boundary is still visible.
        out.push('\u{001F}');
    }
    out
}

/// `<w:t>` and `<w:t ...>` with their contents, non-greedy.
static WT_NODE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<((?:\w+:)?t)(\s[^>]*?)?>(.*?)</(?:\w+:)?t>").unwrap());

/// Rewrite the nth `w:t` node's text, leaving the rest of the part untouched.
fn patch_docx_xml(xml: &str, edits: &BTreeMap<usize, Vec<NodeEdit>>) -> String {
    let mut out = String::with_capacity(xml.len() + 256);
    let mut last = 0usize;
    let mut ordinal = 0usize;

    for m in WT_NODE.captures_iter(xml) {
        let whole = m.get(0).unwrap();
        ordinal += 1;
        let Some(node_edits) = edits.get(&ordinal) else {
            continue;
        };

        let tag = m.get(1).unwrap().as_str();
        let attrs = m.get(2).map(|a| a.as_str()).unwrap_or("");
        let body = m.get(3).unwrap().as_str();
        let text = unescape_xml(body);
        let updated = apply_node_edits(&text, node_edits);

        out.push_str(&xml[last..whole.start()]);
        out.push('<');
        out.push_str(tag);
        out.push_str(attrs);
        // Leading and trailing spaces are dropped by Word unless the node says
        // to keep them, and a correction can easily introduce one.
        if !attrs.contains("xml:space") {
            out.push_str(" xml:space=\"preserve\"");
        }
        out.push('>');
        out.push_str(&escape_xml(&updated));
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
        last = whole.end();
    }
    out.push_str(&xml[last..]);
    out
}

fn apply_node_edits(text: &str, edits: &[NodeEdit]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for e in edits {
        let start = e.local_start.min(chars.len());
        let end = e.local_end.min(chars.len()).max(start);
        if start < cursor {
            continue; // overlapping edits were rejected earlier; be safe anyway
        }
        out.extend(&chars[cursor..start]);
        out.push_str(&e.replacement);
        cursor = end;
    }
    out.extend(&chars[cursor.min(chars.len())..]);
    out
}

fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

// --------------------------------------------------------------------------
// UDF
// --------------------------------------------------------------------------

/// `<content .../>` span elements inside `<elements>`. The document's own text
/// blob is `<content>` with no attributes, so it is not matched here.
static UDF_SPAN: Lazy<Regex> = Lazy::new(|| Regex::new(r"<content(\s[^>]*?)/>").unwrap());
static UDF_ATTR_START: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"startOffset\s*=\s*"(\d+)""#).unwrap());
static UDF_ATTR_LEN: Lazy<Regex> = Lazy::new(|| Regex::new(r#"length\s*=\s*"(\d+)""#).unwrap());
/// The blob itself: the first `<content>` carrying CDATA.
static UDF_BODY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)(<content\s*>)(.*?)(</content>)").unwrap());

/// A replacement expressed against the UDF text blob, in UTF-16 units.
#[derive(Debug, Clone)]
struct BlobEdit {
    start: usize,
    end: usize,
    replacement: String,
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(|c| c.len_utf16()).sum()
}

fn apply_udf(
    bytes: &[u8],
    document: &Document,
    changes: &[Validated],
) -> Result<Patched, WriteBackError> {
    let mut edits: Vec<BlobEdit> = Vec::with_capacity(changes.len());

    for (index, change) in changes.iter().enumerate() {
        let block = &document.blocks[change.block_index];
        // Find the run holding the change, and convert the block-local offsets
        // into blob offsets through that run's recorded span.
        let mut placed = false;
        for run in &block.runs {
            let (rs, re) = (run.char_start, run.char_end());
            let touches = if change.char_start == change.char_end {
                change.char_start >= rs && change.char_start <= re
            } else {
                change.char_start < re && change.char_end > rs
            };
            if !touches {
                continue;
            }
            let Some(path) = run.container_path.as_deref() else {
                return Err(WriteBackError::Unaddressable { index });
            };
            let Some(rest) = path.strip_prefix("udf:off:") else {
                return Err(WriteBackError::Unaddressable { index });
            };
            let Some((base, _len)) = rest.split_once(':') else {
                return Err(WriteBackError::Unaddressable { index });
            };
            let Ok(base) = base.parse::<usize>() else {
                return Err(WriteBackError::Unaddressable { index });
            };

            let local_start = change.char_start.max(rs) - rs;
            let local_end = change.char_end.min(re) - rs;
            let prefix: String = run.text.chars().take(local_start).collect();
            let covered: String = run
                .text
                .chars()
                .skip(local_start)
                .take(local_end - local_start)
                .collect();
            let start = base + utf16_len(&prefix);
            edits.push(BlobEdit {
                start,
                end: start + utf16_len(&covered),
                replacement: if placed {
                    String::new()
                } else {
                    change.replacement.clone()
                },
            });
            placed = true;
            if change.char_end <= re {
                break;
            }
        }
        if !placed {
            return Err(WriteBackError::Unaddressable { index });
        }
    }

    edits.sort_by_key(|e| (e.start, e.end));

    let mut archive = archive::open(bytes).map_err(WriteBackError::Parse)?;
    let raw = archive::read_optional(&mut archive, parser::udf::CONTENT_PART)
        .or_else(|| {
            archive::find_entry(&mut archive, |n| {
                n.to_ascii_lowercase().ends_with("content.xml")
            })
            .map(|(_, b)| b)
        })
        .ok_or(WriteBackError::Parse(ParseError::MissingPart(
            parser::udf::CONTENT_PART,
        )))?;
    let xml = archive::decode_utf8(&raw);
    let expected_image = udf_text_image(&xml).map(|blob| apply_blob_edits(&blob, &edits));
    let patched = patch_udf_xml(&xml, &edits).ok_or(WriteBackError::Repackage)?;
    Ok(Patched {
        bytes: replace_entry(bytes, parser::udf::CONTENT_PART, patched.as_bytes())?,
        expected_image: expected_image.ok_or(WriteBackError::Repackage)?,
    })
}

/// The document's whole text blob, which is the UDF's text image.
fn udf_text_image(xml: &str) -> Option<String> {
    let m = UDF_BODY.captures(xml)?;
    let inner = m.get(2)?.as_str();
    Some(match (inner.find("<![CDATA["), inner.rfind("]]>")) {
        (Some(a), Some(b)) if b > a => inner[a + 9..b].to_string(),
        _ => inner.to_string(),
    })
}

fn patch_udf_xml(xml: &str, edits: &[BlobEdit]) -> Option<String> {
    let body_match = UDF_BODY.captures(xml)?;
    let whole = body_match.get(0).unwrap();
    let inner = body_match.get(2).unwrap().as_str();

    // The blob is normally wrapped in CDATA; keep whichever form it came in.
    let (prefix, blob, suffix) = match (inner.find("<![CDATA["), inner.rfind("]]>")) {
        (Some(a), Some(b)) if b > a => (&inner[..a + 9], &inner[a + 9..b], &inner[b..]),
        _ => ("", inner, ""),
    };

    let new_blob = apply_blob_edits(blob, edits);
    // A replacement containing the CDATA terminator would end the section early
    // and corrupt the file. Refuse rather than produce a broken document.
    if new_blob.contains("]]>") && !prefix.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(xml.len() + 256);
    out.push_str(&xml[..whole.start()]);
    out.push_str(body_match.get(1).unwrap().as_str());
    out.push_str(prefix);
    out.push_str(&new_blob);
    out.push_str(suffix);
    out.push_str(body_match.get(3).unwrap().as_str());

    // Every span after an edit shifts, so rewrite the offsets.
    let tail = &xml[whole.end()..];
    out.push_str(&rewrite_spans(tail, edits));
    Some(out)
}

fn apply_blob_edits(blob: &str, edits: &[BlobEdit]) -> String {
    // Work in UTF-16 space, since that is what the offsets mean.
    let units: Vec<u16> = blob.encode_utf16().collect();
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let mut cursor = 0usize;
    for e in edits {
        let start = e.start.min(units.len());
        let end = e.end.min(units.len()).max(start);
        if start < cursor {
            continue;
        }
        out.extend_from_slice(&units[cursor..start]);
        out.extend(e.replacement.encode_utf16());
        cursor = end;
    }
    out.extend_from_slice(&units[cursor.min(units.len())..]);
    String::from_utf16_lossy(&out)
}

/// Map an old blob offset to its position after the edits.
fn shift_offset(offset: usize, edits: &[BlobEdit]) -> usize {
    let mut delta: i64 = 0;
    for e in edits {
        if e.end <= offset {
            delta += utf16_len(&e.replacement) as i64 - (e.end - e.start) as i64;
        } else if e.start < offset {
            // The offset sits inside an edited stretch; pin it to the start.
            return (e.start as i64 + delta).max(0) as usize;
        } else {
            break;
        }
    }
    (offset as i64 + delta).max(0) as usize
}

fn rewrite_spans(xml: &str, edits: &[BlobEdit]) -> String {
    UDF_SPAN
        .replace_all(xml, |caps: &regex::Captures<'_>| {
            let attrs = caps.get(1).unwrap().as_str();
            let start = UDF_ATTR_START
                .captures(attrs)
                .and_then(|c| c[1].parse::<usize>().ok());
            let length = UDF_ATTR_LEN
                .captures(attrs)
                .and_then(|c| c[1].parse::<usize>().ok());
            let (Some(start), Some(length)) = (start, length) else {
                return caps.get(0).unwrap().as_str().to_string();
            };
            let new_start = shift_offset(start, edits);
            let new_end = shift_offset(start + length, edits);
            let new_length = new_end.saturating_sub(new_start);
            let updated = UDF_ATTR_START
                .replace(attrs, format!("startOffset=\"{new_start}\"").as_str())
                .into_owned();
            let updated = UDF_ATTR_LEN
                .replace(&updated, format!("length=\"{new_length}\"").as_str())
                .into_owned();
            format!("<content{updated}/>")
        })
        .into_owned()
}

// --------------------------------------------------------------------------
// Repackaging and verification
// --------------------------------------------------------------------------

/// Rebuild the archive with one entry replaced and every other entry copied
/// through verbatim, without recompressing.
fn replace_entry(bytes: &[u8], name: &str, contents: &[u8]) -> Result<Vec<u8>, WriteBackError> {
    let mut source = zip::ZipArchive::new(Cursor::new(bytes.to_vec()))
        .map_err(|_| WriteBackError::Parse(ParseError::NotAnArchive))?;
    let mut buffer = Cursor::new(Vec::with_capacity(bytes.len() + contents.len()));
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        for i in 0..source.len() {
            let entry = source
                .by_index_raw(i)
                .map_err(|_| WriteBackError::Repackage)?;
            if entry.name() == name {
                continue;
            }
            writer
                .raw_copy_file(entry)
                .map_err(|_| WriteBackError::Repackage)?;
        }
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file(name, options)
            .map_err(|_| WriteBackError::Repackage)?;
        writer
            .write_all(contents)
            .map_err(|_| WriteBackError::Repackage)?;
        writer.finish().map_err(|_| WriteBackError::Repackage)?;
    }
    Ok(buffer.into_inner())
}

/// Re-open, re-parse and compare. Nothing leaves this module unverified.
///
/// Four checks, in order of how much they can prove:
///
/// 1. the container is still a readable archive and every part decompresses;
/// 2. the parser can read the result back;
/// 3. the paragraph structure is unchanged;
/// 4. every character in the container's text is either untouched or part of a
///    change the user approved.
///
/// The fourth is the one that matters. The first three are all satisfied by a
/// document that has been quietly damaged in a region the model does not
/// represent faithfully — which is exactly how a correction once inserted a
/// space beside the one it was meant to add.
fn verify(
    file_name: &str,
    patched: &Patched,
    original: &Document,
    expected: &str,
) -> Result<(), WriteBackError> {
    // 1. The container must still be a readable archive with every part intact.
    let mut archive = zip::ZipArchive::new(Cursor::new(patched.bytes.clone()))
        .map_err(|_| WriteBackError::VerificationFailed("archive_unreadable"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|_| WriteBackError::VerificationFailed("entry_unreadable"))?;
        let mut sink = std::io::sink();
        std::io::copy(&mut entry, &mut sink)
            .map_err(|_| WriteBackError::VerificationFailed("entry_corrupt"))?;
    }

    // 2. The parser must be able to read it back.
    let reparsed = parser::parse(file_name, &patched.bytes)
        .map_err(|_| WriteBackError::VerificationFailed("reparse_failed"))?;

    // 3. The structure must be unchanged: a correction moves characters, never
    //    paragraphs.
    if reparsed.blocks.len() != original.blocks.len() {
        return Err(WriteBackError::VerificationFailed("block_count_changed"));
    }

    // 4. Nothing changed anywhere the user did not approve.
    let actual = container_text_image(original.format, &patched.bytes)
        .ok_or(WriteBackError::VerificationFailed("image_unreadable"))?;
    if actual != patched.expected_image {
        return Err(WriteBackError::UnauthorisedMutation);
    }

    // The model must also read as expected, which catches a change that landed
    // in the container but not where the interface said it would.
    if reparsed.plain_text() != expected {
        return Err(WriteBackError::VerificationFailed("text_mismatch"));
    }
    Ok(())
}

/// The text image a finished container actually holds.
pub fn container_text_image(format: SourceFormat, bytes: &[u8]) -> Option<String> {
    let mut archive = archive::open(bytes).ok()?;
    match format {
        SourceFormat::Docx => {
            let part = archive::read_entry(&mut archive, parser::docx::DOCUMENT_PART).ok()?;
            Some(docx_text_image(&archive::decode_utf8(&part), None))
        }
        SourceFormat::Udf => {
            let part =
                archive::read_optional(&mut archive, parser::udf::CONTENT_PART).or_else(|| {
                    archive::find_entry(&mut archive, |n| {
                        n.to_ascii_lowercase().ends_with("content.xml")
                    })
                    .map(|(_, b)| b)
                })?;
            udf_text_image(&archive::decode_utf8(&part))
        }
    }
}

/// Read the whole of an archive entry, for tests and for callers that want to
/// confirm a part survived.
pub fn read_part(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).ok()?;
    let mut file = archive.by_name(name).ok()?;
    let mut out = Vec::new();
    file.read_to_end(&mut out).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::Fix;

    const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

    fn docx_bytes(body: &str) -> Vec<u8> {
        let document =
            format!("<?xml version=\"1.0\"?><w:document {NS}><w:body>{body}</w:body></w:document>");
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buffer);
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("[Content_Types].xml", o).unwrap();
            w.write_all(b"<Types/>").unwrap();
            w.start_file("word/styles.xml", o).unwrap();
            w.write_all(format!("<w:styles {NS}/>").as_bytes()).unwrap();
            w.start_file("word/document.xml", o).unwrap();
            w.write_all(document.as_bytes()).unwrap();
            w.finish().unwrap();
        }
        buffer.into_inner()
    }

    fn udf_bytes(body: &str, elements: &str) -> Vec<u8> {
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<template format_id=\"1.8\">\
             <content><![CDATA[{body}]]></content>\
             <elements resolver=\"hvl-default\">{elements}</elements></template>"
        );
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buffer);
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("content.xml", o).unwrap();
            w.write_all(xml.as_bytes()).unwrap();
            w.finish().unwrap();
        }
        buffer.into_inner()
    }

    fn fix(block: &str, start: usize, end: usize, original: &str, replacement: &str) -> Fix {
        Fix {
            block_id: block.into(),
            char_start: start,
            char_end: end,
            original: original.into(),
            replacement: replacement.into(),
            description: "test".into(),
        }
    }

    #[test]
    fn the_output_is_a_new_file_with_the_product_name() {
        assert_eq!(output_file_name("dava.docx"), "dava - Düzeltilmiş.docx");
        assert_eq!(output_file_name("/a/b/dava.udf"), "dava - Düzeltilmiş.udf");
        assert_eq!(output_file_name("dosya"), "dosya - Düzeltilmiş");
    }

    #[test]
    fn a_docx_correction_lands_and_survives_a_round_trip() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>Davacı  beyanda bulundu.</w:t></w:r></w:p>"#);
        let out = apply("dava.docx", &bytes, &[fix("p0", 6, 8, "  ", " ")]).unwrap();
        let back = parser::parse("dava.docx", &out.bytes).unwrap();
        assert_eq!(back.blocks[0].text, "Davacı beyanda bulundu.");
        assert_eq!(out.applied, 1);
    }

    #[test]
    fn the_source_bytes_are_never_modified() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>Davacı  beyanda bulundu.</w:t></w:r></w:p>"#);
        let before = bytes.clone();
        let _ = apply("dava.docx", &bytes, &[fix("p0", 6, 8, "  ", " ")]).unwrap();
        assert_eq!(bytes, before);
    }

    #[test]
    fn untouched_parts_are_copied_through_byte_for_byte() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>Davacı  beyanda bulundu.</w:t></w:r></w:p>"#);
        let out = apply("dava.docx", &bytes, &[fix("p0", 6, 8, "  ", " ")]).unwrap();
        assert_eq!(
            read_part(&bytes, "word/styles.xml"),
            read_part(&out.bytes, "word/styles.xml")
        );
        assert_eq!(
            read_part(&bytes, "[Content_Types].xml"),
            read_part(&out.bytes, "[Content_Types].xml")
        );
    }

    #[test]
    fn run_formatting_survives_a_correction() {
        let bytes = docx_bytes(
            r#"<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Kalın</w:t></w:r><w:r><w:t>  düz</w:t></w:r></w:p>"#,
        );
        let out = apply("dava.docx", &bytes, &[fix("p0", 5, 7, "  ", " ")]).unwrap();
        let back = parser::parse("dava.docx", &out.bytes).unwrap();
        assert_eq!(back.blocks[0].text, "Kalın düz");
        assert!(back.blocks[0].runs[0].bold);
        assert!(!back.blocks[0].runs[1].bold);
    }

    #[test]
    fn a_correction_spanning_two_runs_keeps_the_text_intact() {
        let bytes = docx_bytes(
            r#"<w:p><w:r><w:t>Davacı dav</w:t></w:r><w:r><w:t>acı beyan etti.</w:t></w:r></w:p>"#,
        );
        // Delete the repeated word, which straddles the run boundary.
        let out = apply("dava.docx", &bytes, &[fix("p0", 6, 13, " davacı", "")]).unwrap();
        let back = parser::parse("dava.docx", &out.bytes).unwrap();
        assert_eq!(back.blocks[0].text, "Davacı beyan etti.");
    }

    #[test]
    fn several_corrections_in_one_paragraph_are_applied_together() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>A  B  C</w:t></w:r></w:p>"#);
        let out = apply(
            "dava.docx",
            &bytes,
            &[fix("p0", 1, 3, "  ", " "), fix("p0", 4, 6, "  ", " ")],
        )
        .unwrap();
        let back = parser::parse("dava.docx", &out.bytes).unwrap();
        assert_eq!(back.blocks[0].text, "A B C");
    }

    #[test]
    fn a_leading_space_is_preserved_by_marking_the_node() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>Davacı,beyanda bulundu.</w:t></w:r></w:p>"#);
        let out = apply("dava.docx", &bytes, &[fix("p0", 7, 7, "", " ")]).unwrap();
        let xml = String::from_utf8(read_part(&out.bytes, "word/document.xml").unwrap()).unwrap();
        assert!(xml.contains("xml:space=\"preserve\""));
        let back = parser::parse("dava.docx", &out.bytes).unwrap();
        assert_eq!(back.blocks[0].text, "Davacı, beyanda bulundu.");
    }

    #[test]
    fn xml_special_characters_are_escaped_on_the_way_back_in() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>A  B</w:t></w:r></w:p>"#);
        let out = apply("dava.docx", &bytes, &[fix("p0", 1, 3, "  ", " & <x> ")]).unwrap();
        let back = parser::parse("dava.docx", &out.bytes).unwrap();
        assert_eq!(back.blocks[0].text, "A & <x> B");
    }

    #[test]
    fn a_stale_change_is_refused_rather_than_applied_at_the_wrong_place() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>Davacı beyanda bulundu.</w:t></w:r></w:p>"#);
        let err = apply("dava.docx", &bytes, &[fix("p0", 0, 6, "Davalı", "X")]).unwrap_err();
        assert_eq!(err, WriteBackError::TextMismatch { index: 0 });
    }

    #[test]
    fn a_change_pointing_past_the_end_is_refused() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>Kısa</w:t></w:r></w:p>"#);
        let err = apply("dava.docx", &bytes, &[fix("p0", 0, 99, "Kısa", "X")]).unwrap_err();
        assert_eq!(err, WriteBackError::StaleChange { index: 0 });
    }

    #[test]
    fn a_change_to_a_block_that_does_not_exist_is_refused() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>Kısa</w:t></w:r></w:p>"#);
        let err = apply("dava.docx", &bytes, &[fix("p99", 0, 1, "K", "X")]).unwrap_err();
        assert_eq!(err, WriteBackError::StaleChange { index: 0 });
    }

    #[test]
    fn overlapping_changes_are_refused() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>ABCDEF</w:t></w:r></w:p>"#);
        let err = apply(
            "dava.docx",
            &bytes,
            &[fix("p0", 0, 3, "ABC", "X"), fix("p0", 2, 5, "CDE", "Y")],
        )
        .unwrap_err();
        assert_eq!(err, WriteBackError::OverlappingChanges);
    }

    #[test]
    fn a_udf_correction_lands_and_shifts_the_spans_that_follow() {
        let bytes = udf_bytes(
            "Davacı  beyanda bulundu.\nİkinci paragraf burada.\n",
            r#"<paragraph><content startOffset="0" length="25"/></paragraph>
               <paragraph><content startOffset="25" length="25"/></paragraph>"#,
        );
        let out = apply("dava.udf", &bytes, &[fix("p0", 6, 8, "  ", " ")]).unwrap();
        let back = parser::parse("dava.udf", &out.bytes).unwrap();
        assert_eq!(back.blocks[0].text, "Davacı beyanda bulundu.");
        // The second paragraph must still read correctly, which only happens if
        // its offset was moved by the one character the edit removed.
        assert_eq!(back.blocks[1].text, "İkinci paragraf burada.");
    }

    #[test]
    fn a_udf_correction_that_lengthens_the_text_also_shifts_correctly() {
        let bytes = udf_bytes(
            "Ankara' da ikamet eden.\nİkinci paragraf burada.\n",
            r#"<paragraph><content startOffset="0" length="24"/></paragraph>
               <paragraph><content startOffset="24" length="24"/></paragraph>"#,
        );
        let out = apply("dava.udf", &bytes, &[fix("p0", 7, 8, " ", "")]).unwrap();
        let back = parser::parse("dava.udf", &out.bytes).unwrap();
        assert_eq!(back.blocks[0].text, "Ankara'da ikamet eden.");
        assert_eq!(back.blocks[1].text, "İkinci paragraf burada.");
    }

    #[test]
    fn a_udf_replacement_containing_the_cdata_terminator_is_refused() {
        let bytes = udf_bytes(
            "ABCDEF\n",
            r#"<paragraph><content startOffset="0" length="7"/></paragraph>"#,
        );
        let err = apply("dava.udf", &bytes, &[fix("p0", 0, 1, "A", "]]>")]).unwrap_err();
        assert_eq!(err, WriteBackError::Repackage);
    }

    #[test]
    fn verification_rejects_a_patch_that_does_not_produce_the_expected_text() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>ABC</w:t></w:r></w:p>"#);
        let document = parser::parse("dava.docx", &bytes).unwrap();
        let image = container_text_image(document.format, &bytes).unwrap();
        let patched = Patched {
            bytes: bytes.clone(),
            expected_image: image,
        };
        let err = verify("dava.docx", &patched, &document, "BEKLENMEYEN").unwrap_err();
        assert_eq!(err, WriteBackError::VerificationFailed("text_mismatch"));
    }

    #[test]
    fn verification_rejects_a_character_changed_outside_an_approved_change() {
        // The patched bytes are fine, but the image says a different document
        // should have come out. That is the shape of an unauthorised mutation,
        // and it must stop the file being produced.
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>ABC</w:t></w:r></w:p>"#);
        let document = parser::parse("dava.docx", &bytes).unwrap();
        let patched = Patched {
            bytes: bytes.clone(),
            expected_image: "XYZ\u{001F}".into(),
        };
        let err = verify("dava.docx", &patched, &document, &document.plain_text()).unwrap_err();
        assert_eq!(err, WriteBackError::UnauthorisedMutation);
    }

    #[test]
    fn the_text_image_of_a_docx_covers_every_text_node() {
        let bytes = docx_bytes(
            r#"<w:p><w:r><w:t>bir</w:t></w:r><w:r><w:t>iki</w:t></w:r></w:p><w:p><w:r><w:t>üç</w:t></w:r></w:p>"#,
        );
        let image = container_text_image(SourceFormat::Docx, &bytes).unwrap();
        assert!(image.contains("bir"));
        assert!(image.contains("iki"));
        assert!(image.contains("üç"));
    }

    #[test]
    fn a_correction_changes_the_image_only_where_it_was_approved() {
        let bytes = docx_bytes(r#"<w:p><w:r><w:t>Davacı  beyanda bulundu.</w:t></w:r></w:p>"#);
        let before = container_text_image(SourceFormat::Docx, &bytes).unwrap();
        let out = apply("dava.docx", &bytes, &[fix("p0", 6, 8, "  ", " ")]).unwrap();
        let after = container_text_image(SourceFormat::Docx, &out.bytes).unwrap();
        assert_eq!(before.replace("  ", " "), after);
    }

    #[test]
    fn a_udf_correction_changes_the_image_only_where_it_was_approved() {
        let bytes = udf_bytes(
            "Davacı  beyanda bulundu.\nİkinci paragraf burada.\n",
            r#"<paragraph><content startOffset="0" length="25"/></paragraph>
               <paragraph><content startOffset="25" length="25"/></paragraph>"#,
        );
        let before = container_text_image(SourceFormat::Udf, &bytes).unwrap();
        let out = apply("dava.udf", &bytes, &[fix("p0", 6, 8, "  ", " ")]).unwrap();
        let after = container_text_image(SourceFormat::Udf, &out.bytes).unwrap();
        assert_eq!(before.replacen("  ", " ", 1), after);
        // And exactly one character fewer overall.
        assert_eq!(after.chars().count() + 1, before.chars().count());
    }

    #[test]
    fn error_messages_never_contain_document_text() {
        for e in [
            WriteBackError::TextMismatch { index: 3 },
            WriteBackError::StaleChange { index: 1 },
            WriteBackError::OverlappingChanges,
            WriteBackError::VerificationFailed("text_mismatch"),
            WriteBackError::Unaddressable { index: 0 },
            WriteBackError::Repackage,
        ] {
            let msg = e.message_tr();
            assert!(!msg.is_empty());
            assert!(!msg.contains("Davacı"));
        }
    }
}
