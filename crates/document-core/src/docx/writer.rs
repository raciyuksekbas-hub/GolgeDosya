//! Canonical model -> DOCX.
//!
//! Emits a minimal but fully valid WordprocessingML package: `[Content_Types].xml`,
//! `_rels/.rels`, `word/document.xml`, `word/styles.xml`, `word/numbering.xml`,
//! `word/_rels/document.xml.rels`, plus header/footer parts and `word/media/*`.
//!
//! Every part that is referenced is declared in `[Content_Types].xml` and reachable through
//! a relationship — that is what `writer::validate` re-checks before the bytes are allowed
//! out, because a package Word refuses to open is worse than a failed conversion.

use std::io::Write;

use crate::docx::*;
use crate::error::{ConvError, ErrorCode, Result};
use crate::model::*;
use crate::units::{pt_to_emu, pt_to_halfpoints, pt_to_twips};
use crate::warnings::{Warning, WarningCode, WarningSink};
use crate::xml::{esc_attr, esc_text, sanitize_text};

/// Word rejects a paragraph carrying more than this many custom tab stops, reporting the
/// whole document as corrupt rather than ignoring the excess.
const MAX_TAB_STOPS: usize = 64;
/// Upper bound Word accepts for `w:tab/@w:pos` and indent measures, in twips (22 inches).
const MAX_POSITION_TWIPS: i64 = 31_680;

struct Media {
    name: String,
    bytes: std::sync::Arc<Vec<u8>>,
    ext: &'static str,
}

/// Package-wide media pool. Image *bytes* live once in `word/media/`; each part that shows
/// an image gets its own relationship pointing at that shared target.
#[derive(Default)]
struct MediaPool {
    items: Vec<Media>,
}

impl MediaPool {
    /// Register an image and return its package-relative target (`media/imageN.png`).
    fn add(&mut self, img: &Image) -> String {
        let idx = self.items.len() + 1;
        let ext = img.format.extension();
        let name = format!("media/image{idx}.{ext}");
        self.items.push(Media {
            name: name.clone(),
            bytes: img.data.clone(),
            ext,
        });
        name
    }
}

/// Relationships for ONE package part.
///
/// OOXML relationships are part-scoped: a relationship referenced from `header1.xml` must
/// live in `word/_rels/header1.xml.rels`, and its `rId` numbering is independent of every
/// other part. Emitting a header image's relationship into the document part's `.rels` —
/// which is what Tavzih did — leaves the reference unresolvable and Word renders
/// "Resim görüntülenemiyor."
#[derive(Default)]
struct Rels {
    items: Vec<(String, &'static str, String)>, // (rid, type, target)
    next: usize,
}

impl Rels {
    fn add(&mut self, ty: &'static str, target: impl Into<String>) -> String {
        self.next += 1;
        let rid = format!("rId{}", self.next);
        self.items.push((rid.clone(), ty, target.into()));
        rid
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn to_xml(&self) -> String {
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        s.push_str(&format!("<Relationships xmlns=\"{NS_PKG_REL}\">"));
        for (id, ty, tgt) in &self.items {
            s.push_str(&format!(
                "<Relationship Id=\"{id}\" Type=\"{ty}\" Target=\"{}\"/>",
                esc_attr(tgt)
            ));
        }
        s.push_str("</Relationships>");
        s
    }
}

/// Build the whole `.docx` package.
pub fn write_docx(doc: &Document, warn: &mut WarningSink) -> Result<Vec<u8>> {
    let section = doc
        .sections
        .first()
        .ok_or_else(|| ConvError::engine("document has no section"))?;

    // One relationship scope per part; one shared pool for the image bytes.
    // The usable text column: a tab stop or indent beyond it is unreachable.
    let text_width_tw = pt_to_twips(
        (section.page.width_pt - section.page.margin_left_pt - section.page.margin_right_pt)
            .max(72.0),
    );

    let mut doc_rels = Rels::default();
    let mut media = MediaPool::default();

    doc_rels.add(REL_STYLES, "styles.xml");
    let has_lists = !doc.lists.is_empty();
    if has_lists {
        doc_rels.add(REL_NUMBERING, "numbering.xml");
    }

    // Header and footer parts are built first: each returns its own relationship set, and
    // any image inside them registers in THAT set, not in the document's.
    let header = match &section.header {
        Some(h) => Some(hf_part("hdr", h, doc, &mut media, warn, text_width_tw)?),
        None => None,
    };
    let footer = match &section.footer {
        Some(f) => Some(hf_part("ftr", f, doc, &mut media, warn, text_width_tw)?),
        None => None,
    };
    let header_rid = header
        .as_ref()
        .map(|_| doc_rels.add(REL_HEADER, "header1.xml"));
    let footer_rid = footer
        .as_ref()
        .map(|_| doc_rels.add(REL_FOOTER, "footer1.xml"));

    let mut body = String::new();
    write_blocks(
        &section.blocks,
        doc,
        &mut body,
        &mut doc_rels,
        &mut media,
        warn,
        text_width_tw,
    )?;
    for extra in doc.sections.iter().skip(1) {
        write_blocks(
            &extra.blocks,
            doc,
            &mut body,
            &mut doc_rels,
            &mut media,
            warn,
            text_width_tw,
        )?;
    }
    if doc.sections.len() > 1 {
        warn.push(
            Warning::new(WarningCode::MultiSectionFlattened)
                .detail(format!("{} bölüm", doc.sections.len())),
        );
    }
    // Word requires at least one paragraph in the body.
    if body.is_empty() {
        body.push_str("<w:p/>");
    }
    body.push_str(&sect_pr(
        section,
        header_rid.as_deref(),
        footer_rid.as_deref(),
    ));

    let document_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"{NS_W}\" xmlns:r=\"{NS_R}\" xmlns:wp=\"{NS_WP}\" \
         xmlns:a=\"{NS_A}\" xmlns:pic=\"{NS_PIC}\"><w:body>{body}</w:body></w:document>"
    );

    // --- assemble the package ---
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let put = |z: &mut zip::ZipWriter<_>, name: &str, data: &[u8]| -> Result<()> {
            z.start_file(name, o)
                .map_err(|e| ConvError::new(ErrorCode::OutputWriteFailed, e.to_string()))?;
            z.write_all(data)
                .map_err(|e| ConvError::new(ErrorCode::OutputWriteFailed, e.to_string()))?;
            Ok(())
        };

        put(
            &mut z,
            "[Content_Types].xml",
            content_types(&media, has_lists, header.is_some(), footer.is_some()).as_bytes(),
        )?;
        put(&mut z, "_rels/.rels", package_rels().as_bytes())?;
        put(&mut z, "word/document.xml", document_xml.as_bytes())?;
        put(
            &mut z,
            "word/_rels/document.xml.rels",
            doc_rels.to_xml().as_bytes(),
        )?;
        put(&mut z, "word/styles.xml", styles_xml(doc).as_bytes())?;
        if has_lists {
            put(&mut z, "word/numbering.xml", numbering_xml(doc).as_bytes())?;
        }
        for (part, built) in [("header1", &header), ("footer1", &footer)] {
            if let Some((xml, rels)) = built {
                put(&mut z, &format!("word/{part}.xml"), xml.as_bytes())?;
                // Only write a .rels for a part that actually has relationships; an empty
                // one is legal but noise.
                if !rels.is_empty() {
                    put(
                        &mut z,
                        &format!("word/_rels/{part}.xml.rels"),
                        rels.to_xml().as_bytes(),
                    )?;
                }
            }
        }
        for m in &media.items {
            put(&mut z, &format!("word/{}", m.name), m.bytes.as_ref())?;
        }
        z.finish()
            .map_err(|e| ConvError::new(ErrorCode::OutputWriteFailed, e.to_string()))?;
    }
    Ok(cur.into_inner())
}

fn package_rels() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <Relationships xmlns=\"{NS_PKG_REL}\">\
         <Relationship Id=\"rId1\" Type=\"{REL_DOCUMENT}\" Target=\"word/document.xml\"/>\
         </Relationships>"
    )
}

fn content_types(media: &MediaPool, numbering: bool, header: bool, footer: bool) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    s.push_str(&format!("<Types xmlns=\"{NS_CT}\">"));
    s.push_str("<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>");
    s.push_str("<Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    let mut seen: Vec<&str> = Vec::new();
    for m in &media.items {
        if !seen.contains(&m.ext) {
            seen.push(m.ext);
            let ct = match m.ext {
                "png" => "image/png",
                "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "bmp" => "image/bmp",
                _ => "application/octet-stream",
            };
            s.push_str(&format!(
                "<Default Extension=\"{}\" ContentType=\"{ct}\"/>",
                m.ext
            ));
        }
    }
    s.push_str("<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>");
    s.push_str("<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>");
    if numbering {
        s.push_str("<Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/>");
    }
    if header {
        s.push_str("<Override PartName=\"/word/header1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/>");
    }
    if footer {
        s.push_str("<Override PartName=\"/word/footer1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml\"/>");
    }
    s.push_str("</Types>");
    s
}

fn styles_xml(doc: &Document) -> String {
    let f = &doc.meta.default_font;
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:styles xmlns:w=\"{NS_W}\">\
         <w:docDefaults><w:rPrDefault><w:rPr>\
         <w:rFonts w:ascii=\"{fam}\" w:hAnsi=\"{fam}\" w:cs=\"{fam}\"/>\
         <w:sz w:val=\"{sz}\"/><w:szCs w:val=\"{sz}\"/>\
         </w:rPr></w:rPrDefault>\
         <w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault>\
         </w:docDefaults>\
         <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\">\
         <w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
         </w:styles>",
        fam = esc_attr(&f.family),
        sz = pt_to_halfpoints(f.size_pt)
    )
}

fn numbering_xml(doc: &Document) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    s.push_str(&format!("<w:numbering xmlns:w=\"{NS_W}\">"));
    for l in &doc.lists {
        let (fmt, txt, font) = match &l.kind {
            ListKind::Bullet(BulletKind::Ellipse) => {
                ("bullet", "\u{2022}".to_string(), Some("Symbol"))
            }
            ListKind::Number(NumberKind::DecimalDot) => ("decimal", "%1.".to_string(), None),
            ListKind::Number(NumberKind::LowerAlphaParen) => {
                ("lowerLetter", "%1)".to_string(), None)
            }
        };
        s.push_str(&format!(
            "<w:abstractNum w:abstractNumId=\"{id}\">",
            id = l.list_id
        ));
        // Emit nine levels so nested lists have somewhere to land.
        for lvl in 0..9u32 {
            let ind = 720 + 360 * lvl as i64;
            s.push_str(&format!(
                "<w:lvl w:ilvl=\"{lvl}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{fmt}\"/>\
                 <w:lvlText w:val=\"{txt}\"/><w:lvlJc w:val=\"left\"/>\
                 <w:pPr><w:ind w:left=\"{ind}\" w:hanging=\"360\"/></w:pPr>{rpr}</w:lvl>",
                txt = esc_attr(&txt),
                rpr = match font {
                    Some(fo) =>
                        format!("<w:rPr><w:rFonts w:ascii=\"{fo}\" w:hAnsi=\"{fo}\"/></w:rPr>"),
                    None => String::new(),
                }
            ));
        }
        s.push_str("</w:abstractNum>");
    }
    for l in &doc.lists {
        s.push_str(&format!(
            "<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"{id}\"/></w:num>",
            id = l.list_id
        ));
    }
    s.push_str("</w:numbering>");
    s
}

fn sect_pr(s: &Section, header_rid: Option<&str>, footer_rid: Option<&str>) -> String {
    let p = &s.page;
    let (w, h) = (p.width_pt, p.height_pt);
    let orient = match p.orientation {
        Orientation::Landscape => " w:orient=\"landscape\"",
        Orientation::Portrait => "",
    };
    let mut out = String::from("<w:sectPr>");
    if let Some(r) = header_rid {
        out.push_str(&format!(
            "<w:headerReference w:type=\"default\" r:id=\"{r}\"/>"
        ));
    }
    if let Some(r) = footer_rid {
        out.push_str(&format!(
            "<w:footerReference w:type=\"default\" r:id=\"{r}\"/>"
        ));
    }
    out.push_str(&format!(
        "<w:pgSz w:w=\"{}\" w:h=\"{}\"{orient}/>",
        pt_to_twips(w),
        pt_to_twips(h)
    ));
    out.push_str(&format!(
        "<w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"0\"/>",
        pt_to_twips(p.margin_top_pt),
        pt_to_twips(p.margin_right_pt),
        pt_to_twips(p.margin_bottom_pt),
        pt_to_twips(p.margin_left_pt),
        pt_to_twips(p.header_offset_pt),
        pt_to_twips(p.footer_offset_pt),
    ));
    out.push_str("</w:sectPr>");
    out
}

/// Build a header or footer part **with its own relationship scope**.
fn hf_part(
    tag: &str,
    hf: &HeaderFooter,
    doc: &Document,
    media: &mut MediaPool,
    warn: &mut WarningSink,
    text_width_tw: i64,
) -> Result<(String, Rels)> {
    let mut rels = Rels::default();
    let mut inner = String::new();
    write_blocks(
        &hf.blocks,
        doc,
        &mut inner,
        &mut rels,
        media,
        warn,
        text_width_tw,
    )?;

    // A UDF page-number field becomes a real, updatable Word PAGE field rather than a
    // frozen number or a dropped feature (EXP-013).
    if let Some(pn) = &hf.page_number {
        inner.push_str(&page_number_paragraph(pn));
        warn.warn(WarningCode::PageNumberFieldApproximated);
    } else if !hf.page_number_attrs.is_empty() {
        warn.warn(WarningCode::PageNumberFieldDropped);
    }

    if inner.is_empty() {
        inner.push_str("<w:p/>");
    }
    Ok((
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:{tag} xmlns:w=\"{NS_W}\" xmlns:r=\"{NS_R}\" xmlns:wp=\"{NS_WP}\" \
         xmlns:a=\"{NS_A}\" xmlns:pic=\"{NS_PIC}\">{inner}</w:{tag}>"
        ),
        rels,
    ))
}

/// A paragraph containing a genuine Word `PAGE` field, and `NUMPAGES` when the source
/// showed a "current / total" layout. Word recalculates these on repagination, which a
/// hard-coded number would not.
fn page_number_paragraph(pn: &PageNumberField) -> String {
    let mut rpr = String::new();
    if let Some(f) = &pn.font_family {
        rpr.push_str(&format!(
            "<w:rFonts w:ascii=\"{f}\" w:hAnsi=\"{f}\" w:cs=\"{f}\"/>",
            f = esc_attr(f)
        ));
    }
    if pn.bold {
        rpr.push_str("<w:b/>");
    }
    if pn.italic {
        rpr.push_str("<w:i/>");
    }
    if let Some(c) = pn.color {
        if c != Color::BLACK {
            rpr.push_str(&format!("<w:color w:val=\"{}\"/>", c.to_hex()));
        }
    }
    if let Some(sz) = pn.font_size_pt {
        let hp = pt_to_halfpoints(sz);
        rpr.push_str(&format!("<w:sz w:val=\"{hp}\"/><w:szCs w:val=\"{hp}\"/>"));
    }
    let rpr = if rpr.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{rpr}</w:rPr>")
    };

    let text_run = |t: &str| {
        format!(
            "<w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
            esc_text(t)
        )
    };
    // A complex field: begin, instruction, separate, cached result, end.
    let field = |instr: &str| {
        format!(
            "<w:r>{rpr}<w:fldChar w:fldCharType=\"begin\"/></w:r>\
             <w:r>{rpr}<w:instrText xml:space=\"preserve\"> {instr} </w:instrText></w:r>\
             <w:r>{rpr}<w:fldChar w:fldCharType=\"separate\"/></w:r>\
             <w:r>{rpr}<w:t>1</w:t></w:r>\
             <w:r>{rpr}<w:fldChar w:fldCharType=\"end\"/></w:r>"
        )
    };

    let mut body = String::new();
    if let Some(p) = &pn.prefix {
        body.push_str(&text_run(p));
    }
    body.push_str(&field("PAGE"));
    if pn.has_total() {
        body.push_str(&text_run(pn.separator.as_deref().unwrap_or("/")));
        body.push_str(&field("NUMPAGES"));
    }
    // Page numbers are centred in every observed specimen; UDF carries no alignment for
    // them, so this is stated as an approximation rather than a certainty.
    format!("<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>{body}</w:p>")
}

/// Write a sequence of blocks.
///
/// OOXML requires a paragraph after a table (and at the end of a `w:tc`), otherwise Word
/// reports the file as damaged. That filler is *structure*, not content — emitting it
/// unconditionally makes every DOCX -> UDF -> DOCX cycle grow one blank line per table.
/// So it is emitted only where it is actually needed: when a table is last, or when two
/// tables are adjacent. A paragraph that already follows serves the same purpose.
fn write_blocks(
    blocks: &[Block],
    doc: &Document,
    out: &mut String,
    rels: &mut Rels,
    media: &mut MediaPool,
    warn: &mut WarningSink,
    text_width_tw: i64,
) -> Result<()> {
    for (i, b) in blocks.iter().enumerate() {
        write_block(b, doc, out, rels, media, warn, text_width_tw)?;
        if matches!(b, Block::Table(_)) {
            let next_is_paragraph = matches!(
                blocks.get(i + 1),
                Some(Block::Paragraph(_)) | Some(Block::PageBreak)
            );
            if !next_is_paragraph {
                out.push_str("<w:p/>");
            }
        }
    }
    Ok(())
}

fn write_block(
    b: &Block,
    doc: &Document,
    out: &mut String,
    rels: &mut Rels,
    media: &mut MediaPool,
    warn: &mut WarningSink,
    text_width_tw: i64,
) -> Result<()> {
    match b {
        Block::Paragraph(p) => write_paragraph(p, doc, out, rels, media, warn, None, text_width_tw),
        Block::Table(t) => write_table(t, doc, out, rels, media, warn, text_width_tw),
        Block::PageBreak => {
            out.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write_paragraph(
    p: &Paragraph,
    doc: &Document,
    out: &mut String,
    rels: &mut Rels,
    media: &mut MediaPool,
    warn: &mut WarningSink,
    extra_ppr: Option<&str>,
    text_width_tw: i64,
) -> Result<()> {
    out.push_str("<w:p>");
    out.push_str(&ppr(&p.props, extra_ppr, text_width_tw, warn));
    for r in &p.runs {
        match r {
            Run::Text { text, props } => {
                let t = sanitize_text(text);
                if t.is_empty() {
                    continue;
                }
                // A literal tab inside <w:t> is not a tab to Word; it needs <w:tab/>.
                // Tab stops carry legal pleadings, so this must be exact.
                for (i, piece) in t.split('\t').enumerate() {
                    if i > 0 {
                        out.push_str(&format!("<w:r>{}<w:tab/></w:r>", rpr(props, doc)));
                    }
                    if !piece.is_empty() {
                        out.push_str(&format!(
                            "<w:r>{}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
                            rpr(props, doc),
                            esc_text(piece)
                        ));
                    }
                }
            }
            Run::Tab { props } => {
                out.push_str(&format!("<w:r>{}<w:tab/></w:r>", rpr(props, doc)));
            }
            Run::LineBreak { props } => {
                out.push_str(&format!("<w:r>{}<w:br/></w:r>", rpr(props, doc)));
            }
            Run::Image(img) => {
                out.push_str(&write_image(img, rels, media, warn));
            }
        }
    }
    out.push_str("</w:p>");
    Ok(())
}

fn write_image(
    img: &Image,
    rels: &mut Rels,
    media: &mut MediaPool,
    warn: &mut WarningSink,
) -> String {
    if img.format == ImageFormat::Unsupported {
        warn.push(Warning::new(WarningCode::ImageFormatUnsupported).detail("bilinmeyen"));
        return String::new();
    }
    // Bytes go into the shared package pool; the relationship goes into the CALLING
    // PART's own scope, which is what makes a header image resolvable from header1.xml.
    let target = media.add(img);
    let idx = media.items.len();
    let rid = rels.add(REL_IMAGE, target);

    // Fall back to a sane default when the source gave no extent.
    let (w, h) = if img.width_pt > 0.5 && img.height_pt > 0.5 {
        (img.width_pt, img.height_pt)
    } else {
        (72.0, 72.0)
    };
    let (cx, cy) = (pt_to_emu(w), pt_to_emu(h));
    format!(
        "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
         <wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{idx}\" name=\"Resim {idx}\"/>\
         <a:graphic xmlns:a=\"{NS_A}\"><a:graphicData uri=\"{NS_PIC}\">\
         <pic:pic xmlns:pic=\"{NS_PIC}\">\
         <pic:nvPicPr><pic:cNvPr id=\"{idx}\" name=\"Resim {idx}\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\
         </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )
}

/// Emit `w:pPr`.
///
/// Children are written in CT_PPr schema order (numPr, tabs, spacing, ind, jc); Word is
/// strict about this sequence.
///
/// Tab stops are bounded here because UYAP writes the editor's full default ruler — real
/// documents carry 455 stops running to 223 inches, of which only the first dozen fall
/// inside the page. Passing those through produced tens of thousands of `<w:tab>` elements
/// and a document Word refused to open at all.
fn ppr(p: &ParaProps, extra: Option<&str>, text_width_tw: i64, warn: &mut WarningSink) -> String {
    let mut s = String::from("<w:pPr>");
    if let Some(e) = extra {
        s.push_str(e);
    }
    if let Some(l) = &p.list {
        s.push_str(&format!(
            "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{}\"/></w:numPr>",
            l.level.saturating_sub(1),
            l.list_id
        ));
    }

    if !p.tab_stops.is_empty() {
        // A stop beyond the text column can never be reached, so discarding it changes
        // nothing the reader would see. Only losing a *reachable* stop is a fidelity cost.
        let limit = text_width_tw.clamp(1, MAX_POSITION_TWIPS);
        let mut stops: Vec<(i64, TabAlign)> = p
            .tab_stops
            .iter()
            .map(|t| (pt_to_twips(t.pos_pt), t.align))
            .filter(|(pos, _)| *pos > 0 && *pos <= limit)
            .collect();
        stops.sort_by_key(|(pos, _)| *pos);
        stops.dedup_by_key(|(pos, _)| *pos);

        let reachable = stops.len();
        if reachable > MAX_TAB_STOPS {
            stops.truncate(MAX_TAB_STOPS);
            warn.push(Warning::new(WarningCode::TabStopsTrimmed).detail(format!(
                "{reachable} durak, {MAX_TAB_STOPS} sınırına indirildi"
            )));
        }

        if !stops.is_empty() {
            if p.tab_stops.iter().any(|t| t.align != TabAlign::Left) {
                warn.warn(WarningCode::TabAlignmentApproximated);
            }
            s.push_str("<w:tabs>");
            for (pos, align) in &stops {
                let v = match align {
                    TabAlign::Left => "left",
                    TabAlign::Center => "center",
                    TabAlign::Right => "right",
                    TabAlign::Decimal => "decimal",
                };
                s.push_str(&format!("<w:tab w:val=\"{v}\" w:pos=\"{pos}\"/>"));
            }
            s.push_str("</w:tabs>");
        }
    }

    let mut sp = String::new();
    if p.space_before_pt != 0.0 {
        sp.push_str(&format!(" w:before=\"{}\"", clamp_twips(p.space_before_pt)));
    }
    if p.space_after_pt != 0.0 {
        sp.push_str(&format!(" w:after=\"{}\"", clamp_twips(p.space_after_pt)));
    }
    if p.line_spacing_extra != 0.0 {
        let line =
            (((p.line_spacing_extra + 1.0) * 240.0).round() as i64).clamp(1, MAX_POSITION_TWIPS);
        sp.push_str(&format!(" w:line=\"{line}\" w:lineRule=\"auto\""));
    }
    if !sp.is_empty() {
        s.push_str(&format!("<w:spacing{sp}/>"));
    }

    let mut ind = String::new();
    if p.left_indent_pt != 0.0 {
        ind.push_str(&format!(" w:left=\"{}\"", clamp_twips(p.left_indent_pt)));
    }
    if p.right_indent_pt != 0.0 {
        ind.push_str(&format!(" w:right=\"{}\"", clamp_twips(p.right_indent_pt)));
    }
    // w:firstLine and w:hanging are mutually exclusive in OOXML; hanging wins when both are
    // present, matching how Word itself resolves the pair.
    if p.hanging_pt != 0.0 {
        ind.push_str(&format!(" w:hanging=\"{}\"", clamp_twips(p.hanging_pt)));
    } else if p.first_line_indent_pt != 0.0 {
        ind.push_str(&format!(
            " w:firstLine=\"{}\"",
            clamp_twips(p.first_line_indent_pt)
        ));
    }
    if !ind.is_empty() {
        s.push_str(&format!("<w:ind{ind}/>"));
    }

    let jc = match p.alignment {
        Alignment::Left => None,
        Alignment::Center => Some("center"),
        Alignment::Right => Some("right"),
        Alignment::Justify => Some("both"),
    };
    if let Some(j) = jc {
        s.push_str(&format!("<w:jc w:val=\"{j}\"/>"));
    }

    s.push_str("</w:pPr>");
    s
}

/// Keep a measure inside the range Word accepts. A value beyond it makes Word treat the
/// whole document as corrupt rather than clamping it itself.
fn clamp_twips(pt: f32) -> i64 {
    pt_to_twips(pt).clamp(-MAX_POSITION_TWIPS, MAX_POSITION_TWIPS)
}

fn rpr(r: &RunProps, doc: &Document) -> String {
    let mut s = String::new();
    if let Some(f) = &r.font_family {
        if *f != doc.meta.default_font.family {
            s.push_str(&format!(
                "<w:rFonts w:ascii=\"{f}\" w:hAnsi=\"{f}\" w:cs=\"{f}\"/>",
                f = esc_attr(f)
            ));
        }
    }
    if r.bold {
        s.push_str("<w:b/><w:bCs/>");
    }
    if r.italic {
        s.push_str("<w:i/><w:iCs/>");
    }
    if r.strike {
        s.push_str("<w:strike/>");
    }
    if r.underline {
        s.push_str("<w:u w:val=\"single\"/>");
    }
    match r.vert_align {
        VertAlign::Superscript => s.push_str("<w:vertAlign w:val=\"superscript\"/>"),
        VertAlign::Subscript => s.push_str("<w:vertAlign w:val=\"subscript\"/>"),
        VertAlign::Baseline => {}
    }
    if let Some(c) = r.color {
        if c != Color::BLACK {
            s.push_str(&format!("<w:color w:val=\"{}\"/>", c.to_hex()));
        }
    }
    if let Some(sz) = r.font_size_pt {
        if (sz - doc.meta.default_font.size_pt).abs() > 0.01 {
            let hp = pt_to_halfpoints(sz);
            s.push_str(&format!("<w:sz w:val=\"{hp}\"/><w:szCs w:val=\"{hp}\"/>"));
        }
    }
    if s.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{s}</w:rPr>")
    }
}

fn write_table(
    t: &Table,
    doc: &Document,
    out: &mut String,
    rels: &mut Rels,
    media: &mut MediaPool,
    warn: &mut WarningSink,
    text_width_tw: i64,
) -> Result<()> {
    let cols = t.column_count().max(1);
    // Word wants absolute grid widths; scale the relative UDF widths onto the A4 text block.
    let total: f32 = (0..cols)
        .map(|i| t.column_widths.get(i).copied().unwrap_or(1.0).max(1.0))
        .sum();
    let grid_span_tw = text_width_tw as f32; // the section's usable text column, in twips
    out.push_str("<w:tbl><w:tblPr>");
    out.push_str("<w:tblW w:w=\"0\" w:type=\"auto\"/>");
    if t.border == TableBorder::Cell {
        out.push_str(
            "<w:tblBorders>\
             <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
             <w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
             <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
             <w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
             <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
             <w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
             </w:tblBorders>",
        );
    }
    out.push_str("</w:tblPr><w:tblGrid>");
    let mut grid_tw: Vec<i64> = Vec::with_capacity(cols);
    for i in 0..cols {
        let w = t.column_widths.get(i).copied().unwrap_or(1.0).max(1.0);
        let tw = ((w / total) * grid_span_tw).round() as i64;
        grid_tw.push(tw.max(100));
        out.push_str(&format!("<w:gridCol w:w=\"{}\"/>", tw.max(100)));
    }
    out.push_str("</w:tblGrid>");

    for row in &t.rows {
        out.push_str("<w:tr>");
        if row.is_header || row.height_pt.is_some() {
            out.push_str("<w:trPr>");
            if row.is_header {
                out.push_str("<w:tblHeader/>");
            }
            if let Some(h) = row.height_pt {
                out.push_str(&format!("<w:trHeight w:val=\"{}\"/>", pt_to_twips(h)));
            }
            out.push_str("</w:trPr>");
        }
        let mut col = 0usize;
        for cell in &row.cells {
            let span = cell.grid_span.max(1);
            let w: i64 = (col..(col + span).min(cols)).map(|i| grid_tw[i]).sum();
            out.push_str("<w:tc><w:tcPr>");
            out.push_str(&format!("<w:tcW w:w=\"{}\" w:type=\"dxa\"/>", w.max(100)));
            if span > 1 {
                out.push_str(&format!("<w:gridSpan w:val=\"{span}\"/>"));
            }
            match cell.vertical_align {
                CellVAlign::Center => out.push_str("<w:vAlign w:val=\"center\"/>"),
                CellVAlign::Bottom => out.push_str("<w:vAlign w:val=\"bottom\"/>"),
                CellVAlign::Top => {}
            }
            out.push_str("</w:tcPr>");
            if cell.blocks.is_empty() {
                out.push_str("<w:p/>");
            } else {
                write_blocks(&cell.blocks, doc, out, rels, media, warn, text_width_tw)?;
                // A w:tc must end with a paragraph. write_blocks already added one if the
                // cell ends in a table, so only a non-paragraph tail needs help here.
                if !matches!(
                    cell.blocks.last(),
                    Some(Block::Paragraph(_)) | Some(Block::Table(_))
                ) {
                    out.push_str("<w:p/>");
                }
            }
            out.push_str("</w:tc>");
            col += span;
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    // The required trailing paragraph is added by write_blocks only when needed.
    Ok(())
}

/// Structural validation of an emitted package. Runs on every conversion; on failure the
/// output is discarded and the user gets `OUTPUT_VALIDATION_FAILED`.
///
/// The important check is **relationship scoping**: for every part that references an
/// `r:embed` / `r:id`, that id must be defined in *that part's own* `.rels`. A package can
/// open in Word and still show "Resim görüntülenemiyor" when this is wrong, so validating
/// only that the media file exists is not enough.
pub fn validate(bytes: &[u8]) -> Result<()> {
    let fail = |m: String| ConvError::new(ErrorCode::OutputValidationFailed, m);
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| fail(format!("output is not a readable ZIP: {e}")))?;

    let names: Vec<String> = (0..z.len())
        .filter_map(|i| z.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    for required in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/_rels/document.xml.rels",
    ] {
        if !names.contains(&required.to_string()) {
            return Err(fail(format!("output package is missing {required}")));
        }
    }

    let read = |z: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str| -> Result<String> {
        use std::io::Read;
        let mut f = z.by_name(name).map_err(|e| fail(format!("{name}: {e}")))?;
        let mut s = String::new();
        f.read_to_string(&mut s)
            .map_err(|e| fail(format!("{name}: {e}")))?;
        Ok(s)
    };

    // Parse one .rels file into (id -> target).
    let parse_rels = |xml: &str| -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut rest = xml;
        while let Some(i) = rest.find("<Relationship ") {
            let after = &rest[i..];
            let end = after.find("/>").unwrap_or(after.len());
            let tag = &after[..end];
            let grab = |k: &str| -> Option<String> {
                let key = format!("{k}=\"");
                let a = tag.find(&key)? + key.len();
                let b = tag[a..].find('"')? + a;
                Some(tag[a..b].to_string())
            };
            if let (Some(id), Some(t)) = (grab("Id"), grab("Target")) {
                out.push((id, t));
            }
            rest = &after[end.min(after.len())..];
            if rest.is_empty() {
                break;
            }
        }
        out
    };

    // Every part that can reference a relationship, with the .rels that must define it.
    let mut parts: Vec<(String, String)> = vec![(
        "word/document.xml".into(),
        "word/_rels/document.xml.rels".into(),
    )];
    for n in &names {
        if (n.starts_with("word/header") || n.starts_with("word/footer")) && n.ends_with(".xml") {
            let base = n.trim_start_matches("word/");
            parts.push((n.clone(), format!("word/_rels/{base}.rels")));
        }
    }

    for (part, rels_path) in parts {
        let xml = read(&mut z, &part)?;
        let rels_xml = if names.contains(&rels_path) {
            read(&mut z, &rels_path)?
        } else {
            String::new()
        };
        let rels = parse_rels(&rels_xml);

        // Targets must resolve to real parts.
        for (_, target) in &rels {
            if target.starts_with("http") {
                continue;
            }
            let resolved = format!("word/{}", target.trim_start_matches('/'));
            if !names.contains(&resolved) {
                return Err(fail(format!(
                    "{rels_path}: relationship target {resolved} is not in the package"
                )));
            }
        }

        // Every r:embed / r:id used by the part must be defined in the part's own scope.
        for key in ["r:embed=\"", "r:id=\""] {
            let mut rest = xml.as_str();
            while let Some(i) = rest.find(key) {
                let after = &rest[i + key.len()..];
                let j = after
                    .find('"')
                    .ok_or_else(|| fail("malformed r:id".into()))?;
                let rid = &after[..j];
                if !rels.iter().any(|(id, _)| id == rid) {
                    return Err(fail(format!(
                        "{part} references {rid}, which is not defined in {rels_path} \
                         (relationships are part-scoped)"
                    )));
                }
                rest = &after[j..];
            }
        }
    }

    // Word-acceptance limits, not just XML well-formedness.
    //
    // A package can be perfectly well-formed XML, resolve every relationship, and still be
    // refused by Word. That is exactly what happened: UYAP's default ruler produced 16,217
    // <w:tab> elements with positions out to 223 inches, and Word offered Text Recovery
    // instead of opening the file. Validation that only parsed XML reported it as success.
    for part in names.iter().filter(|n| {
        n.as_str() == "word/document.xml"
            || (n.starts_with("word/header") || n.starts_with("word/footer")) && n.ends_with(".xml")
    }) {
        let xml = read(&mut z, part)?;

        // Per-paragraph tab-stop count.
        for (i, block) in xml.split("<w:tabs>").enumerate().skip(1) {
            let end = block.find("</w:tabs>").unwrap_or(block.len());
            let n = block[..end].matches("<w:tab ").count();
            if n > MAX_TAB_STOPS {
                return Err(fail(format!(
                    "{part}: paragraph {i} declares {n} tab stops, more than the {MAX_TAB_STOPS} \
                     Word accepts"
                )));
            }
        }

        // Positions and indents must be inside the range Word accepts.
        for (attr, pat) in [
            ("w:pos", "w:pos=\""),
            ("w:left", "w:left=\""),
            ("w:right", "w:right=\""),
            ("w:hanging", "w:hanging=\""),
            ("w:firstLine", "w:firstLine=\""),
        ] {
            let mut rest = xml.as_str();
            while let Some(i) = rest.find(pat) {
                let after = &rest[i + pat.len()..];
                let j = after.find('"').unwrap_or(0);
                if let Ok(v) = after[..j].parse::<i64>() {
                    if v.abs() > MAX_POSITION_TWIPS {
                        return Err(fail(format!(
                            "{part}: {attr}={v} twips is outside the range Word accepts \
                             (+/-{MAX_POSITION_TWIPS})"
                        )));
                    }
                }
                rest = &after[j..];
            }
        }
    }

    // The document part must re-parse and must declare a body.
    let doc_xml = read(&mut z, "word/document.xml")?;
    if !doc_xml.contains("<w:body>") {
        return Err(fail("document.xml declares no body".into()));
    }
    for part in names.iter().filter(|n| n.ends_with(".xml")) {
        let xml = read(&mut z, part)?;
        let mut rd = quick_xml::Reader::from_str(&xml);
        rd.config_mut().check_end_names = true;
        loop {
            match rd.read_event() {
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => return Err(fail(format!("{part} is not well-formed: {e}"))),
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::reader::read_docx;
    use std::sync::Arc;

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

    fn roundtrip(d: &Document) -> Document {
        let mut w = WarningSink::new();
        let bytes = write_docx(d, &mut w).unwrap();
        validate(&bytes).expect("emitted DOCX must validate");
        let mut w2 = WarningSink::new();
        read_docx(&bytes, &mut w2).unwrap()
    }

    #[test]
    fn emitted_package_validates_and_reopens() {
        let d = doc(vec![Block::Paragraph(Paragraph::plain("Merhaba"))]);
        let back = roundtrip(&d);
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        assert_eq!(p.text(), "Merhaba");
    }

    #[test]
    fn turkish_text_survives_a_docx_roundtrip() {
        let t = "Çağdaş Türkiye Cumhuriyeti — İİK, HMK, ıİğĞüÜşŞöÖçÇ";
        let back = roundtrip(&doc(vec![Block::Paragraph(Paragraph::plain(t))]));
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        assert_eq!(p.text(), t);
    }

    #[test]
    fn run_formatting_survives() {
        let d = doc(vec![Block::Paragraph(Paragraph {
            props: ParaProps::default(),
            runs: vec![Run::Text {
                text: "x".into(),
                props: RunProps {
                    bold: true,
                    italic: true,
                    underline: true,
                    strike: true,
                    font_size_pt: Some(16.0),
                    color: Some(Color {
                        r: 0xC0,
                        g: 0x00,
                        b: 0x00,
                    }),
                    ..Default::default()
                },
            }],
        })]);
        let back = roundtrip(&d);
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        let rp = p.runs[0].props().unwrap();
        assert!(rp.bold && rp.italic && rp.underline && rp.strike);
        assert_eq!(rp.font_size_pt, Some(16.0));
        assert_eq!(
            rp.color,
            Some(Color {
                r: 0xC0,
                g: 0,
                b: 0
            })
        );
    }

    #[test]
    fn paragraph_geometry_survives_exactly() {
        // These values are exact multiples of a twip, so nothing should drift.
        let props = ParaProps {
            alignment: Alignment::Justify,
            left_indent_pt: 36.0,
            right_indent_pt: 18.0,
            hanging_pt: 21.0,
            space_before_pt: 6.0,
            space_after_pt: 12.0,
            line_spacing_extra: 0.5,
            tab_stops: vec![
                TabStop {
                    pos_pt: 56.0,
                    align: TabAlign::Left,
                },
                TabStop {
                    pos_pt: 120.0,
                    align: TabAlign::Right,
                },
            ],
            ..Default::default()
        };
        let back = roundtrip(&doc(vec![Block::Paragraph(Paragraph {
            props: props.clone(),
            runs: vec![Run::Text {
                text: "x".into(),
                props: RunProps::default(),
            }],
        })]));
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        assert_eq!(p.props.alignment, Alignment::Justify);
        assert_eq!(p.props.left_indent_pt, 36.0);
        assert_eq!(p.props.right_indent_pt, 18.0);
        assert_eq!(p.props.hanging_pt, 21.0);
        assert_eq!(p.props.space_before_pt, 6.0);
        assert_eq!(p.props.space_after_pt, 12.0);
        assert!((p.props.line_spacing_extra - 0.5).abs() < 0.001);
        assert_eq!(p.props.tab_stops.len(), 2);
        assert_eq!(p.props.tab_stops[1].align, TabAlign::Right);
    }

    #[test]
    fn tables_with_gridspan_survive() {
        let t = Table {
            column_widths: vec![100.0, 100.0, 100.0],
            border: TableBorder::Cell,
            rows: vec![
                TableRow {
                    cells: vec![
                        TableCell {
                            blocks: vec![Block::Paragraph(Paragraph::plain("geniş"))],
                            grid_span: 2,
                            ..Default::default()
                        },
                        TableCell {
                            blocks: vec![Block::Paragraph(Paragraph::plain("dar"))],
                            grid_span: 1,
                            ..Default::default()
                        },
                    ],
                    is_header: true,
                    ..Default::default()
                },
                TableRow {
                    cells: (0..3)
                        .map(|i| TableCell {
                            blocks: vec![Block::Paragraph(Paragraph::plain(&format!("h{i}")))],
                            grid_span: 1,
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let back = roundtrip(&doc(vec![Block::Table(t)]));
        let Block::Table(bt) = &back.sections[0].blocks[0] else {
            panic!("table lost")
        };
        assert_eq!(bt.rows.len(), 2);
        assert_eq!(bt.rows[0].cells[0].grid_span, 2);
        assert!(bt.rows[0].is_header);
        assert_eq!(bt.rows[1].cells.len(), 3);
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
        let back = roundtrip(&d);
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        let Run::Image(img) = p
            .runs
            .iter()
            .find(|r| matches!(r, Run::Image(_)))
            .expect("image lost")
        else {
            panic!()
        };
        assert_eq!(img.data.as_ref(), &png);
        assert!((img.width_pt - 108.0).abs() < 0.01);
    }

    #[test]
    fn header_footer_and_page_setup_survive() {
        let mut d = doc(vec![Block::Paragraph(Paragraph::plain("gövde"))]);
        d.sections[0].header = Some(HeaderFooter {
            blocks: vec![Block::Paragraph(Paragraph::plain("üst"))],
            ..Default::default()
        });
        d.sections[0].footer = Some(HeaderFooter {
            blocks: vec![Block::Paragraph(Paragraph::plain("alt"))],
            ..Default::default()
        });
        d.sections[0].page.orientation = Orientation::Landscape;
        d.sections[0].page.width_pt = 841.89;
        d.sections[0].page.height_pt = 595.276;
        let back = roundtrip(&d);
        assert_eq!(back.sections[0].page.orientation, Orientation::Landscape);
        let h = back.sections[0].header.as_ref().expect("header lost");
        let Block::Paragraph(hp) = &h.blocks[0] else {
            panic!()
        };
        assert_eq!(hp.text(), "üst");
        let f = back.sections[0].footer.as_ref().expect("footer lost");
        let Block::Paragraph(fp) = &f.blocks[0] else {
            panic!()
        };
        assert_eq!(fp.text(), "alt");
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
                    text: "bir".into(),
                    props: RunProps::default(),
                }],
            }),
            Block::Paragraph(Paragraph {
                props: ParaProps {
                    list: Some(ListRef {
                        list_id: 2,
                        level: 2,
                    }),
                    ..Default::default()
                },
                runs: vec![Run::Text {
                    text: "iki".into(),
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
        let back = roundtrip(&d);
        let Block::Paragraph(p0) = &back.sections[0].blocks[0] else {
            panic!()
        };
        let Block::Paragraph(p1) = &back.sections[0].blocks[1] else {
            panic!()
        };
        assert!(p0.props.list.is_some(), "list membership lost");
        assert_eq!(
            p1.props.list.as_ref().unwrap().level,
            2,
            "nesting level lost"
        );
        assert_eq!(back.lists.len(), 2);
    }

    #[test]
    fn page_breaks_survive() {
        let back = roundtrip(&doc(vec![
            Block::Paragraph(Paragraph::plain("a")),
            Block::PageBreak,
            Block::Paragraph(Paragraph::plain("b")),
        ]));
        assert!(
            back.sections[0]
                .blocks
                .iter()
                .any(|b| matches!(b, Block::PageBreak)),
            "explicit page break lost"
        );
    }

    #[test]
    fn tabs_survive() {
        let back = roundtrip(&doc(vec![Block::Paragraph(Paragraph::plain(
            "Dosya\t:\t2025/450",
        ))]));
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        assert_eq!(p.text(), "Dosya\t:\t2025/450");
    }

    #[test]
    fn a_non_zip_fails_cleanly_as_invalid_docx() {
        let mut w = WarningSink::new();
        let e = read_docx(b"not a docx at all", &mut w).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidDocx);
    }

    #[test]
    fn validator_rejects_a_package_with_a_dangling_relationship() {
        let d = doc(vec![Block::Paragraph(Paragraph::plain("x"))]);
        let mut w = WarningSink::new();
        let bytes = write_docx(&d, &mut w).unwrap();
        // Rebuild the package without styles.xml, leaving its relationship dangling.
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut cur);
            let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            for i in 0..z.len() {
                let mut f = z.by_index(i).unwrap();
                let n = f.name().to_string();
                if n == "word/styles.xml" {
                    continue;
                }
                let mut v = Vec::new();
                use std::io::Read;
                f.read_to_end(&mut v).unwrap();
                zw.start_file(n, o).unwrap();
                zw.write_all(&v).unwrap();
            }
            zw.finish().unwrap();
        }
        let e = validate(&cur.into_inner()).unwrap_err();
        assert_eq!(e.code, ErrorCode::OutputValidationFailed);
    }
}
