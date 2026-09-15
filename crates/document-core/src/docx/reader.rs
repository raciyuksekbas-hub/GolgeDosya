//! DOCX -> canonical model.
//!
//! Style resolution: `docDefaults` -> paragraph style chain (`w:basedOn`) -> direct
//! formatting. Anything Word can express that UDF cannot is detected here and reported as
//! a warning by the caller, never dropped in silence.

use std::collections::HashMap;
use std::sync::Arc;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::docx::*;
use crate::error::{ConvError, ErrorCode, Result};
use crate::model::*;
use crate::security::{self, ArchiveBudget, DepthGuard};
use crate::units::{emu_to_pt, halfpoints_to_pt, twips_to_pt};
use crate::warnings::{Warning, WarningCode, WarningSink};

/// Everything pulled out of the package before the body is walked.
struct Pkg {
    document: Vec<u8>,
    styles: Option<Vec<u8>>,
    numbering: Option<Vec<u8>>,
    /// relationship id -> (type, target), from `word/_rels/document.xml.rels`
    rels: HashMap<String, (String, String)>,
    /// Relationships owned by other parts, keyed by part file name (`header1.xml`).
    /// OOXML scopes relationships per part, so a header image resolves only here.
    part_rels: HashMap<String, HashMap<String, (String, String)>>,
    /// part name (relative to `word/`) -> bytes
    media: HashMap<String, Arc<Vec<u8>>>,
    headers: HashMap<String, (String, Vec<u8>)>,
    footers: HashMap<String, (String, Vec<u8>)>,
}

#[derive(Clone, Default, Debug)]
struct StyleEntry {
    based_on: Option<String>,
    para: PartialPara,
    run: PartialRun,
}

#[derive(Clone, Default, Debug)]
struct PartialPara {
    alignment: Option<Alignment>,
    left: Option<f32>,
    right: Option<f32>,
    first_line: Option<f32>,
    hanging: Option<f32>,
    before: Option<f32>,
    after: Option<f32>,
    line_extra: Option<f32>,
    tabs: Option<Vec<TabStop>>,
    num_id: Option<u32>,
    num_level: Option<u32>,
}

#[derive(Clone, Default, Debug)]
struct PartialRun {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    vert: Option<VertAlign>,
    family: Option<String>,
    size: Option<f32>,
    color: Option<Color>,
}

impl PartialRun {
    fn overlay(&mut self, o: &PartialRun) {
        if o.bold.is_some() {
            self.bold = o.bold;
        }
        if o.italic.is_some() {
            self.italic = o.italic;
        }
        if o.underline.is_some() {
            self.underline = o.underline;
        }
        if o.strike.is_some() {
            self.strike = o.strike;
        }
        if o.vert.is_some() {
            self.vert = o.vert;
        }
        if o.family.is_some() {
            self.family = o.family.clone();
        }
        if o.size.is_some() {
            self.size = o.size;
        }
        if o.color.is_some() {
            self.color = o.color;
        }
    }

    fn into_props(self) -> RunProps {
        RunProps {
            bold: self.bold.unwrap_or(false),
            italic: self.italic.unwrap_or(false),
            underline: self.underline.unwrap_or(false),
            strike: self.strike.unwrap_or(false),
            vert_align: self.vert.unwrap_or_default(),
            font_family: self.family,
            font_size_pt: self.size,
            color: self.color,
        }
    }
}

impl PartialPara {
    fn overlay(&mut self, o: &PartialPara) {
        if o.alignment.is_some() {
            self.alignment = o.alignment;
        }
        if o.left.is_some() {
            self.left = o.left;
        }
        if o.right.is_some() {
            self.right = o.right;
        }
        if o.first_line.is_some() {
            self.first_line = o.first_line;
        }
        if o.hanging.is_some() {
            self.hanging = o.hanging;
        }
        if o.before.is_some() {
            self.before = o.before;
        }
        if o.after.is_some() {
            self.after = o.after;
        }
        if o.line_extra.is_some() {
            self.line_extra = o.line_extra;
        }
        if o.tabs.is_some() {
            self.tabs = o.tabs.clone();
        }
        if o.num_id.is_some() {
            self.num_id = o.num_id;
        }
        if o.num_level.is_some() {
            self.num_level = o.num_level;
        }
    }
}

struct Styles {
    map: HashMap<String, StyleEntry>,
    default_para: PartialPara,
    default_run: PartialRun,
    default_para_style: Option<String>,
}

impl Styles {
    fn resolve(&self, style_id: Option<&str>) -> (PartialPara, PartialRun) {
        let mut para = self.default_para.clone();
        let mut run = self.default_run.clone();
        // Word's default paragraph style applies before any explicit w:pStyle.
        let mut chain: Vec<&StyleEntry> = Vec::new();
        let mut cursor = style_id
            .map(|s| s.to_string())
            .or_else(|| self.default_para_style.clone());
        let mut hops = 0;
        while let Some(id) = cursor {
            let Some(e) = self.map.get(&id) else { break };
            chain.push(e);
            cursor = e.based_on.clone();
            hops += 1;
            if hops > 32 {
                break; // cyclic w:basedOn; stop rather than loop
            }
        }
        for e in chain.iter().rev() {
            para.overlay(&e.para);
            run.overlay(&e.run);
        }
        (para, run)
    }
}

pub fn read_docx(bytes: &[u8], warn: &mut WarningSink) -> Result<Document> {
    let mut pkg = load_package(bytes)?;
    let styles = parse_styles(pkg.styles.as_deref())?;
    let numbering = parse_numbering(pkg.numbering.as_deref())?;

    let doc_xml = std::mem::take(&mut pkg.document);
    let mut b = BodyBuilder {
        pkg: &mut pkg,
        styles: &styles,
        numbering: &numbering,
        warn,
        lists: Vec::new(),
        used_num: HashMap::new(),
        active_part: None,
    };
    let (blocks, sect) = b.walk_body(&doc_xml)?;

    let mut section = Section {
        page: sect.page,
        header: None,
        footer: None,
        blocks,
    };

    // Word may define default / first-page / even-page variants; UDF has one of each.
    if sect.header_ids.len() > 1 || sect.footer_ids.len() > 1 {
        b.warn.warn(WarningCode::HeaderFooterVariantDropped);
    }
    if let Some((rid, kind)) = sect.header_ids.first().cloned() {
        if let Some((part, x)) = b.pkg.headers.get(&rid).cloned() {
            let mut hf = b.walk_header_footer(&part, &x)?;
            if kind == "first" {
                hf.stop_page = Some(1);
            }
            section.header = Some(hf);
        }
    }
    if let Some((rid, _)) = sect.footer_ids.first().cloned() {
        if let Some((part, x)) = b.pkg.footers.get(&rid).cloned() {
            section.footer = Some(b.walk_header_footer(&part, &x)?);
        }
    }

    let lists = std::mem::take(&mut b.lists);
    let default_font = FontSpec {
        family: styles
            .default_run
            .family
            .clone()
            .unwrap_or_else(|| "Times New Roman".into()),
        size_pt: styles.default_run.size.unwrap_or(12.0),
    };

    Ok(Document {
        meta: Metadata {
            title: None,
            author: None,
            default_font,
        },
        sections: vec![section],
        lists,
    })
}

fn load_package(bytes: &[u8]) -> Result<Pkg> {
    let mut budget = ArchiveBudget::new();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| ConvError::invalid_docx(format!("not a ZIP container: {e}")))?;

    let mut document = None;
    let mut styles = None;
    let mut numbering = None;
    let mut rels_xml = None;
    let mut part_rels_xml: HashMap<String, Vec<u8>> = HashMap::new();
    let mut media = HashMap::new();
    let mut headers_raw: HashMap<String, Vec<u8>> = HashMap::new();
    let mut footers_raw: HashMap<String, Vec<u8>> = HashMap::new();

    for i in 0..z.len() {
        budget.count_entry()?;
        let mut f = z
            .by_index(i)
            .map_err(|e| ConvError::invalid_docx(format!("unreadable archive entry: {e}")))?;
        let name = f.name().to_string();
        if name.ends_with('/') {
            continue;
        }
        security::check_entry_name(&name)?;
        budget.check_entry_size(f.compressed_size(), f.size())?;
        let lower = name.to_ascii_lowercase();

        let want = lower == "word/document.xml"
            || lower == "word/styles.xml"
            || lower == "word/numbering.xml"
            || lower == "word/_rels/document.xml.rels"
            || (lower.starts_with("word/_rels/") && lower.ends_with(".xml.rels"))
            || lower.starts_with("word/media/")
            || (lower.starts_with("word/header") && lower.ends_with(".xml"))
            || (lower.starts_with("word/footer") && lower.ends_with(".xml"));
        if !want {
            continue;
        }

        // Beyan edilen boyut denetimi geçti, ama gerçek deflate akışı çok daha
        // büyük olabilir; okuma sert sınıra kadar yapılır (zip bomb koruması).
        let declared = f.size();
        let v = budget.read_entry_capped(&mut f, declared, security::MAX_ENTRY_UNCOMPRESSED)?;

        match lower.as_str() {
            "word/document.xml" => document = Some(v),
            "word/styles.xml" => styles = Some(v),
            "word/numbering.xml" => numbering = Some(v),
            "word/_rels/document.xml.rels" => rels_xml = Some(v),
            _ => {
                if lower.starts_with("word/_rels/") && lower.ends_with(".xml.rels") {
                    let base = name["word/_rels/".len()..]
                        .trim_end_matches(".rels")
                        .to_string();
                    part_rels_xml.insert(base, v);
                } else if lower.starts_with("word/media/") {
                    budget.add_image(v.len())?;
                    media.insert(name["word/".len()..].to_string(), Arc::new(v));
                } else if lower.starts_with("word/header") {
                    headers_raw.insert(name["word/".len()..].to_string(), v);
                } else if lower.starts_with("word/footer") {
                    footers_raw.insert(name["word/".len()..].to_string(), v);
                }
            }
        }
    }

    let document = document.ok_or_else(|| {
        ConvError::new(
            ErrorCode::InvalidDocx,
            "package contains no word/document.xml",
        )
    })?;
    security::reject_doctype(&document)?;
    if let Some(s) = &styles {
        security::reject_doctype(s)?;
    }

    let rels = parse_rels(rels_xml.as_deref())?;
    let mut part_rels = HashMap::new();
    for (part, xml) in &part_rels_xml {
        part_rels.insert(part.clone(), parse_rels(Some(xml))?);
    }

    // Key headers/footers by relationship id so sectPr references resolve, and keep the
    // part file name: its own .rels is where any image inside it is declared.
    let mut headers: HashMap<String, (String, Vec<u8>)> = HashMap::new();
    let mut footers: HashMap<String, (String, Vec<u8>)> = HashMap::new();
    for (rid, (ty, target)) in &rels {
        let t = target
            .trim_start_matches("/word/")
            .trim_start_matches("word/")
            .to_string();
        if ty == REL_HEADER {
            if let Some(v) = headers_raw.get(&t) {
                headers.insert(rid.clone(), (t.clone(), v.clone()));
            }
        } else if ty == REL_FOOTER {
            if let Some(v) = footers_raw.get(&t) {
                footers.insert(rid.clone(), (t.clone(), v.clone()));
            }
        }
    }

    Ok(Pkg {
        document,
        styles,
        numbering,
        rels,
        part_rels,
        media,
        headers,
        footers,
    })
}

fn parse_rels(xml: Option<&[u8]>) -> Result<HashMap<String, (String, String)>> {
    let mut out = HashMap::new();
    let Some(xml) = xml else { return Ok(out) };
    security::reject_doctype(xml)?;
    let mut rd = Reader::from_reader(xml);
    rd.config_mut().check_end_names = false;
    loop {
        match rd.read_event() {
            Err(e) => return Err(ConvError::invalid_docx(format!("rels XML error: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) | Ok(Event::Empty(e))
                if e.local_name().as_ref() == b"Relationship" =>
            {
                let id = a(&e, "Id");
                let ty = a(&e, "Type");
                let tg = a(&e, "Target");
                if let (Some(id), Some(ty), Some(tg)) = (id, ty, tg) {
                    out.insert(id, (ty, tg));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn a(e: &BytesStart, key: &str) -> Option<String> {
    for at in e.attributes().flatten() {
        if at.key.local_name().as_ref() == key.as_bytes() {
            return Some(String::from_utf8_lossy(&at.value).to_string());
        }
    }
    None
}

fn af(e: &BytesStart, key: &str) -> Option<f32> {
    a(e, key).and_then(|v| v.trim().parse().ok())
}

/// `w:val` defaulting to true, which is how OOXML toggles work: `<w:b/>` means on.
fn toggle(e: &BytesStart) -> bool {
    match a(e, "val").as_deref() {
        None => true,
        Some(v) => !matches!(v, "0" | "false" | "off"),
    }
}

fn parse_styles(xml: Option<&[u8]>) -> Result<Styles> {
    let mut s = Styles {
        map: HashMap::new(),
        default_para: PartialPara::default(),
        default_run: PartialRun::default(),
        default_para_style: None,
    };
    let Some(xml) = xml else { return Ok(s) };
    let mut rd = Reader::from_reader(xml);
    rd.config_mut().check_end_names = false;

    let mut cur: Option<(String, StyleEntry, bool)> = None; // (id, entry, is_default)
    let mut in_doc_defaults = false;
    let mut guard = DepthGuard::new();

    loop {
        let ev = rd.read_event();
        match ev {
            Err(e) => return Err(ConvError::invalid_docx(format!("styles XML error: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let empty = matches!(ev, Ok(Event::Empty(_)));
                if !empty {
                    guard.enter()?;
                }
                let e = e.clone();
                match e.local_name().as_ref() {
                    b"docDefaults" => in_doc_defaults = true,
                    b"style" => {
                        let id = a(&e, "styleId").unwrap_or_default();
                        let is_def = a(&e, "default").as_deref() == Some("1")
                            && a(&e, "type").as_deref() == Some("paragraph");
                        cur = Some((id, StyleEntry::default(), is_def));
                    }
                    b"basedOn" => {
                        if let Some((_, c, _)) = cur.as_mut() {
                            c.based_on = a(&e, "val");
                        }
                    }
                    _ => {
                        let (p, r) = match cur.as_mut() {
                            Some((_, c, _)) => (&mut c.para, &mut c.run),
                            None if in_doc_defaults => (&mut s.default_para, &mut s.default_run),
                            None => continue,
                        };
                        apply_ppr_child(&e, p);
                        apply_rpr_child(&e, r);
                    }
                }
            }
            Ok(Event::End(e)) => {
                guard.leave();
                match e.local_name().as_ref() {
                    b"docDefaults" => in_doc_defaults = false,
                    b"style" => {
                        if let Some((id, entry, is_def)) = cur.take() {
                            if is_def {
                                s.default_para_style = Some(id.clone());
                            }
                            s.map.insert(id, entry);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(s)
}

fn apply_ppr_child(e: &BytesStart, p: &mut PartialPara) {
    match e.local_name().as_ref() {
        b"jc" => {
            p.alignment = match a(e, "val").as_deref() {
                Some("center") => Some(Alignment::Center),
                Some("right") | Some("end") => Some(Alignment::Right),
                Some("both") | Some("distribute") => Some(Alignment::Justify),
                Some("left") | Some("start") => Some(Alignment::Left),
                _ => None,
            };
        }
        b"ind" => {
            if let Some(v) = af(e, "left").or_else(|| af(e, "start")) {
                p.left = Some(twips_to_pt(v));
            }
            if let Some(v) = af(e, "right").or_else(|| af(e, "end")) {
                p.right = Some(twips_to_pt(v));
            }
            if let Some(v) = af(e, "firstLine") {
                p.first_line = Some(twips_to_pt(v));
            }
            if let Some(v) = af(e, "hanging") {
                p.hanging = Some(twips_to_pt(v));
            }
        }
        b"spacing" => {
            if let Some(v) = af(e, "before") {
                p.before = Some(twips_to_pt(v));
            }
            if let Some(v) = af(e, "after") {
                p.after = Some(twips_to_pt(v));
            }
            // w:line is in 240ths of a line for "auto", i.e. 240 == single spacing.
            if let Some(v) = af(e, "line") {
                let rule = a(e, "lineRule").unwrap_or_else(|| "auto".into());
                p.line_extra = Some(match rule.as_str() {
                    "auto" => (v / 240.0 - 1.0).max(0.0),
                    // exact/atLeast are absolute twips; approximate against a 12 pt line.
                    _ => ((twips_to_pt(v) / 12.0) - 1.0).max(0.0),
                });
            }
        }
        b"numPr" => {}
        b"numId" => {
            if let Some(v) = af(e, "val") {
                p.num_id = Some(v as u32);
            }
        }
        b"ilvl" => {
            if let Some(v) = af(e, "val") {
                p.num_level = Some(v as u32);
            }
        }
        b"tab" => {
            if let Some(pos) = af(e, "pos") {
                let align = match a(e, "val").as_deref() {
                    Some("center") => TabAlign::Center,
                    Some("right") | Some("end") => TabAlign::Right,
                    Some("decimal") => TabAlign::Decimal,
                    _ => TabAlign::Left,
                };
                p.tabs.get_or_insert_with(Vec::new).push(TabStop {
                    pos_pt: twips_to_pt(pos),
                    align,
                });
            }
        }
        _ => {}
    }
}

fn apply_rpr_child(e: &BytesStart, r: &mut PartialRun) {
    match e.local_name().as_ref() {
        b"b" => r.bold = Some(toggle(e)),
        b"i" => r.italic = Some(toggle(e)),
        b"u" => {
            r.underline = Some(
                !matches!(a(e, "val").as_deref(), Some("none") | None if a(e, "val").is_some()),
            )
            .map(|_| a(e, "val").as_deref() != Some("none"));
        }
        b"strike" => r.strike = Some(toggle(e)),
        b"dstrike" => r.strike = Some(toggle(e)),
        b"vertAlign" => {
            r.vert = match a(e, "val").as_deref() {
                Some("superscript") => Some(VertAlign::Superscript),
                Some("subscript") => Some(VertAlign::Subscript),
                _ => Some(VertAlign::Baseline),
            };
        }
        b"sz" => {
            if let Some(v) = af(e, "val") {
                r.size = Some(halfpoints_to_pt(v));
            }
        }
        b"rFonts" => {
            if let Some(v) = a(e, "ascii")
                .or_else(|| a(e, "hAnsi"))
                .or_else(|| a(e, "cs"))
            {
                r.family = Some(v);
            }
        }
        b"color" => {
            if let Some(v) = a(e, "val") {
                if v != "auto" {
                    r.color = Color::from_hex(&v);
                }
            }
        }
        _ => {}
    }
}

#[derive(Default)]
struct NumberingMap {
    /// numId -> (abstractId, level -> fmt token)
    num_to_abstract: HashMap<u32, u32>,
    abstract_levels: HashMap<(u32, u32), String>,
}

fn parse_numbering(xml: Option<&[u8]>) -> Result<NumberingMap> {
    let mut m = NumberingMap::default();
    let Some(xml) = xml else { return Ok(m) };
    let mut rd = Reader::from_reader(xml);
    rd.config_mut().check_end_names = false;
    let mut cur_abstract: Option<u32> = None;
    let mut cur_lvl: Option<u32> = None;
    let mut cur_num: Option<u32> = None;
    loop {
        match rd.read_event() {
            Err(e) => return Err(ConvError::invalid_docx(format!("numbering XML error: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"abstractNum" => cur_abstract = af(&e, "abstractNumId").map(|v| v as u32),
                b"lvl" => cur_lvl = af(&e, "ilvl").map(|v| v as u32),
                b"numFmt" => {
                    if let (Some(ab), Some(l), Some(v)) = (cur_abstract, cur_lvl, a(&e, "val")) {
                        m.abstract_levels.insert((ab, l), v);
                    }
                }
                b"num" => cur_num = af(&e, "numId").map(|v| v as u32),
                b"abstractNumId" => {
                    if let (Some(n), Some(v)) = (cur_num, af(&e, "val")) {
                        m.num_to_abstract.insert(n, v as u32);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"abstractNum" => cur_abstract = None,
                b"lvl" => cur_lvl = None,
                b"num" => cur_num = None,
                _ => {}
            },
            _ => {}
        }
    }
    Ok(m)
}

#[derive(Default)]
struct SectInfo {
    page: PageSetup,
    header_ids: Vec<(String, String)>,
    footer_ids: Vec<(String, String)>,
}

struct BodyBuilder<'a> {
    pkg: &'a mut Pkg,
    styles: &'a Styles,
    numbering: &'a NumberingMap,
    warn: &'a mut WarningSink,
    lists: Vec<ListDef>,
    used_num: HashMap<u32, u32>,
    /// Part whose relationship scope is currently active (`header1.xml`), or None for the
    /// document part.
    active_part: Option<String>,
}

impl<'a> BodyBuilder<'a> {
    fn walk_header_footer(&mut self, part: &str, xml: &[u8]) -> Result<HeaderFooter> {
        self.active_part = Some(part.to_string());
        let r = self.walk_body(xml);
        self.active_part = None;
        let (blocks, _) = r?;
        Ok(HeaderFooter {
            blocks,
            stop_page: None,
            start_page: None,
            page_number_attrs: vec![],
            page_number: None,
        })
    }

    fn walk_body(&mut self, xml: &[u8]) -> Result<(Vec<Block>, SectInfo)> {
        security::reject_doctype(xml)?;
        let mut rd = Reader::from_reader(xml);
        rd.config_mut().trim_text(false);
        rd.config_mut().check_end_names = false;

        let mut guard = DepthGuard::new();
        let mut out: Vec<Block> = Vec::new();
        let mut sect = SectInfo::default();
        let mut sect_count = 0usize;

        // Parser state
        let mut para: Option<Paragraph> = None;
        let mut para_style: Option<String> = None;
        let mut direct_para = PartialPara::default();
        let mut in_ppr = false;
        let mut in_rpr = false;
        let mut in_para_rpr = false;
        let mut in_tabs = false;
        let mut in_sect_pr = false;
        let mut in_num_pr = false;
        let mut run_direct = PartialRun::default();
        let mut pending_break_page = false;
        let mut in_deleted = false;
        // Word "complex field" komut metni (`<w:instrText>`): HYPERLINK hedefi,
        // REF/PAGEREF yer imi, MERGEFIELD veri adı gibi KULLANICIYA GÖSTERİLMEYEN
        // alan kodu. Görünen değer separate'ten sonra ayrı gelir; kod bastırılmazsa
        // gövde metnine sızar. `in_deleted` ile aynı mantık.
        let mut in_instr = false;
        let mut preserve_space = false;

        let mut tables: Vec<Table> = Vec::new();
        let mut rows: Vec<TableRow> = Vec::new();
        let mut cells: Vec<TableCell> = Vec::new();
        let mut pending_cell_span: usize = 1;
        let mut pending_cell_valign = CellVAlign::Top;
        let mut pending_vmerge_continue = false;
        let mut in_tcpr = false;
        let mut in_trpr = false;
        let mut pending_row_header = false;
        let mut pending_row_height: Option<f32> = None;
        let mut in_drawing_extent = false;
        let mut pending_img: Option<(f32, f32)> = None;

        loop {
            let ev = rd.read_event();
            let is_empty = matches!(ev, Ok(Event::Empty(_)));
            match ev {
                Err(e) => return Err(ConvError::invalid_docx(format!("document XML error: {e}"))),
                Ok(Event::Eof) => break,
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    if !is_empty {
                        guard.enter()?;
                    }
                    let ln = e.local_name();
                    let name = ln.as_ref();
                    match name {
                        b"p" if !in_sect_pr => {
                            if is_empty {
                                // <w:p/> is a blank line. quick-xml emits no End event for
                                // it, so it must be flushed here or it disappears.
                                push_to(
                                    &mut cells,
                                    &mut out,
                                    Block::Paragraph(Paragraph::default()),
                                );
                            } else {
                                para = Some(Paragraph::default());
                                para_style = None;
                                direct_para = PartialPara::default();
                            }
                        }
                        b"pPr" => in_ppr = true,
                        b"pStyle" if in_ppr => para_style = a(e, "val"),
                        b"rPr" => {
                            if in_ppr {
                                in_para_rpr = true;
                            } else {
                                in_rpr = true;
                                run_direct = PartialRun::default();
                            }
                        }
                        b"tabs" if in_ppr => in_tabs = true,
                        b"numPr" if in_ppr => in_num_pr = true,
                        b"sectPr" => {
                            in_sect_pr = true;
                            sect_count += 1;
                        }
                        b"pgSz" if in_sect_pr => {
                            if let Some(w) = af(e, "w") {
                                sect.page.width_pt = twips_to_pt(w);
                            }
                            if let Some(h) = af(e, "h") {
                                sect.page.height_pt = twips_to_pt(h);
                            }
                            if a(e, "orient").as_deref() == Some("landscape") {
                                sect.page.orientation = Orientation::Landscape;
                            }
                        }
                        b"pgMar" if in_sect_pr => {
                            if let Some(v) = af(e, "left") {
                                sect.page.margin_left_pt = twips_to_pt(v);
                            }
                            if let Some(v) = af(e, "right") {
                                sect.page.margin_right_pt = twips_to_pt(v);
                            }
                            if let Some(v) = af(e, "top") {
                                sect.page.margin_top_pt = twips_to_pt(v);
                            }
                            if let Some(v) = af(e, "bottom") {
                                sect.page.margin_bottom_pt = twips_to_pt(v);
                            }
                            if let Some(v) = af(e, "header") {
                                sect.page.header_offset_pt = twips_to_pt(v);
                            }
                            if let Some(v) = af(e, "footer") {
                                sect.page.footer_offset_pt = twips_to_pt(v);
                            }
                        }
                        b"headerReference" if in_sect_pr => {
                            if let Some(id) = a(e, "id") {
                                let kind = a(e, "type").unwrap_or_else(|| "default".into());
                                if kind == "default" {
                                    sect.header_ids.insert(0, (id, kind));
                                } else {
                                    sect.header_ids.push((id, kind));
                                }
                            }
                        }
                        b"footerReference" if in_sect_pr => {
                            if let Some(id) = a(e, "id") {
                                let kind = a(e, "type").unwrap_or_else(|| "default".into());
                                if kind == "default" {
                                    sect.footer_ids.insert(0, (id, kind));
                                } else {
                                    sect.footer_ids.push((id, kind));
                                }
                            }
                        }

                        // --- tables ---
                        b"tbl" => tables.push(Table {
                            border: TableBorder::Cell,
                            ..Default::default()
                        }),
                        b"gridCol" => {
                            if let (Some(t), Some(w)) = (tables.last_mut(), af(e, "w")) {
                                t.column_widths.push(twips_to_pt(w));
                            }
                        }
                        b"tblBorders" => {}
                        b"tr" => {
                            rows.push(TableRow::default());
                            pending_row_header = false;
                            pending_row_height = None;
                        }
                        b"trPr" => in_trpr = true,
                        b"tblHeader" if in_trpr => pending_row_header = true,
                        b"trHeight" if in_trpr => {
                            pending_row_height = af(e, "val").map(twips_to_pt)
                        }
                        b"tc" => {
                            cells.push(TableCell::default());
                            pending_cell_span = 1;
                            pending_cell_valign = CellVAlign::Top;
                            pending_vmerge_continue = false;
                        }
                        b"tcPr" => in_tcpr = true,
                        b"gridSpan" if in_tcpr => {
                            pending_cell_span = af(e, "val").unwrap_or(1.0).max(1.0) as usize;
                        }
                        b"vMerge" if in_tcpr => {
                            // Absent w:val, or val="continue", means "part of the cell above".
                            let v = a(e, "val").unwrap_or_else(|| "continue".into());
                            if v == "continue" {
                                pending_vmerge_continue = true;
                            }
                            self.warn.warn(WarningCode::TableVerticalMergeFlattened);
                        }
                        b"vAlign" if in_tcpr => {
                            pending_cell_valign = match a(e, "val").as_deref() {
                                Some("center") => CellVAlign::Center,
                                Some("bottom") => CellVAlign::Bottom,
                                _ => CellVAlign::Top,
                            };
                        }

                        // --- runs ---
                        b"t" => preserve_space = a(e, "space").as_deref() == Some("preserve"),
                        b"tab" if !in_ppr => {
                            if let Some(p) = para.as_mut() {
                                p.runs.push(Run::Tab {
                                    props: self.run_props(&para_style, &run_direct),
                                });
                            }
                        }
                        b"tab" if in_tabs => apply_ppr_child(e, &mut direct_para),
                        b"br" => {
                            if a(e, "type").as_deref() == Some("page") {
                                pending_break_page = true;
                            } else if let Some(p) = para.as_mut() {
                                p.runs.push(Run::LineBreak {
                                    props: self.run_props(&para_style, &run_direct),
                                });
                            }
                        }
                        b"extent" => {
                            in_drawing_extent = true;
                            let cx = af(e, "cx").unwrap_or(0.0);
                            let cy = af(e, "cy").unwrap_or(0.0);
                            pending_img = Some((emu_to_pt(cx), emu_to_pt(cy)));
                        }
                        b"blip" => {
                            if let Some(rid) = a(e, "embed") {
                                let (w, h) = pending_img.take().unwrap_or((0.0, 0.0));
                                self.push_image(&mut para, &rid, w, h)?;
                            }
                        }
                        b"imagedata" => {
                            if let Some(rid) = a(e, "id") {
                                self.push_image(&mut para, &rid, 0.0, 0.0)?;
                            }
                        }

                        // --- things Word can express and UDF cannot ---
                        b"del" => {
                            in_deleted = true;
                            self.warn.warn(WarningCode::TrackedChangesFlattened);
                        }
                        b"ins" => self.warn.warn(WarningCode::TrackedChangesFlattened),
                        b"commentReference" => self.warn.warn(WarningCode::CommentsDropped),
                        b"footnoteReference" | b"endnoteReference" => {
                            self.warn.warn(WarningCode::FootnotesDropped)
                        }
                        b"hyperlink" => self.warn.warn(WarningCode::HyperlinkFlattened),
                        b"txbxContent" => self.warn.warn(WarningCode::TextboxFlattened),
                        b"oMath" | b"oMathPara" => self.warn.warn(WarningCode::EquationDropped),
                        b"sdt" => self.warn.warn(WarningCode::ContentControlFlattened),
                        b"object" | b"OLEObject" => {
                            self.warn.warn(WarningCode::EmbeddedObjectDropped)
                        }
                        b"fldChar" => self.warn.warn(WarningCode::FieldFlattened),
                        b"instrText" => {
                            // Boş `<w:instrText/>` metin taşımaz ve End üretmez; bayrağı
                            // yalnızca gerçek Start için kur, yoksa sonraki paragrafın
                            // metni yanlışlıkla bastırılır.
                            if !is_empty {
                                in_instr = true;
                            }
                            self.warn.warn(WarningCode::FieldFlattened);
                        }
                        _ => {}
                    }
                    if in_ppr && !in_para_rpr && !in_tabs {
                        apply_ppr_child(e, &mut direct_para);
                    }
                    if in_num_pr {
                        apply_ppr_child(e, &mut direct_para);
                    }
                    if in_rpr {
                        apply_rpr_child(e, &mut run_direct);
                    }
                }

                Ok(Event::Text(t)) => {
                    if in_deleted {
                        continue; // deleted text is not part of the current document
                    }
                    if in_instr {
                        continue; // alan komut kodu; kullanıcıya gösterilen metin değil
                    }
                    if let Some(p) = para.as_mut() {
                        let s = t.unescape().unwrap_or_default().to_string();
                        if s.is_empty() {
                            continue;
                        }
                        if !preserve_space && s.trim().is_empty() {
                            continue;
                        }
                        let props = self.run_props(&para_style, &run_direct);
                        if s.contains('\t') {
                            for (i, piece) in s.split('\t').enumerate() {
                                if i > 0 {
                                    p.runs.push(Run::Tab {
                                        props: props.clone(),
                                    });
                                }
                                if !piece.is_empty() {
                                    p.runs.push(Run::Text {
                                        text: piece.to_string(),
                                        props: props.clone(),
                                    });
                                }
                            }
                        } else {
                            p.runs.push(Run::Text { text: s, props });
                        }
                    }
                }

                Ok(Event::End(ref e)) => {
                    guard.leave();
                    match e.local_name().as_ref() {
                        b"pPr" => in_ppr = false,
                        b"rPr" => {
                            if in_para_rpr {
                                in_para_rpr = false;
                            } else {
                                in_rpr = false;
                            }
                        }
                        b"tabs" => in_tabs = false,
                        b"numPr" => in_num_pr = false,
                        b"sectPr" => in_sect_pr = false,
                        b"tcPr" => in_tcpr = false,
                        b"trPr" => in_trpr = false,
                        b"t" => preserve_space = false,
                        b"del" => in_deleted = false,
                        b"instrText" => in_instr = false,
                        b"extent" => in_drawing_extent = false,
                        b"r" => run_direct = PartialRun::default(),
                        b"p" => {
                            if let Some(mut p) = para.take() {
                                p.props = self.para_props(&para_style, &direct_para);
                                let block = Block::Paragraph(p);
                                if pending_break_page {
                                    pending_break_page = false;
                                    push_to(&mut cells, &mut out, Block::PageBreak);
                                }
                                push_to(&mut cells, &mut out, block);
                            }
                        }
                        b"tc" => {
                            if let Some(mut c) = cells.pop() {
                                c.grid_span = pending_cell_span;
                                c.vertical_align = pending_cell_valign;
                                if pending_vmerge_continue {
                                    // Flatten: keep an empty cell so the geometry survives.
                                    c.blocks.clear();
                                }
                                if let Some(r) = rows.last_mut() {
                                    r.cells.push(c);
                                }
                            }
                        }
                        b"tr" => {
                            if let Some(mut r) = rows.pop() {
                                r.is_header = pending_row_header;
                                r.height_pt = pending_row_height;
                                if let Some(t) = tables.last_mut() {
                                    t.rows.push(r);
                                }
                            }
                        }
                        b"tbl" => {
                            if let Some(t) = tables.pop() {
                                push_to(&mut cells, &mut out, Block::Table(t));
                            }
                        }
                        b"body" => break,
                        _ => {}
                    }
                    let _ = in_drawing_extent;
                }
                _ => {}
            }
        }

        if sect_count > 1 {
            self.warn.push(
                Warning::new(WarningCode::MultiSectionFlattened)
                    .detail(format!("{sect_count} bölüm")),
            );
        }
        normalize_blocks(&mut out);
        Ok((out, sect))
    }

    fn push_image(
        &mut self,
        para: &mut Option<Paragraph>,
        rid: &str,
        w: f32,
        h: f32,
    ) -> Result<()> {
        // Relationships are part-scoped: an image referenced from header1.xml is declared
        // in word/_rels/header1.xml.rels, not in the document part's .rels. Resolve in the
        // active part's scope first, then fall back to the document's.
        let looked_up = self
            .active_part
            .as_ref()
            .and_then(|p| self.pkg.part_rels.get(p))
            .and_then(|m| m.get(rid))
            .or_else(|| self.pkg.rels.get(rid))
            .cloned();
        let Some((ty, target)) = looked_up else {
            return Ok(());
        };
        if ty != REL_IMAGE {
            return Ok(());
        }
        let key = target
            .trim_start_matches("/word/")
            .trim_start_matches("word/")
            .to_string();
        let Some(bytes) = self.pkg.media.get(&key).cloned() else {
            self.warn
                .push(Warning::new(WarningCode::ImageFormatUnsupported).detail("eksik görsel"));
            return Ok(());
        };
        let format = ImageFormat::sniff(&bytes);
        if format == ImageFormat::Unsupported {
            let ext = key.rsplit('.').next().unwrap_or("?").to_uppercase();
            self.warn
                .push(Warning::new(WarningCode::ImageFormatUnsupported).detail(ext));
            return Ok(());
        }
        if let Some(p) = para.as_mut() {
            p.runs.push(Run::Image(Image {
                data: bytes,
                format,
                width_pt: w,
                height_pt: h,
            }));
        }
        Ok(())
    }

    fn run_props(&self, style: &Option<String>, direct: &PartialRun) -> RunProps {
        let (_, mut r) = self.styles.resolve(style.as_deref());
        r.overlay(direct);
        r.into_props()
    }

    fn para_props(&mut self, style: &Option<String>, direct: &PartialPara) -> ParaProps {
        let (mut p, _) = self.styles.resolve(style.as_deref());
        p.overlay(direct);

        let list = p.num_id.and_then(|num_id| {
            if num_id == 0 {
                return None;
            }
            let level = p.num_level.unwrap_or(0) + 1;
            let id = *self
                .used_num
                .entry(num_id)
                .or_insert_with(|| self.lists.len() as u32 + 1);
            if !self.lists.iter().any(|l| l.list_id == id) {
                let fmt = self
                    .numbering
                    .num_to_abstract
                    .get(&num_id)
                    .and_then(|ab| {
                        self.numbering
                            .abstract_levels
                            .get(&(*ab, p.num_level.unwrap_or(0)))
                    })
                    .cloned()
                    .unwrap_or_else(|| "decimal".to_string());
                let kind = match fmt.as_str() {
                    "bullet" => ListKind::Bullet(BulletKind::Ellipse),
                    "decimal" | "decimalZero" => ListKind::Number(NumberKind::DecimalDot),
                    "lowerLetter" => ListKind::Number(NumberKind::LowerAlphaParen),
                    _ => {
                        self.warn.push(
                            Warning::new(WarningCode::ListNumberingApproximated)
                                .detail(fmt.clone()),
                        );
                        ListKind::Number(NumberKind::DecimalDot)
                    }
                };
                self.lists.push(ListDef { list_id: id, kind });
            }
            Some(ListRef { list_id: id, level })
        });

        ParaProps {
            alignment: p.alignment.unwrap_or_default(),
            left_indent_pt: p.left.unwrap_or(0.0),
            right_indent_pt: p.right.unwrap_or(0.0),
            first_line_indent_pt: p.first_line.unwrap_or(0.0).max(0.0),
            hanging_pt: p.hanging.unwrap_or(0.0).max(0.0),
            space_before_pt: p.before.unwrap_or(0.0),
            space_after_pt: p.after.unwrap_or(0.0),
            line_spacing_extra: p.line_extra.unwrap_or(0.0),
            tab_stops: p.tabs.unwrap_or_default(),
            list,
            style_name: style.clone(),
        }
    }
}

/// Remove the paragraph OOXML forces after a trailing table.
///
/// A `w:tbl` at the end of a body or a `w:tc` **must** be followed by a `w:p`, or Word
/// reports the document as damaged. That paragraph is therefore structurally mandated at
/// exactly that position and carries no authored content — it is indistinguishable from a
/// blank line the user typed, because Word would have written it either way.
///
/// Leaving it in makes every DOCX -> UDF -> DOCX cycle grow one blank line per table.
/// The rule is deliberately narrow: drop *one* empty, unstyled, list-free paragraph, and
/// only when it is the last block and directly follows a table.
fn drop_mandatory_trailing_paragraph(blocks: &mut Vec<Block>) {
    let n = blocks.len();
    if n < 2 {
        return;
    }
    let tail_is_filler = matches!(
        blocks.last(),
        Some(Block::Paragraph(p))
            if p.runs.is_empty()
                && p.props.list.is_none()
                && p.props == ParaProps { style_name: p.props.style_name.clone(), ..Default::default() }
    );
    if tail_is_filler && matches!(blocks.get(n - 2), Some(Block::Table(_))) {
        blocks.pop();
    }
}

/// Apply the rule to a body and, recursively, to every table cell inside it.
fn normalize_blocks(blocks: &mut Vec<Block>) {
    for b in blocks.iter_mut() {
        if let Block::Table(t) = b {
            for r in &mut t.rows {
                for c in &mut r.cells {
                    normalize_blocks(&mut c.blocks);
                }
            }
        }
    }
    drop_mandatory_trailing_paragraph(blocks);
}

/// Route a finished block to the innermost open container.
///
/// A block belongs to the cell currently being read, if there is one; otherwise to the body.
/// A block that arrives while a table is open but no cell is — which only happens on
/// malformed input — still goes to the body rather than being discarded, because losing
/// text is never the right answer.
fn push_to(cells: &mut [TableCell], out: &mut Vec<Block>, b: Block) {
    match cells.last_mut() {
        Some(c) => c.blocks.push(b),
        None => out.push(b),
    }
}

/// Cheap structural probe: is this a WordprocessingML package at all?
pub fn probe(bytes: &[u8]) -> Result<()> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| ConvError::invalid_docx(format!("not a ZIP container: {e}")))?;
    if z.by_name("word/document.xml").is_err() {
        return Err(ConvError::new(
            ErrorCode::InvalidDocx,
            "package has no word/document.xml",
        ));
    }
    Ok(())
}
