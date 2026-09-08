//! UDF parser (UYAP Belge Formatı).
//!
//! A UDF file is a ZIP holding `content.xml`. Unlike DOCX, the text is not
//! interleaved with the markup: the whole document body sits in one CDATA
//! block, and `<elements>` describes paragraphs and formatting as
//! `(startOffset, length)` spans into that blob.
//!
//! Two details matter and are handled explicitly:
//!
//! * Offsets are Java string indices, i.e. UTF-16 code units, not codepoints.
//!   Turkish letters are all in the BMP so the two usually agree, but a single
//!   astral character anywhere in the document would shift every later span.
//!   Offsets are therefore mapped through a UTF-16 index rather than assumed.
//! * The element tree is optional in practice. When it is missing or yields
//!   nothing, the blob is split on line breaks so the document still opens.

use super::archive::{self, decode_utf8};
use super::{block_from_runs, finalize_blocks, limits, ParseError};
use crate::cdm::{Alignment, Block, BlockKind, Document, ParagraphStyle, Run, SourceFormat};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::HashMap;

pub const CONTENT_PART: &str = "content.xml";

pub fn parse(bytes: &[u8]) -> Result<Document, ParseError> {
    let mut archive = archive::open(bytes)?;
    let xml_bytes = archive::read_optional(&mut archive, CONTENT_PART)
        .or_else(|| {
            archive::find_entry(&mut archive, |n| {
                let l = n.to_ascii_lowercase();
                l.ends_with("content.xml") || l.ends_with(".xml")
            })
            .map(|(_, b)| b)
        })
        .ok_or(ParseError::MissingPart(CONTENT_PART))?;
    let xml = decode_utf8(&xml_bytes);
    let blocks = parse_content(&xml)?;
    Ok(Document::new(SourceFormat::Udf, finalize_blocks(blocks)))
}

/// Maps Java (UTF-16) offsets onto codepoint offsets in the text blob.
pub struct OffsetMap {
    chars: Vec<char>,
    /// `utf16_prefix[i]` is the UTF-16 length of `chars[..i]`.
    utf16_prefix: Vec<usize>,
}

impl OffsetMap {
    pub fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let mut utf16_prefix = Vec::with_capacity(chars.len() + 1);
        let mut acc = 0usize;
        utf16_prefix.push(0);
        for c in &chars {
            acc += c.len_utf16();
            utf16_prefix.push(acc);
        }
        Self {
            chars,
            utf16_prefix,
        }
    }

    pub fn char_len(&self) -> usize {
        self.chars.len()
    }

    /// Codepoint index for a UTF-16 offset, clamped into range.
    pub fn to_char_index(&self, utf16_offset: usize) -> usize {
        match self.utf16_prefix.binary_search(&utf16_offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1).min(self.chars.len()),
        }
    }

    pub fn slice_utf16(&self, start: usize, length: usize) -> String {
        let a = self.to_char_index(start);
        let b = self.to_char_index(start + length);
        self.chars[a.min(self.chars.len())..b.min(self.chars.len())]
            .iter()
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
struct UdfFormat {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    family: Option<String>,
    size: Option<f32>,
    foreground: Option<String>,
}

impl UdfFormat {
    fn read(e: &BytesStart<'_>) -> Self {
        Self {
            bold: attr(e, b"bold").map(|v| v == "true"),
            italic: attr(e, b"italic").map(|v| v == "true"),
            underline: attr(e, b"underline").map(|v| v == "true"),
            family: attr(e, b"family"),
            size: attr(e, b"size").and_then(|v| v.parse::<f32>().ok()),
            foreground: attr(e, b"foreground"),
        }
    }

    /// Fill anything unset from `base`.
    fn under(mut self, base: &UdfFormat) -> Self {
        self.bold = self.bold.or(base.bold);
        self.italic = self.italic.or(base.italic);
        self.underline = self.underline.or(base.underline);
        self.family = self.family.or_else(|| base.family.clone());
        self.size = self.size.or(base.size);
        self.foreground = self.foreground.or_else(|| base.foreground.clone());
        self
    }
}

/// Java's `StyleConstants` alignment constants, as written by UYAP.
fn alignment_from(v: &str) -> Option<Alignment> {
    match v.trim() {
        "0" => Some(Alignment::Left),
        "1" => Some(Alignment::Center),
        "2" => Some(Alignment::Right),
        "3" => Some(Alignment::Justify),
        _ => None,
    }
}

pub fn parse_content(xml: &str) -> Result<Vec<Block>, ParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut body = String::new();
    let mut styles: HashMap<String, UdfFormat> = HashMap::new();
    // Spans are gathered first because `<content>` (the blob) and `<elements>`
    // may appear in either order across UYAP versions.
    let mut paragraphs: Vec<PendingParagraph> = Vec::new();
    let mut current: Option<PendingParagraph> = None;

    let mut depth = 0usize;
    let mut in_body_content = false;
    let mut in_elements = false;
    let mut table_depth = 0usize;

    loop {
        match reader.read_event() {
            Err(_) => return Err(ParseError::MalformedXml),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                depth += 1;
                if depth > limits::MAX_XML_DEPTH {
                    return Err(ParseError::ResourceLimit("xml_depth"));
                }
                match local_name(e.name().as_ref()) {
                    b"content" if !in_elements => in_body_content = true,
                    b"content" if in_elements => {
                        if let Some(p) = current.as_mut() {
                            p.spans.push(PendingSpan::read(&e, &styles));
                        }
                    }
                    b"elements" => in_elements = true,
                    b"table" => table_depth += 1,
                    b"paragraph" => {
                        current = Some(PendingParagraph::read(&e, table_depth > 0));
                    }
                    b"style" => {
                        if let Some(name) = attr(&e, b"name") {
                            styles.insert(name, UdfFormat::read(&e));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => match local_name(e.name().as_ref()) {
                b"content" if in_elements => {
                    if let Some(p) = current.as_mut() {
                        p.spans.push(PendingSpan::read(&e, &styles));
                    }
                }
                b"style" => {
                    if let Some(name) = attr(&e, b"name") {
                        styles.insert(name, UdfFormat::read(&e));
                    }
                }
                b"paragraph" => {
                    // A self-closing paragraph is an empty one; keep it, since
                    // a blank numbered item is itself a finding.
                    paragraphs.push(PendingParagraph::read(&e, table_depth > 0));
                }
                _ => {}
            },
            Ok(Event::CData(c)) => {
                if in_body_content {
                    body.push_str(&String::from_utf8_lossy(c.as_ref()));
                }
            }
            Ok(Event::Text(t)) => {
                if in_body_content {
                    if let Ok(s) = t.unescape() {
                        body.push_str(&s);
                    }
                }
            }
            Ok(Event::End(e)) => {
                depth = depth.saturating_sub(1);
                match local_name(e.name().as_ref()) {
                    b"content" if in_body_content => in_body_content = false,
                    b"elements" => in_elements = false,
                    b"table" => table_depth = table_depth.saturating_sub(1),
                    b"paragraph" => {
                        if let Some(p) = current.take() {
                            paragraphs.push(p);
                            if paragraphs.len() > limits::MAX_BLOCKS {
                                return Err(ParseError::ResourceLimit("block_count"));
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if body.trim().is_empty() {
        return Err(ParseError::EmptyDocument);
    }

    let map = OffsetMap::new(&body);
    let blocks = build_blocks(&map, &paragraphs);
    if blocks.iter().all(|b| b.text.trim().is_empty()) {
        // The element tree told us nothing usable. Fall back to line splitting
        // so a document from an unfamiliar UDF revision still opens.
        return Ok(fallback_blocks(&body));
    }
    Ok(blocks)
}

#[derive(Debug, Clone)]
struct PendingSpan {
    start: usize,
    length: usize,
    fmt: UdfFormat,
}

impl PendingSpan {
    fn read(e: &BytesStart<'_>, styles: &HashMap<String, UdfFormat>) -> Self {
        let base = attr(e, b"style")
            .and_then(|s| styles.get(&s).cloned())
            .or_else(|| styles.get("default").cloned())
            .unwrap_or_default();
        Self {
            start: attr(e, b"startOffset")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            length: attr(e, b"length").and_then(|v| v.parse().ok()).unwrap_or(0),
            fmt: UdfFormat::read(e).under(&base),
        }
    }
}

#[derive(Debug, Clone)]
struct PendingParagraph {
    alignment: Option<Alignment>,
    left_indent: Option<f32>,
    right_indent: Option<f32>,
    first_line_indent: Option<f32>,
    space_above: Option<f32>,
    space_below: Option<f32>,
    line_spacing: Option<f32>,
    in_table: bool,
    spans: Vec<PendingSpan>,
}

impl PendingParagraph {
    fn read(e: &BytesStart<'_>, in_table: bool) -> Self {
        let f = |k: &[u8]| attr(e, k).and_then(|v| v.parse::<f32>().ok());
        Self {
            alignment: attr(e, b"Alignment").as_deref().and_then(alignment_from),
            left_indent: f(b"LeftIndent"),
            right_indent: f(b"RightIndent"),
            first_line_indent: f(b"FirstLineIndent"),
            space_above: f(b"SpaceAbove"),
            space_below: f(b"SpaceBelow"),
            line_spacing: f(b"LineSpacing").map(|v| if v <= 0.0 { 1.0 } else { v }),
            in_table,
            spans: Vec::new(),
        }
    }
}

/// Turn one span into a run.
fn span_run(map: &OffsetMap, start: usize, length: usize, fmt: &UdfFormat) -> Option<Run> {
    let text = map.slice_utf16(start, length);
    if text.is_empty() {
        return None;
    }
    Some(Run {
        text,
        bold: fmt.bold.unwrap_or(false),
        italic: fmt.italic.unwrap_or(false),
        underline: fmt.underline.unwrap_or(false),
        font_family: fmt.family.clone(),
        font_size: fmt.size,
        color: fmt.foreground.clone(),
        char_start: 0,
        container_path: Some(format!("udf:off:{start}:{length}")),
    })
}

fn build_blocks(map: &OffsetMap, paragraphs: &[PendingParagraph]) -> Vec<Block> {
    let mut out = Vec::with_capacity(paragraphs.len());
    for p in paragraphs {
        let mut spans: Vec<&PendingSpan> = p.spans.iter().filter(|s| s.length > 0).collect();
        spans.sort_by_key(|s| (s.start, s.length));

        let mut runs: Vec<Run> = Vec::with_capacity(spans.len() + 1);
        let mut cursor = spans.first().map(|s| s.start).unwrap_or(0);

        for (i, s) in spans.iter().enumerate() {
            // A paragraph's spans are not always contiguous. UYAP writes a span
            // where formatting is declared and leaves the rest of the paragraph
            // uncovered, so the characters in a gap belong to the paragraph just
            // as much as the ones inside a span.
            //
            // Dropping them is not a cosmetic loss. The model then disagrees
            // with the document — a missing space appears to be missing — and
            // every offset after the gap maps back to the wrong place, which is
            // how a correction can land next to the character it was meant to
            // replace. The gap is therefore materialised as its own run, with
            // its own address, so it is both visible and editable.
            if s.start > cursor {
                let inherit = if i > 0 { &spans[i - 1].fmt } else { &s.fmt };
                if let Some(run) = span_run(map, cursor, s.start - cursor, inherit) {
                    runs.push(run);
                }
            }
            let begin = s.start.max(cursor);
            let end = s.start + s.length;
            if end <= begin {
                continue; // wholly covered by a span already emitted
            }
            if let Some(run) = span_run(map, begin, end - begin, &s.fmt) {
                runs.push(run);
            }
            cursor = end;
        }

        // UDF keeps the paragraph break inside the span, so trim it off the
        // block text: line breaks are structure, not content.
        if let Some(last) = runs.last_mut() {
            while last.text.ends_with('\n') || last.text.ends_with('\r') {
                last.text.pop();
            }
        }
        runs.retain(|r| !r.text.is_empty());
        let style = ParagraphStyle {
            alignment: p.alignment,
            indent_left: p.left_indent,
            indent_right: p.right_indent,
            first_line_indent: p.first_line_indent,
            spacing_before: p.space_above,
            spacing_after: p.space_below,
            line_spacing: p.line_spacing,
            style_name: None,
        };
        let kind = if p.in_table {
            BlockKind::TableCell
        } else {
            BlockKind::Paragraph
        };
        out.push(block_from_runs(kind, runs, style, None));
    }
    out
}

/// Split the raw blob into paragraphs when the element tree is unusable.
fn fallback_blocks(body: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in body.split('\n') {
        let text = line.trim_end_matches('\r').to_string();
        let utf16_len: usize = text.chars().map(|c| c.len_utf16()).sum();
        if !text.trim().is_empty() {
            let run = Run {
                text: text.clone(),
                char_start: 0,
                container_path: Some(format!("udf:off:{offset}:{utf16_len}")),
                ..Default::default()
            };
            out.push(block_from_runs(
                BlockKind::Paragraph,
                vec![run],
                ParagraphStyle::default(),
                None,
            ));
        }
        offset += utf16_len + 1; // the newline itself
    }
    finalize_blocks(out)
}

fn local_name(qname: &[u8]) -> &[u8] {
    match qname.iter().position(|&b| b == b':') {
        Some(i) => &qname[i + 1..],
        None => qname,
    }
}

fn attr(e: &BytesStart<'_>, local: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() {
        if local_name(a.key.as_ref()) == local {
            return Some(String::from_utf8_lossy(&a.value).into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(body: &str, elements: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<template format_id=\"1.8\">\
             <content><![CDATA[{body}]]></content>\
             <elements resolver=\"hvl-default\">{elements}</elements></template>"
        )
    }

    #[test]
    fn spans_are_sliced_out_of_the_content_blob() {
        let xml = content(
            "Sayın Mahkeme\nDavacı beyanı\n",
            r#"<paragraph Alignment="3"><content startOffset="0" length="14"/></paragraph>
               <paragraph Alignment="0"><content startOffset="14" length="14"/></paragraph>"#,
        );
        let b = finalize_blocks(parse_content(&xml).unwrap());
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].text, "Sayın Mahkeme");
        assert_eq!(b[1].text, "Davacı beyanı");
        assert_eq!(b[0].style.alignment, Some(Alignment::Justify));
    }

    #[test]
    fn run_formatting_is_read_from_span_attributes() {
        let xml = content(
            "KALIN düz\n",
            r#"<paragraph><content startOffset="0" length="6" bold="true" family="Times New Roman" size="12"/><content startOffset="6" length="4"/></paragraph>"#,
        );
        let b = finalize_blocks(parse_content(&xml).unwrap());
        assert_eq!(b[0].runs.len(), 2);
        assert!(b[0].runs[0].bold);
        assert!(!b[0].runs[1].bold);
        assert_eq!(b[0].runs[0].font_size, Some(12.0));
    }

    #[test]
    fn a_named_style_supplies_defaults_for_spans_that_reference_it() {
        let xml = "<template><content><![CDATA[metin\n]]></content>\
             <styles><style name=\"default\" family=\"Arial\" size=\"11\"/></styles>\
             <elements><paragraph><content startOffset=\"0\" length=\"6\" style=\"default\"/></paragraph></elements></template>";
        let b = finalize_blocks(parse_content(xml).unwrap());
        assert_eq!(b[0].runs[0].font_family.as_deref(), Some("Arial"));
        assert_eq!(b[0].runs[0].font_size, Some(11.0));
    }

    #[test]
    fn offsets_are_utf16_based_so_astral_characters_do_not_shift_spans() {
        // The emoji is two UTF-16 units but one codepoint. Reading the offset
        // as a codepoint index would misalign the second span by one.
        let body = "a\u{1F600}bc\n";
        let map = OffsetMap::new(body);
        assert_eq!(map.slice_utf16(0, 1), "a");
        assert_eq!(map.slice_utf16(3, 2), "bc");
    }

    #[test]
    fn characters_between_two_spans_are_not_dropped() {
        // UYAP leaves the space between two formatted stretches uncovered. If
        // the parser skips it the model reads "kayıtlar,müvekkilin" and a
        // missing space is reported that the document does not have.
        let xml = content(
            "gösterir kayıtlar, müvekkilin evi\n",
            r#"<paragraph><content startOffset="0" length="18"/><content startOffset="19" length="15"/></paragraph>"#,
        );
        let b = finalize_blocks(parse_content(&xml).unwrap());
        assert_eq!(b[0].text, "gösterir kayıtlar, müvekkilin evi");
    }

    #[test]
    fn a_gap_run_carries_its_own_address_so_it_can_be_edited() {
        let xml = content(
            "gösterir kayıtlar, müvekkilin evi\n",
            r#"<paragraph><content startOffset="0" length="18"/><content startOffset="19" length="15"/></paragraph>"#,
        );
        let b = finalize_blocks(parse_content(&xml).unwrap());
        let paths: Vec<&str> = b[0]
            .runs
            .iter()
            .filter_map(|r| r.container_path.as_deref())
            .collect();
        assert!(
            paths.contains(&"udf:off:18:1"),
            "gap run missing: {paths:?}"
        );
    }

    #[test]
    fn overlapping_spans_do_not_duplicate_text() {
        let xml = content(
            "abcdefghij\n",
            r#"<paragraph><content startOffset="0" length="6"/><content startOffset="3" length="8"/></paragraph>"#,
        );
        let b = finalize_blocks(parse_content(&xml).unwrap());
        assert_eq!(b[0].text, "abcdefghij");
    }

    #[test]
    fn out_of_range_offsets_are_clamped_not_panicked_on() {
        let xml = content(
            "kısa\n",
            r#"<paragraph><content startOffset="9000" length="9000"/></paragraph>"#,
        );
        let r = parse_content(&xml);
        assert!(r.is_ok());
    }

    #[test]
    fn a_document_without_a_usable_element_tree_still_opens() {
        let xml = content("Birinci satır\nİkinci satır\n", "");
        let b = parse_content(&xml).unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].text, "Birinci satır");
    }

    #[test]
    fn table_paragraphs_are_marked_as_cells() {
        let xml = content(
            "Hücre\n",
            r#"<table><row><cell><paragraph><content startOffset="0" length="6"/></paragraph></cell></row></table>"#,
        );
        let b = finalize_blocks(parse_content(&xml).unwrap());
        assert_eq!(b[0].kind, BlockKind::TableCell);
    }

    #[test]
    fn spans_carry_a_container_path_for_write_back() {
        let xml = content(
            "metin\n",
            r#"<paragraph><content startOffset="0" length="6"/></paragraph>"#,
        );
        let b = finalize_blocks(parse_content(&xml).unwrap());
        assert_eq!(b[0].runs[0].container_path.as_deref(), Some("udf:off:0:6"));
    }

    #[test]
    fn an_empty_body_is_reported_rather_than_producing_a_blank_document() {
        let xml = content("   ", "");
        assert_eq!(parse_content(&xml).unwrap_err(), ParseError::EmptyDocument);
    }

    #[test]
    fn malformed_xml_is_an_error() {
        assert!(parse_content("<template><content><![CDATA[x").is_err());
    }
}
