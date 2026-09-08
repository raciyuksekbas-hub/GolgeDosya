//! DOCX parser.
//!
//! Plain-text extraction would be useless here: half the rules are about
//! formatting, so runs, paragraph properties and style inheritance all have to
//! survive into the model. The parser walks `word/document.xml` as a stream and
//! resolves each paragraph against `word/styles.xml`.
//!
//! Every text node is addressed by its ordinal in document order
//! (`docx:t:<n>`). Write-back re-walks the same file and patches the nth node,
//! which keeps the rest of the package byte-identical.

use super::archive::{self, decode_utf8};
use super::{block_from_runs, finalize_blocks, limits, ParseError};
use crate::cdm::{Alignment, BlockKind, Document, ParagraphStyle, Run, SourceFormat};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::HashMap;

pub const DOCUMENT_PART: &str = "word/document.xml";
const STYLES_PART: &str = "word/styles.xml";

/// Key under which `w:docDefaults` is stored in the style table.
///
/// A style id cannot contain a NUL, so this cannot collide with a real style.
/// The document defaults are the bottom of the inheritance chain: most Word
/// documents set the body font there and nowhere else, so a parser that skips
/// them reports every paragraph as having no font at all.
const DOC_DEFAULTS: &str = "\u{0}docDefaults";

pub fn parse(bytes: &[u8]) -> Result<Document, ParseError> {
    let mut archive = archive::open(bytes)?;
    let document = archive::read_entry(&mut archive, DOCUMENT_PART)
        .map_err(|_| ParseError::MissingPart(DOCUMENT_PART))?;
    let styles = archive::read_optional(&mut archive, STYLES_PART)
        .map(|b| parse_styles(&decode_utf8(&b)))
        .unwrap_or_default();
    let blocks = parse_document(&decode_utf8(&document), &styles)?;
    Ok(Document::new(SourceFormat::Docx, finalize_blocks(blocks)))
}

/// Formatting a named style contributes, before run-level overrides.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleDef {
    pub name: Option<String>,
    pub based_on: Option<String>,
    pub outline_level: Option<u8>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub alignment: Option<Alignment>,
    pub line_spacing: Option<f32>,
    pub spacing_before: Option<f32>,
    pub spacing_after: Option<f32>,
    pub indent_left: Option<f32>,
    pub indent_right: Option<f32>,
    pub first_line_indent: Option<f32>,
}

pub type StyleTable = HashMap<String, StyleDef>;

/// Resolve a style through its `basedOn` chain, nearest definition winning.
fn resolve(styles: &StyleTable, id: &str) -> StyleDef {
    let mut chain = Vec::new();
    let mut current = Some(id.to_string());
    let mut guard = 0;
    while let Some(cid) = current {
        if guard > 16 {
            break; // cyclic basedOn; stop rather than spin
        }
        guard += 1;
        match styles.get(&cid) {
            Some(s) => {
                chain.push(s.clone());
                current = s.based_on.clone();
            }
            None => break,
        }
    }
    let mut out = StyleDef::default();
    // Walk from the furthest ancestor inwards so the nearest wins.
    for s in chain.iter().rev() {
        macro_rules! take {
            ($f:ident) => {
                if s.$f.is_some() {
                    out.$f = s.$f.clone();
                }
            };
        }
        take!(name);
        take!(outline_level);
        take!(bold);
        take!(italic);
        take!(underline);
        take!(font_family);
        take!(font_size);
        take!(alignment);
        take!(line_spacing);
        take!(spacing_before);
        take!(spacing_after);
        take!(indent_left);
        take!(indent_right);
        take!(first_line_indent);
    }
    out
}

pub fn parse_styles(xml: &str) -> StyleTable {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut table = StyleTable::new();
    let mut current: Option<(String, StyleDef)> = None;
    let mut doc_defaults = StyleDef::default();
    let mut in_doc_defaults = false;
    let mut depth = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Eof) | Err(_) => break,
            Ok(Event::Start(e)) => {
                depth += 1;
                if depth > limits::MAX_XML_DEPTH {
                    break;
                }
                let name = local_name(e.name().as_ref()).to_vec();
                if name.as_slice() == b"docDefaults" {
                    in_doc_defaults = true;
                    continue;
                }
                if in_doc_defaults {
                    apply_format_tag(&name, &e, &mut doc_defaults);
                } else {
                    handle_style_tag(&e, &mut current);
                }
            }
            Ok(Event::Empty(e)) => {
                let name = local_name(e.name().as_ref()).to_vec();
                if in_doc_defaults {
                    apply_format_tag(&name, &e, &mut doc_defaults);
                } else {
                    handle_style_tag(&e, &mut current);
                }
            }
            Ok(Event::End(e)) => {
                depth = depth.saturating_sub(1);
                match local_name(e.name().as_ref()) {
                    b"docDefaults" => in_doc_defaults = false,
                    b"style" => {
                        if let Some((id, def)) = current.take() {
                            table.insert(id, def);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if let Some((id, def)) = current.take() {
        table.insert(id, def);
    }
    if doc_defaults != StyleDef::default() {
        table.insert(DOC_DEFAULTS.to_string(), doc_defaults);
    }
    table
}

fn handle_style_tag(e: &BytesStart<'_>, current: &mut Option<(String, StyleDef)>) {
    let name = local_name(e.name().as_ref()).to_vec();
    match name.as_slice() {
        b"style" => {
            let id = attr(e, b"styleId").unwrap_or_default();
            if !id.is_empty() {
                *current = Some((id, StyleDef::default()));
            }
        }
        _ => {
            let Some((_, def)) = current.as_mut() else {
                return;
            };
            apply_format_tag(&name, e, def);
            if name.as_slice() == b"name" {
                def.name = attr(e, b"val");
            } else if name.as_slice() == b"basedOn" {
                def.based_on = attr(e, b"val");
            } else if name.as_slice() == b"outlineLvl" {
                def.outline_level = attr(e, b"val").and_then(|v| v.parse::<u8>().ok());
            }
        }
    }
}

/// Shared handling for the formatting tags that appear identically in
/// `styles.xml`, in `w:pPr` and in `w:rPr`.
fn apply_format_tag(name: &[u8], e: &BytesStart<'_>, def: &mut StyleDef) {
    match name {
        b"b" => def.bold = Some(on_off(e)),
        b"i" => def.italic = Some(on_off(e)),
        b"u" => {
            let v = attr(e, b"val").unwrap_or_else(|| "single".into());
            def.underline = Some(v != "none");
        }
        b"rFonts" => {
            def.font_family = attr(e, b"ascii")
                .or_else(|| attr(e, b"hAnsi"))
                .or_else(|| attr(e, b"cs"));
        }
        b"sz" => {
            // DOCX stores half-points.
            def.font_size = attr(e, b"val")
                .and_then(|v| v.parse::<f32>().ok())
                .map(|v| v / 2.0);
        }
        b"jc" => {
            def.alignment = attr(e, b"val").and_then(|v| match v.as_str() {
                "left" | "start" => Some(Alignment::Left),
                "center" => Some(Alignment::Center),
                "right" | "end" => Some(Alignment::Right),
                "both" | "distribute" => Some(Alignment::Justify),
                _ => None,
            });
        }
        b"spacing" => {
            // Twentieths of a point.
            def.spacing_before = attr(e, b"before")
                .and_then(|v| v.parse::<f32>().ok())
                .map(twips);
            def.spacing_after = attr(e, b"after")
                .and_then(|v| v.parse::<f32>().ok())
                .map(twips);
            if let Some(line) = attr(e, b"line").and_then(|v| v.parse::<f32>().ok()) {
                let rule = attr(e, b"lineRule").unwrap_or_else(|| "auto".into());
                def.line_spacing = Some(if rule == "auto" {
                    // 240 twips = one line.
                    line / 240.0
                } else {
                    // Exact/atLeast are absolute; express against a 12pt line.
                    twips(line) / 12.0
                });
            }
        }
        b"ind" => {
            def.indent_left = attr(e, b"left")
                .or_else(|| attr(e, b"start"))
                .and_then(|v| v.parse::<f32>().ok())
                .map(twips);
            def.indent_right = attr(e, b"right")
                .or_else(|| attr(e, b"end"))
                .and_then(|v| v.parse::<f32>().ok())
                .map(twips);
            def.first_line_indent = attr(e, b"firstLine")
                .and_then(|v| v.parse::<f32>().ok())
                .map(twips)
                .or_else(|| {
                    attr(e, b"hanging")
                        .and_then(|v| v.parse::<f32>().ok())
                        .map(|v| -twips(v))
                });
        }
        _ => {}
    }
}

fn twips(v: f32) -> f32 {
    v / 20.0
}

/// `<w:b/>`, `<w:b w:val="1"/>` and `<w:b w:val="0"/>` all occur in the wild.
fn on_off(e: &BytesStart<'_>) -> bool {
    match attr(e, b"val") {
        None => true,
        Some(v) => !matches!(v.as_str(), "0" | "false" | "off"),
    }
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

/// Parser state for one paragraph being assembled.
#[derive(Default)]
struct ParaState {
    style_id: Option<String>,
    props: StyleDef,
    runs: Vec<Run>,
    /// Formatting of the run currently open.
    run_fmt: StyleDef,
    in_run_props: bool,
    in_para_props: bool,
    numbered: bool,
}

pub fn parse_document(xml: &str, styles: &StyleTable) -> Result<Vec<Block>, ParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut blocks: Vec<Block> = Vec::new();
    let mut para = ParaState::default();
    let mut in_paragraph = false;
    let mut table_depth = 0usize;
    // Text nodes are counted across the whole part so write-back can address
    // one by ordinal without re-deriving the structure.
    let mut text_ordinal = 0usize;
    let mut depth = 0usize;
    // `w:del` wraps text removed by a tracked change: it is not in the document.
    let mut suppress_depth = 0usize;
    let mut preserve_space = false;
    let mut in_text = false;

    loop {
        let event = reader.read_event();
        match event {
            Err(_) => return Err(ParseError::MalformedXml),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                depth += 1;
                if depth > limits::MAX_XML_DEPTH {
                    return Err(ParseError::ResourceLimit("xml_depth"));
                }
                let name = local_name(e.name().as_ref()).to_vec();
                if suppress_depth > 0 {
                    continue;
                }
                match name.as_slice() {
                    b"del" | b"instrText" | b"fldChar" => suppress_depth = depth,
                    b"tbl" => table_depth += 1,
                    b"p" => {
                        in_paragraph = true;
                        para = ParaState::default();
                    }
                    b"pPr" => para.in_para_props = true,
                    b"rPr" => para.in_run_props = true,
                    b"r" => {
                        para.run_fmt = StyleDef::default();
                    }
                    b"t" => {
                        in_text = true;
                        preserve_space = attr(&e, b"space").as_deref() == Some("preserve");
                        text_ordinal += 1;
                    }
                    b"pStyle" => para.style_id = attr(&e, b"val"),
                    b"numPr" => para.numbered = true,
                    _ => {
                        if para.in_run_props {
                            apply_format_tag(&name, &e, &mut para.run_fmt);
                        } else if para.in_para_props {
                            apply_format_tag(&name, &e, &mut para.props);
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                if suppress_depth > 0 {
                    continue;
                }
                let name = local_name(e.name().as_ref()).to_vec();
                match name.as_slice() {
                    b"pStyle" => para.style_id = attr(&e, b"val"),
                    b"numPr" => para.numbered = true,
                    // A tab or a break is structure, not a text node. It gets no
                    // container address, so write-back can never try to edit it.
                    b"tab" => {
                        if in_paragraph {
                            push_synthetic(&mut para, "\t");
                        }
                    }
                    b"br" | b"cr" => {
                        if in_paragraph {
                            push_synthetic(&mut para, " ");
                        }
                    }
                    _ => {
                        if para.in_run_props {
                            apply_format_tag(&name, &e, &mut para.run_fmt);
                        } else if para.in_para_props {
                            apply_format_tag(&name, &e, &mut para.props);
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if suppress_depth > 0 || !in_text || !in_paragraph {
                    continue;
                }
                // `xml:space` governs how Word renders the node, not how it is
                // stored, so the text is taken exactly as written either way.
                let _ = preserve_space;
                let s = t
                    .unescape()
                    .map_err(|_| ParseError::MalformedXml)?
                    .into_owned();
                if !s.is_empty() {
                    push_text(&mut para, &s, text_ordinal);
                }
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().as_ref()).to_vec();
                if suppress_depth > 0 && depth <= suppress_depth {
                    suppress_depth = 0;
                    depth = depth.saturating_sub(1);
                    continue;
                }
                depth = depth.saturating_sub(1);
                if suppress_depth > 0 {
                    continue;
                }
                match name.as_slice() {
                    b"tbl" => table_depth = table_depth.saturating_sub(1),
                    b"t" => in_text = false,
                    b"pPr" => para.in_para_props = false,
                    b"rPr" => para.in_run_props = false,
                    b"p" => {
                        if in_paragraph {
                            let state = std::mem::take(&mut para);
                            blocks.push(finish_paragraph(state, styles, table_depth > 0));
                            if blocks.len() > limits::MAX_BLOCKS {
                                return Err(ParseError::ResourceLimit("block_count"));
                            }
                        }
                        in_paragraph = false;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(blocks)
}

/// Append text that has no backing text node in the container.
fn push_synthetic(para: &mut ParaState, s: &str) {
    let f = &para.run_fmt;
    para.runs.push(Run {
        text: s.to_string(),
        bold: f.bold.unwrap_or(false),
        italic: f.italic.unwrap_or(false),
        underline: f.underline.unwrap_or(false),
        font_family: f.font_family.clone(),
        font_size: f.font_size,
        color: None,
        char_start: 0,
        container_path: None,
    });
}

fn push_text(para: &mut ParaState, s: &str, ordinal: usize) {
    let f = &para.run_fmt;
    let path = format!("docx:t:{ordinal}");
    // Merge into the previous run when it is the same text node and formatting,
    // so a `<w:t>` split across events stays one run.
    if let Some(last) = para.runs.last_mut() {
        if last.container_path.as_deref() == Some(path.as_str()) {
            last.text.push_str(s);
            return;
        }
    }
    para.runs.push(Run {
        text: s.to_string(),
        bold: f.bold.unwrap_or(false),
        italic: f.italic.unwrap_or(false),
        underline: f.underline.unwrap_or(false),
        font_family: f.font_family.clone(),
        font_size: f.font_size,
        color: None,
        char_start: 0,
        container_path: Some(path),
    });
}

use crate::cdm::Block;

fn finish_paragraph(mut state: ParaState, styles: &StyleTable, in_table: bool) -> Block {
    let mut resolved = state
        .style_id
        .as_ref()
        .map(|id| resolve(styles, id))
        .unwrap_or_default();
    // The document defaults sit below every named style.
    if let Some(defaults) = styles.get(DOC_DEFAULTS) {
        macro_rules! fill {
            ($f:ident) => {
                if resolved.$f.is_none() {
                    resolved.$f = defaults.$f.clone();
                }
            };
        }
        fill!(bold);
        fill!(italic);
        fill!(underline);
        fill!(font_family);
        fill!(font_size);
        fill!(alignment);
        fill!(line_spacing);
        fill!(spacing_before);
        fill!(spacing_after);
        fill!(indent_left);
        fill!(indent_right);
        fill!(first_line_indent);
    }

    // Style provides the baseline; direct paragraph properties override it.
    let merged = |direct: Option<f32>, from_style: Option<f32>| direct.or(from_style);
    let style = ParagraphStyle {
        alignment: state.props.alignment.or(resolved.alignment),
        indent_left: merged(state.props.indent_left, resolved.indent_left),
        indent_right: merged(state.props.indent_right, resolved.indent_right),
        first_line_indent: merged(state.props.first_line_indent, resolved.first_line_indent),
        spacing_before: merged(state.props.spacing_before, resolved.spacing_before),
        spacing_after: merged(state.props.spacing_after, resolved.spacing_after),
        line_spacing: merged(state.props.line_spacing, resolved.line_spacing),
        style_name: resolved.name.clone().or_else(|| state.style_id.clone()),
    };

    // Run formatting falls back to the paragraph style's character formatting.
    for r in state.runs.iter_mut() {
        if r.font_family.is_none() {
            r.font_family = resolved.font_family.clone();
        }
        if r.font_size.is_none() {
            r.font_size = resolved.font_size;
        }
        if !r.bold {
            r.bold = resolved.bold.unwrap_or(false);
        }
        if !r.italic {
            r.italic = resolved.italic.unwrap_or(false);
        }
        if !r.underline {
            r.underline = resolved.underline.unwrap_or(false);
        }
    }

    let is_heading = resolved.outline_level.is_some()
        || resolved
            .name
            .as_deref()
            .map(|n| {
                let l = n.to_ascii_lowercase();
                l.starts_with("heading") || l.starts_with("başlık") || l.starts_with("baslik")
            })
            .unwrap_or(false);

    let kind = if in_table {
        BlockKind::TableCell
    } else if is_heading {
        BlockKind::Heading
    } else if state.numbered {
        BlockKind::ListItem
    } else {
        BlockKind::Paragraph
    };

    block_from_runs(kind, state.runs, style, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

    fn doc(body: &str) -> Vec<Block> {
        let xml = format!("<w:document {NS}><w:body>{body}</w:body></w:document>");
        finalize_blocks(parse_document(&xml, &StyleTable::new()).unwrap())
    }

    #[test]
    fn a_paragraph_becomes_one_block_with_concatenated_runs() {
        let b = doc(r#"<w:p><w:r><w:t>Sayın </w:t></w:r><w:r><w:t>Mahkeme</w:t></w:r></w:p>"#);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, "Sayın Mahkeme");
        assert_eq!(b[0].runs.len(), 2);
        assert_eq!(b[0].runs[1].char_start, 6);
    }

    #[test]
    fn run_formatting_is_captured() {
        let b = doc(
            r#"<w:p><w:r><w:rPr><w:b/><w:sz w:val="28"/><w:rFonts w:ascii="Times New Roman"/></w:rPr><w:t>Kalın</w:t></w:r></w:p>"#,
        );
        let r = &b[0].runs[0];
        assert!(r.bold);
        assert_eq!(r.font_size, Some(14.0));
        assert_eq!(r.font_family.as_deref(), Some("Times New Roman"));
    }

    #[test]
    fn bold_val_zero_turns_bold_off() {
        let b = doc(r#"<w:p><w:r><w:rPr><w:b w:val="0"/></w:rPr><w:t>Düz</w:t></w:r></w:p>"#);
        assert!(!b[0].runs[0].bold);
    }

    #[test]
    fn paragraph_properties_are_read_in_points() {
        let b = doc(
            r#"<w:p><w:pPr><w:jc w:val="both"/><w:ind w:left="720" w:firstLine="360"/><w:spacing w:before="240" w:after="120" w:line="360" w:lineRule="auto"/></w:pPr><w:r><w:t>Metin</w:t></w:r></w:p>"#,
        );
        let s = &b[0].style;
        assert_eq!(s.alignment, Some(Alignment::Justify));
        assert_eq!(s.indent_left, Some(36.0));
        assert_eq!(s.first_line_indent, Some(18.0));
        assert_eq!(s.spacing_before, Some(12.0));
        assert_eq!(s.spacing_after, Some(6.0));
        assert_eq!(s.line_spacing, Some(1.5));
    }

    #[test]
    fn tracked_deletions_are_not_part_of_the_document() {
        let b = doc(
            r#"<w:p><w:r><w:t>Kalan</w:t></w:r><w:del><w:r><w:delText>Silinen</w:delText></w:r></w:del></w:p>"#,
        );
        assert_eq!(b[0].text, "Kalan");
    }

    #[test]
    fn preserved_space_survives_the_round_trip() {
        let b = doc(
            r#"<w:p><w:r><w:t xml:space="preserve">a </w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"#,
        );
        assert_eq!(b[0].text, "a b");
    }

    #[test]
    fn a_tab_gets_no_container_address_so_it_is_never_edited() {
        let b = doc(r#"<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t></w:r></w:p>"#);
        let synthetic: Vec<_> = b[0]
            .runs
            .iter()
            .filter(|r| r.container_path.is_none())
            .collect();
        assert_eq!(synthetic.len(), 1);
        assert_eq!(synthetic[0].text, "\t");
    }

    #[test]
    fn text_nodes_are_addressed_by_ordinal_for_write_back() {
        let b = doc(r#"<w:p><w:r><w:t>bir</w:t></w:r></w:p><w:p><w:r><w:t>iki</w:t></w:r></w:p>"#);
        assert_eq!(b[0].runs[0].container_path.as_deref(), Some("docx:t:1"));
        assert_eq!(b[1].runs[0].container_path.as_deref(), Some("docx:t:2"));
    }

    #[test]
    fn table_cells_are_marked_and_never_become_headings() {
        let b = doc(
            r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>A. BAŞLIK</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        );
        assert_eq!(b[0].kind, BlockKind::TableCell);
    }

    #[test]
    fn a_named_heading_style_marks_the_block_as_a_heading() {
        let mut styles = StyleTable::new();
        styles.insert(
            "Heading1".into(),
            StyleDef {
                name: Some("heading 1".into()),
                outline_level: Some(0),
                ..Default::default()
            },
        );
        let xml = format!(
            "<w:document {NS}><w:body><w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Giriş</w:t></w:r></w:p></w:body></w:document>"
        );
        let b = finalize_blocks(parse_document(&xml, &styles).unwrap());
        assert_eq!(b[0].kind, BlockKind::Heading);
    }

    #[test]
    fn style_inheritance_follows_based_on() {
        let mut styles = StyleTable::new();
        styles.insert(
            "Base".into(),
            StyleDef {
                font_family: Some("Times New Roman".into()),
                font_size: Some(12.0),
                ..Default::default()
            },
        );
        styles.insert(
            "Child".into(),
            StyleDef {
                based_on: Some("Base".into()),
                font_size: Some(14.0),
                ..Default::default()
            },
        );
        let r = resolve(&styles, "Child");
        assert_eq!(r.font_family.as_deref(), Some("Times New Roman"));
        assert_eq!(r.font_size, Some(14.0));
    }

    #[test]
    fn a_cyclic_based_on_chain_terminates() {
        let mut styles = StyleTable::new();
        styles.insert(
            "A".into(),
            StyleDef {
                based_on: Some("B".into()),
                ..Default::default()
            },
        );
        styles.insert(
            "B".into(),
            StyleDef {
                based_on: Some("A".into()),
                ..Default::default()
            },
        );
        let _ = resolve(&styles, "A");
    }

    #[test]
    fn malformed_xml_is_an_error_not_a_panic() {
        let r = parse_document("<w:p><w:r><w:t>açık", &StyleTable::new());
        assert!(r.is_err() || r.unwrap().is_empty());
    }

    #[test]
    fn document_defaults_supply_the_body_font_when_nothing_else_does() {
        // Most real documents set the body font only in `w:docDefaults`, so a
        // parser that ignores it reports every run as having no font.
        let styles_xml = format!(
            r#"<w:styles {NS}><w:docDefaults><w:rPrDefault><w:rPr>
               <w:rFonts w:ascii="Times New Roman"/><w:sz w:val="24"/>
               </w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#
        );
        let styles = parse_styles(&styles_xml);
        let xml = format!(
            "<w:document {NS}><w:body><w:p><w:r><w:t>metin</w:t></w:r></w:p></w:body></w:document>"
        );
        let b = finalize_blocks(parse_document(&xml, &styles).unwrap());
        assert_eq!(b[0].runs[0].font_family.as_deref(), Some("Times New Roman"));
        assert_eq!(b[0].runs[0].font_size, Some(12.0));
    }

    #[test]
    fn a_named_style_overrides_the_document_default() {
        let styles_xml = format!(
            r#"<w:styles {NS}><w:docDefaults><w:rPrDefault><w:rPr>
               <w:rFonts w:ascii="Times New Roman"/></w:rPr></w:rPrDefault></w:docDefaults>
               <w:style w:styleId="Alt"><w:name w:val="Alt"/><w:rPr><w:rFonts w:ascii="Arial"/></w:rPr></w:style></w:styles>"#
        );
        let styles = parse_styles(&styles_xml);
        let xml = format!(
            "<w:document {NS}><w:body><w:p><w:pPr><w:pStyle w:val=\"Alt\"/></w:pPr><w:r><w:t>metin</w:t></w:r></w:p></w:body></w:document>"
        );
        let b = finalize_blocks(parse_document(&xml, &styles).unwrap());
        assert_eq!(b[0].runs[0].font_family.as_deref(), Some("Arial"));
    }

    #[test]
    fn styles_part_parses_into_a_table() {
        let xml = format!(
            r#"<w:styles {NS}><w:style w:styleId="Heading1"><w:name w:val="heading 1"/><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style></w:styles>"#
        );
        let t = parse_styles(&xml);
        let s = t.get("Heading1").unwrap();
        assert_eq!(s.name.as_deref(), Some("heading 1"));
        assert_eq!(s.bold, Some(true));
        assert_eq!(s.font_size, Some(16.0));
    }
}
