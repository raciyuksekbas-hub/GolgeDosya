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
    headers: HashMap<String, (String, Arc<Vec<u8>>)>,
    footers: HashMap<String, (String, Arc<Vec<u8>>)>,
}

#[derive(Clone, Default, Debug)]
struct StyleEntry {
    based_on: Option<String>,
    para: PartialPara,
    run: PartialRun,
    /// Tablo stilinin `w:tblPr/w:tblBorders` bildirimi.
    table_borders: BorderSet,
}

/// Bir tablonun ya da hücrenin kenarlık bildirimi. `None`: bu katman o kenarı
/// söylemiyor (alttaki katmandan gelir); `Some(true)`: çizgi var; `Some(false)`:
/// açıkça yok (`none`/`nil`).
///
/// Word'de görünen kenarlık üç katmandan çıkar: tablo stili → tablonun kendi
/// `w:tblBorders`'ı → hücrenin `w:tcBorders`'ı. Dilekçe şablonlarındaki başlık
/// bloğu (Davacı / Vekili / Konu) çoğu zaman `w:tblBorders` "single" taşıyan,
/// ama HER hücresinde `w:tcBorders` "none" olan görünmez bir yerleşim
/// tablosudur. Yalnız tablo katmanı okunursa Word'de görünmeyen çizgiler UDF'te
/// çizilir.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct BorderSet {
    top: Option<bool>,
    left: Option<bool>,
    bottom: Option<bool>,
    right: Option<bool>,
    inside_h: Option<bool>,
    inside_v: Option<bool>,
}

impl BorderSet {
    fn overlay(&mut self, o: &BorderSet) {
        for (mine, theirs) in [
            (&mut self.top, o.top),
            (&mut self.left, o.left),
            (&mut self.bottom, o.bottom),
            (&mut self.right, o.right),
            (&mut self.inside_h, o.inside_h),
            (&mut self.inside_v, o.inside_v),
        ] {
            if theirs.is_some() {
                *mine = theirs;
            }
        }
    }

    /// `w:top`, `w:left`/`w:start`, `w:bottom`, `w:right`/`w:end`, `w:insideH`,
    /// `w:insideV` öğelerinden birini kaydeder; başka bir öğeyse `false`.
    /// Çapraz çizgiler (`w:tl2br`, `w:tr2bl`) kenar değildir.
    fn record(&mut self, e: &BytesStart) -> bool {
        let visible = !matches!(a(e, "val").as_deref(), None | Some("none") | Some("nil"));
        let slot = match e.local_name().as_ref() {
            b"top" => &mut self.top,
            b"left" | b"start" => &mut self.left,
            b"bottom" => &mut self.bottom,
            b"right" | b"end" => &mut self.right,
            b"insideH" => &mut self.inside_h,
            b"insideV" => &mut self.inside_v,
            _ => return false,
        };
        *slot = Some(visible);
        true
    }
}

/// Okunmakta olan tablonun kenarlık bilgisi (iç içe tablolar için yığın).
#[derive(Default)]
struct TableBorderState {
    style: Option<String>,
    direct: BorderSet,
    /// Satır satır hücreler: (kapladığı ızgara sütunu sayısı, `w:tcBorders`).
    rows: Vec<Vec<(usize, BorderSet)>>,
}

/// UDF bir tabloda yalnız iki şey tutabilir: bütün hücre kenarlıkları ya da
/// hiç kenarlık. Word'de görünen kenarlıklar hesaplanır; hiçbir kenar
/// görünmüyorsa tablo kenarlıksız yazılır. Yalnız bazı kenarlar görünüyorsa
/// çizgiler kaybolmasın diye bütün kenarlıklar çizilir ve bu `true` döner
/// (yaklaşık aktarım uyarısı).
///
/// Ortak kenar Word'deki gibi çözülür: iki komşudan biri açıkça çiziyorsa
/// kenar görünür ("none" görünür bir çizgiyi silmez).
fn effective_table_border(
    base: &BorderSet,
    rows: &[Vec<(usize, BorderSet)>],
) -> (TableBorder, bool) {
    // Her satırdaki hücrelerin ızgara aralıkları [başlangıç, bitiş).
    let spans: Vec<Vec<(usize, usize, BorderSet)>> = rows
        .iter()
        .map(|row| {
            let mut col = 0;
            row.iter()
                .map(|(span, b)| {
                    let start = col;
                    col += (*span).max(1);
                    (start, col, *b)
                })
                .collect()
        })
        .collect();
    let width = |r: usize| spans[r].last().map(|c| c.1).unwrap_or(0);
    let overlaps = |r: usize, s: usize, e: usize| {
        spans[r]
            .iter()
            .filter(move |c| c.0 < e && s < c.1)
            .map(|c| c.2)
    };
    let (mut any, mut all, mut edges) = (false, true, 0usize);
    for (r, row) in spans.iter().enumerate() {
        for (i, &(s, e, own)) in row.iter().enumerate() {
            let last_row = r + 1 == spans.len();
            let top = own.top.or(if r == 0 { base.top } else { base.inside_h }) == Some(true)
                || (r > 0 && overlaps(r - 1, s, e).any(|n| n.bottom == Some(true)));
            let bottom = own
                .bottom
                .or(if last_row { base.bottom } else { base.inside_h })
                == Some(true)
                || (!last_row && overlaps(r + 1, s, e).any(|n| n.top == Some(true)));
            let left = own.left.or(if s == 0 { base.left } else { base.inside_v }) == Some(true)
                || (i > 0 && row[i - 1].2.right == Some(true));
            let right = own.right.or(if e >= width(r) {
                base.right
            } else {
                base.inside_v
            }) == Some(true)
                || (i + 1 < row.len() && row[i + 1].2.left == Some(true));
            for v in [top, bottom, left, right] {
                edges += 1;
                any |= v;
                all &= v;
            }
        }
    }
    if edges == 0 || !any {
        (TableBorder::None, false)
    } else {
        (TableBorder::Cell, !all)
    }
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
    /// Bu katmanın EKLEDİĞİ duraklar (bkz. `overlay`).
    tabs: Option<Vec<TabStop>>,
    /// Bu katmanın `w:val="clear"` ile sildiği, alttan gelen durak konumları (pt).
    tab_clears: Vec<f32>,
    num_id: Option<u32>,
    num_level: Option<u32>,
    contextual: Option<bool>,
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
    /// `w:caps` / `w:smallCaps`: Word metni BÜYÜK harfle gösterir. UDF'te bu
    /// özellik yok; metin büyük harfe çevrilerek yazılır.
    caps: Option<bool>,
    /// `w:vanish`: gizli metin. Yazdırılmaz, belgeye ait sayılmaz.
    vanish: Option<bool>,
    /// `w:rStyle`: karakter stili (yalnız doğrudan biçimde anlamlı).
    char_style: Option<String>,
    /// `w:highlight`: `Some(None)` açıkça "none" demektir (alttakini siler).
    highlight: Option<Option<Color>>,
    /// Run gölgelemesi (`w:rPr/w:shd` dolgusu); `Some(None)` = "auto".
    shading: Option<Option<Color>>,
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
        if o.caps.is_some() {
            self.caps = o.caps;
        }
        if o.vanish.is_some() {
            self.vanish = o.vanish;
        }
        if o.highlight.is_some() {
            self.highlight = o.highlight;
        }
        if o.shading.is_some() {
            self.shading = o.shading;
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
            // Word vurgulamayı gölgelemenin ÜSTÜNE çizer.
            background: self.highlight.flatten().or(self.shading.flatten()),
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
        // Word'de durak mirası BİRLEŞİMDİR: alttan gelen duraklar ∪ bu katmanınkiler,
        // eksi bu katmanda `clear` ile silinen konumlar; aynı konumda üstteki kazanır.
        // Eskiden paragrafın listesi stilinkinin YERİNE geçiyordu.
        if o.tabs.is_some() || !o.tab_clears.is_empty() {
            let mine = o.tabs.as_deref().unwrap_or(&[]);
            let mut merged = self.tabs.take().unwrap_or_default();
            merged.retain(|t| {
                !o.tab_clears
                    .iter()
                    .copied()
                    .chain(mine.iter().map(|m| m.pos_pt))
                    .any(|pos| same_tab_pos(pos, t.pos_pt))
            });
            merged.extend_from_slice(mine);
            merged.sort_by(|a, b| {
                a.pos_pt
                    .partial_cmp(&b.pos_pt)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.tabs = Some(merged);
        }
        if o.num_id.is_some() {
            self.num_id = o.num_id;
        }
        if o.num_level.is_some() {
            self.num_level = o.num_level;
        }
        if o.contextual.is_some() {
            self.contextual = o.contextual;
        }
    }
}

/// İki durak konumu aynı twip mi? (Konumlar twip/20 olarak saklanır.)
fn same_tab_pos(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.025
}

struct Styles {
    map: HashMap<String, StyleEntry>,
    default_para: PartialPara,
    default_run: PartialRun,
    default_para_style: Option<String>,
    /// `w:type="table" w:default="1"`: stili olmayan tablonun stili.
    default_table_style: Option<String>,
}

impl Styles {
    /// Tablo stilinin (ve `w:basedOn` zincirinin) kenarlıkları. Stil verilmemiş
    /// ya da bulunamamışsa Word belgenin varsayılan tablo stilini kullanır;
    /// o da yoksa ("Normal Tablo") hiç kenarlık yoktur.
    fn table_borders(&self, style_id: Option<&str>) -> BorderSet {
        let mut chain: Vec<&StyleEntry> = Vec::new();
        let mut cursor = style_id
            .filter(|id| self.map.contains_key(*id))
            .map(|s| s.to_string())
            .or_else(|| self.default_table_style.clone());
        let mut hops = 0;
        while let Some(id) = cursor {
            let Some(e) = self.map.get(&id) else { break };
            chain.push(e);
            cursor = e.based_on.clone();
            hops += 1;
            if hops > 32 {
                break;
            }
        }
        let mut out = BorderSet::default();
        for e in chain.iter().rev() {
            out.overlay(&e.table_borders);
        }
        out
    }

    /// Bir karakter stilinin (`w:rStyle`) kendi zinciri (basedOn), belge
    /// varsayılanları olmadan: paragraf stilinin üstüne bindirilir.
    fn char_style_run(&self, style_id: &str) -> PartialRun {
        let mut chain: Vec<&StyleEntry> = Vec::new();
        let mut cursor = Some(style_id.to_string());
        while let Some(id) = cursor {
            let Some(e) = self.map.get(&id) else { break };
            chain.push(e);
            cursor = e.based_on.clone();
            if chain.len() > 32 {
                break; // döngüsel w:basedOn
            }
        }
        let mut run = PartialRun::default();
        for e in chain.iter().rev() {
            run.overlay(&e.run);
        }
        run
    }

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

/// Word'ün `<w:contextualSpacing/>` kuralını MATERYALLEŞTİR.
///
/// Kural: bu işareti taşıyan bir paragraf, KENDİSİYLE AYNI STİLDEKİ bir
/// komşusuyla yan yanaysa aradaki boşluk çizilmez. Word bunu render sırasında
/// yapar; UDF'te böyle bir kural yoktur, dolayısıyla karar burada verilip
/// sonuç yazılmalıdır.
///
/// İşaret yok sayıldığında boşluk her paragrafta gerçekten yazılıyordu:
/// gerçek bir 52 sayfalık sözleşmede bu işareti taşıyan 165 paragraf vardı,
/// metin dikey olarak açılıyor ve sayfa akışı değişiyordu.
///
/// Yalnız ARDIŞIK paragraflara bakılır; araya tablo veya sayfa sonu girerse
/// komşuluk biter — Word de öyle davranır.
fn apply_contextual_spacing(blocks: &mut [Block]) {
    /// İki paragraf Word'ün gözünde "aynı stil" mi?
    fn same_style(a: &Paragraph, b: &Paragraph) -> bool {
        a.props.style_name == b.props.style_name
    }
    for i in 0..blocks.len() {
        let (left, right) = blocks.split_at_mut(i + 1);
        let (Some(Block::Paragraph(current)), Some(Block::Paragraph(next))) =
            (left.last_mut(), right.first_mut())
        else {
            continue;
        };
        if !same_style(current, next) {
            continue;
        }
        if current.props.contextual_spacing {
            current.props.space_after_pt = 0.0;
        }
        if next.props.contextual_spacing {
            next.props.space_before_pt = 0.0;
        }
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
    let (mut blocks, sect) = b.walk_body(&doc_xml)?;
    apply_contextual_spacing(&mut blocks);

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
    // Kabı önce tanı: boş dosya ve parola korumalı/eski Word, "geçerli bir Word
    // belgesi değil ya da bozulmuş" yerine kendi sebebiyle bildirilir (OR-07).
    match security::sniff_container(bytes) {
        security::Container::Empty => {
            return Err(ConvError::new(
                ErrorCode::InputEmpty,
                "file is empty (0 bytes)",
            ))
        }
        security::Container::OleCompound => {
            return Err(ConvError::new(
                ErrorCode::ProtectedOrLegacyDocx,
                "OLE compound file: password-protected Word document or legacy .doc",
            ))
        }
        _ => {}
    }
    let mut budget = ArchiveBudget::new();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| ConvError::invalid_docx(format!("not a ZIP container: {e}")))?;

    let mut document = None;
    let mut styles = None;
    let mut numbering = None;
    let mut rels_xml = None;
    let mut part_rels_xml: HashMap<String, Vec<u8>> = HashMap::new();
    let mut media = HashMap::new();
    // Üst/alt bilgi parçalarının yalnız YERİ not edilir; belgenin ilişkileri
    // okunduktan sonra yalnız başvurulanlar açılır (aşağıda).
    let mut hf_entries: Vec<(usize, String)> = Vec::new();

    for i in 0..z.len() {
        budget.count_entry()?;
        let mut f = z.by_index(i).map_err(entry_error)?;
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
        if lower.starts_with("word/header") || lower.starts_with("word/footer") {
            hf_entries.push((i, name["word/".len()..].to_string()));
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

    let part_of = |target: &str| {
        target
            .trim_start_matches("/word/")
            .trim_start_matches("word/")
            .to_string()
    };

    // Yalnız belgenin başvurduğu üst/alt bilgi parçaları açılır. Saha denetimi
    // (OR-02): her `word/header*.xml` ve `word/footer*.xml` koşulsuz belleğe
    // açılıyordu; başvurusuz, boyutu hakkında yalan söyleyen 16 parça 1,9 MB'lık
    // bir dosyadan 2,3 GB bellek tüketti. Hiçbir yerden başvurulmayan parça
    // belgenin parçası değildir; açılmaz, bozuksa belgeyi de düşürmez. Gerçek
    // derlemde (199 DOCX) her üst/alt bilgi parçası başvuruludur, en çok 6 tane.
    let referenced: std::collections::HashSet<String> = rels
        .values()
        .filter(|(ty, _)| ty == REL_HEADER || ty == REL_FOOTER)
        .map(|(_, target)| part_of(target))
        .collect();
    let mut hf_raw: HashMap<String, Arc<Vec<u8>>> = HashMap::new();
    for (i, part) in hf_entries {
        if !referenced.contains(&part) {
            continue;
        }
        let mut f = z.by_index(i).map_err(entry_error)?;
        let declared = f.size();
        let v = budget.read_entry_capped(&mut f, declared, security::MAX_ENTRY_UNCOMPRESSED)?;
        hf_raw.insert(part, Arc::new(v));
    }

    // Key headers/footers by relationship id so sectPr references resolve, and keep the
    // part file name: its own .rels is where any image inside it is declared.
    let mut headers: HashMap<String, (String, Arc<Vec<u8>>)> = HashMap::new();
    let mut footers: HashMap<String, (String, Arc<Vec<u8>>)> = HashMap::new();
    for (rid, (ty, target)) in &rels {
        let t = part_of(target);
        if ty == REL_HEADER {
            if let Some(v) = hf_raw.get(&t) {
                headers.insert(rid.clone(), (t.clone(), Arc::clone(v)));
            }
        } else if ty == REL_FOOTER {
            if let Some(v) = hf_raw.get(&t) {
                footers.insert(rid.clone(), (t.clone(), Arc::clone(v)));
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

/// Arşiv girdisi açılamadı. Şifreli girdi (ZipCrypto/AES) bozuk sayılmaz:
/// kullanıcıya parolayı kaldırması söylenir (OR-07).
fn entry_error(e: zip::result::ZipError) -> ConvError {
    match e {
        zip::result::ZipError::UnsupportedArchive(msg)
            if msg == zip::result::ZipError::PASSWORD_REQUIRED || msg.contains("encrypted") =>
        {
            ConvError::new(
                ErrorCode::ProtectedOrLegacyDocx,
                format!("encrypted archive entry: {msg}"),
            )
        }
        e => ConvError::invalid_docx(format!("unreadable archive entry: {e}")),
    }
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
        default_table_style: None,
    };
    let Some(xml) = xml else { return Ok(s) };
    let mut rd = Reader::from_reader(xml);
    rd.config_mut().check_end_names = false;

    let mut cur: Option<(String, StyleEntry, bool)> = None; // (id, entry, is_default)
    let mut in_doc_defaults = false;
    let mut in_tbl_borders = false;
    let mut in_rpr = false;
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
                        let is_default = a(&e, "default").as_deref() == Some("1");
                        let kind = a(&e, "type");
                        if is_default && kind.as_deref() == Some("table") {
                            s.default_table_style = Some(id.clone());
                        }
                        let is_def = is_default && kind.as_deref() == Some("paragraph");
                        cur = Some((id, StyleEntry::default(), is_def));
                    }
                    b"tblBorders" if cur.is_some() && !empty => in_tbl_borders = true,
                    _ if in_tbl_borders => {
                        if let Some((_, c, _)) = cur.as_mut() {
                            c.table_borders.record(&e);
                        }
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
                        if e.local_name().as_ref() == b"rPr" && !empty {
                            in_rpr = true;
                        }
                        apply_ppr_child(&e, p);
                        // `w:shd` paragrafta (`w:pPr`) ve hücrede (`w:tcPr`) de var;
                        // yalnız `w:rPr` içindeki metnin arka planıdır.
                        if in_rpr || e.local_name().as_ref() != b"shd" {
                            apply_rpr_child(&e, r);
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                guard.leave();
                match e.local_name().as_ref() {
                    b"docDefaults" => in_doc_defaults = false,
                    b"tblBorders" => in_tbl_borders = false,
                    b"rPr" => in_rpr = false,
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
        // Word: "aynı stildeki paragraflar arasına boşluk ekleme". Yok
        // sayıldığında o boşluk her paragrafta gerçekten yazılır. Gerçek bir
        // 52 sayfalık sözleşmede bu işareti taşıyan 165 paragraf vardı.
        b"contextualSpacing" => {
            p.contextual = Some(
                a(e, "val")
                    .map(|v| v != "0" && v != "false")
                    .unwrap_or(true),
            );
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
                    // `clear` bir durak DEĞİLDİR: alttan gelen durağı siler. Eskiden
                    // sol durak oluyordu (gerçek 7 belgede 67 silinmiş durak UDF'te).
                    Some("clear") => {
                        p.tab_clears.push(twips_to_pt(pos));
                        return;
                    }
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

/// Metin kutusu paragrafı okunurken bekleyen dış paragrafın durumu.
struct HostParagraph {
    para: Option<Paragraph>,
    style: Option<String>,
    direct: PartialPara,
    mark_removed: bool,
    run: PartialRun,
}

/// Açık alanlardan herhangi birinin KOD kısmında mıyız?
fn in_field_code(stack: &[bool]) -> bool {
    stack.iter().any(|in_code| *in_code)
}

/// Türkçe büyük harf: `i` → `İ`, `ı` → `I` (Word, Türkçe metinde böyle gösterir).
fn turkish_upper(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'i' => vec!['İ'],
            _ => c.to_uppercase().collect(),
        })
        .collect()
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
        // `w:caps` ile `w:smallCaps` ayrı açılıp kapanabilir; ikisinden biri
        // açıksa metin büyük harfle görünür.
        b"caps" | b"smallCaps" => {
            let on = toggle(e);
            if on || r.caps.is_none() {
                r.caps = Some(on);
            }
        }
        b"vanish" => r.vanish = Some(toggle(e)),
        b"rStyle" => r.char_style = a(e, "val"),
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
        // Vurgulama ve run gölgelemesi UDF'te `content@background` olur. Eskiden
        // ikisi de sessizce düşüyordu (12 gerçek belgede 64 vurgulu run). Beyaz,
        // sayfanın kendi rengidir (UDF'te hücre gölgesi de yok): görünür bir şey
        // taşımaz; webden yapıştırılmış metindeki beyaz `w:shd` gürültü olarak
        // UDF'e taşınmaz.
        b"highlight" => {
            let name = a(e, "val").unwrap_or_default();
            r.highlight = Some(
                HIGHLIGHT_COLORS
                    .iter()
                    .find(|(n, _)| *n == name)
                    .and_then(|(_, hex)| Color::from_hex(hex))
                    .filter(|c| c.to_hex() != "FFFFFF"),
            );
        }
        b"shd" => {
            r.shading = Some(
                a(e, "fill")
                    .filter(|f| !f.eq_ignore_ascii_case("auto"))
                    .and_then(|f| Color::from_hex(&f))
                    .filter(|c| c.to_hex() != "FFFFFF"),
            );
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
        // Bir revizyon KAYDININ içinde miyiz? (>0 ise her şey yok sayılır.)
        let mut revision_depth: usize = 0;
        let mut in_rpr = false;
        let mut in_para_rpr = false;
        let mut in_tabs = false;
        let mut in_sect_pr = false;
        let mut in_num_pr = false;
        let mut run_direct = PartialRun::default();
        let mut pending_break_page = false;
        // Silinen (`w:del`) ya da başka yere taşınan (`w:moveFrom`) içeriğin
        // derinliği; >0 iken metin, sekme, satır sonu ve resim güncel belgede
        // yoktur. Bayrak değil sayaç: iç içe kayıtlar birbirini kapatmaz.
        let mut deleted_depth: usize = 0;
        // Paragraf İŞARETİ silinmiş ya da taşınmış mı (`w:pPr/w:rPr` içinde
        // boş `w:del`/`w:moveFrom`). İçeriği de silinmiş böyle bir paragraf
        // kabul edilmiş belgede yoktur.
        let mut para_mark_removed = false;
        // Word "complex field" komut metni (`<w:instrText>`): HYPERLINK hedefi,
        // REF/PAGEREF yer imi, MERGEFIELD veri adı gibi KULLANICIYA GÖSTERİLMEYEN
        // alan kodu. Görünen değer separate'ten sonra ayrı gelir; kod bastırılmazsa
        // gövde metnine sızar. `deleted_depth` ile aynı mantık.
        let mut in_instr = false;
        let mut preserve_space = false;
        // Yalnız WordprocessingML `w:t` metni belge metnidir. Eskiden paragraf
        // içindeki HER metin düğümü yazılıyordu: çizim konumları
        // (`wp:posOffset`), `wp:align`, `w:fldData`, OLE bağlantı bilgisi ve
        // denklem metni (`m:t`) belgeye rakam/sözcük olarak sızıyordu.
        let mut in_w_t = false;
        // Açık alanlar (fldChar begin … separate … end): `true` = alanın KOD
        // kısmındayız. İç içe alanda içteki alanın sonucu dıştakinin kodunun
        // parçasıdır ve gösterilmez.
        let mut field_stack: Vec<bool> = Vec::new();
        // `mc:AlternateContent` içindeki `mc:Fallback` derinliği: aynı içeriğin
        // eski biçimli kopyasıdır (metin kutusu, resim). İşlenirse metin iki
        // kez çıkıyordu.
        let mut fallback_depth: usize = 0;
        // Metin kutusu (`w:txbxContent`) paragrafı bir paragrafın İÇİNDE açılır.
        // Eskiden içteki paragraf dıştakini siliyordu: kutudan önceki ve
        // sonraki metin ve paragraf biçimi kayboluyordu. Dıştaki paragrafın
        // durumu burada bekler; içteki paragraflar ondan sonra yazılır.
        let mut host_stack: Vec<HostParagraph> = Vec::new();
        let mut floats: Vec<Block> = Vec::new();
        // Çizim şekli (`wps:wsp`) içinde metin kutusu var mı?
        let mut shape_stack: Vec<bool> = Vec::new();
        let mut hidden_text_dropped = false;

        let mut tables: Vec<Table> = Vec::new();
        let mut table_borders: Vec<TableBorderState> = Vec::new();
        let mut cell_borders = BorderSet::default();
        let mut in_tbl_borders = false;
        let mut in_tc_borders = false;
        let mut in_tblpr_ex = false;
        let mut table_count = 0usize;
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
                    // Değişiklik-izleme KAYDI, belgenin GÜNCEL hâli değildir.
                    //
                    // `w:pPrChange` / `w:rPrChange` / `w:tcPrChange` /
                    // `w:trPrChange` / `w:tblPrChange` / `w:sectPrChange`
                    // içinde, revizyondan ÖNCEKİ (yani reddedilmiş)
                    // özellikler tam bir `w:pPr` olarak durur;
                    // `w:tblGridChange` eski sütun ızgarasını taşır. Okuyucu
                    // bunları ayırt etmiyordu ve güncel değerlerin ÜSTÜNE
                    // yazıyorlardı: paragraf eski aralığıyla, eski stiliyle ve
                    // eski numaralandırmasıyla dönüşüyor; eski ızgaranın
                    // sütunları tabloya EKLENİYORDU (iki sütunlu tablo UDF'te
                    // beş sütun).
                    // Gerçek sözleşmenin değişik sürümünde 2 `pPrChange`,
                    // 24 `rPrChange` ve 50 `tcPrChange` bloğu var.
                    //
                    // Stil adı ayrıca `apply_contextual_spacing` kuralının
                    // anahtarıdır; yanlış stil, boşluk kararını da bozar.
                    if matches!(
                        name,
                        b"pPrChange"
                            | b"rPrChange"
                            | b"tcPrChange"
                            | b"trPrChange"
                            | b"tblPrChange"
                            | b"tblGridChange"
                            | b"sectPrChange"
                    ) {
                        if !is_empty {
                            revision_depth += 1;
                        }
                        continue;
                    }
                    if revision_depth > 0 {
                        continue;
                    }
                    if name == b"Fallback" {
                        if !is_empty {
                            fallback_depth += 1;
                        }
                        continue;
                    }
                    if fallback_depth > 0 {
                        continue;
                    }
                    // Kenarlık bildirimlerinin çocukları (`w:top`, `w:insideH`…)
                    // paragraf ya da sayfa kenarlığıyla aynı adları taşır; yalnız
                    // tablo ve hücre kenarlığı içindeyken kaydedilir.
                    if in_tbl_borders {
                        if let Some(t) = table_borders.last_mut() {
                            t.direct.record(e);
                        }
                        continue;
                    }
                    if in_tc_borders {
                        cell_borders.record(e);
                        continue;
                    }
                    match name {
                        b"tblStyle" => {
                            if let Some(t) = table_borders.last_mut() {
                                t.style = a(e, "val");
                            }
                        }
                        // Satır istisnaları (`w:tblPrEx`) tabloyu değil o satırı anlatır.
                        b"tblPrEx" if !is_empty => in_tblpr_ex = true,
                        b"tblBorders" if !is_empty && !in_tblpr_ex => in_tbl_borders = true,
                        b"tcBorders" if !is_empty => in_tc_borders = true,
                        _ => {}
                    }
                    match name {
                        b"p" if !in_sect_pr => {
                            if is_empty {
                                // <w:p/> is a blank line. quick-xml emits no End event for
                                // it, so it must be flushed here or it disappears.
                                if para.is_some() {
                                    floats.push(Block::Paragraph(Paragraph::default()));
                                } else {
                                    push_to(
                                        &mut cells,
                                        &mut out,
                                        Block::Paragraph(Paragraph::default()),
                                    );
                                }
                            } else {
                                if para.is_some() {
                                    // Metin kutusunun paragrafı: dıştakini beklet.
                                    host_stack.push(HostParagraph {
                                        para: para.take(),
                                        style: para_style.take(),
                                        direct: std::mem::take(&mut direct_para),
                                        mark_removed: para_mark_removed,
                                        run: std::mem::take(&mut run_direct),
                                    });
                                }
                                para = Some(Paragraph::default());
                                para_style = None;
                                direct_para = PartialPara::default();
                                para_mark_removed = false;
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
                        b"tbl" => {
                            table_borders.push(TableBorderState::default());
                            tables.push(Table {
                                border: TableBorder::Cell,
                                ..Default::default()
                            })
                        }
                        b"gridCol" => {
                            if let (Some(t), Some(w)) = (tables.last_mut(), af(e, "w")) {
                                t.column_widths.push(twips_to_pt(w));
                            }
                        }
                        b"tr" => {
                            if let Some(t) = table_borders.last_mut() {
                                t.rows.push(Vec::new());
                            }
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
                            cell_borders = BorderSet::default();
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
                        // Biçim, run'ın kendi `w:rPr`'ından gelir. `w:sdtPr/w:rPr`
                        // (denetimin yer tutucusuna ait biçim) eskiden ilk içerik
                        // run'ına sızıyordu.
                        b"r" if !is_empty => run_direct = PartialRun::default(),
                        b"t" => {
                            preserve_space = a(e, "space").as_deref() == Some("preserve");
                            // `m:t` (denklem) ve `a:t` (DrawingML) belge metni değildir.
                            let qname = e.name();
                            let prefix = qname.prefix().map(|p| p.into_inner().to_vec());
                            in_w_t =
                                !is_empty && !matches!(prefix.as_deref(), Some(b"m") | Some(b"a"));
                        }
                        b"tab" if !in_ppr && (deleted_depth > 0 || in_field_code(&field_stack)) => {
                        }
                        b"tab" if !in_ppr => {
                            if let Some(p) = para.as_mut() {
                                p.runs.push(Run::Tab {
                                    props: self.run_props(&para_style, &run_direct),
                                });
                            }
                        }
                        b"tab" if in_tabs => apply_ppr_child(e, &mut direct_para),
                        // Mutlak konumlu sekme: sekme olarak taşınır (eskiden düşüyor,
                        // iki yanındaki sözcükler birleşiyordu).
                        b"ptab" if deleted_depth == 0 && !in_field_code(&field_stack) => {
                            if let Some(p) = para.as_mut() {
                                p.runs.push(Run::Tab {
                                    props: self.run_props(&para_style, &run_direct),
                                });
                            }
                        }
                        // Satır sonu (`w:cr`) `w:br` gibidir.
                        b"cr" if deleted_depth == 0 && !in_field_code(&field_stack) => {
                            if let Some(p) = para.as_mut() {
                                p.runs.push(Run::LineBreak {
                                    props: self.run_props(&para_style, &run_direct),
                                });
                            }
                        }
                        // Bölünmez kısa çizgi: "2024-129" eskiden "2024129" oluyordu —
                        // başka, geçerli görünen bir sayı.
                        b"noBreakHyphen" if deleted_depth == 0 && !in_field_code(&field_stack) => {
                            if let Some(p) = para.as_mut() {
                                p.runs.push(Run::Text {
                                    text: "-".into(),
                                    props: self.run_props(&para_style, &run_direct),
                                });
                            }
                        }
                        // Sembol yazı tipi karakteri: karakter ve yazı tipiyle taşınır.
                        b"sym" if deleted_depth == 0 && !in_field_code(&field_stack) => {
                            let ch = a(e, "char")
                                .and_then(|h| u32::from_str_radix(h.trim(), 16).ok())
                                .and_then(char::from_u32);
                            match (ch, para.as_mut()) {
                                (Some(ch), Some(p)) => {
                                    let mut props = self.run_props(&para_style, &run_direct);
                                    if let Some(font) = a(e, "font") {
                                        props.font_family = Some(font);
                                    }
                                    p.runs.push(Run::Text {
                                        text: ch.to_string(),
                                        props,
                                    });
                                }
                                (None, Some(_)) => self.warn.push(
                                    Warning::new(WarningCode::RunFeatureDropped).detail("sembol"),
                                ),
                                _ => {}
                            }
                        }
                        // Tarih ve sayfa blokları Word'de açılışta hesaplanır.
                        b"dayLong" | b"dayShort" | b"monthLong" | b"monthShort" | b"yearLong"
                        | b"yearShort" | b"pgNum" => self.warn.warn(WarningCode::FieldFlattened),
                        b"fldSimple" => self.warn.warn(WarningCode::FieldFlattened),
                        // Çizim şekli: metin kutusu değilse UDF'te karşılığı yok.
                        b"wsp" if !is_empty => shape_stack.push(false),
                        b"txbx" => {
                            if let Some(top) = shape_stack.last_mut() {
                                *top = true;
                            }
                        }
                        b"line" | b"wgp" => self.warn.warn(WarningCode::ShapeDropped),
                        b"br" | b"cr" if deleted_depth > 0 || in_field_code(&field_stack) => {}
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
                        b"blip" if deleted_depth > 0 => {
                            pending_img = None;
                        }
                        b"blip" => {
                            if let Some(rid) = a(e, "embed") {
                                let (w, h) = pending_img.take().unwrap_or((0.0, 0.0));
                                self.push_image(&mut para, &rid, w, h)?;
                            }
                        }
                        b"imagedata" if deleted_depth > 0 => {}
                        b"imagedata" => {
                            if let Some(rid) = a(e, "id") {
                                self.push_image(&mut para, &rid, 0.0, 0.0)?;
                            }
                        }

                        // --- things Word can express and UDF cannot ---
                        // Paragraf işareti ya da tablo satırı silindiğinde Word
                        // BOŞ bir `<w:del/>` yazar (`w:pPr/w:rPr`, `w:trPr`
                        // içinde). Boş öğenin End olayı gelmez: bayrak eskiden
                        // takılı kalıyor, bir sonraki `</w:del>`'e kadar bütün
                        // güncel metni sessizce yutuyordu.
                        b"del" | b"moveFrom" => {
                            if !is_empty {
                                deleted_depth += 1;
                            } else if in_para_rpr {
                                para_mark_removed = true;
                            }
                            self.warn.warn(WarningCode::TrackedChangesFlattened);
                        }
                        b"ins" | b"moveTo" => self.warn.warn(WarningCode::TrackedChangesFlattened),
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
                        b"fldChar" => {
                            if deleted_depth == 0 {
                                match a(e, "fldCharType").as_deref() {
                                    Some("begin") => field_stack.push(true),
                                    Some("separate") => {
                                        if let Some(top) = field_stack.last_mut() {
                                            *top = false;
                                        }
                                    }
                                    Some("end") => {
                                        field_stack.pop();
                                    }
                                    _ => {}
                                }
                            }
                            self.warn.warn(WarningCode::FieldFlattened)
                        }
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
                    if !in_w_t || revision_depth > 0 || fallback_depth > 0 {
                        continue; // yalnız w:t metni belge metnidir
                    }
                    if deleted_depth > 0 {
                        continue; // deleted text is not part of the current document
                    }
                    if in_instr || in_field_code(&field_stack) {
                        continue; // alan komut kodu; kullanıcıya gösterilen metin değil
                    }
                    if let Some(p) = para.as_mut() {
                        let s = t.unescape().unwrap_or_default().to_string();
                        if s.is_empty() {
                            continue;
                        }
                        // Yalnız XML boşluğu biçimlemedir. `trim()` U+00A0'ı (bölünmez
                        // boşluk) da siliyordu; Word onu xml:space olmadan yazar ve
                        // "Madde 5" "Madde5" oluyordu.
                        if !preserve_space
                            && s.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
                        {
                            continue;
                        }
                        // `w:t` içindeki LF/CR satır sonu DEĞİLDİR: Word onu boşluk
                        // olarak gösterir (satır sonu `w:br`/`w:cr`'dır). Ham kalınca
                        // UDF tamponunda öğe ağacında olmayan bir paragraf açıyor
                        // (gerçek 14 belgede 43 LF), CR ise XML satır sonu
                        // normalleştirmesiyle silinip sonraki bütün ofsetleri
                        // kaydırıyordu.
                        let s = if s.contains(['\r', '\n']) {
                            s.replace("\r\n", " ").replace(['\r', '\n'], " ")
                        } else {
                            s
                        };
                        let run = self.partial_run(&para_style, &run_direct);
                        if run.vanish == Some(true) {
                            // Gizli metin yazdırılmaz; mahkemeye gidecek belgeye girmemeli.
                            if !hidden_text_dropped {
                                hidden_text_dropped = true;
                                self.warn.push(
                                    Warning::new(WarningCode::RunFeatureDropped)
                                        .detail("gizli metin"),
                                );
                            }
                            continue;
                        }
                        let s = if run.caps == Some(true) {
                            turkish_upper(&s)
                        } else {
                            s
                        };
                        let props = run.into_props();
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
                    let ln = e.local_name();
                    if matches!(
                        ln.as_ref(),
                        b"pPrChange"
                            | b"rPrChange"
                            | b"tcPrChange"
                            | b"trPrChange"
                            | b"tblPrChange"
                            | b"tblGridChange"
                            | b"sectPrChange"
                    ) {
                        revision_depth = revision_depth.saturating_sub(1);
                        continue;
                    }
                    if revision_depth > 0 {
                        continue;
                    }
                    if ln.as_ref() == b"Fallback" {
                        fallback_depth = fallback_depth.saturating_sub(1);
                        continue;
                    }
                    if fallback_depth > 0 {
                        continue;
                    }
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
                        b"tblBorders" => in_tbl_borders = false,
                        b"tcBorders" => in_tc_borders = false,
                        b"tblPrEx" => in_tblpr_ex = false,
                        b"t" => {
                            preserve_space = false;
                            in_w_t = false;
                        }
                        b"wsp" => {
                            if shape_stack.pop() == Some(false) {
                                self.warn.warn(WarningCode::ShapeDropped);
                            }
                        }
                        b"del" | b"moveFrom" => deleted_depth = deleted_depth.saturating_sub(1),
                        b"instrText" => in_instr = false,
                        b"extent" => in_drawing_extent = false,
                        b"r" => {
                            run_direct = PartialRun::default();
                            // Güvenlik ağı: `<w:instrText>` her zaman bir run içindedir.
                            // Bozuk bir belge onu kapatmadan run'ı bitirirse
                            // (check_end_names kapalı, quick-xml reddetmez), bayrağı
                            // burada sıfırlamazsak takılı kalıp sonraki tüm görünür
                            // metni yutar — sessiz veri kaybı. Run sonunda kesin kapat.
                            in_instr = false;
                        }
                        b"p" => {
                            if let Some(mut p) = para.take() {
                                p.props = self.para_props(&para_style, &direct_para);
                                if pending_break_page {
                                    pending_break_page = false;
                                    push_to(&mut cells, &mut out, Block::PageBreak);
                                }
                                // İşareti ve bütün içeriği silinmiş (ya da
                                // taşınmış) paragraf kabul edilmiş belgede yoktur;
                                // tutulursa yerinde boş bir satır kalır. İçeriği
                                // kalan paragraf ayrı tutulur: Word onu sonrakiyle
                                // birleştirir, burada paragraf yapısı yaklaşık kalır.
                                let keep = !(para_mark_removed && p.runs.is_empty());
                                para_mark_removed = false;
                                if let Some(host) = host_stack.pop() {
                                    // Metin kutusunun paragrafı: dıştaki paragraf bitince yazılır.
                                    if keep {
                                        floats.push(Block::Paragraph(p));
                                    }
                                    para = host.para;
                                    para_style = host.style;
                                    direct_para = host.direct;
                                    para_mark_removed = host.mark_removed;
                                    run_direct = host.run;
                                } else {
                                    if keep {
                                        push_to(&mut cells, &mut out, Block::Paragraph(p));
                                    }
                                    for f in floats.drain(..) {
                                        push_to(&mut cells, &mut out, f);
                                    }
                                    // Bozuk belgede kapanmayan alan kodu sonraki
                                    // paragrafların metnini yutmasın: kod kısmı
                                    // paragraf sonunda biter.
                                    field_stack.retain(|in_code| !in_code);
                                }
                            }
                        }
                        b"tc" => {
                            if let Some(row) =
                                table_borders.last_mut().and_then(|t| t.rows.last_mut())
                            {
                                row.push((pending_cell_span, cell_borders));
                            }
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
                            if let Some(mut t) = tables.pop() {
                                table_count += 1;
                                if let Some(state) = table_borders.pop() {
                                    let mut base =
                                        self.styles.table_borders(state.style.as_deref());
                                    base.overlay(&state.direct);
                                    let (border, partial) =
                                        effective_table_border(&base, &state.rows);
                                    t.border = border;
                                    if partial {
                                        self.warn.push(
                                            Warning::new(WarningCode::TableBordersApproximated)
                                                .at(format!("Tablo {table_count}")),
                                        );
                                    }
                                }
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
        self.partial_run(style, direct).into_props()
    }

    /// Word'ün sırası: belge varsayılanı → paragraf stili → karakter stili
    /// (`w:rStyle`) → doğrudan biçim. Karakter stili eskiden hiç çözülmüyordu:
    /// kalın bir karakter stiliyle yazılmış metin UDF'te düz çıkıyordu.
    fn partial_run(&self, style: &Option<String>, direct: &PartialRun) -> PartialRun {
        let (_, mut r) = self.styles.resolve(style.as_deref());
        if let Some(cs) = direct.char_style.as_deref() {
            r.overlay(&self.styles.char_style_run(cs));
        }
        r.overlay(direct);
        r
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
            contextual_spacing: p.contextual.unwrap_or(false),
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
