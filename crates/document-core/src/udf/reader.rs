//! UDF -> canonical model.
//!
//! Reads the ZIP, extracts the flat content buffer, then walks the element tree resolving
//! `(startOffset, length)` against the buffer. Offsets are UTF-16 code units, so the buffer
//! is held as `Vec<u16>` and sliced there — this is what makes Turkish text and any astral
//! character land on the right boundaries.
//!
//! Tolerant by design: genuine UYAP output contains duplicate tab stops, elements that
//! overrun the buffer by a character, and legacy `format_id` values. None of those should
//! cost the user their document.

use std::io::Read;
use std::sync::Arc;

use base64::Engine as _;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::error::{ConvError, ErrorCode, Result};
use crate::model::*;
use crate::security::{self, ArchiveBudget, DepthGuard};
use crate::udf::*;
use crate::warnings::{Warning, WarningCode, WarningSink};

/// `format_id` values seen in genuine specimens (EXP-001). Anything else still parses,
/// but the user is told.
const KNOWN_FORMAT_IDS: &[&str] = &["1.7", "1.8"];

pub fn read_udf(bytes: &[u8], warn: &mut WarningSink) -> Result<Document> {
    let mut budget = ArchiveBudget::new();
    let mut zipf = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| ConvError::invalid_udf(format!("not a ZIP container: {e}")))?;

    let mut content: Option<Vec<u8>> = None;
    let mut saw_signature = false;

    for i in 0..zipf.len() {
        budget.count_entry()?;
        let mut f = zipf
            .by_index(i)
            .map_err(|e| ConvError::invalid_udf(format!("unreadable archive entry: {e}")))?;
        let name = f.name().to_string();
        security::check_entry_name(&name)?;
        budget.check_entry_size(f.compressed_size(), f.size())?;
        if name.eq_ignore_ascii_case("sign.sgn") {
            saw_signature = true;
            continue;
        }
        if name.eq_ignore_ascii_case("content.xml") {
            let mut v = Vec::with_capacity(f.size().min(8 * 1024 * 1024) as usize);
            f.read_to_end(&mut v)
                .map_err(|e| ConvError::invalid_udf(format!("content.xml unreadable: {e}")))?;
            content = Some(v);
        }
    }

    let content =
        content.ok_or_else(|| ConvError::invalid_udf("archive contains no content.xml"))?;
    security::reject_doctype(&content)?;

    if saw_signature {
        warn.warn(WarningCode::SignatureDropped);
    }

    parse_content_xml(&content, &mut budget, warn)
}

struct Ctx<'a> {
    /// The flat buffer as UTF-16 code units — the addressing unit of the format.
    buf: Vec<u16>,
    warn: &'a mut WarningSink,
    budget: &'a mut ArchiveBudget,
    next_list_id: u32,
    lists: Vec<ListDef>,
    saw_field: bool,
    /// Set when a `<field>` declared a span that resolved to nothing — visible text lost.
    lost_field_text: bool,
}

impl<'a> Ctx<'a> {
    /// Slice the buffer, clamping rather than failing: real specimens occasionally declare
    /// a length that runs one unit past the end, and refusing the whole document over that
    /// would be the wrong trade.
    fn slice(&self, start: usize, len: usize) -> String {
        if start >= self.buf.len() {
            return String::new();
        }
        let end = start.saturating_add(len).min(self.buf.len());
        String::from_utf16_lossy(&self.buf[start..end])
    }
}

fn parse_content_xml(
    xml: &[u8],
    budget: &mut ArchiveBudget,
    warn: &mut WarningSink,
) -> Result<Document> {
    let text = std::str::from_utf8(xml)
        .map_err(|e| ConvError::invalid_udf(format!("content.xml is not UTF-8: {e}")))?;

    if let Some(v) = attr_of_root(text, "format_id") {
        if !KNOWN_FORMAT_IDS.contains(&v.as_str()) {
            warn.push(Warning::new(WarningCode::UnknownFormatVersion).detail(v));
        }
    }

    // The content buffer is CDATA; take it literally, including any split sections.
    let buf_str = extract_content_buffer(text)?;
    if buf_str.chars().count() > security::MAX_CONTENT_CHARS {
        return Err(ConvError::unsafe_archive(
            "content buffer exceeds size limit",
        ));
    }
    let buf: Vec<u16> = buf_str.encode_utf16().collect();

    let mut ctx = Ctx {
        buf,
        warn,
        budget,
        next_list_id: 1,
        lists: Vec::new(),
        saw_field: false,
        lost_field_text: false,
    };

    let page = parse_page_format(text);
    let default_font = parse_default_style(text);

    let (header, footer, blocks) = parse_elements(text, &mut ctx)?;

    if ctx.saw_field {
        // The interactivity is gone, but the text the reader could see is preserved.
        // That is an expected transformation, not a loss — unless a span failed to resolve.
        ctx.warn.warn(WarningCode::FormFieldsStaticized);
        if ctx.lost_field_text {
            ctx.warn.warn(WarningCode::FormContentLost);
        }
    }

    let lists = std::mem::take(&mut ctx.lists);

    Ok(Document {
        meta: Metadata {
            title: None,
            author: None,
            default_font,
        },
        sections: vec![Section {
            page,
            header,
            footer,
            blocks,
        }],
        lists,
    })
}

fn attr_of_root(text: &str, key: &str) -> Option<String> {
    let i = text.find("<template")?;
    let head = &text[i..text[i..].find('>').map(|j| i + j).unwrap_or(text.len())];
    let k = format!("{key}=\"");
    let j = head.find(&k)? + k.len();
    let e = head[j..].find('"')? + j;
    Some(head[j..e].to_string())
}

/// Pull the text between `<content><![CDATA[` and the matching `]]></content>`, rejoining
/// any CDATA sections that were split to escape a literal `]]>`.
fn extract_content_buffer(text: &str) -> Result<String> {
    let open = text
        .find("<content>")
        .ok_or_else(|| ConvError::invalid_udf("no <content> element"))?;
    let cd = text[open..]
        .find("<![CDATA[")
        .map(|i| open + i + "<![CDATA[".len())
        .ok_or_else(|| ConvError::invalid_udf("<content> has no CDATA section"))?;
    let close = text[cd..]
        .find("]]></content>")
        .map(|i| cd + i)
        .ok_or_else(|| ConvError::invalid_udf("unterminated content buffer"))?;
    Ok(text[cd..close].replace("]]]]><![CDATA[>", "]]>"))
}

fn parse_page_format(text: &str) -> PageSetup {
    let mut p = PageSetup::a4_uyap();
    let Some(i) = text.find("<pageFormat") else {
        return p;
    };
    let head = &text[i..text[i..].find("/>").map(|j| i + j).unwrap_or(text.len())];
    let g = |k: &str| -> Option<f32> {
        let key = format!("{k}=\"");
        let a = head.find(&key)? + key.len();
        let b = head[a..].find('"')? + a;
        head[a..b].parse().ok()
    };
    if let Some(v) = g("leftMargin") {
        p.margin_left_pt = v;
    }
    if let Some(v) = g("rightMargin") {
        p.margin_right_pt = v;
    }
    if let Some(v) = g("topMargin") {
        p.margin_top_pt = v;
    }
    if let Some(v) = g("bottomMargin") {
        p.margin_bottom_pt = v;
    }
    if let Some(v) = g("headerFOffset") {
        p.header_offset_pt = v;
    }
    if let Some(v) = g("footerFOffset") {
        p.footer_offset_pt = v;
    }
    if let Some(v) = g("paperOrientation") {
        if (v as u8) == orientation::LANDSCAPE {
            p.orientation = Orientation::Landscape;
            std::mem::swap(&mut p.width_pt, &mut p.height_pt);
        }
    }
    p
}

fn parse_default_style(text: &str) -> FontSpec {
    let mut f = FontSpec::default();
    let needle = format!("name=\"{DEFAULT_RESOLVER}\"");
    let Some(i) = text.find(&needle) else {
        return f;
    };
    let head = &text[i..text[i..].find("/>").map(|j| i + j).unwrap_or(text.len())];
    if let Some(a) = head.find("family=\"") {
        let a = a + "family=\"".len();
        if let Some(b) = head[a..].find('"') {
            f.family = head[a..a + b].to_string();
        }
    }
    if let Some(a) = head.find("size=\"") {
        let a = a + "size=\"".len();
        if let Some(b) = head[a..].find('"') {
            if let Ok(v) = head[a..a + b].parse::<f32>() {
                f.size_pt = v;
            }
        }
    }
    f
}

type Parsed = (Option<HeaderFooter>, Option<HeaderFooter>, Vec<Block>);

fn parse_elements(text: &str, ctx: &mut Ctx) -> Result<Parsed> {
    let start = text
        .find("<elements")
        .ok_or_else(|| ConvError::invalid_udf("no <elements> tree"))?;
    let region = &text[start..];

    let mut rd = Reader::from_str(region);
    rd.config_mut().trim_text(false);
    rd.config_mut().check_end_names = false;

    let mut guard = DepthGuard::new();
    let mut header = None;
    let mut footer = None;
    let mut body: Vec<Block> = Vec::new();

    // Stack of block sinks: body, header/footer, table cell.
    let mut hf_stack: Vec<(String, HeaderFooter)> = Vec::new();
    let mut table_stack: Vec<Table> = Vec::new();
    let mut row_stack: Vec<TableRow> = Vec::new();
    let mut cell_stack: Vec<TableCell> = Vec::new();
    let mut para: Option<Paragraph> = None;

    loop {
        match rd.read_event() {
            Err(e) => return Err(ConvError::invalid_udf(format!("XML error: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                guard.enter()?;
                let name = local_name(&e);
                match name.as_str() {
                    "header" | "footer" => {
                        hf_stack.push((name.clone(), parse_hf_attrs(&e)));
                    }
                    "table" => table_stack.push(parse_table_attrs(&e, ctx)),
                    "row" => row_stack.push(parse_row_attrs(&e)),
                    "cell" => cell_stack.push(parse_cell_attrs(&e)),
                    "paragraph" => {
                        para = Some(Paragraph {
                            props: parse_para_props(&e, ctx),
                            runs: vec![],
                        })
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = local_name(&e);
                match name.as_str() {
                    // `field` belongs here: it is a self-closing leaf carrying
                    // (startOffset, length) into the content buffer and run formatting,
                    // exactly like `content`. Treating it as metadata dropped every form
                    // label and value from the output.
                    "content" | "tab" | "space" | "image" | "field" => {
                        if name == "field" {
                            ctx.saw_field = true;
                        }
                        if let Some(p) = para.as_mut() {
                            let before = p.runs.len();
                            p.runs.extend(parse_leaf(&name, &e, ctx)?);
                            if name == "field" && p.runs.len() == before {
                                // The field declared a span we could not resolve; that is a
                                // visible label or value going missing, so say so.
                                ctx.lost_field_text = true;
                            }
                        }
                    }
                    "paragraph" => {
                        // Self-closing paragraph: an empty line.
                        let p = Paragraph {
                            props: parse_para_props(&e, ctx),
                            runs: vec![],
                        };
                        push_block(
                            Block::Paragraph(p),
                            &mut cell_stack,
                            &mut row_stack,
                            &mut table_stack,
                            &mut hf_stack,
                            &mut body,
                        );
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                guard.leave();
                let name = local_name_end(&e);
                match name.as_str() {
                    "paragraph" => {
                        if let Some(mut p) = para.take() {
                            trim_paragraph_terminator(&mut p);
                            push_block(
                                Block::Paragraph(p),
                                &mut cell_stack,
                                &mut row_stack,
                                &mut table_stack,
                                &mut hf_stack,
                                &mut body,
                            );
                        }
                    }
                    "cell" => {
                        if let Some(c) = cell_stack.pop() {
                            if let Some(r) = row_stack.last_mut() {
                                r.cells.push(c);
                            }
                        }
                    }
                    "row" => {
                        if let Some(r) = row_stack.pop() {
                            if let Some(t) = table_stack.last_mut() {
                                t.rows.push(r);
                            }
                        }
                    }
                    "table" => {
                        if let Some(mut t) = table_stack.pop() {
                            reconcile_table_grid(&mut t);
                            push_block(
                                Block::Table(t),
                                &mut cell_stack,
                                &mut row_stack,
                                &mut table_stack,
                                &mut hf_stack,
                                &mut body,
                            );
                        }
                    }
                    "header" | "footer" => {
                        if let Some((kind, hf)) = hf_stack.pop() {
                            if kind == "header" {
                                header = Some(hf);
                            } else {
                                footer = Some(hf);
                            }
                        }
                    }
                    "elements" => break,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Ok((header, footer, body))
}

fn push_block(
    b: Block,
    cell: &mut [TableCell],
    row: &mut [TableRow],
    table: &mut [Table],
    hf: &mut [(String, HeaderFooter)],
    body: &mut Vec<Block>,
) {
    if let Some(c) = cell.last_mut() {
        c.blocks.push(b);
    } else if let Some(_r) = row.last_mut() {
        // A block directly inside a row without a cell is malformed; keep it rather than
        // dropping text.
        if let Some(t) = table.last_mut() {
            if let Some(rr) = t.rows.last_mut() {
                if let Some(cc) = rr.cells.last_mut() {
                    cc.blocks.push(b);
                    return;
                }
            }
        }
        body.push(b);
    } else if let Some((_, h)) = hf.last_mut() {
        h.blocks.push(b);
    } else {
        body.push(b);
    }
}

/// The buffer's paragraph terminator belongs to the format, not to the text. Drop the
/// trailing newline that the last run carries so the canonical model holds clean text.
fn trim_paragraph_terminator(p: &mut Paragraph) {
    while let Some(Run::Text { text, .. }) = p.runs.last_mut() {
        if text.ends_with('\n') {
            text.pop();
            if text.is_empty() {
                p.runs.pop();
                continue;
            }
        }
        break;
    }
}

fn local_name(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).to_string()
}

fn local_name_end(e: &quick_xml::events::BytesEnd) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).to_string()
}

fn attr(e: &BytesStart, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if a.key.local_name().as_ref() == key.as_bytes() {
            return Some(String::from_utf8_lossy(&a.value).to_string());
        }
    }
    None
}

fn attr_f32(e: &BytesStart, key: &str) -> Option<f32> {
    attr(e, key).and_then(|v| v.trim().parse().ok())
}

fn attr_usize(e: &BytesStart, key: &str) -> Option<usize> {
    attr(e, key).and_then(|v| v.trim().parse().ok())
}

fn attr_bool(e: &BytesStart, key: &str) -> bool {
    attr(e, key)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn parse_hf_attrs(e: &BytesStart) -> HeaderFooter {
    let mut page_number_attrs = Vec::new();
    for a in e.attributes().flatten() {
        let k = String::from_utf8_lossy(a.key.local_name().as_ref()).to_string();
        if k.starts_with("pageNumber-") {
            page_number_attrs.push((k, String::from_utf8_lossy(&a.value).to_string()));
        }
    }
    // A `pageNumber-spec` is what marks the header/footer as carrying a page number
    // (EXP-013). The spec string itself is opaque, but the surrounding attributes carry
    // everything needed to rebuild a real, updatable Word field.
    let page_number = attr(e, "pageNumber-spec").map(|spec| PageNumberField {
        prefix: attr(e, "pageNumber-foreStr").filter(|v| !v.is_empty()),
        separator: attr(e, "pageNumber-seperator").filter(|v| !v.is_empty()),
        start_at: attr(e, "pageNumber-pageStartNumStr").and_then(|v| v.trim().parse().ok()),
        font_family: attr(e, "pageNumber-fontFace").filter(|v| !v.is_empty()),
        font_size_pt: attr(e, "pageNumber-fontSize").and_then(|v| v.trim().parse().ok()),
        bold: attr(e, "pageNumber-fontBold")
            .map(|v| v == "true")
            .unwrap_or(false),
        italic: attr(e, "pageNumber-fontItalic")
            .map(|v| v == "true")
            .unwrap_or(false),
        color: attr(e, "pageNumber-color")
            .and_then(|v| v.trim().parse::<i32>().ok())
            .map(Color::from_java_argb),
        raw_spec: Some(spec),
    });
    HeaderFooter {
        blocks: Vec::new(),
        stop_page: attr(e, "stopPage").and_then(|v| v.parse().ok()),
        start_page: attr(e, "startPage").and_then(|v| v.parse().ok()),
        page_number_attrs,
        page_number,
    }
}

fn parse_table_attrs(e: &BytesStart, _ctx: &mut Ctx) -> Table {
    let widths = attr(e, "columnSpans")
        .map(|s| {
            s.split(',')
                .filter_map(|v| v.trim().parse::<f32>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let cols = attr_usize(e, "columnCount").unwrap_or(widths.len().max(1));
    let mut column_widths = widths;
    while column_widths.len() < cols {
        column_widths.push(1.0);
    }
    Table {
        name: attr(e, "tableName"),
        column_widths,
        border: match attr(e, "border").as_deref() {
            Some("borderNone") => TableBorder::None,
            _ => TableBorder::Cell,
        },
        rows: Vec::new(),
        props: ParaProps::default(),
    }
}

fn parse_row_attrs(e: &BytesStart) -> TableRow {
    TableRow {
        cells: Vec::new(),
        is_header: attr(e, "rowType").as_deref() == Some("headerRow"),
        height_pt: attr_f32(e, "height").filter(|v| *v > 0.0),
    }
}

fn parse_cell_attrs(e: &BytesStart) -> TableCell {
    TableCell {
        blocks: Vec::new(),
        grid_span: 1,
        vertical_align: match attr(e, "align").as_deref() {
            Some("vcenter") => CellVAlign::Center,
            Some("vbottom") => CellVAlign::Bottom,
            _ => CellVAlign::Top,
        },
    }
}

/// Recover each cell's horizontal span. A row with fewer cells than the grid has merged
/// cells; distribute the grid columns over its cells proportionally to the row's own
/// `columnSpans` when present, else spread the remainder over the last cell (EXP-006).
fn reconcile_table_grid(t: &mut Table) {
    let cols = t.column_widths.len().max(1);
    for row in &mut t.rows {
        let n = row.cells.len();
        if n == 0 {
            continue;
        }
        if n >= cols {
            for c in row.cells.iter_mut() {
                c.grid_span = 1;
            }
            continue;
        }
        // Merged row: give each cell a proportional share of the grid, remainder to the last.
        let base = cols / n;
        let extra = cols % n;
        for (i, c) in row.cells.iter_mut().enumerate() {
            c.grid_span = base + usize::from(i < extra);
        }
    }
}

fn parse_para_props(e: &BytesStart, ctx: &mut Ctx) -> ParaProps {
    // UDF `Hanging` is the hanging-indent AMOUNT measured from `LeftIndent`: the first line
    // starts at LeftIndent, continuation lines at LeftIndent + Hanging (docs/qa audit).
    // The canonical model uses OOXML semantics, where `left_indent_pt` is where the body
    // sits — so the two must be composed here. Reading `LeftIndent` alone put the first
    // line at a negative position and pushed legal labels off the page.
    let hanging = attr_f32(e, "Hanging").unwrap_or(0.0).max(0.0);
    let mut p = ParaProps {
        alignment: match attr_usize(e, "Alignment").unwrap_or(0) {
            x if x == align::CENTER as usize => Alignment::Center,
            x if x == align::RIGHT as usize => Alignment::Right,
            x if x == align::JUSTIFIED as usize => Alignment::Justify,
            _ => Alignment::Left,
        },
        left_indent_pt: attr_f32(e, "LeftIndent").unwrap_or(0.0) + hanging,
        right_indent_pt: attr_f32(e, "RightIndent").unwrap_or(0.0),
        first_line_indent_pt: attr_f32(e, "FirstLineIndent").unwrap_or(0.0),
        hanging_pt: hanging,
        space_before_pt: attr_f32(e, "SpaceAbove").unwrap_or(0.0),
        space_after_pt: attr_f32(e, "SpaceBelow").unwrap_or(0.0),
        line_spacing_extra: attr_f32(e, "LineSpacing").unwrap_or(0.0),
        tab_stops: Vec::new(),
        list: None,
        style_name: attr(e, "description"),
    };

    if let Some(ts) = attr(e, "TabSet") {
        let mut stops: Vec<f32> = ts
            .split(',')
            .filter_map(|s| s.split(':').next()?.trim().parse::<f32>().ok())
            .collect();
        stops.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        stops.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        p.tab_stops = stops
            .into_iter()
            .map(|pos| TabStop {
                pos_pt: pos,
                align: TabAlign::Left,
            })
            .collect();
    }

    if let Some(id) = attr(e, "ListId").and_then(|v| v.parse::<u32>().ok()) {
        let level = attr(e, "ListLevel")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1);
        p.list = Some(ListRef { list_id: id, level });
        if !ctx.lists.iter().any(|d| d.list_id == id) {
            let kind = if attr_bool(e, "Bulleted") {
                ListKind::Bullet(BulletKind::Ellipse)
            } else {
                match attr(e, "NumberType").as_deref() {
                    Some(list_tokens::CHAR_SMALL_PAREN) => {
                        ListKind::Number(NumberKind::LowerAlphaParen)
                    }
                    _ => ListKind::Number(NumberKind::DecimalDot),
                }
            };
            ctx.lists.push(ListDef { list_id: id, kind });
            ctx.next_list_id = ctx.next_list_id.max(id + 1);
        }
    }
    p
}

fn parse_run_props(e: &BytesStart) -> RunProps {
    RunProps {
        bold: attr_bool(e, "bold"),
        italic: attr_bool(e, "italic"),
        underline: attr_bool(e, "underline"),
        strike: false,
        vert_align: VertAlign::Baseline,
        font_family: attr(e, "family"),
        font_size_pt: attr_f32(e, "size"),
        color: attr(e, "foreground")
            .and_then(|v| v.trim().parse::<i32>().ok())
            .map(Color::from_java_argb),
    }
}

/// Returns zero or more runs.
///
/// Genuine UYAP output marks only some tabs with a `<tab>` element — the corpus holds 2117
/// literal `\t` characters against 103 `<tab>` elements. A tab is a tab either way, so a
/// literal tab inside a `<content>` run is split out here. Without this the DOCX side,
/// which does emit a real `<w:tab/>` for every tab, would disagree with the UDF side about
/// how many tabs a document has, and tab stops carry the layout of a legal pleading.
fn parse_leaf(name: &str, e: &BytesStart, ctx: &mut Ctx) -> Result<Vec<Run>> {
    let start = attr_usize(e, "startOffset").unwrap_or(0);
    let len = attr_usize(e, "length").unwrap_or(0);
    let props = parse_run_props(e);

    match name {
        "image" => {
            let Some(b64) = attr(e, "imageData") else {
                return Ok(Vec::new());
            };
            let cleaned: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
            let data = base64::engine::general_purpose::STANDARD
                .decode(cleaned.as_bytes())
                .map_err(|e| ConvError::invalid_udf(format!("image is not valid base64: {e}")))?;
            ctx.budget.add_image(data.len())?;
            let format = ImageFormat::sniff(&data);
            Ok(vec![Run::Image(Image {
                data: Arc::new(data),
                format,
                width_pt: attr_f32(e, "width").unwrap_or(0.0),
                height_pt: attr_f32(e, "height").unwrap_or(0.0),
            })])
        }
        "tab" => Ok(vec![Run::Tab { props }]),
        _ => {
            let text = ctx.slice(start, len);
            if text.is_empty() {
                return Ok(Vec::new());
            }
            if !text.contains('\t') {
                return Ok(vec![Run::Text { text, props }]);
            }
            let mut out = Vec::new();
            for (i, piece) in text.split('\t').enumerate() {
                if i > 0 {
                    out.push(Run::Tab {
                        props: props.clone(),
                    });
                }
                if !piece.is_empty() {
                    out.push(Run::Text {
                        text: piece.to_string(),
                        props: props.clone(),
                    });
                }
            }
            Ok(out)
        }
    }
}

/// Cheap structural probe used by the UI to tell a real UDF from a renamed file before a
/// full parse.
pub fn probe(bytes: &[u8]) -> Result<()> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| ConvError::invalid_udf(format!("not a ZIP container: {e}")))?;
    if z.by_name("content.xml").is_err() {
        return Err(ConvError::new(
            ErrorCode::InvalidUdf,
            "archive contains no content.xml",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::udf::writer;

    fn roundtrip(doc: &Document) -> (Document, Vec<Warning>) {
        let mut w = WarningSink::new();
        let xml = writer::write_content_xml(doc, &mut w).unwrap();
        writer::validate(&xml).unwrap();
        let zipped = writer::package(&xml).unwrap();
        let mut w2 = WarningSink::new();
        let back = read_udf(&zipped, &mut w2).unwrap();
        (back, w2.into_vec())
    }

    fn doc(blocks: Vec<Block>) -> Document {
        Document {
            meta: Metadata::default(),
            sections: vec![Section {
                blocks,
                ..Default::default()
            }],
            lists: vec![],
        }
    }

    #[test]
    fn text_and_turkish_survive_a_udf_roundtrip() {
        let t = "Çağdaş Türkiye Cumhuriyeti — İİK, HMK, ıİğĞüÜşŞöÖçÇ";
        let d = doc(vec![
            Block::Paragraph(Paragraph::plain(t)),
            Block::Paragraph(Paragraph::plain("İkinci paragraf")),
        ]);
        let (back, _) = roundtrip(&d);
        let ps: Vec<String> = back.sections[0]
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph(p) => Some(p.text()),
                _ => None,
            })
            .collect();
        assert_eq!(ps, vec![t.to_string(), "İkinci paragraf".to_string()]);
    }

    #[test]
    fn run_formatting_survives() {
        let d = doc(vec![Block::Paragraph(Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run::Text {
                    text: "kalın".into(),
                    props: RunProps {
                        bold: true,
                        ..Default::default()
                    },
                },
                Run::Text {
                    text: "eğik".into(),
                    props: RunProps {
                        italic: true,
                        ..Default::default()
                    },
                },
                Run::Text {
                    text: "altı".into(),
                    props: RunProps {
                        underline: true,
                        ..Default::default()
                    },
                },
            ],
        })]);
        let (back, _) = roundtrip(&d);
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        assert_eq!(p.runs.len(), 3);
        assert!(p.runs[0].props().unwrap().bold);
        assert!(p.runs[1].props().unwrap().italic);
        assert!(p.runs[2].props().unwrap().underline);
    }

    #[test]
    fn paragraph_geometry_survives() {
        let props = ParaProps {
            alignment: Alignment::Justify,
            left_indent_pt: 28.35,
            right_indent_pt: 14.0,
            first_line_indent_pt: 21.0,
            space_before_pt: 6.0,
            space_after_pt: 12.0,
            line_spacing_extra: 0.5,
            tab_stops: vec![TabStop {
                pos_pt: 56.7,
                align: TabAlign::Left,
            }],
            ..Default::default()
        };
        let d = doc(vec![Block::Paragraph(Paragraph {
            props: props.clone(),
            runs: vec![Run::Text {
                text: "x".into(),
                props: RunProps::default(),
            }],
        })]);
        let (back, _) = roundtrip(&d);
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        assert_eq!(p.props.alignment, Alignment::Justify);
        assert!((p.props.left_indent_pt - 28.35).abs() < 0.01);
        assert!((p.props.line_spacing_extra - 0.5).abs() < 0.001);
        assert_eq!(p.props.tab_stops.len(), 1);
    }

    #[test]
    fn tables_including_a_horizontal_merge_survive() {
        let t = Table {
            column_widths: vec![100.0, 100.0, 100.0],
            border: TableBorder::Cell,
            rows: vec![
                TableRow {
                    cells: vec![
                        TableCell {
                            blocks: vec![Block::Paragraph(Paragraph::plain("birleşik"))],
                            grid_span: 2,
                            ..Default::default()
                        },
                        TableCell {
                            blocks: vec![Block::Paragraph(Paragraph::plain("tek"))],
                            grid_span: 1,
                            ..Default::default()
                        },
                    ],
                    is_header: true,
                    ..Default::default()
                },
                TableRow {
                    cells: vec![
                        TableCell {
                            blocks: vec![Block::Paragraph(Paragraph::plain("a"))],
                            grid_span: 1,
                            ..Default::default()
                        },
                        TableCell {
                            blocks: vec![Block::Paragraph(Paragraph::plain("b"))],
                            grid_span: 1,
                            ..Default::default()
                        },
                        TableCell {
                            blocks: vec![Block::Paragraph(Paragraph::plain("c"))],
                            grid_span: 1,
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let (back, _) = roundtrip(&doc(vec![Block::Table(t)]));
        let Block::Table(bt) = &back.sections[0].blocks[0] else {
            panic!("table lost")
        };
        assert_eq!(bt.rows.len(), 2);
        assert_eq!(bt.rows[0].cells.len(), 2);
        assert_eq!(bt.rows[1].cells.len(), 3);
        assert!(bt.rows[0].is_header);
        // The merged cell must still report a span of 2 after the round trip.
        assert_eq!(bt.rows[0].cells[0].grid_span, 2);
        let Block::Paragraph(p) = &bt.rows[0].cells[0].blocks[0] else {
            panic!()
        };
        assert_eq!(p.text(), "birleşik");
    }

    #[test]
    fn header_and_footer_survive() {
        let mut d = doc(vec![Block::Paragraph(Paragraph::plain("gövde"))]);
        d.sections[0].header = Some(HeaderFooter {
            blocks: vec![Block::Paragraph(Paragraph::plain("üst bilgi"))],
            stop_page: Some(1),
            ..Default::default()
        });
        d.sections[0].footer = Some(HeaderFooter {
            blocks: vec![Block::Paragraph(Paragraph::plain("alt bilgi"))],
            ..Default::default()
        });
        let (back, _) = roundtrip(&d);
        let h = back.sections[0].header.as_ref().expect("header lost");
        let f = back.sections[0].footer.as_ref().expect("footer lost");
        assert_eq!(h.stop_page, Some(1));
        let Block::Paragraph(hp) = &h.blocks[0] else {
            panic!()
        };
        assert_eq!(hp.text(), "üst bilgi");
        let Block::Paragraph(fp) = &f.blocks[0] else {
            panic!()
        };
        assert_eq!(fp.text(), "alt bilgi");
        // The body must not have absorbed the header text.
        let Block::Paragraph(bp) = &back.sections[0].blocks[0] else {
            panic!()
        };
        assert_eq!(bp.text(), "gövde");
    }

    #[test]
    fn images_survive_byte_for_byte() {
        let png = crate::testutil::minimal_png();
        let d = doc(vec![Block::Paragraph(Paragraph {
            props: ParaProps::default(),
            runs: vec![Run::Image(Image {
                data: Arc::new(png.clone()),
                format: ImageFormat::Png,
                width_pt: 108.0,
                height_pt: 112.0,
            })],
        })]);
        let (back, _) = roundtrip(&d);
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        let Run::Image(img) = &p.runs[0] else {
            panic!("image lost")
        };
        assert_eq!(img.data.as_ref(), &png, "PNG must not be re-encoded");
        assert!((img.width_pt - 108.0).abs() < 0.01);
    }

    #[test]
    fn lists_survive_with_their_kind() {
        let mut d = doc(vec![
            Block::Paragraph(Paragraph {
                props: ParaProps {
                    list: Some(ListRef {
                        list_id: 1,
                        level: 1,
                    }),
                    ..Default::default()
                },
                runs: vec![Run::Text {
                    text: "madde".into(),
                    props: RunProps::default(),
                }],
            }),
            Block::Paragraph(Paragraph {
                props: ParaProps {
                    list: Some(ListRef {
                        list_id: 2,
                        level: 1,
                    }),
                    ..Default::default()
                },
                runs: vec![Run::Text {
                    text: "işaret".into(),
                    props: RunProps::default(),
                }],
            }),
        ]);
        d.lists = vec![
            ListDef {
                list_id: 1,
                kind: ListKind::Number(NumberKind::DecimalDot),
            },
            ListDef {
                list_id: 2,
                kind: ListKind::Bullet(BulletKind::Ellipse),
            },
        ];
        let (back, _) = roundtrip(&d);
        assert_eq!(back.lists.len(), 2);
        assert!(back
            .lists
            .iter()
            .any(|l| l.kind == ListKind::Bullet(BulletKind::Ellipse)));
        assert!(back
            .lists
            .iter()
            .any(|l| l.kind == ListKind::Number(NumberKind::DecimalDot)));
    }

    #[test]
    fn tabs_survive_as_tabs() {
        let d = doc(vec![Block::Paragraph(Paragraph::plain(
            "Dosya No\t:\t2025/450",
        ))]);
        let (back, _) = roundtrip(&d);
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        assert_eq!(p.text(), "Dosya No\t:\t2025/450");
        assert_eq!(
            p.runs
                .iter()
                .filter(|r| matches!(r, Run::Tab { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn a_non_zip_file_fails_cleanly_as_invalid_udf() {
        let mut w = WarningSink::new();
        let e = read_udf(b"this is definitely not a zip archive", &mut w).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidUdf);
    }

    #[test]
    fn a_zip_without_content_xml_fails_cleanly() {
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut cur);
            let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            zw.start_file("readme.txt", o).unwrap();
            use std::io::Write;
            zw.write_all(b"hi").unwrap();
            zw.finish().unwrap();
        }
        let mut w = WarningSink::new();
        let e = read_udf(&cur.into_inner(), &mut w).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidUdf);
    }

    #[test]
    fn a_signed_udf_warns_that_the_signature_is_dropped() {
        let d = doc(vec![Block::Paragraph(Paragraph::plain("x"))]);
        let mut w = WarningSink::new();
        let xml = writer::write_content_xml(&d, &mut w).unwrap();
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            use std::io::Write;
            let mut zw = zip::ZipWriter::new(&mut cur);
            let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            zw.start_file("content.xml", o).unwrap();
            zw.write_all(&xml).unwrap();
            zw.start_file("sign.sgn", o).unwrap();
            zw.write_all(b"signature-bytes").unwrap();
            zw.finish().unwrap();
        }
        let mut w2 = WarningSink::new();
        read_udf(&cur.into_inner(), &mut w2).unwrap();
        assert!(w2.has(WarningCode::SignatureDropped));
    }

    #[test]
    fn page_setup_survives() {
        let mut d = doc(vec![Block::Paragraph(Paragraph::plain("x"))]);
        d.sections[0].page.margin_left_pt = 56.7;
        let (back, _) = roundtrip(&d);
        assert!((back.sections[0].page.margin_left_pt - 56.7).abs() < 0.01);
    }
}
