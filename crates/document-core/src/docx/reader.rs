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
    /// `word/settings.xml`: tek/çift sayfa üst bilgisi gibi belge geneli ayarlar.
    settings: Option<Vec<u8>>,
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
    /// Alt çizgi tek düz çizgi DEĞİL (çift, kalın, noktalı, dalgalı, yalnız
    /// sözcükler…): UDF'te (Swing) yalnız tek çizgi var.
    underline_styled: Option<bool>,
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
        if o.underline_styled.is_some() {
            self.underline_styled = o.underline_styled;
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
///
/// Kural her blok dizisinde ayrı uygulanır: tablo hücresinin ve üst/alt
/// bilginin paragrafları da (bkz. `walk_header_footer`). Eskiden yalnız gövdenin
/// üst düzeyine uygulanıyordu; hücredeki ve antetteki aynı stilli paragraflar
/// arasında Word'ün çizmediği boşluk UDF'e yazılıyordu (Source kipinde görünür).
fn apply_contextual_spacing(blocks: &mut [Block]) {
    /// İki paragraf Word'ün gözünde "aynı stil" mi?
    fn same_style(a: &Paragraph, b: &Paragraph) -> bool {
        a.props.style_name == b.props.style_name
    }
    for b in blocks.iter_mut() {
        if let Block::Table(t) = b {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    apply_contextual_spacing(&mut cell.blocks);
                }
            }
        }
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
        num_state: NumState::default(),
        active_part: None,
        page_number: None,
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
    let settings = parse_settings(b.pkg.settings.as_deref())?;
    section.header = b.header_footer(&sect.header_refs, sect.title_pg, &settings, true)?;
    section.footer = b.header_footer(&sect.footer_refs, sect.title_pg, &settings, false)?;

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
    let mut settings = None;
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
            || lower == "word/settings.xml"
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
            "word/settings.xml" => settings = Some(v),
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
        settings,
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
    mark: PartialRun,
}

/// Açık alanlardan herhangi birinin KOD kısmında mıyız?
fn in_field_code(stack: &[bool]) -> bool {
    stack.iter().any(|in_code| *in_code)
}

/// UDF `<header>`/`<footer>` üzerindeki `pageNumber-*` öznitelikleri, yazılış sırasıyla.
type PageNumberAttrs = Vec<(String, String)>;

/// Paragraftaki en dıştaki bir alan: komutu ve son görünen değerinin `runs`
/// içindeki aralığı (`start..end`).
#[derive(Default)]
struct FieldSpan {
    instr: String,
    start: Option<usize>,
    end: Option<usize>,
}

/// Alan komutu bir sayfa numarası mı: yalnız `PAGE` ya da `NUMPAGES`, ve yalnız
/// biçimi değiştirmeyen anahtarlarla (`\* MERGEFORMAT`, `\* Arabic`). `\* ROMAN`
/// gibi bir biçimin UYAP karşılığı bilinmediği için tanınmaz.
fn page_field_kind(instr: &str) -> Option<&'static str> {
    let mut tokens = instr.split_whitespace();
    let kind = match tokens.next()?.to_ascii_uppercase().as_str() {
        "PAGE" => "PAGE",
        "NUMPAGES" => "NUMPAGES",
        _ => return None,
    };
    while let Some(t) = tokens.next() {
        let format = match t.strip_prefix("\\*") {
            Some("") => tokens.next()?,
            Some(rest) => rest,
            None => return None,
        };
        if !matches!(
            format.to_ascii_uppercase().as_str(),
            "MERGEFORMAT" | "ARABIC" | "CHARFORMAT"
        ) {
            return None;
        }
    }
    Some(kind)
}

/// Üst/alt bilgideki paragraf yalnız [ön metin] + PAGE [+ ayırıcı + NUMPAGES]
/// [+ boşluk] taşıyorsa: UYAP'ın canlı sayfa numarası ve `pageNumber-*`
/// öznitelikleri.
///
/// `pageNumber-spec`'in kodlaması bilinmiyor; yalnız gerçek UDF'lerde görülen
/// değerler, görüldükleri öznitelik kümesiyle yazılır: alt bilgide "X / Y" →
/// BSP32_2120 (100 belge), alt bilgide "X" → BSP32_40 (18), üst bilgide "X / Y" →
/// BSP32_2116 (3). Karşılığı görülmemiş biçim (üst bilgide yalnız "X", sekmeyle
/// hizalanmış numara, başka alanlar) çevrilmez; son görünen değerinde kalır.
fn page_number_field(
    p: &Paragraph,
    fields: &[FieldSpan],
    fallback: &RunProps,
    is_header: bool,
) -> Option<(PageNumberField, PageNumberAttrs)> {
    // Yalnız düz metin run'larından oluşan aralığın metni.
    let literal = |from: usize, to: usize| -> Option<String> {
        p.runs
            .get(from..to)?
            .iter()
            .map(|r| match r {
                Run::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    };
    let span = |f: &FieldSpan| match (f.start, f.end) {
        (Some(s), Some(e)) if s <= e => Some((s, e)),
        _ => None,
    };
    let (page, total) = match fields {
        [page] => (page, None),
        [page, total] => (page, Some(total)),
        _ => return None,
    };
    if page_field_kind(&page.instr)? != "PAGE" {
        return None;
    }
    let (page_start, page_end) = span(page)?;
    let prefix = literal(0, page_start)?;
    let (separator, tail_from) = match total {
        None => (None, page_end),
        Some(t) => {
            if page_field_kind(&t.instr)? != "NUMPAGES" {
                return None;
            }
            let (total_start, total_end) = span(t)?;
            let sep = literal(page_end, total_start)?;
            if sep.trim().is_empty() {
                return None;
            }
            (Some(sep), total_end)
        }
    };
    if !literal(tail_from, p.runs.len())?.trim().is_empty() {
        return None;
    }
    let spec = match (is_header, separator.is_some()) {
        (false, true) => "BSP32_2120",
        (false, false) => "BSP32_40",
        (true, true) => "BSP32_2116",
        (true, false) => return None,
    };

    // Numaranın görünümü: alanın son görünen değerinin run'ı, yoksa ön metninki.
    let props = p.runs[page_start..page_end]
        .iter()
        .chain(&p.runs[..page_start])
        .find_map(|r| match r {
            Run::Text { props, .. } => Some(props),
            _ => None,
        })
        .unwrap_or(fallback);
    let face = props
        .font_family
        .clone()
        .or_else(|| fallback.font_family.clone())
        .unwrap_or_else(|| FontSpec::default().family);
    let size = props
        .font_size_pt
        .or(fallback.font_size_pt)
        .unwrap_or(FontSpec::default().size_pt)
        .round();
    let color = props.color.unwrap_or(Color::BLACK);
    // Gerçek UDF'lerde alt bilgide boş, üst bilgide "1".
    let start = if is_header { "1" } else { "" };
    let kv = |k: &str, v: String| (format!("pageNumber-{k}"), v);
    let attrs = match &separator {
        Some(sep) => vec![
            kv("spec", spec.into()),
            kv("seperator", sep.clone()),
            kv("fontBold", props.bold.to_string()),
            kv("fontItalic", props.italic.to_string()),
            kv("fontFace", face.clone()),
            kv("fontSize", format!("{}", size as i32)),
            kv("color", color.to_java_argb().to_string()),
            kv("foreStr", prefix.clone()),
            kv("pageStartNumStr", start.into()),
        ],
        // BSP32_40 örneklerinde kalın/italik ve ayırıcı öznitelikleri yoktur.
        None => vec![
            kv("spec", spec.into()),
            kv("color", color.to_java_argb().to_string()),
            kv("fontFace", face.clone()),
            kv("fontSize", format!("{}", size as i32)),
            kv("foreStr", prefix.clone()),
            kv("pageStartNumStr", start.into()),
        ],
    };
    let field = PageNumberField {
        prefix: (!prefix.is_empty()).then_some(prefix),
        start_at: is_header.then_some(1),
        bold: separator.is_some() && props.bold,
        italic: separator.is_some() && props.italic,
        separator,
        font_family: Some(face),
        font_size_pt: Some(size),
        color: Some(color),
        raw_spec: Some(spec.into()),
    };
    Some((field, attrs))
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
            r.underline_styled = Some(!matches!(
                a(e, "val").as_deref(),
                None | Some("single") | Some("none")
            ));
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

/// Etiketten sonra Word'ün koyduğu ayraç (`w:suff`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LabelSuffix {
    #[default]
    Tab,
    Space,
    Nothing,
}

/// Bir numaralandırma düzeyi (`w:lvl`): Word'ün bu düzeyde gösterdiği etiketin
/// tarifi.
///
/// Okuyucu eskiden yalnız `numFmt`'i okuyordu: "%1)" "1." oluyor, "3.2" "2."
/// oluyor, 5'ten başlayan liste 1'den başlıyordu (gerçek belgelerde yüzlerce
/// paragraf, uyarısız).
#[derive(Clone, Debug, Default)]
struct NumLevel {
    /// `w:start`; yoksa 0 (ECMA-376 §17.9.25).
    start: i64,
    /// `w:numFmt`; yoksa ondalık.
    fmt: Option<String>,
    /// `w:lvlText`: `%1`…`%9` yer tutuculu etiket kalıbı.
    text: Option<String>,
    /// `w:lvlRestart`: bu düzeyi hangi (1 tabanlı) düzey yeniden başlatır; 0 = hiçbiri.
    restart: Option<i64>,
    /// `w:isLgl`: etiketteki bütün sayaçlar ondalık gösterilir.
    legal: bool,
    suffix: LabelSuffix,
    /// `w:pStyle`: bu düzeye bağlı paragraf stili (başlık numaralandırması).
    style: Option<String>,
    /// `w:pPr`: düzeyin girintisi ve sekme durakları.
    para: PartialPara,
    /// `w:rPr`: etiketin biçimi (madde iminin yazı tipi dâhil).
    run: PartialRun,
}

impl NumLevel {
    fn fmt(&self) -> &str {
        self.fmt.as_deref().unwrap_or("decimal")
    }
}

#[derive(Default)]
struct AbstractNum {
    levels: HashMap<u32, NumLevel>,
    /// `w:numStyleLink`: düzeyler bir liste stilinin listesinde tanımlı.
    style_link: Option<String>,
}

#[derive(Default)]
struct NumInstance {
    abstract_id: u32,
    /// `w:lvlOverride/w:startOverride`
    start_overrides: HashMap<u32, i64>,
    /// `w:lvlOverride/w:lvl`
    level_overrides: HashMap<u32, NumLevel>,
    /// `w:lvlOverride` var mı. Word'ün "Numaralandırmayı yeniden başlat"ı
    /// böyle bir `w:num` yazar: bu liste kendi sayacıyla başlar.
    overridden: bool,
}

#[derive(Default)]
struct NumberingMap {
    abstracts: HashMap<u32, AbstractNum>,
    nums: HashMap<u32, NumInstance>,
}

impl NumberingMap {
    /// Soyut listenin düzeyleri. `w:numStyleLink` taşıyan soyut listenin kendi
    /// düzeyi yoktur; liste stilinin (`w:numPr`) gösterdiği listeye bakılır.
    fn levels_of(&self, abstract_id: u32, styles: &Styles) -> Option<&HashMap<u32, NumLevel>> {
        let ab = self.abstracts.get(&abstract_id)?;
        if ab.levels.is_empty() {
            if let Some(link) = &ab.style_link {
                let (p, _) = styles.resolve(Some(link));
                let linked = self.nums.get(&p.num_id?)?;
                return self
                    .abstracts
                    .get(&linked.abstract_id)
                    .map(|a| &a.levels)
                    .filter(|l| !l.is_empty());
            }
        }
        Some(&ab.levels)
    }

    fn level<'s>(
        &'s self,
        num: &'s NumInstance,
        ilvl: u32,
        styles: &Styles,
    ) -> Option<&'s NumLevel> {
        num.level_overrides
            .get(&ilvl)
            .or_else(|| self.levels_of(num.abstract_id, styles)?.get(&ilvl))
    }

    fn start(&self, num: &NumInstance, ilvl: u32, styles: &Styles) -> i64 {
        num.start_overrides
            .get(&ilvl)
            .copied()
            .or_else(|| self.level(num, ilvl, styles).map(|l| l.start))
            .unwrap_or(0)
    }

    /// Paragraf stiline `w:lvl/w:pStyle` ile bağlı düzey.
    fn level_of_style(&self, num: &NumInstance, style: &str, styles: &Styles) -> Option<u32> {
        self.levels_of(num.abstract_id, styles)?
            .iter()
            .find(|(_, l)| l.style.as_deref() == Some(style))
            .map(|(k, _)| *k)
    }
}

fn parse_numbering(xml: Option<&[u8]>) -> Result<NumberingMap> {
    let mut m = NumberingMap::default();
    let Some(xml) = xml else { return Ok(m) };
    let mut rd = Reader::from_reader(xml);
    rd.config_mut().check_end_names = false;
    let mut cur_abstract: Option<(u32, AbstractNum)> = None;
    let mut cur_num: Option<(u32, NumInstance)> = None;
    let mut cur_override: Option<u32> = None;
    let mut cur_lvl: Option<(u32, NumLevel)> = None;
    let (mut in_ppr, mut in_rpr) = (false, false);
    loop {
        let ev = rd.read_event();
        let empty = matches!(ev, Ok(Event::Empty(_)));
        match ev {
            Err(e) => return Err(ConvError::invalid_docx(format!("numbering XML error: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let ln = e.local_name();
                match ln.as_ref() {
                    b"abstractNum" => {
                        cur_abstract =
                            af(e, "abstractNumId").map(|v| (v as u32, AbstractNum::default()))
                    }
                    b"num" => cur_num = af(e, "numId").map(|v| (v as u32, NumInstance::default())),
                    b"abstractNumId" => {
                        if let (Some((_, n)), Some(v)) = (cur_num.as_mut(), af(e, "val")) {
                            n.abstract_id = v as u32;
                        }
                    }
                    b"lvlOverride" => {
                        cur_override = af(e, "ilvl").map(|v| v as u32);
                        if let Some((_, n)) = cur_num.as_mut() {
                            n.overridden = true;
                        }
                    }
                    b"startOverride" => {
                        if let (Some((_, n)), Some(l), Some(v)) =
                            (cur_num.as_mut(), cur_override, af(e, "val"))
                        {
                            n.start_overrides.insert(l, v as i64);
                        }
                    }
                    b"numStyleLink" => {
                        if let Some((_, ab)) = cur_abstract.as_mut() {
                            ab.style_link = a(e, "val");
                        }
                    }
                    b"lvl" if !empty => {
                        let il = af(e, "ilvl").unwrap_or(0.0).max(0.0) as u32;
                        cur_lvl = Some((il, NumLevel::default()));
                    }
                    name => {
                        let Some((_, l)) = cur_lvl.as_mut() else {
                            continue;
                        };
                        match name {
                            b"pPr" => in_ppr = !empty,
                            b"rPr" => in_rpr = !empty,
                            _ if in_ppr => apply_ppr_child(e, &mut l.para),
                            _ if in_rpr => apply_rpr_child(e, &mut l.run),
                            b"start" => l.start = af(e, "val").unwrap_or(0.0) as i64,
                            b"numFmt" => l.fmt = a(e, "val"),
                            b"lvlText" => l.text = Some(a(e, "val").unwrap_or_default()),
                            b"lvlRestart" => l.restart = af(e, "val").map(|v| v as i64),
                            b"isLgl" => l.legal = toggle(e),
                            b"suff" => {
                                l.suffix = match a(e, "val").as_deref() {
                                    Some("space") => LabelSuffix::Space,
                                    Some("nothing") => LabelSuffix::Nothing,
                                    _ => LabelSuffix::Tab,
                                }
                            }
                            b"pStyle" => l.style = a(e, "val"),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => match e.local_name().as_ref() {
                b"abstractNum" => {
                    if let Some((id, ab)) = cur_abstract.take() {
                        m.abstracts.insert(id, ab);
                    }
                }
                b"num" => {
                    if let Some((id, n)) = cur_num.take() {
                        m.nums.insert(id, n);
                    }
                }
                b"lvlOverride" => cur_override = None,
                b"pPr" => in_ppr = false,
                b"rPr" => in_rpr = false,
                b"lvl" => {
                    if let Some((il, l)) = cur_lvl.take() {
                        if let Some((_, n)) = cur_num.as_mut() {
                            n.level_overrides.insert(il, l);
                        } else if let Some((_, ab)) = cur_abstract.as_mut() {
                            ab.levels.insert(il, l);
                        }
                    }
                    in_ppr = false;
                    in_rpr = false;
                }
                _ => {}
            },
            _ => {}
        }
    }
    Ok(m)
}

/// Word'ün sayaçlarının paylaşıldığı birim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum CounterGroup {
    /// Geçersiz kılma taşımayan bütün `w:num`'lar soyut listenin sayacını
    /// paylaşır: Word numarayı bir `numId`'den ötekine SÜRDÜRÜR.
    Abstract(u32),
    /// `w:lvlOverride` taşıyan `w:num` kendi sayacıyla başlar.
    Num(u32),
}

/// Numaralandırmanın belge sırasındaki durumu.
#[derive(Default)]
struct NumState {
    /// Word'de her düzeyin o anki değeri (`None`: düzey henüz kullanılmadı ya da
    /// yeniden başlayacak).
    word: HashMap<CounterGroup, [Option<i64>; 9]>,
    /// UDF listelerinin kendi sırası (ListId → 1 tabanlı düzey değerleri):
    /// her madde düzeyinde bir ilerler, daha derin düzeyler baştan başlar.
    udf: HashMap<u32, [i64; 10]>,
}

/// Word'ün numara biçimiyle bir sayı. `None`: biçim desteklenmiyor.
fn format_number(n: i64, fmt: &str) -> Option<String> {
    // Kötü niyetli bir belgede `w:start` çok büyük olabilir: "aaaa…" ya da
    // "mmmm…" bellek tüketmesin.
    const MAX_REPEAT: i64 = 64;
    Some(match fmt {
        "decimal" => n.to_string(),
        "decimalZero" if (0..10).contains(&n) => format!("0{n}"),
        "decimalZero" => n.to_string(),
        "lowerLetter" | "upperLetter" if n > 0 && (n - 1) / 26 < MAX_REPEAT => {
            let c = (b'a' + ((n - 1) % 26) as u8) as char;
            let s = c.to_string().repeat(((n - 1) / 26 + 1) as usize);
            if fmt == "upperLetter" {
                s.to_uppercase()
            } else {
                s
            }
        }
        "lowerRoman" | "upperRoman" if n > 0 && n < 4000 => {
            let mut v = n;
            let mut s = String::new();
            for (k, r) in [
                (1000, "m"),
                (900, "cm"),
                (500, "d"),
                (400, "cd"),
                (100, "c"),
                (90, "xc"),
                (50, "l"),
                (40, "xl"),
                (10, "x"),
                (9, "ix"),
                (5, "v"),
                (4, "iv"),
                (1, "i"),
            ] {
                while v >= k {
                    s.push_str(r);
                    v -= k;
                }
            }
            if fmt == "upperRoman" {
                s.to_uppercase()
            } else {
                s
            }
        }
        "none" | "bullet" => String::new(),
        _ => return None,
    })
}

/// Word'ün bu paragrafta gösterdiği etiket (ayraçsız) ve biçimin birebir
/// hesaplanıp hesaplanamadığı (`false`: desteklenmeyen biçim ondalık yazıldı).
fn word_label(
    level: &NumLevel,
    levels: &[Option<&NumLevel>],
    values: &[Option<i64>; 9],
    starts: &[i64; 9],
) -> (String, bool) {
    let text = level.text.as_deref().unwrap_or("");
    if level.fmt() == "bullet" {
        return (text.to_string(), true);
    }
    let (mut out, mut exact) = (String::new(), true);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let slot = chars
            .peek()
            .and_then(|d| d.to_digit(10))
            .filter(|d| c == '%' && (1..=9).contains(d));
        let Some(d) = slot else {
            out.push(c);
            continue;
        };
        chars.next();
        let j = d as usize - 1;
        // Henüz kullanılmamış üst düzey başlangıcının bir eksiğiyle görünür:
        // Word'ün üst başlıksız alt başlıkta gösterdiği "0.1".
        let v = values[j].unwrap_or(starts[j].saturating_sub(1));
        let fmt = if level.legal {
            "decimal"
        } else {
            levels[j].map_or("decimal", |l| l.fmt())
        };
        match format_number(v, fmt) {
            Some(s) => out.push_str(&s),
            None => {
                exact = false;
                out.push_str(&v.to_string());
            }
        }
    }
    (out, exact)
}

/// Sembol yazı tipindeki (Symbol, Wingdings) madde imini Unicode karşılığıyla
/// yazar: UYAP bu yazı tiplerinin özel kod noktalarını göstermez. İkinci değer
/// `true` ise karşılığı bilinmeyen bir sembol kaldı; yazı tipi korunmalı.
fn bullet_glyph(text: &str, font: Option<&str>) -> (String, bool) {
    let mut unmapped = false;
    let s = text
        .chars()
        .map(|c| match (c as u32, font) {
            (0xF0B7, _) => '•',
            (0xF06C, Some("Wingdings")) => '●',
            (0xF06E, Some("Wingdings")) => '■',
            (0xF0A7, Some("Wingdings")) => '▪',
            (0xF076, Some("Wingdings")) => '❖',
            (0xF0D8, Some("Wingdings")) => '➢',
            (0xF0FC, Some("Wingdings")) => '✓',
            (0xF000..=0xF0FF, _) => {
                unmapped = true;
                c
            }
            _ => c,
        })
        .collect();
    (s, unmapped)
}

fn is_symbol_font(f: &str) -> bool {
    f == "Symbol" || f.starts_with("Wingdings") || f == "Webdings"
}

/// Word düzeyi, UDF'te birebir aynı görünen ve gerçek UYAP dosyalarında
/// görülmüş bir liste türüne mi karşılık geliyor? Etiket yalnız bu düzeyin
/// kendi sayacını göstermeli ("%2)" gibi), işaret türün işaretiyle aynı olmalı.
fn native_kind(level: &NumLevel, ilvl: u32) -> Option<ListKind> {
    let text = level.text.as_deref().unwrap_or("");
    if level.fmt() == "bullet" {
        let font = level.run.family.as_deref();
        return Some(ListKind::Bullet(match text {
            // Yuvarlak dolu madde imi: UDF `BULLET_TYPE_ELLIPSE`.
            "\u{F0B7}" if font == Some("Symbol") => BulletKind::Ellipse,
            "\u{F06C}" if font == Some("Wingdings") => BulletKind::Ellipse,
            "\u{2022}" | "\u{25CF}" => BulletKind::Ellipse,
            // UDF'in ok ve üçgen imleri Word'e bu imlerle yazılır
            // (docx/writer.rs); geri dönüşte aynı türe döner.
            "\u{F0D8}" if font == Some("Wingdings") => BulletKind::Arrow,
            "\u{25BA}" => BulletKind::Triangle,
            _ => return None,
        }));
    }
    let punct = text.strip_prefix(format!("%{}", ilvl + 1).as_str())?;
    let fmt = if level.legal { "decimal" } else { level.fmt() };
    Some(ListKind::Number(match (fmt, punct) {
        ("decimal", ".") => NumberKind::DecimalDot,
        ("decimal", ")") => NumberKind::DecimalParen,
        ("lowerLetter", ".") => NumberKind::LowerAlphaDot,
        ("lowerLetter", ")") => NumberKind::LowerAlphaParen,
        ("upperRoman", ".") => NumberKind::UpperRomanDot,
        _ => return None,
    }))
}

/// Bir uyarı ayrıntısı için etiket örneği: her sayaç 1 olarak ("%1.%2." → "1.1.").
fn label_sample(level: &NumLevel, levels: &[Option<&NumLevel>]) -> String {
    let ones = [Some(1); 9];
    word_label(level, levels, &ones, &[1; 9]).0
}

/// `word/settings.xml`'deki, sayfaya bağlı biçimi değiştiren belge ayarları.
#[derive(Default)]
struct DocSettings {
    /// `w:evenAndOddHeaders`: çift sayfalar "even" üst/alt bilgisini gösterir.
    /// Kapalıyken "even" parçası hiçbir sayfada basılmaz.
    even_and_odd_headers: bool,
}

fn parse_settings(xml: Option<&[u8]>) -> Result<DocSettings> {
    let mut s = DocSettings::default();
    let Some(xml) = xml else { return Ok(s) };
    security::reject_doctype(xml)?;
    let mut rd = Reader::from_reader(xml);
    rd.config_mut().check_end_names = false;
    loop {
        match rd.read_event() {
            Err(e) => return Err(ConvError::invalid_docx(format!("settings XML error: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) | Ok(Event::Empty(e))
                if e.local_name().as_ref() == b"evenAndOddHeaders" =>
            {
                s.even_and_odd_headers = toggle(&e);
            }
            _ => {}
        }
    }
    Ok(s)
}

/// Word'de hiçbir şey göstermeyen üst/alt bilgi: yalnız boş paragraflar.
///
/// Word "İlk sayfa farklı" seçildiğinde ilk sayfa için BOŞ bir parça yazar; anlamı
/// "üst bilgi ikinci sayfadan başlar"dır, bir içerik değil.
fn hf_is_blank(hf: &HeaderFooter) -> bool {
    hf.page_number.is_none()
        && hf.blocks.iter().all(|b| match b {
            Block::Paragraph(p) => p.runs.iter().all(|r| match r {
                Run::Text { text, .. } => text.trim().is_empty(),
                Run::Tab { .. } | Run::LineBreak { .. } => true,
                Run::Image(_) => false,
            }),
            Block::Table(_) | Block::PageBreak => false,
        })
}

#[derive(Default)]
struct SectInfo {
    page: PageSetup,
    /// Üst/alt bilgi başvuruları, çeşidine göre (`default`/`first`/`even`) ilişki
    /// kimliği. Birden çok `w:sectPr` varsa sonraki aynı çeşidi ezer.
    header_refs: HashMap<String, String>,
    footer_refs: HashMap<String, String>,
    /// Son `w:sectPr`'nin `w:titlePg`'si: "ilk sayfa farklı". Kapalıyken "first"
    /// parçası hiçbir sayfada gösterilmez.
    title_pg: bool,
}

struct BodyBuilder<'a> {
    pkg: &'a mut Pkg,
    styles: &'a Styles,
    numbering: &'a NumberingMap,
    warn: &'a mut WarningSink,
    lists: Vec<ListDef>,
    /// Sayaç grubu → UDF ListId. Aynı soyut listeyi paylaşan `w:num`'lar Word'de
    /// numarayı sürdürür; UDF'te de aynı liste olmalılar.
    used_num: HashMap<CounterGroup, u32>,
    num_state: NumState,
    /// Part whose relationship scope is currently active (`header1.xml`), or None for the
    /// document part.
    active_part: Option<String>,
    /// Üst/alt bilgi okunurken: UYAP'ın canlı sayfa numarasına çevrilen paragraf.
    page_number: Option<PageNumberProbe>,
}

/// Bir üst/alt bilgi parçasında sayfa numarası paragrafı arayışı.
struct PageNumberProbe {
    is_header: bool,
    /// Bulunan numara, UDF öznitelikleri ve paragrafın biçimi (parça boş kalırsa
    /// yerine konan boş paragraf için).
    found: Option<(PageNumberField, PageNumberAttrs, ParaProps)>,
}

impl<'a> BodyBuilder<'a> {
    /// Word'ün bölümde GERÇEKTEN gösterdiği üst ya da alt bilgiyi UDF'nin tek
    /// parçasına indirir.
    ///
    /// - "first" parçası yalnız `w:titlePg` açıkken, "even" parçası yalnız
    ///   `w:evenAndOddHeaders` açıkken basılır. Basılmayan bir çeşit kayıp değildir;
    ///   eskiden yine de "aktarılamadı" (KAYIP) uyarısı veriyordu (gerçek 9 belge).
    /// - Boş ilk sayfa + dolu diğer sayfalar → `startPage="2"`; dolu ilk sayfa + boş
    ///   diğer sayfalar → `stopPage="1"` (UYAP'ın iki gerçek biçimi).
    /// - İki parça aynıysa her sayfadadır. İkisi de dolu ve farklıysa UDF tek parça
    ///   tutar: diğer sayfalarınki korunur, ilk sayfanınki bildirilir.
    fn header_footer(
        &mut self,
        refs: &HashMap<String, String>,
        title_pg: bool,
        settings: &DocSettings,
        is_header: bool,
    ) -> Result<Option<HeaderFooter>> {
        let parts = if is_header {
            &self.pkg.headers
        } else {
            &self.pkg.footers
        };
        let get = |ty: &str| refs.get(ty).and_then(|rid| parts.get(rid)).cloned();
        let default = get("default");
        // OOXML: ilk bölümde "first" başvurusu yoksa ilk sayfa BOŞ bir parça gösterir.
        let first = title_pg.then(|| get("first"));
        if settings.even_and_odd_headers && get("even").is_some() {
            self.warn.warn(WarningCode::HeaderFooterVariantDropped);
        }

        let mut out = match default {
            Some((part, x)) => Some(self.walk_header_footer(&part, &x, is_header)?),
            None => None,
        };
        let Some(first) = first else {
            return Ok(out);
        };
        // İlk sayfa parçası ayrı bir uyarı toplayıcısıyla okunur: kullanılmazsa
        // (boşsa ya da diğerinin aynısıysa) uyarıları da iki kez sayılmaz.
        let (first_hf, first_warn) = match first {
            Some((part, x)) => {
                let saved = std::mem::take(&mut *self.warn);
                let r = self.walk_header_footer(&part, &x, is_header);
                let first_warn = std::mem::replace(&mut *self.warn, saved);
                (Some(r?), first_warn)
            }
            None => (None, WarningSink::new()),
        };
        let first_blank = first_hf.as_ref().map_or(true, hf_is_blank);
        match out.as_mut() {
            Some(d) if !hf_is_blank(d) => {
                if first_blank {
                    d.start_page = Some(2);
                } else if first_hf.as_ref() != Some(d) {
                    self.warn.warn(WarningCode::HeaderFooterVariantDropped);
                }
            }
            _ => {
                if let (Some(mut f), false) = (first_hf, first_blank) {
                    f.stop_page = Some(1);
                    self.warn.absorb(first_warn);
                    out = Some(f);
                }
            }
        }
        Ok(out)
    }

    fn walk_header_footer(
        &mut self,
        part: &str,
        xml: &[u8],
        is_header: bool,
    ) -> Result<HeaderFooter> {
        self.active_part = Some(part.to_string());
        // Üst/alt bilginin listeleri gövdenin numarasını sürdürmez.
        let body_numbers = std::mem::take(&mut self.num_state);
        self.page_number = Some(PageNumberProbe {
            is_header,
            found: None,
        });
        let r = self.walk_body(xml);
        self.num_state = body_numbers;
        self.active_part = None;
        let probe = self.page_number.take();
        let (mut blocks, _) = r?;
        apply_contextual_spacing(&mut blocks);
        let mut hf = HeaderFooter {
            blocks,
            stop_page: None,
            start_page: None,
            page_number_attrs: vec![],
            page_number: None,
        };
        if let Some((pn, attrs, props)) = probe.and_then(|p| p.found) {
            // UYAP'ın kendi biçimi: numara buffer'da metin değildir ve parça en az
            // bir (boş) paragraf taşır (gerçek UDF'lerde numaralı alt bilgilerin
            // çoğu tek boş paragraftır).
            if hf.blocks.is_empty() {
                hf.blocks.push(Block::Paragraph(Paragraph {
                    props,
                    runs: vec![],
                }));
            }
            hf.page_number = Some(pn);
            hf.page_number_attrs = attrs;
            self.warn.warn(WarningCode::UdfPageNumberApproximated);
        }
        Ok(hf)
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
        // Paragraf işaretinin biçimi (`w:pPr/w:rPr`): Word liste numarasını bu
        // biçimle gösterir.
        let mut mark_run = PartialRun::default();
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
        // Paragrafın en dıştaki alanları (komut + son görünen değerin run aralığı):
        // üst/alt bilgideki PAGE/NUMPAGES paragrafı bunlarla tanınır.
        // `para_fields_plain` iç içe alanda, metin kutusunda ya da bölünen
        // paragrafta düşer; o paragraf sayfa numarasına çevrilmez.
        let mut para_fields: Vec<FieldSpan> = Vec::new();
        let mut para_fields_plain = true;
        // Açık `w:fldSimple` derinliği (sonucu içindeki run'lardır).
        let mut simple_depth: usize = 0;
        // Paragrafta bir alan görüldü: sayfa numarasına çevrilmezse paragraf
        // sonunda FieldFlattened bildirilir.
        let mut field_seen = false;
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
                                        mark: std::mem::take(&mut mark_run),
                                    });
                                    para_fields_plain = false;
                                } else {
                                    para_fields.clear();
                                    para_fields_plain = true;
                                }
                                para = Some(Paragraph::default());
                                para_style = None;
                                direct_para = PartialPara::default();
                                mark_run = PartialRun::default();
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
                            // `w:titlePg` bölüme aittir, öncekinden miras kalmaz.
                            sect.title_pg = false;
                        }
                        b"titlePg" if in_sect_pr => sect.title_pg = toggle(e),
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
                        b"headerReference" | b"footerReference" if in_sect_pr => {
                            if let Some(id) = a(e, "id") {
                                let kind = a(e, "type").unwrap_or_else(|| "default".into());
                                if name == b"headerReference" {
                                    sect.header_refs.insert(kind, id);
                                } else {
                                    sect.footer_refs.insert(kind, id);
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
                        // Komut özniteliktedir, son görünen değer içindeki run'lardır.
                        b"fldSimple" => {
                            field_seen = true;
                            let at = para.as_ref().map(|p| p.runs.len()).unwrap_or(0);
                            if field_stack.is_empty() && simple_depth == 0 && para.is_some() {
                                para_fields.push(FieldSpan {
                                    instr: a(e, "instr").unwrap_or_default(),
                                    start: Some(at),
                                    end: is_empty.then_some(at),
                                });
                            } else {
                                para_fields_plain = false;
                            }
                            if !is_empty {
                                simple_depth += 1;
                            }
                        }
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
                                let at = para.as_ref().map(|p| p.runs.len());
                                // Yalnız en dıştaki alanın sonucu paragrafta görünür.
                                let outer = field_stack.len() == 1 && simple_depth == 0;
                                match a(e, "fldCharType").as_deref() {
                                    Some("begin") => {
                                        if field_stack.is_empty() && simple_depth == 0 {
                                            para_fields.push(FieldSpan::default());
                                        } else {
                                            para_fields_plain = false;
                                        }
                                        field_stack.push(true)
                                    }
                                    Some("separate") => {
                                        if let (true, Some(f)) = (outer, para_fields.last_mut()) {
                                            f.start = at;
                                        }
                                        if let Some(top) = field_stack.last_mut() {
                                            *top = false;
                                        }
                                    }
                                    Some("end") => {
                                        if let (true, Some(f)) = (outer, para_fields.last_mut()) {
                                            f.start = f.start.or(at);
                                            f.end = at;
                                        }
                                        field_stack.pop();
                                    }
                                    _ => {}
                                }
                            }
                            field_seen = true;
                        }
                        b"instrText" => {
                            // Boş `<w:instrText/>` metin taşımaz ve End üretmez; bayrağı
                            // yalnızca gerçek Start için kur, yoksa sonraki paragrafın
                            // metni yanlışlıkla bastırılır.
                            if !is_empty {
                                in_instr = true;
                            }
                            field_seen = true;
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
                    if in_para_rpr {
                        apply_rpr_child(e, &mut mark_run);
                    }
                }

                Ok(Event::Text(t)) => {
                    // En dıştaki alanın komutu (PAGE, NUMPAGES…): gösterilmez ama
                    // sayfa numarasını tanımak için okunur.
                    if in_instr
                        && field_stack.len() == 1
                        && simple_depth == 0
                        && deleted_depth == 0
                        && revision_depth == 0
                        && fallback_depth == 0
                    {
                        if let Some(f) = para_fields.last_mut() {
                            f.instr.push_str(&t.unescape().unwrap_or_default());
                        }
                    }
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
                        // Çift/kalın/noktalı alt çizgi UDF'te tek çizgi olur: metin
                        // altı çizili kalır, biçim farkı sessiz kalmaz.
                        if run.underline == Some(true) && run.underline_styled == Some(true) {
                            self.warn.push(
                                Warning::new(WarningCode::RunFeatureDropped)
                                    .detail("alt çizgi biçimi tek çizgi oldu"),
                            );
                        }
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
                        b"fldSimple" => {
                            if simple_depth == 1 && field_stack.is_empty() {
                                if let (Some(f), Some(p)) = (para_fields.last_mut(), para.as_ref())
                                {
                                    f.end = Some(p.runs.len());
                                }
                            }
                            simple_depth = simple_depth.saturating_sub(1);
                        }
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
                                // Liste sayacı yalnız kabul edilmiş belgede var
                                // olan paragrafta ilerler.
                                if keep {
                                    self.number_paragraph(
                                        &para_style,
                                        &direct_para,
                                        &mark_run,
                                        &mut p,
                                    );
                                }
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
                                    mark_run = host.mark;
                                    if std::mem::take(&mut field_seen) {
                                        self.warn.warn(WarningCode::FieldFlattened);
                                    }
                                } else {
                                    // Üst/alt bilgide yalnız [ön metin] + PAGE [+ ayırıcı +
                                    // NUMPAGES] taşıyan paragraf UYAP'ın canlı sayfa
                                    // numarasıdır: donmuş rakamlar buffer'a yazılmaz.
                                    // Eskiden her UDF sayfasında aynı numara basılıyordu
                                    // (gerçek 118 Word belgesi).
                                    let page_number = match self.page_number.as_ref() {
                                        Some(probe)
                                            if probe.found.is_none()
                                                && keep
                                                && para_fields_plain
                                                && floats.is_empty() =>
                                        {
                                            let fallback =
                                                self.run_props(&para_style, &PartialRun::default());
                                            page_number_field(
                                                &p,
                                                &para_fields,
                                                &fallback,
                                                probe.is_header,
                                            )
                                        }
                                        _ => None,
                                    };
                                    if let Some((pn, attrs)) = page_number {
                                        if let Some(probe) = self.page_number.as_mut() {
                                            probe.found = Some((pn, attrs, p.props.clone()));
                                        }
                                        field_seen = false;
                                    } else if keep {
                                        push_to(&mut cells, &mut out, Block::Paragraph(p));
                                    }
                                    if std::mem::take(&mut field_seen) {
                                        self.warn.warn(WarningCode::FieldFlattened);
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

        // Kapanmayan paragraftaki alan da bildirilir.
        if field_seen {
            self.warn.warn(WarningCode::FieldFlattened);
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

        // Liste üyeliği `number_paragraph`'ta, belge sırasıyla verilir.
        let list = None;

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

    /// Numaralı paragrafa Word'ün gösterdiği etiketi verir.
    ///
    /// Word'ün sayaçları belge sırasıyla yürütülür (ECMA-376 §17.9): aynı soyut
    /// listeyi paylaşan `w:num`'lar numarayı sürdürür, `startOverride` yeniden
    /// başlatır, bir düzey ilerleyince alt düzeyler baştan başlar (`w:lvlRestart`
    /// başka bir şey söylemiyorsa), `w:isLgl` bütün sayaçları ondalık gösterir.
    ///
    /// Düzey UDF'te birebir aynı görünen bir türe karşılık geliyor ve Word'ün
    /// numarası UDF listesinin kendi sırasıyla aynıysa paragraf UDF listesi olur.
    /// Değilse Word'ün gösterdiği etiket paragrafın başına METİN olarak yazılır,
    /// paragraf düzeyin girintisini alır ve bu bildirilir: "1." gösteren bir UDF
    /// listesi, Word'de "3.2" ya da "5." olan bir maddeyi sessizce başka bir
    /// numarayla yayımlamamalı.
    fn number_paragraph(
        &mut self,
        style: &Option<String>,
        direct: &PartialPara,
        mark: &PartialRun,
        p: &mut Paragraph,
    ) {
        let (numbering, styles) = (self.numbering, self.styles);
        let (mut resolved, _) = styles.resolve(style.as_deref());
        resolved.overlay(direct);
        let Some(num_id) = resolved.num_id.filter(|n| *n != 0) else {
            return;
        };
        // Tanımsız bir `numId`'ye Word numara koymaz; eskiden "1." uyduruluyordu.
        let Some(num) = numbering.nums.get(&num_id) else {
            return;
        };
        let linked_style = style.as_deref().or(styles.default_para_style.as_deref());
        let ilvl = resolved
            .num_level
            .or_else(|| linked_style.and_then(|s| numbering.level_of_style(num, s, styles)))
            .unwrap_or(0)
            .min(8);
        let Some(level) = numbering.level(num, ilvl, styles) else {
            return;
        };
        let levels: Vec<Option<&NumLevel>> =
            (0..9).map(|k| numbering.level(num, k, styles)).collect();
        let starts: [i64; 9] = std::array::from_fn(|k| numbering.start(num, k as u32, styles));

        // Word'ün sayacı.
        let group = if num.overridden {
            CounterGroup::Num(num_id)
        } else {
            CounterGroup::Abstract(num.abstract_id)
        };
        let k = ilvl as usize;
        let counters = self.num_state.word.entry(group).or_insert([None; 9]);
        counters[k] = Some(counters[k].map_or(starts[k], |v| v.saturating_add(1)));
        for (m, slot) in counters.iter_mut().enumerate().skip(k + 1) {
            // `w:lvlRestart`: düzey m, (1 tabanlı) bu düzey ya da daha üstü
            // kullanılınca baştan başlar; 0 = hiç. Kendisinden derin bir düzeyi
            // gösteriyorsa yok sayılır.
            let restart_after = match levels[m].and_then(|l| l.restart) {
                Some(0) => continue,
                Some(v) if v > 0 && (v as usize) <= m => v as usize,
                _ => m,
            };
            if k < restart_after {
                *slot = None;
            }
        }
        let values = *counters;
        let (label, exact) = word_label(level, &levels, &values, &starts);
        let bullet = level.fmt() == "bullet";

        // UDF'te birebir aynı görünüyor mu?
        let lv = ilvl + 1;
        if let Some(kind) = native_kind(level, ilvl) {
            let udf_next = self
                .used_num
                .get(&group)
                .and_then(|id| self.num_state.udf.get(id))
                .map_or(1, |c| c[lv as usize] + 1);
            if bullet || values[k] == Some(udf_next) {
                let id = match self.used_num.get(&group) {
                    Some(id) => *id,
                    None => {
                        let id = self.lists.len() as u32 + 1;
                        self.used_num.insert(group, id);
                        id
                    }
                };
                match self.lists.iter_mut().find(|d| d.list_id == id) {
                    Some(def) => {
                        if !def.levels.iter().any(|(l, _)| *l == lv) {
                            def.levels.push((lv, kind));
                        }
                    }
                    None => {
                        let mut def = ListDef::new(id, kind);
                        def.levels.push((lv, kind));
                        self.lists.push(def);
                    }
                }
                let c = self.num_state.udf.entry(id).or_insert([0; 10]);
                c[lv as usize] += 1;
                for deeper in c.iter_mut().skip(lv as usize + 1) {
                    *deeper = 0;
                }
                p.props.list = Some(ListRef {
                    list_id: id,
                    level: lv,
                });
                return;
            }
        }

        // Word'ün gösterdiği etiket metin olarak. Paragraf, Word'deki gibi
        // düzeyin girintisini alır: stil → düzey → doğrudan biçim.
        let (base, _) = styles.resolve(style.as_deref());
        let (mut left, mut right, mut first) = (None, None, None::<f32>);
        let mut tabs = base.tabs.clone();
        for layer in [&base, &level.para, direct] {
            left = layer.left.or(left);
            right = layer.right.or(right);
            // `firstLine` ile `hanging` aynı özelliğin iki yönüdür; üst katman
            // hangisini söylerse alttakinin ikisini de geçersiz kılar.
            if let Some(h) = layer.hanging {
                first = Some(-h);
            } else if let Some(f) = layer.first_line {
                first = Some(f);
            }
            if layer.tabs.is_some() {
                tabs = layer.tabs.clone();
            }
        }
        let left = left.unwrap_or(0.0);
        let first = first.unwrap_or(0.0);
        let mut tabs = tabs.unwrap_or_default();
        if level.suffix == LabelSuffix::Tab
            && first < 0.0
            && !tabs
                .iter()
                .any(|t| t.pos_pt > left + first + 0.01 && t.pos_pt <= left + 0.01)
        {
            // Asılı girintide Word, etiketten sonraki sekmeyi gövdenin başladığı
            // yere götürür (örtük sekme durağı).
            tabs.push(TabStop {
                pos_pt: left,
                align: TabAlign::Left,
            });
            tabs.sort_by(|a, b| a.pos_pt.total_cmp(&b.pos_pt));
        }
        p.props.left_indent_pt = left;
        p.props.right_indent_pt = right.unwrap_or(0.0);
        p.props.first_line_indent_pt = first.max(0.0);
        p.props.hanging_pt = (-first).max(0.0);
        p.props.tab_stops = tabs;

        // Boş etiket (`numFmt` "none", boş `lvlText`): Word numara göstermez;
        // uydurulmaz.
        if label.is_empty() {
            return;
        }

        let mut run = self.partial_run(style, mark);
        run.overlay(&level.run);
        let caps = run.caps == Some(true);
        let mut props = run.into_props();
        let mut text = if caps { turkish_upper(&label) } else { label };
        if bullet {
            let (glyph, keep_font) = bullet_glyph(&text, level.run.family.as_deref());
            if !keep_font && props.font_family.as_deref().is_some_and(is_symbol_font) {
                props.font_family = None;
            }
            text = glyph;
        }
        let detail = if text.trim().is_empty() {
            // Boşluktan ibaret işaret: görünmez, ama sekmesiyle metni kaydırır.
            "görünmeyen liste işareti metin olarak yazıldı".to_string()
        } else if bullet {
            format!("“{}” madde imi metin olarak yazıldı", text.trim())
        } else if native_kind(level, ilvl).is_some() && starts[k] != 1 {
            format!("başlangıç {}: numara metin olarak yazıldı", starts[k])
        } else if native_kind(level, ilvl).is_some() {
            format!(
                "“{}” numarası Word'deki sırasıyla metin olarak yazıldı",
                label_sample(level, &levels)
            )
        } else {
            format!(
                "“{}” numarası metin olarak yazıldı",
                label_sample(level, &levels)
            )
        };
        self.warn
            .push(Warning::new(WarningCode::ListNumberingApproximated).detail(detail));
        if !exact {
            self.warn.push(
                Warning::new(WarningCode::ListNumberingApproximated)
                    .detail(format!("{} biçimi ondalık sayıyla yazıldı", level.fmt())),
            );
        }
        let mut label_runs = Vec::with_capacity(2);
        match level.suffix {
            LabelSuffix::Tab => {
                label_runs.push(Run::Text {
                    text,
                    props: props.clone(),
                });
                label_runs.push(Run::Tab { props });
            }
            LabelSuffix::Space => label_runs.push(Run::Text {
                text: format!("{text} "),
                props,
            }),
            LabelSuffix::Nothing => label_runs.push(Run::Text { text, props }),
        }
        p.runs.splice(0..0, label_runs);
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
