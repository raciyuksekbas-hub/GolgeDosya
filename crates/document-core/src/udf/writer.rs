//! Canonical model -> UDF.
//!
//! The single hard constraint of the format (EXP-002) is that the flat content buffer and
//! the element tree must agree perfectly: paragraphs tile the buffer contiguously, each
//! span ends with its `\n`, an image costs exactly one offset, a tab is both a literal
//! `\t` and a `<tab>` element.
//!
//! Therefore the buffer and the tree are produced in ONE pass from ONE cursor. They cannot
//! drift because there is no second source of truth. Afterwards `writer::validate` re-parses
//! the emitted XML and asserts the tiling property before the bytes are allowed out.

use std::io::Write;

use base64::Engine as _;

use crate::error::{ConvError, ErrorCode, Result};
use crate::model::*;
use crate::udf::*;
use crate::units::{udf_float, udf_round};
use crate::warnings::{Warning, WarningCode, WarningSink};
use crate::xml::{cdata_safe, esc_attr, sanitize_text};

/// Emits `content.xml` for a document.
pub struct UdfWriter<'a> {
    /// The flat character buffer.
    buf: String,
    /// Length of `buf` in UTF-16 code units — the unit Java (and therefore UDF) counts in.
    cursor: usize,
    warn: &'a mut WarningSink,
}

/// Serialize a document to the bytes of a complete UDF `content.xml`.
pub fn write_content_xml(doc: &Document, warn: &mut WarningSink) -> Result<Vec<u8>> {
    if doc.sections.len() > 1 {
        warn.push(
            Warning::new(WarningCode::MultiSectionFlattened)
                .detail(format!("{} bölüm", doc.sections.len())),
        );
    }
    let section = doc
        .sections
        .first()
        .ok_or_else(|| ConvError::engine("document has no section"))?;

    let mut w = UdfWriter {
        buf: String::new(),
        cursor: 0,
        warn,
    };

    // Order matters: observed specimens place header content at the lowest offsets,
    // then the body, then the footer. We reproduce that ordering exactly.
    let header_xml = match &section.header {
        Some(h) => Some(w.write_header_footer("header", h, doc)?),
        None => None,
    };

    let mut body_xml = String::new();
    for block in &section.blocks {
        // A section beyond the first is appended to the same body (flattened, warned above).
        w.write_block(block, doc, &mut body_xml)?;
    }
    for extra in doc.sections.iter().skip(1) {
        for block in &extra.blocks {
            w.write_block(block, doc, &mut body_xml)?;
        }
    }

    let footer_xml = match &section.footer {
        Some(f) => Some(w.write_header_footer("footer", f, doc)?),
        None => None,
    };

    let UdfWriter { buf, .. } = w;

    let mut out = String::with_capacity(buf.len() * 2 + 4096);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n\n");
    out.push_str(&format!("<template format_id=\"{FORMAT_ID}\" >\n"));
    out.push_str("<content><![CDATA[");
    out.push_str(&cdata_safe(&buf));
    out.push_str("]]></content>\n");

    out.push_str(&page_properties(&section.page, warn));

    out.push_str(&format!("<elements resolver=\"{DEFAULT_RESOLVER}\" >\n"));
    if let Some(h) = header_xml {
        out.push_str(&h);
    }
    out.push_str(&body_xml);
    if let Some(f) = footer_xml {
        out.push_str(&f);
    }
    out.push_str("</elements>\n");

    out.push_str(&styles(&doc.meta.default_font));
    out.push_str("</template>");

    Ok(out.into_bytes())
}

fn page_properties(p: &PageSetup, warn: &mut WarningSink) -> String {
    // UDF's `pageFormat` carries a named media size and no explicit width/height, so a
    // page that is not A4 genuinely cannot be represented. Say so precisely: name the
    // source size AND the target size with their real dimensions. Calling 612x792 pt "A4"
    // is simply false, and a page geometry change is a real loss, not an approximation.
    if !p.is_a4() {
        warn.push(
            Warning::new(WarningCode::PageSizeChanged)
                .detail(format!("{} -> A4 (595x842 pt)", p.size_label())),
        );
    }
    let orient = match p.orientation {
        Orientation::Portrait => orientation::PORTRAIT,
        Orientation::Landscape => {
            warn.warn(WarningCode::PageOrientationUnverified);
            orientation::LANDSCAPE
        }
    };
    format!(
        "<properties><pageFormat mediaSizeName=\"{}\" leftMargin=\"{}\" rightMargin=\"{}\" \
         topMargin=\"{}\" bottomMargin=\"{}\" paperOrientation=\"{}\" headerFOffset=\"{}\" \
         footerFOffset=\"{}\" /></properties>\n",
        MEDIA_A4,
        udf_float(p.margin_left_pt),
        udf_float(p.margin_right_pt),
        udf_float(p.margin_top_pt),
        udf_float(p.margin_bottom_pt),
        orient,
        udf_float(p.header_offset_pt),
        udf_float(p.footer_offset_pt),
    )
}

fn styles(f: &FontSpec) -> String {
    // Exactly the shape observed as the document default in every specimen (EXP-003).
    format!(
        "<styles><style name=\"{DEFAULT_RESOLVER}\" family=\"{}\" size=\"{}\" \
         description=\"Gövde\" /></styles>\n",
        esc_attr(&f.family),
        f.size_pt.round() as i32
    )
}

impl<'a> UdfWriter<'a> {
    /// Append text to the buffer and return the (startOffset, length) it occupies,
    /// measured in UTF-16 code units exactly as Java would.
    fn push_text(&mut self, s: &str) -> (usize, usize) {
        let start = self.cursor;
        let len: usize = s.encode_utf16().count();
        self.buf.push_str(s);
        self.cursor += len;
        (start, len)
    }

    fn write_header_footer(
        &mut self,
        tag: &str,
        hf: &HeaderFooter,
        doc: &Document,
    ) -> Result<String> {
        let mut inner = String::new();
        for b in &hf.blocks {
            self.write_block(b, doc, &mut inner)?;
        }
        let mut attrs = String::new();
        if let Some(sp) = hf.stop_page {
            attrs.push_str(&format!(" stopPage=\"{sp}\""));
        }
        if let Some(sp) = hf.start_page {
            attrs.push_str(&format!(" startPage=\"{sp}\""));
        }
        // Observed on every header/footer: opaque white background, opaque white foreground.
        attrs.push_str(" background=\"-1\" foreground=\"-1\"");
        for (k, v) in &hf.page_number_attrs {
            attrs.push_str(&format!(" {}=\"{}\"", k, esc_attr(v)));
        }
        Ok(format!("<{tag}{attrs}>{inner}</{tag}>\n"))
    }

    fn write_block(&mut self, block: &Block, doc: &Document, out: &mut String) -> Result<()> {
        match block {
            Block::Paragraph(p) => self.write_paragraph(p, doc, out),
            Block::Table(t) => self.write_table(t, doc, out),
            Block::PageBreak => {
                // EXP-012: no page-break representation exists anywhere in the observed
                // UDF corpus - no form feed in the buffer, no attribute on any element.
                // Emitting a blank paragraph would add a line the author never wrote
                // while still not breaking the page, so the break is dropped and the
                // loss is reported instead of being disguised.
                self.warn.warn(WarningCode::PageBreakDropped);
                Ok(())
            }
        }
    }

    fn write_paragraph(&mut self, p: &Paragraph, doc: &Document, out: &mut String) -> Result<()> {
        let attrs = self.paragraph_attrs(&p.props, doc);
        let mut children = String::new();

        for run in &p.runs {
            match run {
                Run::Text { text, props } => {
                    let clean = sanitize_text(text);
                    if clean.is_empty() {
                        continue;
                    }
                    // A literal tab inside a text run must still be mirrored by a <tab>
                    // element, so split on tabs and emit each piece separately.
                    for (i, piece) in clean.split('\t').enumerate() {
                        if i > 0 {
                            let (s, l) = self.push_text("\t");
                            children.push_str(&format!(
                                "<tab{} startOffset=\"{s}\" length=\"{l}\" />",
                                self.run_attrs(props, doc)
                            ));
                        }
                        if !piece.is_empty() {
                            let (s, l) = self.push_text(piece);
                            children.push_str(&format!(
                                "<content{} startOffset=\"{s}\" length=\"{l}\" />",
                                self.run_attrs(props, doc)
                            ));
                        }
                    }
                }
                Run::Tab { props } => {
                    let (s, l) = self.push_text("\t");
                    children.push_str(&format!(
                        "<tab{} startOffset=\"{s}\" length=\"{l}\" />",
                        self.run_attrs(props, doc)
                    ));
                }
                Run::LineBreak { props } => {
                    // UDF has no intra-paragraph line break: the buffer's only line
                    // terminator is the paragraph one, and emitting '\n' here would create
                    // a second paragraph and desynchronise the tiling. A soft break becomes
                    // a space, which keeps the text readable and the offsets sound - but it
                    // does merge two visual lines into one, so it is reported rather than
                    // done quietly.
                    self.warn.warn(WarningCode::LineBreakConverted);
                    let (s, l) = self.push_text(" ");
                    children.push_str(&format!(
                        "<content{} startOffset=\"{s}\" length=\"{l}\" />",
                        self.run_attrs(props, doc)
                    ));
                }
                Run::Image(img) => {
                    children.push_str(&self.write_image(img)?);
                }
            }
        }

        // Every paragraph owns its terminating newline (EXP-002).
        let (s, l) = self.push_text("\n");
        children.push_str(&format!("<content startOffset=\"{s}\" length=\"{l}\" />"));

        out.push_str(&format!("<paragraph{attrs}>{children}</paragraph>\n"));
        Ok(())
    }

    fn write_image(&mut self, img: &Image) -> Result<String> {
        let (bytes, fmt) = match img.format {
            ImageFormat::Png => (img.data.as_ref().clone(), ImageFormat::Png),
            ImageFormat::Unsupported => {
                self.warn.push(
                    Warning::new(WarningCode::ImageFormatUnsupported).detail("vektör/bilinmeyen"),
                );
                return Ok(String::new());
            }
            other => {
                // UDF is only ever observed carrying PNG (EXP-008), so anything else is
                // transcoded rather than embedded and hoped for.
                match transcode_to_png(img.data.as_ref()) {
                    Ok(png) => {
                        self.warn.push(
                            Warning::new(WarningCode::ImageTranscodedToPng)
                                .detail(other.extension().to_uppercase()),
                        );
                        (png, ImageFormat::Png)
                    }
                    Err(_) => {
                        self.warn.push(
                            Warning::new(WarningCode::ImageFormatUnsupported)
                                .detail(other.extension().to_uppercase()),
                        );
                        return Ok(String::new());
                    }
                }
            }
        };
        let _ = fmt;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        // The image occupies exactly one buffer offset, filled with U+00B8 (EXP-002).
        let (s, l) = self.push_text(&IMAGE_PLACEHOLDER.to_string());
        Ok(format!(
            "<image imageData=\"{b64}\" width=\"{}\" height=\"{}\" startOffset=\"{s}\" length=\"{l}\" />",
            udf_float(img.width_pt),
            udf_float(img.height_pt)
        ))
    }

    fn write_table(&mut self, t: &Table, doc: &Document, out: &mut String) -> Result<()> {
        let cols = t.column_count().max(1);
        let widths = normalized_widths(&t.column_widths, cols);
        let border = match t.border {
            TableBorder::None => "borderNone",
            TableBorder::Cell => "borderCell",
        };
        let name = t.name.clone().unwrap_or_else(|| "Tablo1".to_string());
        let geom = self.paragraph_attrs(&t.props, doc);

        out.push_str(&format!(
            "<table tableName=\"{}\" columnCount=\"{}\" columnSpans=\"{}\" border=\"{}\" header=\"false\"{}>",
            esc_attr(&name),
            cols,
            widths.iter().map(|w| w.to_string()).collect::<Vec<_>>().join(","),
            border,
            geom
        ));

        for (ri, row) in t.rows.iter().enumerate() {
            let mut row_attrs = format!(
                " rowName=\"row{}\" rowType=\"{}\"",
                ri + 1,
                if row.is_header {
                    "headerRow"
                } else {
                    "dataRow"
                }
            );
            if let Some(h) = row.height_pt {
                row_attrs.push_str(&format!(" height=\"{}\"", udf_float(h)));
            }
            // A row whose cells do not fill the grid carries its own widths — this is how
            // a horizontal merge is expressed (EXP-006).
            if row.cells.len() != cols {
                let rw = row_widths(row, &widths, cols);
                row_attrs.push_str(&format!(
                    " columnSpans=\"{}\"",
                    rw.iter()
                        .map(|w| w.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
            out.push_str(&format!("<row{row_attrs}>"));
            for cell in &row.cells {
                let va = match cell.vertical_align {
                    CellVAlign::Center => " align=\"vcenter\"",
                    CellVAlign::Bottom => " align=\"vbottom\"",
                    CellVAlign::Top => "",
                };
                out.push_str(&format!("<cell{va}>"));
                if cell.blocks.is_empty() {
                    // A cell must contain at least one paragraph or the offsets of the
                    // surrounding tiling would have a hole in them.
                    self.write_paragraph(&Paragraph::default(), doc, out)?;
                } else {
                    for b in &cell.blocks {
                        match b {
                            Block::Table(_) => {
                                self.warn.warn(WarningCode::TableNestedFlattened);
                                if let Block::Table(inner) = b {
                                    for r in &inner.rows {
                                        for c in &r.cells {
                                            for ib in &c.blocks {
                                                if let Block::Paragraph(p) = ib {
                                                    self.write_paragraph(p, doc, out)?;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            other => self.write_block(other, doc, out)?,
                        }
                    }
                }
                out.push_str("</cell>");
            }
            out.push_str("</row>");
        }
        out.push_str("</table>\n");
        Ok(())
    }

    fn paragraph_attrs(&mut self, p: &ParaProps, doc: &Document) -> String {
        let mut a = String::new();
        a.push_str(&format!(
            " Alignment=\"{}\"",
            match p.alignment {
                Alignment::Left => align::LEFT,
                Alignment::Center => align::CENTER,
                Alignment::Right => align::RIGHT,
                Alignment::Justify => align::JUSTIFIED,
            }
        ));
        // Canonical `left_indent_pt` is where the paragraph body sits (OOXML semantics);
        // UDF `LeftIndent` is where the FIRST line sits, with `Hanging` the offset to the
        // body. Decompose so the round trip is exact (docs/qa audit).
        let udf_left = p.left_indent_pt - p.hanging_pt;
        if udf_left.abs() > 0.001 {
            a.push_str(&format!(" LeftIndent=\"{}\"", udf_float(udf_left)));
        }
        if p.right_indent_pt != 0.0 {
            a.push_str(&format!(
                " RightIndent=\"{}\"",
                udf_float(p.right_indent_pt)
            ));
        }
        if p.first_line_indent_pt != 0.0 {
            a.push_str(&format!(
                " FirstLineIndent=\"{}\"",
                udf_float(p.first_line_indent_pt)
            ));
        }
        if p.hanging_pt != 0.0 {
            a.push_str(&format!(" Hanging=\"{}\"", udf_float(p.hanging_pt)));
        }
        if p.space_before_pt != 0.0 {
            a.push_str(&format!(" SpaceAbove=\"{}\"", udf_float(p.space_before_pt)));
        }
        if p.space_after_pt != 0.0 {
            a.push_str(&format!(" SpaceBelow=\"{}\"", udf_float(p.space_after_pt)));
        }
        if p.line_spacing_extra != 0.0 {
            a.push_str(&format!(
                " LineSpacing=\"{}\"",
                udf_float(p.line_spacing_extra)
            ));
        }
        if !p.tab_stops.is_empty() {
            let mut stops: Vec<f32> = p.tab_stops.iter().map(|t| udf_round(t.pos_pt)).collect();
            if p.tab_stops.iter().any(|t| t.align != TabAlign::Left) {
                self.warn.warn(WarningCode::TabAlignmentApproximated);
            }
            stops.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
            stops.dedup();
            let s: Vec<String> = stops
                .iter()
                .map(|v| format!("{}:0:0", udf_float(*v)))
                .collect();
            a.push_str(&format!(" TabSet=\"{}\"", s.join(",")));
        }
        if let Some(l) = &p.list {
            a.push_str(&format!(
                " ListId=\"{}\" ListLevel=\"{}\"",
                l.list_id, l.level
            ));
            if l.level > 1 {
                self.warn.warn(WarningCode::ListNestingUnverified);
            }
            match doc
                .lists
                .iter()
                .find(|d| d.list_id == l.list_id)
                .map(|d| &d.kind)
            {
                Some(ListKind::Bullet(BulletKind::Ellipse)) => {
                    a.push_str(&format!(
                        " Bulleted=\"true\" BulletType=\"{}\"",
                        list_tokens::BULLET_ELLIPSE
                    ));
                }
                Some(ListKind::Number(k)) => {
                    let tok = match k {
                        NumberKind::DecimalDot => list_tokens::NUMBER_DOT,
                        NumberKind::LowerAlphaParen => list_tokens::CHAR_SMALL_PAREN,
                    };
                    a.push_str(&format!(" Numbered=\"true\" NumberType=\"{tok}\""));
                }
                None => {
                    a.push_str(&format!(
                        " Numbered=\"true\" NumberType=\"{}\"",
                        list_tokens::NUMBER_DOT
                    ));
                }
            }
        }
        a
    }

    fn run_attrs(&mut self, r: &RunProps, doc: &Document) -> String {
        let mut a = String::new();
        if r.bold {
            a.push_str(" bold=\"true\"");
        }
        if r.italic {
            a.push_str(" italic=\"true\"");
        }
        if r.underline {
            a.push_str(" underline=\"true\"");
        }
        // Not representable (EXP-003). Report precisely rather than inventing an attribute.
        if r.strike {
            self.warn
                .push(Warning::new(WarningCode::RunFeatureDropped).detail("üstü çizili"));
        }
        match r.vert_align {
            VertAlign::Superscript => self
                .warn
                .push(Warning::new(WarningCode::RunFeatureDropped).detail("üst simge")),
            VertAlign::Subscript => self
                .warn
                .push(Warning::new(WarningCode::RunFeatureDropped).detail("alt simge")),
            VertAlign::Baseline => {}
        }
        if let Some(f) = &r.font_family {
            if *f != doc.meta.default_font.family {
                a.push_str(&format!(" family=\"{}\"", esc_attr(f)));
            }
        }
        if let Some(s) = r.font_size_pt {
            if (s - doc.meta.default_font.size_pt).abs() > 0.01 {
                a.push_str(&format!(" size=\"{}\"", s.round() as i32));
            }
        }
        if let Some(c) = r.color {
            if c != Color::BLACK {
                a.push_str(&format!(" foreground=\"{}\"", c.to_java_argb()));
            }
        }
        a
    }
}

fn transcode_to_png(bytes: &[u8]) -> std::result::Result<Vec<u8>, image::ImageError> {
    let img = image::load_from_memory(bytes)?;
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

/// UDF column widths are relative integers. Normalise, guaranteeing every column is >= 1
/// so a zero-width column cannot collapse a table.
fn normalized_widths(src: &[f32], cols: usize) -> Vec<u32> {
    let mut v: Vec<u32> = (0..cols)
        .map(|i| src.get(i).copied().unwrap_or(1.0).max(1.0).round() as u32)
        .collect();
    if v.iter().all(|w| *w == 0) {
        v = vec![1; cols];
    }
    v
}

/// Widths for a row that spans columns: distribute the grid widths across the row's actual
/// cells according to each cell's grid span (EXP-006).
fn row_widths(row: &TableRow, grid: &[u32], cols: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(row.cells.len());
    let mut col = 0usize;
    for cell in &row.cells {
        let span = cell.grid_span.max(1);
        let mut w = 0u32;
        for k in 0..span {
            if col + k < cols {
                w += grid.get(col + k).copied().unwrap_or(1);
            }
        }
        out.push(w.max(1));
        col += span;
    }
    out
}

/// Package a `content.xml` into the UDF ZIP container.
///
/// Only `content.xml` is written. We never fabricate a `sign.sgn`: an electronic signature
/// we did not compute would be a forgery (EXP-001).
pub fn package(content_xml: &[u8]) -> Result<Vec<u8>> {
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cur);
        let opts: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zw.start_file("content.xml", opts)
            .map_err(|e| ConvError::new(ErrorCode::OutputWriteFailed, e.to_string()))?;
        zw.write_all(content_xml)
            .map_err(|e| ConvError::new(ErrorCode::OutputWriteFailed, e.to_string()))?;
        zw.finish()
            .map_err(|e| ConvError::new(ErrorCode::OutputWriteFailed, e.to_string()))?;
    }
    Ok(cur.into_inner())
}

/// Re-parse emitted `content.xml` and assert the invariant that makes UDF valid: the
/// element spans must tile the content buffer exactly, with no gap, overlap or overrun.
///
/// This runs on every conversion. A document that fails it is never written to disk;
/// the user gets `OUTPUT_VALIDATION_FAILED` and an untouched source.
pub fn validate(content_xml: &[u8]) -> Result<()> {
    let s = std::str::from_utf8(content_xml)
        .map_err(|e| ConvError::new(ErrorCode::OutputValidationFailed, e.to_string()))?;

    let start = s
        .find("<![CDATA[")
        .ok_or_else(|| ConvError::new(ErrorCode::OutputValidationFailed, "no content buffer"))?
        + "<![CDATA[".len();
    let end = s[start..]
        .rfind("]]></content>")
        .ok_or_else(|| ConvError::new(ErrorCode::OutputValidationFailed, "unterminated buffer"))?
        + start;
    // Undo the CDATA split-escape before measuring, exactly as the reader does, or a
    // document containing a literal "]]>" is rejected by its own validator.
    let buf = s[start..end].replace("]]]]><![CDATA[>", "]]>");
    let buf_len: usize = buf.encode_utf16().count();

    // Collect every (startOffset, length) leaf in document order.
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let tail = &s[end..];
    let mut rest = tail;
    while let Some(i) = rest.find("startOffset=\"") {
        let after = &rest[i + "startOffset=\"".len()..];
        let j = after.find('"').ok_or_else(|| {
            ConvError::new(ErrorCode::OutputValidationFailed, "malformed startOffset")
        })?;
        let so: usize = after[..j].parse().map_err(|_| {
            ConvError::new(ErrorCode::OutputValidationFailed, "non-numeric startOffset")
        })?;
        let after2 = &after[j..];
        let k = after2.find("length=\"").ok_or_else(|| {
            ConvError::new(
                ErrorCode::OutputValidationFailed,
                "startOffset without length",
            )
        })?;
        let after3 = &after2[k + "length=\"".len()..];
        let m = after3
            .find('"')
            .ok_or_else(|| ConvError::new(ErrorCode::OutputValidationFailed, "malformed length"))?;
        let ln: usize = after3[..m]
            .parse()
            .map_err(|_| ConvError::new(ErrorCode::OutputValidationFailed, "non-numeric length"))?;
        spans.push((so, ln));
        rest = &after3[m..];
    }

    if spans.is_empty() {
        return Err(ConvError::new(
            ErrorCode::OutputValidationFailed,
            "no addressable elements",
        ));
    }

    spans.sort_unstable();
    let mut expect = 0usize;
    for (so, ln) in &spans {
        if *so != expect {
            return Err(ConvError::new(
                ErrorCode::OutputValidationFailed,
                format!("offset discontinuity: expected {expect}, found {so}"),
            ));
        }
        expect = so + ln;
    }
    if expect != buf_len {
        return Err(ConvError::new(
            ErrorCode::OutputValidationFailed,
            format!("elements cover {expect} of {buf_len} buffer units"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn doc_with(blocks: Vec<Block>) -> Document {
        Document {
            meta: Metadata::default(),
            sections: vec![Section {
                blocks,
                ..Default::default()
            }],
            lists: vec![],
        }
    }

    fn emit(doc: &Document) -> (String, Vec<Warning>) {
        let mut w = WarningSink::new();
        let xml = write_content_xml(doc, &mut w).unwrap();
        validate(&xml).expect("emitted UDF must satisfy the tiling invariant");
        (String::from_utf8(xml).unwrap(), w.into_vec())
    }

    #[test]
    fn plain_paragraphs_tile_the_buffer() {
        let doc = doc_with(vec![
            Block::Paragraph(Paragraph::plain("Bir")),
            Block::Paragraph(Paragraph::plain("İki")),
        ]);
        let (xml, _) = emit(&doc);
        assert!(xml.contains("Bir\nİki\n"));
    }

    #[test]
    fn turkish_text_survives_verbatim() {
        let t = "Çağdaş Türkiye Cumhuriyeti — İİK, HMK, ıİğĞüÜşŞöÖçÇ";
        let doc = doc_with(vec![Block::Paragraph(Paragraph::plain(t))]);
        let (xml, _) = emit(&doc);
        assert!(
            xml.contains(t),
            "Turkish text must round-trip byte-for-byte"
        );
    }

    #[test]
    fn offsets_are_utf16() {
        // An astral character is 2 UTF-16 units; Java counts 2, so we must too.
        let doc = doc_with(vec![Block::Paragraph(Paragraph::plain("a\u{1F600}b"))]);
        let (xml, _) = emit(&doc);
        assert!(xml.contains("startOffset=\"0\" length=\"4\""), "got: {xml}");
    }

    #[test]
    fn a_tab_is_both_a_character_and_an_element() {
        let doc = doc_with(vec![Block::Paragraph(Paragraph::plain("a\tb"))]);
        let (xml, _) = emit(&doc);
        assert!(xml.contains("<tab"), "tab element missing");
        let cd = &xml[xml.find("<![CDATA[").unwrap()..];
        assert!(cd.contains("a\tb"), "literal tab missing from buffer");
    }

    #[test]
    fn an_image_costs_exactly_one_offset() {
        let png = crate::testutil::minimal_png();
        let doc = doc_with(vec![Block::Paragraph(Paragraph {
            props: ParaProps::default(),
            runs: vec![Run::Image(Image {
                data: Arc::new(png),
                format: ImageFormat::Png,
                width_pt: 10.0,
                height_pt: 20.0,
            })],
        })]);
        let (xml, _) = emit(&doc);
        assert!(xml.contains("<image imageData=\"iVBOR"));
        assert!(xml.contains(IMAGE_PLACEHOLDER.to_string().as_str()));
    }

    #[test]
    fn strike_and_superscript_warn_rather_than_invent_attributes() {
        let doc = doc_with(vec![Block::Paragraph(Paragraph {
            props: ParaProps::default(),
            runs: vec![Run::Text {
                text: "x".into(),
                props: RunProps {
                    strike: true,
                    vert_align: VertAlign::Superscript,
                    ..Default::default()
                },
            }],
        })]);
        let (xml, warns) = emit(&doc);
        assert!(
            !xml.contains("strike"),
            "must not invent a strikethrough attribute"
        );
        assert!(!xml.contains("Superscript"));
        assert_eq!(
            warns
                .iter()
                .filter(|w| w.code == WarningCode::RunFeatureDropped)
                .count(),
            2
        );
    }

    #[test]
    fn horizontal_merge_emits_row_level_column_spans() {
        let t = Table {
            column_widths: vec![100.0, 100.0, 100.0],
            rows: vec![TableRow {
                cells: vec![
                    TableCell {
                        grid_span: 2,
                        ..Default::default()
                    },
                    TableCell {
                        grid_span: 1,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let doc = doc_with(vec![Block::Table(t)]);
        let (xml, _) = emit(&doc);
        assert!(xml.contains("columnCount=\"3\""));
        // The merged cell's width is the sum of the two grid columns it covers.
        assert!(
            xml.contains("<row rowName=\"row1\" rowType=\"dataRow\" columnSpans=\"200,100\">"),
            "got {xml}"
        );
    }

    #[test]
    fn empty_cells_still_get_a_paragraph_so_tiling_holds() {
        let t = Table {
            column_widths: vec![50.0, 50.0],
            rows: vec![TableRow {
                cells: vec![TableCell::default(), TableCell::default()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let doc = doc_with(vec![Block::Table(t)]);
        // validate() inside emit() is the real assertion here.
        let (xml, _) = emit(&doc);
        assert_eq!(xml.matches("<cell").count(), 2);
    }

    #[test]
    fn line_spacing_and_alignment_use_the_observed_encoding() {
        let doc = doc_with(vec![Block::Paragraph(Paragraph {
            props: ParaProps {
                alignment: Alignment::Justify,
                line_spacing_extra: 0.5,
                ..Default::default()
            },
            runs: vec![Run::Text {
                text: "x".into(),
                props: RunProps::default(),
            }],
        })]);
        let (xml, _) = emit(&doc);
        assert!(xml.contains("Alignment=\"3\""));
        assert!(xml.contains("LineSpacing=\"0.5\""));
    }

    #[test]
    fn validator_catches_a_deliberately_corrupted_offset() {
        let doc = doc_with(vec![Block::Paragraph(Paragraph::plain("abc"))]);
        let mut w = WarningSink::new();
        let xml = write_content_xml(&doc, &mut w).unwrap();
        let broken = String::from_utf8(xml)
            .unwrap()
            .replace("startOffset=\"0\"", "startOffset=\"7\"");
        let e = validate(broken.as_bytes()).unwrap_err();
        assert_eq!(e.code, ErrorCode::OutputValidationFailed);
    }

    #[test]
    fn package_produces_a_readable_zip_with_only_content_xml() {
        let doc = doc_with(vec![Block::Paragraph(Paragraph::plain("x"))]);
        let mut w = WarningSink::new();
        let xml = write_content_xml(&doc, &mut w).unwrap();
        let zipped = package(&xml).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(zipped)).unwrap();
        assert_eq!(z.len(), 1);
        assert_eq!(z.by_index(0).unwrap().name(), "content.xml");
    }
}
