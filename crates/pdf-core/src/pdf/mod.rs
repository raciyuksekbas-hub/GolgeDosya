pub mod stamp;
pub mod tolerant;
pub mod validate;
pub use validate::validate_document;

pub use tolerant::{load_pdf_tolerant, TolerantLoadResult};

use crate::error::{EklerError, Result};
use lopdf::{dictionary, Document as LopdfDoc, Object, ObjectId};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct PdfInfo {
    pub page_count: usize,
    pub is_signed: bool,
    pub pages_dimensions: Vec<(f32, f32)>, // (width_pt, height_pt)
    pub is_repaired: bool,
    pub repair_note: Option<String>,
}

/// PDF dosyasını okur, sayfa sayısını, sayfa boyutlarını, imza ve onarım durumunu tespit eder.
pub fn inspect_pdf(path: &Path) -> Result<PdfInfo> {
    let bytes = std::fs::read(path).map_err(|e| EklerError::IoError {
        path: path.to_path_buf(),
        source: e,
    })?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("belge.pdf");

    let load_res = load_pdf_tolerant(&bytes, file_name)?;
    let doc = load_res.document;
    let is_signed = detect_signature(&doc);
    let pages = doc.get_pages();
    let page_count = pages.len();

    let mut dimensions = Vec::with_capacity(page_count);
    for page_num in 1..=(page_count as u32) {
        if let Some(&page_id) = pages.get(&page_num) {
            let (w, h) = get_page_dimensions(&doc, page_id).unwrap_or((595.28, 841.89));
            dimensions.push((w, h));
        } else {
            dimensions.push((595.28, 841.89));
        }
    }

    Ok(PdfInfo {
        page_count,
        is_signed,
        pages_dimensions: dimensions,
        is_repaired: load_res.is_repaired,
        repair_note: load_res.repair_note,
    })
}

pub fn get_page_dimensions(doc: &LopdfDoc, page_id: ObjectId) -> Option<(f32, f32)> {
    let page = resolved_page_dictionary(doc, page_id).ok()?;
    let bounds = page
        .get(b"CropBox")
        .or_else(|_| page.get(b"MediaBox"))
        .ok()?;
    let (_, bounds) = doc.dereference(bounds).ok()?;
    let a = bounds.as_array().ok()?;
    if a.len() != 4 {
        return None;
    }
    let width = a[2].as_float().ok()? - a[0].as_float().ok()?;
    let height = a[3].as_float().ok()? - a[1].as_float().ok()?;
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return None;
    }
    let rotate = page
        .get(b"Rotate")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    match rotate {
        0 | 180 => Some((width, height)),
        90 | 270 => Some((height, width)),
        _ => None,
    }
}

/// Türetilmiş kopyadan elektronik imzanın kriptografik verisini çıkarır.
///
/// Yeniden yazılan bir PDF'te imzanın `/ByteRange`'i artık imzalanan baytları
/// göstermez: imza kriptografik olarak geçersizdir. Onu çıktıda bırakmak
/// kopyayı imzalı gibi gösterir — görüntüleyici bozuk bir imza listeler,
/// GölgeDosya'nın tarayıcısı kopyayı yeniden "imzalı" sayar. Kaldırılanlar:
/// imza sözlükleri (`/Type /Sig`, `/Type /DocTimeStamp`, `/ByteRange` +
/// `/Contents`), imza alanının değeri (`/FT /Sig` alanının `/V`si), Catalog
/// `/Perms` (DocMDP, UR3) ve `/DSS`, `/AcroForm /SigFlags`. İmza alanının
/// sayfadaki görünümü dokunulmadan kalır: kopya, basılmış imzalı bir belge
/// gibi görünür ama imzalı değildir. Dönen sayı kaldırılan imza sözlüğü ve
/// alan değeri sayısıdır.
pub fn strip_signatures(doc: &mut LopdfDoc) -> usize {
    let signatures: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, obj)| obj.as_dict().is_ok_and(|d| is_signature_dictionary(doc, d)))
        .map(|(id, _)| *id)
        .collect();
    if signatures.is_empty() && !has_signature_catalog_entries(doc) {
        return 0;
    }
    // İmza malzemesinin kökleri: imza sözlükleri ve /DSS'in (sertifika, OCSP,
    // CRL) başvurduğu nesneler. Ancak temizlikten SONRA belgeden hâlâ
    // erişilemeyenler boşaltılır; paylaşılan bir nesne yerinde kalır.
    let mut material: Vec<Object> = signatures
        .iter()
        .filter_map(|id| doc.get_object(*id).ok().cloned())
        .collect();
    if let Some(dss) = doc.catalog().ok().and_then(|c| c.get(b"DSS").ok()) {
        material.push(dss.clone());
    }
    let signature_only = reachable_ids(doc, material);

    let mut removed = signatures.len();
    // Nesne null olur, silinmez: ona başvuran her yer null görür, sarkan
    // başvuru kalmaz.
    for id in &signatures {
        doc.objects.insert(*id, Object::Null);
    }
    for obj in doc.objects.values_mut() {
        if let Object::Dictionary(d) = obj {
            if d.get(b"FT").and_then(|t| t.as_name()).ok() == Some(b"Sig")
                && d.remove(b"V").is_some()
            {
                removed += 1;
            }
        }
    }
    let acro_form = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .cloned();
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Perms");
        catalog.remove(b"DSS");
        if let Ok(Object::Dictionary(form)) = catalog.get_mut(b"AcroForm") {
            form.remove(b"SigFlags");
        }
    }
    if let Some(Object::Reference(id)) = acro_form {
        if let Ok(form) = doc.get_dictionary_mut(id) {
            form.remove(b"SigFlags");
        }
    }
    let still_used = reachable_ids(doc, doc.trailer.iter().map(|(_, v)| v.clone()).collect());
    for id in signature_only.difference(&still_used) {
        doc.objects.insert(*id, Object::Null);
    }
    removed
}

fn has_signature_catalog_entries(doc: &LopdfDoc) -> bool {
    doc.catalog()
        .is_ok_and(|c| c.has(b"Perms") || c.has(b"DSS"))
        || doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .and_then(|f| resolved_dict(doc, f))
            .is_some_and(|f| f.has(b"SigFlags"))
        || doc.objects.values().any(|o| {
            o.as_dict().is_ok_and(|d| {
                d.get(b"FT").and_then(|t| t.as_name()).ok() == Some(b"Sig") && d.has(b"V")
            })
        })
}

/// Verilen köklerden (doğrudan nesneler ya da başvurular) erişilen nesne
/// kimlikleri. Akış içerikleri kopyalanmaz; yalnız sözlükleri yürünür.
/// Başvuru zincirinin (`40 0 obj 12 0 R`) her halkası kaydedilir: `get_object`
/// zinciri sessizce atlasaydı hâlâ kullanılan son nesne "erişilemez" sayılıp
/// boşaltılabilirdi.
fn reachable_ids(doc: &LopdfDoc, roots: Vec<Object>) -> HashSet<ObjectId> {
    fn push_children(stack: &mut Vec<Object>, obj: &Object) {
        match obj {
            Object::Array(items) => stack.extend(items.iter().cloned()),
            Object::Dictionary(d) => stack.extend(d.iter().map(|(_, v)| v.clone())),
            Object::Stream(s) => stack.extend(s.dict.iter().map(|(_, v)| v.clone())),
            Object::Reference(_) => stack.push(obj.clone()),
            _ => {}
        }
    }
    let mut seen = HashSet::new();
    let mut stack = Vec::new();
    for root in &roots {
        push_children(&mut stack, root);
    }
    while let Some(obj) = stack.pop() {
        match obj {
            Object::Reference(id) => {
                if seen.insert(id) {
                    if let Some(found) = doc.objects.get(&id) {
                        push_children(&mut stack, found);
                    }
                }
            }
            other => push_children(&mut stack, &other),
        }
    }
    seen
}

/// ISO 32000-1 §12.8.1 imza sözlüğü mü? `/Type /Sig` ya da `/DocTimeStamp`;
/// tür yazılmamışsa zorunlu `/Filter` adı, `/ByteRange` dizisi ve `/Contents`
/// bayt dizesi BİRLİKTE. Anahtar adı tek başına yetmez: sayfanın `/Contents`i
/// bir akış başvurusudur, açıklamanın `/Contents`i metindir; ikisi de imza
/// değildir.
pub fn is_signature_dictionary(doc: &LopdfDoc, dict: &lopdf::Dictionary) -> bool {
    let resolved = |key: &[u8]| {
        dict.get(key)
            .ok()
            .and_then(|v| doc.dereference(v).ok())
            .map(|(_, v)| v)
    };
    matches!(
        dict.get(b"Type").and_then(|t| t.as_name()),
        Ok(b"Sig" | b"DocTimeStamp")
    ) || (matches!(resolved(b"Filter"), Some(Object::Name(_)))
        && matches!(resolved(b"ByteRange"), Some(Object::Array(_)))
        && matches!(resolved(b"Contents"), Some(Object::String(..))))
}

/// PDF'deki dijital imza (/Type /Sig veya /SigFlags) varlığını tespit eder.
pub fn detect_signature(doc: &LopdfDoc) -> bool {
    if let Ok(root) = doc.catalog() {
        if let Ok(acro_form) = root.get(b"AcroForm") {
            let acro_dict = match acro_form {
                Object::Reference(id) => doc.get_dictionary(*id).ok(),
                Object::Dictionary(ref d) => Some(d),
                _ => None,
            };
            if let Some(dict) = acro_dict {
                if let Ok(sig_flags) = dict.get(b"SigFlags") {
                    if let Ok(flags) = sig_flags.as_i64() {
                        if flags & 3 != 0 {
                            return true;
                        }
                    }
                }
            }
        }
    }

    doc.objects
        .values()
        .filter_map(|obj| obj.as_dict().ok())
        .any(|dict| is_signature_dictionary(doc, dict))
}

/// Belirtilen sayfa aralığını (1-indexed [start_page, end_page]) yeni bir PDF olarak çıkarır.
pub fn extract_page_range(doc: &LopdfDoc, start_page: usize, end_page: usize) -> Result<LopdfDoc> {
    let total_pages = doc.get_pages().len();
    if start_page < 1 || end_page > total_pages || start_page > end_page {
        return Err(EklerError::InvalidPdf(format!(
            "Geçersiz sayfa aralığı: {}-{} (Toplam: {})",
            start_page, end_page, total_pages
        )));
    }
    extract_pages(doc, &(start_page..=end_page).collect::<Vec<_>>())
}

/// `pages` sayfalarını (1 tabanlı, çıktı sırasıyla) yeni bir PDF olarak kopyalar.
///
/// Seç, Sırala ve Sil bu tek geçişi kullanır. Sayfalar tek tek ayrı belgelere
/// çıkarılıp sonra birleştirilseydi, sayfaların paylaştığı her kaynak — font,
/// gömülü font programı, görsel, ICC profili, Form XObject — onu kullanan sayfa
/// sayısı kadar kopyalanırdı. Böyle bir sayfaya giden iç bağlantılar da hedef
/// çıktıda kalsa bile kopardı.
pub fn extract_pages(doc: &LopdfDoc, pages: &[usize]) -> Result<LopdfDoc> {
    let source_pages = doc.get_pages();
    let ids = pages
        .iter()
        .map(|p| {
            u32::try_from(*p)
                .ok()
                .and_then(|p| source_pages.get(&p).copied())
                .ok_or_else(|| {
                    EklerError::InvalidPdf(format!(
                        "Geçersiz sayfa: {p} (Toplam: {})",
                        source_pages.len()
                    ))
                })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut out = LopdfDoc::with_version("1.7");
    let pages_id = out.new_object_id();
    let mut copy = PageGraphCopy::new(doc);
    let kids = copy.copy_pages(&mut out, &ids, pages_id)?;
    let layers = copy.copy_optional_content(&mut out)?;
    Ok(finish_page_tree(
        out,
        pages_id,
        kids,
        layers.into_iter().collect(),
    ))
}

/// Birden fazla PDF belgesini sırasıyla tek bir PDF olarak birleştirir.
///
/// Her kaynak belge kendi haritasıyla kopyalanır: bir belgenin sayfalarının
/// paylaştığı kaynak çıktıda tek nesnedir; farklı belgelerin nesneleri
/// birbirine karışmaz.
pub fn merge_documents(docs: &[LopdfDoc]) -> Result<LopdfDoc> {
    let mut out = LopdfDoc::with_version("1.7");
    let pages_id = out.new_object_id();
    let mut kids = Vec::new();
    let mut layers = Vec::new();
    for doc in docs {
        let ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
        let mut copy = PageGraphCopy::new(doc);
        kids.extend(copy.copy_pages(&mut out, &ids, pages_id)?);
        layers.extend(copy.copy_optional_content(&mut out)?);
    }
    Ok(finish_page_tree(out, pages_id, kids, layers))
}

/// Kaynağın isteğe bağlı içerik (katman) yapılandırması, açık ON/OFF
/// listelerine indirgenmiş hâlde. Catalog'daki `/OCProperties` taşınmazsa
/// kaynakta gizli olan katman kopyada görünür olur.
struct OptionalContent {
    groups: Vec<Object>,
    on: Vec<Object>,
    off: Vec<Object>,
    order: Vec<Object>,
    locked: Vec<Object>,
    radio_groups: Vec<Object>,
}

fn finish_page_tree(
    mut out: LopdfDoc,
    pages_id: ObjectId,
    kids: Vec<ObjectId>,
    layers: Vec<OptionalContent>,
) -> LopdfDoc {
    out.set_object(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => kids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => kids.len() as i32,
        },
    );
    let mut catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    };
    if !layers.is_empty() {
        // Birleştirmede her kaynağın yapılandırması açık listelerle birleşir;
        // taban durum varsayılan (ON) kalır, kapalı gruplar /OFF'ta.
        let join = |pick: fn(&OptionalContent) -> &Vec<Object>| {
            layers
                .iter()
                .flat_map(|l| pick(l).iter().cloned())
                .collect::<Vec<_>>()
        };
        let mut config = dictionary! {"OFF" => join(|l| &l.off)};
        for (key, items) in [
            ("ON", join(|l| &l.on)),
            ("Order", join(|l| &l.order)),
            ("Locked", join(|l| &l.locked)),
            ("RBGroups", join(|l| &l.radio_groups)),
        ] {
            if !items.is_empty() {
                config.set(key, items);
            }
        }
        catalog.set(
            "OCProperties",
            dictionary! {"OCGs" => join(|l| &l.groups), "D" => config},
        );
    }
    let catalog_id = out.add_object(catalog);
    out.trailer.set("Root", catalog_id);
    out
}

/// Resolve inheritable page attributes through an arbitrarily nested Pages tree.
pub fn resolved_page_dictionary(src: &LopdfDoc, id: ObjectId) -> Result<lopdf::Dictionary> {
    let mut page = src
        .get_dictionary(id)
        .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
        .clone();
    let mut current = id;
    let mut visited = std::collections::HashSet::new();
    loop {
        if !visited.insert(current) {
            return Err(EklerError::InvalidPdf("Döngüsel sayfa ağacı".into()));
        }
        let dictionary = src
            .get_dictionary(current)
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        for key in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
            if !page.has(key) {
                if let Ok(value) = dictionary.get(key) {
                    page.set(key.to_vec(), value.clone());
                }
            }
        }
        match dictionary.get(b"Parent") {
            Ok(parent) => {
                current = parent
                    .as_reference()
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
            }
            Err(_) => break,
        }
    }
    if !page.has(b"MediaBox") {
        return Err(EklerError::InvalidPdf("Sayfa MediaBox içermiyor".into()));
    }
    page.remove(b"Parent");
    Ok(page)
}

/// Bir kaynak belgenin sayfalarını, nesne grafiğini TEK bir eski→yeni id
/// haritasıyla kopyalar.
///
/// - Aynı kaynak nesnesi ikinci kez görüldüğünde yeniden kopyalanmaz: birden
///   çok sayfanın paylaştığı font, gömülü font programı, görsel, ICC profili,
///   ExtGState ve Form XObject çıktıda tek kanonik nesnedir. Eskiden harita
///   her sayfada sıfırdan açılıyordu; paylaşılan kaynak onu kullanan sayfa
///   sayısı kadar kopyalanıyordu (40 sayfalık belge sıralanınca 10–30 kat).
/// - Çıktıda kalacak sayfalar kopyalamadan ÖNCE haritaya yazılır: bir sayfa,
///   kendi sırası gelmeden başka bir sayfanın bağlantısında görülebilir.
///   Başvuru zinciri (`40 0 obj 12 0 R`) hedefine çözülerek eşlenir.
/// - Kopya özyinelemeyle değil bir iş listesiyle yürür: uzun zincirler yığını
///   taşırıp uygulamayı düşüremez.
/// - Sayfaya ait olmayan yapıya geçilmez, başvuru `null` olur: çıktıda olmayan
///   sayfa ve sayfa ağacı düğümleri, Catalog, yapı ağacı ve yapı öğeleri,
///   anahat, makale zinciri ve boncukları, belge parçaları (`/DPart`), imza
///   değeri, kalan sayfalarda olmayan açıklamalar ve kalan bir widget'ın atası
///   olmayan form alanı düğümleri. `/Type` isteğe bağlı olduğundan bu yapılar
///   tür adı yazılmamışsa biçimleriyle de tanınır. Kalan widget'ın alanı ise
///   korunur; alanın `/Kids`inde yalnız kalan çocuklar durur.
/// - Hedefi çıkarılmış bağlantının `/Dest`i ve hedefi çıkarılmış her GoTo
///   eylemi (`/A`, `/AA` girdileri, `/Next`) yazılmaz; GoTo'nun ardına
///   zincirlenmiş eylemler (`/Next`) onun yerine geçer. Sayfası null'a çözülen
///   hedef de çıkarılmış sayılır. Hedefi kalan bağlantı yeni sayfaya bağlanır;
///   adlandırılmış ve tamsayı sayfa numaralı hedef açık hedefe çözülür.
struct PageGraphCopy<'a> {
    src: &'a LopdfDoc,
    map: HashMap<ObjectId, ObjectId>,
    /// Çıktıda kalan sayfaların açıklamaları ve onların popup'ları.
    annotations: HashSet<ObjectId>,
    /// Kalan widget'ların /Parent zincirindeki form alanı düğümleri.
    fields: HashSet<ObjectId>,
    /// Kaynak sayfaları sırasıyla: tamsayı sayfa numaralı hedefler için.
    page_order: Vec<ObjectId>,
    /// Kimliği ayrılmış, gövdesi henüz yazılmamış kaynak nesneleri.
    pending: Vec<ObjectId>,
    /// `/Names /Dests` ad ağacının dizini; ilk ihtiyaçta bir kez kurulur.
    named: Option<HashMap<Vec<u8>, Object>>,
}

enum LocalDestination {
    /// Hedef sayfa çıktıda: açık hedef dizisi (sayfa başvurusu kopyalanırken
    /// yeni sayfaya bağlanır).
    Kept(Object),
    /// Hedef sayfa çıktıda değil ya da yok.
    Removed,
    /// Bu belgenin bir sayfasına gitmiyor (çözülemeyen ad…).
    NotLocal,
}

impl<'a> PageGraphCopy<'a> {
    fn new(src: &'a LopdfDoc) -> Self {
        Self {
            src,
            map: HashMap::new(),
            annotations: HashSet::new(),
            fields: HashSet::new(),
            page_order: src
                .get_pages()
                .into_values()
                .map(|id| resolve_chain(src, id))
                .collect(),
            pending: Vec::new(),
            named: None,
        }
    }

    fn copy_pages(
        &mut self,
        dst: &mut LopdfDoc,
        pages: &[ObjectId],
        parent: ObjectId,
    ) -> Result<Vec<ObjectId>> {
        // `get_pages`, sayfa ağacı sayfayı bir başvuru nesnesi üzerinden
        // listeliyorsa (`/Kids [40 0 R]`, `40 0 obj 12 0 R`) zincirin başını
        // verir; harita her yerde son kimlikle sorgulanır.
        let pages: Vec<ObjectId> = pages
            .iter()
            .map(|id| resolve_chain(self.src, *id))
            .collect();
        let kids: Vec<ObjectId> = pages
            .iter()
            .map(|old| {
                let new = dst.new_object_id();
                // Aynı sayfa iki kez istenirse başvurular ilk kopyaya bağlanır.
                self.map.entry(*old).or_insert(new);
                new
            })
            .collect();
        let resolved = pages
            .iter()
            .map(|old| resolved_page_dictionary(self.src, *old))
            .collect::<Result<Vec<_>>>()?;
        for page in &resolved {
            self.collect_annotations(page);
        }
        self.collect_fields();
        self.build_name_index();
        for (page, new) in resolved.iter().zip(&kids) {
            let mut copied = self.copy_dictionary(dst, page);
            copied.set("Parent", parent);
            dst.set_object(*new, copied);
        }
        self.drain(dst)?;
        Ok(kids)
    }

    /// Kimliği ayrılmış nesnelerin gövdelerini yazar.
    fn drain(&mut self, dst: &mut LopdfDoc) -> Result<()> {
        while let Some(old) = self.pending.pop() {
            let src = self.src;
            let obj = src.get_object(old).map_err(|e| {
                EklerError::InvalidPdf(format!("Eksik PDF nesnesi {:?}: {}", old, e))
            })?;
            let copied = self.copy_object(dst, obj);
            dst.set_object(self.map[&old], copied);
        }
        Ok(())
    }

    /// Catalog `/OCProperties`i aynı haritayla kopyalar: sayfaların
    /// kullandığı katman grupları çıktıda aynı nesnelerdir. Taban durumu
    /// `/OFF` olan yapılandırma, birleştirilebilsin diye açık `/OFF` listesine
    /// çevrilir. Alternatif yapılandırmalar (`/Configs`) ve kullanım
    /// uygulamaları (`/AS`) taşınmaz; varsayılan görünürlük korunur.
    fn copy_optional_content(&mut self, dst: &mut LopdfDoc) -> Result<Option<OptionalContent>> {
        let src = self.src;
        let Some(properties) = src
            .catalog()
            .ok()
            .and_then(|c| c.get(b"OCProperties").ok())
            .and_then(|p| resolved_dict(src, p))
        else {
            return Ok(None);
        };
        let list = |dict: Option<&lopdf::Dictionary>, key: &[u8]| {
            dict.and_then(|d| d.get(key).ok())
                .and_then(|a| resolved_array(src, a))
                .cloned()
                .unwrap_or_default()
        };
        let groups = list(Some(properties), b"OCGs");
        let config = properties
            .get(b"D")
            .ok()
            .and_then(|d| resolved_dict(src, d));
        let on = list(config, b"ON");
        let off = if config.and_then(|c| name(src, c.get(b"BaseState").ok())) == Some(b"OFF") {
            let on: HashSet<ObjectId> = on.iter().filter_map(|o| o.as_reference().ok()).collect();
            groups
                .iter()
                .filter(|g| !g.as_reference().is_ok_and(|id| on.contains(&id)))
                .cloned()
                .collect()
        } else {
            list(config, b"OFF")
        };
        let mut copy_all = |items: Vec<Object>| {
            items
                .iter()
                .map(|item| self.copy_object(dst, item))
                .collect::<Vec<_>>()
        };
        let layers = OptionalContent {
            groups: copy_all(groups),
            on: copy_all(on),
            off: copy_all(off),
            order: copy_all(list(config, b"Order")),
            locked: copy_all(list(config, b"Locked")),
            radio_groups: copy_all(list(config, b"RBGroups")),
        };
        self.drain(dst)?;
        Ok(Some(layers))
    }

    fn collect_annotations(&mut self, page: &lopdf::Dictionary) {
        let src = self.src;
        let Some(annots) = page
            .get(b"Annots")
            .ok()
            .and_then(|a| resolved_array(src, a))
        else {
            return;
        };
        for item in annots {
            let Object::Reference(id) = item else {
                continue;
            };
            let id = resolve_chain(src, *id);
            self.annotations.insert(id);
            if let Some(Object::Reference(popup)) = src
                .get_dictionary(id)
                .ok()
                .and_then(|a| a.get(b"Popup").ok())
            {
                self.annotations.insert(resolve_chain(src, *popup));
            }
        }
    }

    /// Kalan widget'ların /Parent zincirindeki alan düğümleri korunur.
    fn collect_fields(&mut self) {
        let src = self.src;
        for annotation in self.annotations.clone() {
            let mut current = src
                .get_dictionary(annotation)
                .ok()
                .and_then(|a| a.get(b"Parent").ok())
                .and_then(|p| p.as_reference().ok());
            for _ in 0..64 {
                let Some(id) = current.map(|id| resolve_chain(src, id)) else {
                    break;
                };
                if !self.fields.insert(id) {
                    break;
                }
                current = src
                    .get_dictionary(id)
                    .ok()
                    .and_then(|f| f.get(b"Parent").ok())
                    .and_then(|p| p.as_reference().ok());
            }
        }
    }

    /// `/Names /Dests` ad ağacını bir kez dizine çevirir. Düz ve uzun bir
    /// `/Names` dizisinde her bağlantı için doğrusal arama sırala işlemini
    /// sayfa sayısıyla karesel büyütüyordu.
    fn build_name_index(&mut self) {
        if self.named.is_some() {
            return;
        }
        let src = self.src;
        let mut index = HashMap::new();
        let tree = src
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Names").ok())
            .and_then(|n| resolved_dict(src, n))
            .and_then(|n| n.get(b"Dests").ok())
            .and_then(|t| resolved_dict(src, t));
        if let Some(tree) = tree {
            let mut stack = vec![tree];
            let mut seen = HashSet::new();
            let mut budget = 1_000_000usize;
            while let Some(node) = stack.pop() {
                let Some(left) = budget.checked_sub(1) else {
                    break;
                };
                budget = left;
                if let Some(names) = node.get(b"Names").ok().and_then(|n| resolved_array(src, n)) {
                    for pair in names.chunks(2) {
                        if let [k, v] = pair {
                            if let Some(key) = resolved_text(src, k) {
                                index.entry(key.to_vec()).or_insert_with(|| v.clone());
                            }
                        }
                    }
                }
                let Some(kids) = node.get(b"Kids").ok().and_then(|k| resolved_array(src, k)) else {
                    continue;
                };
                for kid in kids.iter().rev() {
                    if let Object::Reference(id) = kid {
                        // Döngülü ya da aynı düğümü tekrar gösteren /Kids
                        // (ör. `[7 0 R 7 0 R]`) her düğümü bir kez ziyaret eder.
                        if !seen.insert(*id) {
                            continue;
                        }
                    }
                    if let Some(kid) = resolved_dict(src, kid) {
                        stack.push(kid);
                    }
                }
            }
        }
        self.named = Some(index);
    }

    fn copy_object(&mut self, dst: &mut LopdfDoc, obj: &Object) -> Object {
        match obj {
            Object::Reference(id) => self.copy_reference(dst, *id),
            Object::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    if !self.is_removed_goto(item) {
                        out.push(self.copy_object(dst, item));
                        continue;
                    }
                    for kept in self.without_removed_gotos(item) {
                        out.push(self.copy_object(dst, &kept));
                    }
                }
                Object::Array(out)
            }
            Object::Dictionary(dict) => Object::Dictionary(self.copy_dictionary(dst, dict)),
            Object::Stream(stream) => Object::Stream(lopdf::Stream::new(
                self.copy_dictionary(dst, &stream.dict),
                stream.content.clone(),
            )),
            other => other.clone(),
        }
    }

    fn copy_reference(&mut self, dst: &mut LopdfDoc, id: ObjectId) -> Object {
        let id = resolve_chain(self.src, id);
        if let Some(&mapped) = self.map.get(&id) {
            return Object::Reference(mapped);
        }
        if self.outside_page_graph(id) {
            return Object::Null;
        }
        // Kimlik gövdeden ÖNCE ayrılır: döngüler (/Popup ↔ /Parent, kendi
        // kaynaklarına dönen Form XObject) haritada kapanır.
        let new = dst.new_object_id();
        self.map.insert(id, new);
        self.pending.push(id);
        Object::Reference(new)
    }

    fn copy_dictionary(
        &mut self,
        dst: &mut LopdfDoc,
        dict: &lopdf::Dictionary,
    ) -> lopdf::Dictionary {
        let src = self.src;
        let goto = name(src, dict.get(b"S").ok()) == Some(b"GoTo".as_slice());
        let field = is_field_node(src, dict);
        let mut out = lopdf::Dictionary::new();
        for (key, value) in dict.iter() {
            if key == b"Dest" || (goto && key == b"D") {
                match self.local_destination(value) {
                    LocalDestination::Kept(explicit) => {
                        let copied = self.copy_object(dst, &explicit);
                        out.set(key.clone(), copied);
                    }
                    LocalDestination::Removed => {}
                    LocalDestination::NotLocal => {
                        let copied = self.copy_object(dst, value);
                        out.set(key.clone(), copied);
                    }
                }
                continue;
            }
            if field && key == b"Kids" {
                // Alanın çocuklarından yalnız çıktıda kalanlar yazılır.
                let kids: Vec<Object> = resolved_array(src, value)
                    .map(|items| items.to_vec())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|kid| match kid {
                        Object::Reference(id) => {
                            let id = resolve_chain(src, *id);
                            self.map.contains_key(&id) || !self.outside_page_graph(id)
                        }
                        _ => true,
                    })
                    .collect();
                let copied = self.copy_object(dst, &Object::Array(kids));
                out.set(key.clone(), copied);
                continue;
            }
            if !self.is_removed_goto(value) {
                let copied = self.copy_object(dst, value);
                out.set(key.clone(), copied);
                continue;
            }
            let mut replacement = self.without_removed_gotos(value);
            let value = match replacement.len() {
                0 => continue,
                // Tek eylem bekleyen anahtar (/A, /AA girdisi): zincirin ilk eylemi.
                _ if key != b"Next" => replacement.swap_remove(0),
                1 => replacement.swap_remove(0),
                _ => Object::Array(replacement),
            };
            let copied = self.copy_object(dst, &value);
            out.set(key.clone(), copied);
        }
        out
    }

    /// Hedefi çıkarılmış GoTo eylemini, varsa ardına zincirlenmiş eylemlerle
    /// değiştirir; başka her değer olduğu gibi döner.
    fn without_removed_gotos(&self, value: &Object) -> Vec<Object> {
        let mut out = Vec::new();
        let mut pending = vec![value.clone()];
        let mut steps = 0;
        while let Some(current) = pending.pop() {
            steps += 1;
            if steps > 256 {
                break;
            }
            if !self.is_removed_goto(&current) {
                out.push(current);
                continue;
            }
            let next = resolved_dict(self.src, &current).and_then(|a| a.get(b"Next").ok());
            match next.map(|n| self.src.dereference(n).map(|(_, n)| n.clone())) {
                Some(Ok(Object::Array(items))) => pending.extend(items.into_iter().rev()),
                Some(Ok(_)) => pending.push(next.unwrap().clone()),
                _ => {}
            }
        }
        out
    }

    /// Sayfaya ait olmayan, belge düzeyindeki bir nesne mi? Kalan sayfalar
    /// haritada olduğundan buraya gelmez.
    fn outside_page_graph(&self, id: ObjectId) -> bool {
        let src = self.src;
        let Ok(obj) = src.get_object(id) else {
            // Eksik nesneyi iş listesi kendi sözüyle bildirir.
            return false;
        };
        let dict = match obj {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &s.dict,
            _ => return false,
        };
        let kind = name(src, dict.get(b"Type").ok());
        if matches!(
            kind,
            Some(
                b"Page"
                    | b"Pages"
                    | b"Catalog"
                    | b"StructTreeRoot"
                    | b"StructElem"
                    | b"OBJR"
                    | b"MCR"
                    | b"Outlines"
                    | b"Thread"
                    | b"Bead"
                    | b"DPart"
                    | b"DPartRoot"
                    | b"Sig"
                    | b"DocTimeStamp"
            )
        ) {
            return true;
        }
        let subtype = name(src, dict.get(b"Subtype").ok());
        // Açıklamanın /Type'ı isteğe bağlıdır; alt türü ve /Rect'i onu tanıtır.
        if kind == Some(b"Annot".as_slice())
            || (dict.has(b"Rect") && subtype.is_some_and(is_annotation_subtype))
        {
            return !self.annotations.contains(&id);
        }
        if is_field_node(src, dict) {
            return !self.fields.contains(&id);
        }
        // /Type'sız imza değeri, boncuk (/N /V /P /R), yapı öğesi (/S ad + /P
        // + /K ya da /Pg) ve belge parçası (/DParts ya da /Start + /End).
        is_signature_dictionary(src, dict)
            || (dict.has(b"N") && dict.has(b"V") && dict.has(b"P") && dict.has(b"R"))
            || (subtype.is_none()
                && name(src, dict.get(b"S").ok()).is_some()
                && dict.has(b"P")
                && (dict.has(b"K") || dict.has(b"Pg")))
            || dict.has(b"DParts")
            || (dict.get(b"Start").and_then(|s| s.as_reference()).is_ok()
                && dict.get(b"End").and_then(|e| e.as_reference()).is_ok())
    }

    /// Bir `/Dest` ya da GoTo `/D` değeri bu belgenin hangi sayfasına gidiyor?
    fn local_destination(&self, value: &Object) -> LocalDestination {
        let Some(mut explicit) = self.explicit_destination(value, 0) else {
            return LocalDestination::NotLocal;
        };
        let page = match explicit.first() {
            Some(Object::Reference(id)) => {
                let id = resolve_chain(self.src, *id);
                match self.src.get_object(id) {
                    // Yükleyicinin null saydığı ya da dosyada olmayan sayfa.
                    Ok(Object::Null) | Err(_) => return LocalDestination::Removed,
                    _ if is_page_tree_node(self.src, id) => {
                        explicit[0] = Object::Reference(id);
                        id
                    }
                    _ => return LocalDestination::NotLocal,
                }
            }
            // Yerel hedefte tamsayı: 0 tabanlı sayfa numarası. Standart dışı,
            // ama üreticiler yazar ve pdf.js böyle okur; sıra değişince yanlış
            // sayfaya gitmesin diye sayfa başvurusuna çevrilir.
            Some(Object::Integer(n)) => {
                match usize::try_from(*n)
                    .ok()
                    .and_then(|n| self.page_order.get(n))
                {
                    Some(id) => {
                        explicit[0] = Object::Reference(*id);
                        *id
                    }
                    None => return LocalDestination::Removed,
                }
            }
            Some(Object::Null) => return LocalDestination::Removed,
            _ => return LocalDestination::NotLocal,
        };
        if self.map.contains_key(&page) {
            LocalDestination::Kept(Object::Array(explicit))
        } else {
            LocalDestination::Removed
        }
    }

    /// `/Dest` değerini açık hedef dizisine çözer: dizi, `/D` taşıyan sözlük ya
    /// da ad. ISO 32000-1 §12.3.2.3: dize `/Names /Dests` ad ağacında, ad
    /// Catalog `/Dests` sözlüğünde aranır; üreticiler karıştırdığı için öbürü
    /// yedektir.
    fn explicit_destination(&self, value: &Object, depth: usize) -> Option<Vec<Object>> {
        if depth > 8 {
            return None;
        }
        let src = self.src;
        let (_, value) = src.dereference(value).ok()?;
        let (key, string) = match value {
            Object::Array(items) => return Some(items.clone()),
            Object::Dictionary(d) => {
                return self.explicit_destination(d.get(b"D").ok()?, depth + 1)
            }
            Object::Name(key) => (key, false),
            Object::String(key, _) => (key, true),
            _ => return None,
        };
        let from_dests = || {
            src.catalog()
                .ok()
                .and_then(|c| c.get(b"Dests").ok())
                .and_then(|d| resolved_dict(src, d))
                .and_then(|d| d.get(key).ok().cloned())
        };
        let from_tree = || {
            self.named
                .as_ref()
                .and_then(|n| n.get(key.as_slice()).cloned())
        };
        let found = if string {
            from_tree().or_else(from_dests)
        } else {
            from_dests().or_else(from_tree)
        }?;
        self.explicit_destination(&found, depth + 1)
    }

    /// Hedef sayfası çıktıda olmayan bir GoTo eylemi mi?
    fn is_removed_goto(&self, value: &Object) -> bool {
        let Some(action) = resolved_dict(self.src, value) else {
            return false;
        };
        name(self.src, action.get(b"S").ok()) == Some(b"GoTo".as_slice())
            && action
                .get(b"D")
                .is_ok_and(|d| matches!(self.local_destination(d), LocalDestination::Removed))
    }
}

/// Başvuru zincirini (`40 0 obj 12 0 R`) son nesnesinin KİMLİĞİNE çözer.
///
/// `get_object` zinciri kendisi izleyip son nesnenin gövdesini döndürür ama
/// kimliği vermez; harita ve küme sorguları kimlikle yapıldığından zincir
/// burada `objects` üzerinde, lopdf'in kendi sınırıyla (128 adım) yürünür.
fn resolve_chain(src: &LopdfDoc, mut id: ObjectId) -> ObjectId {
    for _ in 0..128 {
        match src.objects.get(&id) {
            Some(Object::Reference(next)) => id = *next,
            _ => break,
        }
    }
    id
}

/// Açıklama olmayan bir form alanı düğümü mü? (ISO 32000-1 §12.7.3.1:
/// `/FT` ya da `/T` ile `/Kids`, `/Parent`, `/V`.) Eylem sözlükleri (`/S`)
/// ve açıklamalar (`/Subtype`) sayılmaz.
fn is_field_node(src: &LopdfDoc, dict: &lopdf::Dictionary) -> bool {
    !dict.has(b"Subtype")
        && name(src, dict.get(b"S").ok()).is_none()
        && (dict.has(b"FT")
            || (dict.has(b"T") && (dict.has(b"Kids") || dict.has(b"Parent") || dict.has(b"V"))))
}

fn is_annotation_subtype(subtype: &[u8]) -> bool {
    matches!(
        subtype,
        b"Text"
            | b"Link"
            | b"FreeText"
            | b"Line"
            | b"Square"
            | b"Circle"
            | b"Polygon"
            | b"PolyLine"
            | b"Highlight"
            | b"Underline"
            | b"Squiggly"
            | b"StrikeOut"
            | b"Stamp"
            | b"Caret"
            | b"Ink"
            | b"Popup"
            | b"FileAttachment"
            | b"Sound"
            | b"Movie"
            | b"Widget"
            | b"Screen"
            | b"PrinterMark"
            | b"TrapNet"
            | b"Watermark"
            | b"3D"
            | b"Redact"
            | b"RichMedia"
            | b"Projection"
    )
}

/// Kaynak belgenin bir sayfası ya da sayfa ağacı düğümü mü?
fn is_page_tree_node(src: &LopdfDoc, id: ObjectId) -> bool {
    src.get_dictionary(id)
        .ok()
        .and_then(|d| d.get(b"Type").ok())
        // /Type dolaylı bir ad olabilir (`/Type 12 0 R`); çözülmeden bakılırsa
        // sayfa düğümü tanınmaz ve ağaç yine sürüklenir.
        .and_then(|t| src.dereference(t).ok())
        .and_then(|(_, t)| t.as_name().ok())
        .is_some_and(|t| t == b"Page" || t == b"Pages")
}

fn name<'o>(src: &'o LopdfDoc, value: Option<&'o Object>) -> Option<&'o [u8]> {
    value
        .and_then(|v| src.dereference(v).ok())
        .and_then(|(_, v)| v.as_name().ok())
}

fn resolved_dict<'o>(src: &'o LopdfDoc, value: &'o Object) -> Option<&'o lopdf::Dictionary> {
    src.dereference(value)
        .ok()
        .and_then(|(_, v)| v.as_dict().ok())
}

fn resolved_array<'o>(src: &'o LopdfDoc, value: &'o Object) -> Option<&'o Vec<Object>> {
    src.dereference(value)
        .ok()
        .and_then(|(_, v)| v.as_array().ok())
}

fn resolved_text<'o>(src: &'o LopdfDoc, value: &'o Object) -> Option<&'o [u8]> {
    src.dereference(value)
        .ok()
        .and_then(|(_, v)| v.as_str().ok().or_else(|| v.as_name().ok()))
}
