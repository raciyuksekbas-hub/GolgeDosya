//! Açıklama görünümlerini YALNIZ çizim kopyasında sayfa içeriğine yassılaştırma.
//!
//! Saha bulgusu (v0.3.2): "Vurgu falan varsa PDF'de, bunu PNG'ye veya JPEG'e
//! dönüştüremiyor." macOS'ta kullanılan CoreGraphics `CGContextDrawPDFPage`
//! yalnız sayfa içeriğini çizer; açıklamaların görünüm akışlarını (`/AP`)
//! çizmez. Ölçüldü: vurgu, not, mürekkep, damga ve doldurulmuş form alanı
//! dahil hiçbir açıklama çizilmiyordu. Dışa aktarma bu yüzden `/Annots`
//! taşıyan HER sayfayı toptan reddediyor, Düzenle önizlemesi de açıklamaları
//! sessizce düşürüyordu.
//!
//! Burada bir görüntüleyicinin ekranda gösterdiğini (ISO 32000-1 §12.5.5)
//! sayfa içeriğine çizeriz: her görünür açıklamanın normal görünümü (`/AP
//! /N`, gerekirse `/AS` alt durumu), BBox × Matrix'in sınır kutusu `/Rect`'e
//! eşlenerek bir Form XObject olarak çağrılır. İşlem YALNIZ bellekteki çizim
//! kopyasında yapılır; kaynak dosya açılır, okunur ve bayt bayt aynı kalır.
//! Yeni PDF kaydetme/sıkıştırma çıktılarında açıklamalar canlı kalır; bu
//! modül oralarda çağrılmaz.
//!
//! Sözleşme:
//! * Açıklamasız sayfaya dokunulmaz (altın görüntüler kaymaz).
//! * Sayfanın kaynakları (miras alınmışsa da) sayfaya KOPYALANIR; paylaşılan
//!   `/Resources`, `/XObject` ya da `/ExtGState` nesnesi yerinde değişmez,
//!   var olan girdiler korunur, yeni ad benzersizdir.
//! * Özgün içerik `q … Q` ile sarılır; açıklama katmanı en sona, görünür
//!   kutuya (CropBox ∩ MediaBox) kırpılarak eklenir. Marka payı sonradan
//!   eklense bile kutu dışındaki bir görünüm açığa çıkmaz.
//! * Ardından `/Annots` ve `/AcroForm` kopyadan TAMAMEN kaldırılır: Windows
//!   renderer'ı açıklamaları kendisi de çiziyorsa iki kez çizilmez, sonraki
//!   adımlar (marka payı denetimi) atlanan açıklamalara takılmaz.
//! * Fonksiyon hata döndürmez. Okunamayan girdi (null, `/Rect`'siz, eksik
//!   nesneye giden `/AP`) atlanır; önizleme açıklama yüzünden asla düşmez.
//!   Görünür olup görünümü kurulamayan açıklamalar yalnız SAYILIR; dışa
//!   aktarmanın reddedip reddetmeyeceğine çağıran karar verir.
//!
//! Bilinçli sınırlar: `/NoZoom` ve `/NoRotate` bayrakları uygulanmaz (not
//! simgesi sayfayla birlikte döner ve ölçeklenir). Görünürlük ekran
//! semantiğidir: `/F` Hidden ya da NoView atlanır, Print bayrağına bakılmaz.
use super::resolved_page_dictionary;
use lopdf::{dictionary, Dictionary, Document as LopdfDoc, Object, ObjectId, Stream};
use std::collections::{HashMap, HashSet};

/// Dışa aktarmanın dar reddi: görünür bir açıklamanın görünümü belgede
/// kayıtlı değil ve güvenle kurulamıyor.
pub const MISSING_APPEARANCE_MESSAGE: &str = "Bu belgedeki bir açıklamanın görünümü belgede kayıtlı değil; görünüm kaybını önlemek için görsele dönüştürülmedi.";

/// Yassılaştırmanın sonucu.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FlattenReport {
    /// Sayfa içeriğine çizilen açıklama sayısı.
    pub drawn: usize,
    /// Görüntüleyicide de hiçbir şey çizmeyen, bu yüzden atlanan açıklama sayısı.
    pub skipped: usize,
    /// Görünür olduğu hâlde görünümü kurulamayan açıklama sayısı.
    pub missing_appearance: usize,
}

const INVISIBLE: i64 = 1;
const HIDDEN: i64 = 2;
const NO_VIEW: i64 = 32;
/// Düğme alanı bayrağı (ISO 32000-1 Tablo 226): basmalı düğmenin değeri yoktur.
const PUSHBUTTON: i64 = 1 << 16;

/// Bir açıklamanın çizimi: hangi form, hangi `cm` ile.
struct Draw {
    source: Source,
    matrix: [f32; 6],
}

enum Source {
    /// Belgede kayıtlı görünüm akışı.
    Stored(ObjectId),
    /// `/AP` taşımayan metin işaretlemesi için kurulan görünüm.
    Built(Stream),
}

enum Plan {
    Skip,
    Draw(Draw),
    Missing,
}

/// Görünür açıklamaları çizim kopyasının sayfa içeriğine çizer, ardından
/// `/Annots` ve `/AcroForm`'u kopyadan kaldırır. Bkz. modül açıklaması.
pub fn flatten_annotations_for_render(doc: &mut LopdfDoc) -> FlattenReport {
    let mut report = FlattenReport::default();
    let layers = OptionalContent::read(doc);
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for page_id in pages {
        let items = match doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|p| p.get(b"Annots").ok())
        {
            None => continue,
            Some(annots) => match doc.dereference(annots) {
                Ok((_, Object::Array(items))) => items.clone(),
                _ => Vec::new(),
            },
        };
        let mut draws = Vec::new();
        for item in &items {
            match plan(doc, &layers, item) {
                Plan::Skip => report.skipped += 1,
                Plan::Missing => report.missing_appearance += 1,
                Plan::Draw(draw) => draws.push(draw),
            }
        }
        let count = draws.len();
        if count > 0 {
            if draw_layer(doc, page_id, draws).is_some() {
                report.drawn += count;
            } else {
                // Sayfa yerleştirilemedi (ör. MediaBox yok): görünüm kaybolur.
                report.missing_appearance += count;
            }
        }
        if let Ok(page) = doc.get_dictionary_mut(page_id) {
            page.remove(b"Annots");
        }
    }
    let root = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|r| r.as_reference().ok());
    if let Some(root) = root {
        if let Ok(catalog) = doc.get_dictionary_mut(root) {
            catalog.remove(b"AcroForm");
        }
    }
    report
}

fn number(doc: &LopdfDoc, value: &Object) -> Option<f32> {
    doc.dereference(value)
        .ok()
        .and_then(|(_, v)| v.as_float().ok())
        .filter(|v| v.is_finite())
}

fn numbers(doc: &LopdfDoc, value: Option<&Object>) -> Option<Vec<f32>> {
    let (_, array) = doc.dereference(value?).ok()?;
    array
        .as_array()
        .ok()?
        .iter()
        .map(|v| number(doc, v))
        .collect()
}

/// Köşe sırası normalize edilmiş dikdörtgen.
fn rectangle(doc: &LopdfDoc, value: Option<&Object>) -> Option<[f32; 4]> {
    let v = numbers(doc, value)?;
    (v.len() == 4).then(|| {
        [
            v[0].min(v[2]),
            v[1].min(v[3]),
            v[0].max(v[2]),
            v[1].max(v[3]),
        ]
    })
}

fn dictionary_at<'o>(doc: &'o LopdfDoc, value: Option<&'o Object>) -> Option<&'o Dictionary> {
    doc.dereference(value?)
        .ok()
        .and_then(|(_, v)| v.as_dict().ok())
}

fn name_at<'o>(doc: &'o LopdfDoc, value: Option<&'o Object>) -> Option<&'o [u8]> {
    doc.dereference(value?)
        .ok()
        .and_then(|(_, v)| v.as_name().ok())
}

/// Alan özniteliği: widget'ta yoksa üst alan zincirinden (`/Parent`) miras.
fn field_attribute<'o>(
    doc: &'o LopdfDoc,
    widget: &'o Dictionary,
    key: &[u8],
) -> Option<&'o Object> {
    let mut current = widget;
    for _ in 0..32 {
        if let Ok(value) = current.get(key) {
            return doc.dereference(value).ok().map(|(_, v)| v);
        }
        current = dictionary_at(doc, current.get(b"Parent").ok())?;
    }
    None
}

/// Belgenin varsayılan katman yapılandırmasında KAPALI gruplar.
struct OptionalContent {
    base_off: bool,
    on: HashSet<ObjectId>,
    off: HashSet<ObjectId>,
}

impl OptionalContent {
    fn read(doc: &LopdfDoc) -> Self {
        let config = doc
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|r| dictionary_at(doc, Some(r)))
            .and_then(|c| dictionary_at(doc, c.get(b"OCProperties").ok()))
            .and_then(|p| dictionary_at(doc, p.get(b"D").ok()));
        let ids = |key: &[u8]| -> HashSet<ObjectId> {
            config
                .and_then(|c| c.get(key).ok())
                .and_then(|a| doc.dereference(a).ok())
                .and_then(|(_, a)| a.as_array().ok())
                .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
                .unwrap_or_default()
        };
        Self {
            base_off: config
                .and_then(|c| name_at(doc, c.get(b"BaseState").ok()))
                .is_some_and(|s| s == b"OFF"),
            on: ids(b"ON"),
            off: ids(b"OFF"),
        }
    }

    fn group_on(&self, id: ObjectId) -> bool {
        if self.base_off {
            self.on.contains(&id)
        } else {
            !self.off.contains(&id)
        }
    }

    /// Açıklamanın `/OC` değeri varsayılan yapılandırmada görünür mü?
    /// Grup (OCG) ya da üyelik sözlüğü (OCMD, `/P` kuralıyla). Çözülemeyen
    /// değer görünür sayılır: görünümü sessizce gizlemek daha kötü bir hatadır.
    fn visible(&self, doc: &LopdfDoc, value: &Object) -> bool {
        let Ok((id, Object::Dictionary(oc))) = doc.dereference(value) else {
            return true;
        };
        let membership =
            name_at(doc, oc.get(b"Type").ok()) == Some(b"OCMD".as_slice()) || oc.has(b"OCGs");
        if !membership {
            return id.is_none_or(|id| self.group_on(id));
        }
        let groups: Vec<ObjectId> = match oc.get(b"OCGs") {
            Ok(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
                _ => vec![*r],
            },
            Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
            _ => Vec::new(),
        };
        if groups.is_empty() {
            return true;
        }
        let states: Vec<bool> = groups.iter().map(|g| self.group_on(*g)).collect();
        match name_at(doc, oc.get(b"P").ok()) {
            Some(b"AllOn") => states.iter().all(|s| *s),
            Some(b"AnyOff") => states.iter().any(|s| !*s),
            Some(b"AllOff") => states.iter().all(|s| !*s),
            _ => states.iter().any(|s| *s),
        }
    }
}

/// Kayıtlı normal görünüm.
enum Stored {
    /// `/AP` ya da `/N` yok, ya da eksik/geçersiz bir nesneye gidiyor.
    Absent,
    /// Alt durum sözlüğü var ama `/AS` onlardan birini seçmiyor: görüntüleyici
    /// hiçbir şey çizmez (ör. işaretlenmemiş onay kutusunun `/Off`'u yoksa).
    NoState,
    Form(ObjectId),
}

fn stored_appearance(doc: &LopdfDoc, annot: &Dictionary) -> Stored {
    let Some(normal) = dictionary_at(doc, annot.get(b"AP").ok()).and_then(|ap| ap.get(b"N").ok())
    else {
        return Stored::Absent;
    };
    let form = |value: &Object| -> Option<ObjectId> {
        match doc.dereference(value).ok()? {
            (Some(id), Object::Stream(_)) => Some(id),
            _ => None,
        }
    };
    match doc.dereference(normal) {
        Ok((Some(id), Object::Stream(_))) => Stored::Form(id),
        Ok((_, Object::Dictionary(states))) => {
            let Some(state) = name_at(doc, annot.get(b"AS").ok()) else {
                return Stored::NoState;
            };
            match states.get(state).ok().and_then(form) {
                Some(id) => Stored::Form(id),
                None => Stored::NoState,
            }
        }
        _ => Stored::Absent,
    }
}

/// ISO 32000-1 §12.5.5: BBox, `/Matrix` ile dönüştürülür; sınır kutusu `/Rect`'e
/// eşleyen A matrisi döner (form kendi `/Matrix`'ini `Do` sırasında uygular).
pub fn appearance_placement(rect: [f32; 4], bbox: [f32; 4], matrix: [f32; 6]) -> Option<[f32; 6]> {
    let [a, b, c, d, e, f] = matrix;
    let corners = [
        (bbox[0], bbox[1]),
        (bbox[2], bbox[1]),
        (bbox[0], bbox[3]),
        (bbox[2], bbox[3]),
    ]
    .map(|(x, y)| (a * x + c * y + e, b * x + d * y + f));
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    // Dejenere eksen (ör. yüksekliği 0 olan çizgi kutusu) ölçeklenmez, yalnız
    // yerine taşınır.
    let sx = if x1 - x0 > f32::EPSILON {
        (rect[2] - rect[0]) / (x1 - x0)
    } else {
        1.0
    };
    let sy = if y1 - y0 > f32::EPSILON {
        (rect[3] - rect[1]) / (y1 - y0)
    } else {
        1.0
    };
    let placed = [sx, 0., 0., sy, rect[0] - sx * x0, rect[1] - sy * y0];
    placed.iter().all(|v| v.is_finite()).then_some(placed)
}

/// Kayıtlı bir form akışını yerleştirir; BBox geçersizse `None`.
fn stored_draw(doc: &LopdfDoc, id: ObjectId, rect: [f32; 4]) -> Option<Draw> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let bbox = rectangle(doc, stream.dict.get(b"BBox").ok())?;
    let matrix = match numbers(doc, stream.dict.get(b"Matrix").ok()) {
        Some(m) if m.len() == 6 => [m[0], m[1], m[2], m[3], m[4], m[5]],
        _ => [1., 0., 0., 1., 0., 0.],
    };
    Some(Draw {
        source: Source::Stored(id),
        matrix: appearance_placement(rect, bbox, matrix)?,
    })
}

fn plan(doc: &LopdfDoc, layers: &OptionalContent, item: &Object) -> Plan {
    let Ok((_, Object::Dictionary(annot))) = doc.dereference(item) else {
        return Plan::Skip;
    };
    let flags = annot
        .get(b"F")
        .ok()
        .and_then(|f| doc.dereference(f).ok())
        .and_then(|(_, f)| f.as_i64().ok())
        .unwrap_or(0);
    let subtype = name_at(doc, annot.get(b"Subtype").ok()).unwrap_or(b"");
    if flags & (HIDDEN | NO_VIEW) != 0
        || (flags & INVISIBLE != 0 && !super::is_annotation_subtype(subtype))
        // Popup'ın sayfada kendi çizimi yoktur; açıldığında görüntüleyicinin
        // penceresidir (§12.5.6.14).
        || subtype == b"Popup"
    {
        return Plan::Skip;
    }
    // Yerleştirilemeyen ya da alanı sıfır olan açıklama (görünmez imza
    // alanı: `/Rect [0 0 0 0]`) ekranda hiçbir şey çizmez.
    let Some(rect) = rectangle(doc, annot.get(b"Rect").ok()) else {
        return Plan::Skip;
    };
    if rect[2] - rect[0] <= 0. || rect[3] - rect[1] <= 0. {
        return Plan::Skip;
    }
    if let Ok(oc) = annot.get(b"OC") {
        if !layers.visible(doc, oc) {
            return Plan::Skip;
        }
    }
    match stored_appearance(doc, annot) {
        Stored::NoState => return Plan::Skip,
        Stored::Form(id) => {
            if let Some(draw) = stored_draw(doc, id, rect) {
                return Plan::Draw(draw);
            }
        }
        Stored::Absent => {}
    }
    // Kayıtlı görünüm yok (ya da kullanılamıyor).
    match subtype {
        b"Highlight" | b"Underline" | b"StrikeOut" => match markup(doc, annot, subtype, rect) {
            Some(Some(draw)) => Plan::Draw(draw),
            Some(None) => Plan::Skip,
            None => Plan::Missing,
        },
        // Görünümsüz bağlantı: Word/LaTeX içindekiler bağlantısı. macOS bugün
        // de çizmiyor; çerçeve görsele taşınmaz.
        b"Link" => Plan::Skip,
        b"Widget" => widget_without_appearance(doc, annot),
        b"Text" | b"FreeText" | b"Line" | b"Square" | b"Circle" | b"Polygon" | b"PolyLine"
        | b"Squiggly" | b"Stamp" | b"Caret" | b"Ink" | b"FileAttachment" | b"Redact" => {
            Plan::Missing
        }
        // Ses, film, ekran, baskı işaretleri, filigran, 3B ve tanınmayan türler:
        // görünüm akışı olmadan görüntüleyici sayfaya bir şey çizmez.
        _ => Plan::Skip,
    }
}

/// `/AP` taşımayan form alanı: gösterilecek bir değeri yoksa (boş alan,
/// imzalanmamış ya da görünmez imza, basmalı düğme) görüntüleyici de bir şey
/// çizmez. Değeri olan alanın görünümü kurulamaz.
fn widget_without_appearance(doc: &LopdfDoc, widget: &Dictionary) -> Plan {
    let kind = field_attribute(doc, widget, b"FT").and_then(|t| t.as_name().ok());
    if kind == Some(b"Sig".as_slice()) {
        return Plan::Skip;
    }
    let field_flags = field_attribute(doc, widget, b"Ff")
        .and_then(|f| f.as_i64().ok())
        .unwrap_or(0);
    if kind == Some(b"Btn".as_slice()) && field_flags & PUSHBUTTON != 0 {
        return Plan::Skip;
    }
    let has_value = match field_attribute(doc, widget, b"V") {
        None | Some(Object::Null) => false,
        Some(Object::String(s, _)) => !s.is_empty(),
        Some(Object::Name(n)) => n.as_slice() != b"Off" && !n.is_empty(),
        Some(Object::Array(a)) => !a.is_empty(),
        Some(_) => true,
    };
    if has_value {
        Plan::Missing
    } else {
        Plan::Skip
    }
}

/// Dörtgenin taban ve üst kenarı: (taban başı, taban sonu, üst başı, üst sonu).
///
/// Acrobat, PDFKit ve pdf.js `/QuadPoints`'i sol üst, sağ üst, sol alt, sağ
/// alt sırasıyla yazar; spec metni saat yönünün tersini tarif eder. Acrobat
/// sırasıyla kurulan çokgen kendini kesiyorsa sıra spec'inkidir.
type Point = (f32, f32);
fn quad_edges(q: [Point; 4]) -> (Point, Point, Point, Point) {
    let cross =
        |o: Point, a: Point, b: Point| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let ring = [q[0], q[1], q[3], q[2]];
    let turns: Vec<f32> = (0..4)
        .map(|i| cross(ring[i], ring[(i + 1) % 4], ring[(i + 2) % 4]))
        .collect();
    let convex = turns.iter().all(|t| *t >= 0.) || turns.iter().all(|t| *t <= 0.);
    if convex {
        (q[2], q[3], q[0], q[1])
    } else {
        (q[0], q[1], q[3], q[2])
    }
}

/// Görünümsüz Highlight / Underline / StrikeOut için görünüm kurar.
///
/// `None`: kurulamıyor. `Some(None)`: rengi saydam, görüntüleyici de çizmez.
fn markup(
    doc: &LopdfDoc,
    annot: &Dictionary,
    subtype: &[u8],
    rect: [f32; 4],
) -> Option<Option<Draw>> {
    let color = match numbers(doc, annot.get(b"C").ok()) {
        Some(c) if c.is_empty() => return Some(None),
        Some(c) if matches!(c.len(), 1 | 3 | 4) => c,
        Some(_) => return None,
        // Renk verilmemiş: görüntüleyicilerin varsayılanı (vurgu sarı, çizgi siyah).
        None if subtype == b"Highlight" => vec![1., 1., 0.],
        None => vec![0.],
    };
    let alpha = annot
        .get(b"CA")
        .ok()
        .and_then(|a| number(doc, a))
        .unwrap_or(1.)
        .clamp(0., 1.);
    if alpha == 0. {
        return Some(None);
    }
    let quads: Vec<[Point; 4]> = match numbers(doc, annot.get(b"QuadPoints").ok()) {
        Some(v) if !v.is_empty() && v.len() % 8 == 0 => v
            .chunks(8)
            .map(|c| [(c[0], c[1]), (c[2], c[3]), (c[4], c[5]), (c[6], c[7])])
            .collect(),
        // Dörtgen yoksa ya da bozuksa açıklamanın kutusu tek dörtgendir.
        _ => vec![[
            (rect[0], rect[3]),
            (rect[2], rect[3]),
            (rect[0], rect[1]),
            (rect[2], rect[1]),
        ]],
    };
    let operator = |fill: bool| match color.len() {
        1 => {
            if fill {
                "g"
            } else {
                "G"
            }
        }
        3 => {
            if fill {
                "rg"
            } else {
                "RG"
            }
        }
        _ => {
            if fill {
                "k"
            } else {
                "K"
            }
        }
    };
    let components = color
        .iter()
        .map(|c| c.clamp(0., 1.).to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let mut ops = String::from("/GDAnnotGS gs\n");
    let mut bounds = rect;
    let mut grow = |p: Point| {
        bounds = [
            bounds[0].min(p.0),
            bounds[1].min(p.1),
            bounds[2].max(p.0),
            bounds[3].max(p.1),
        ];
    };
    let highlight = subtype == b"Highlight";
    if highlight {
        ops.push_str(&format!("{components} {}\n", operator(true)));
    } else {
        ops.push_str(&format!("{components} {}\n", operator(false)));
    }
    for q in quads {
        let (b0, b1, t0, t1) = quad_edges(q);
        for p in [b0, b1, t0, t1] {
            grow(p);
        }
        if highlight {
            ops.push_str(&format!(
                "{} {} m {} {} l {} {} l {} {} l h f\n",
                b0.0, b0.1, b1.0, b1.1, t1.0, t1.1, t0.0, t0.1
            ));
            continue;
        }
        let mid = |a: Point, b: Point| ((a.0 + b.0) / 2., (a.1 + b.1) / 2.);
        let (bm, tm) = (mid(b0, b1), mid(t0, t1));
        let height = ((tm.0 - bm.0).powi(2) + (tm.1 - bm.1).powi(2)).sqrt();
        if height <= 0. {
            continue;
        }
        let width = (height / 14.).max(0.5);
        let (from, to) = if subtype == b"StrikeOut" {
            (mid(b0, t0), mid(b1, t1))
        } else {
            // Alt çizgi tabana oturur: çizginin alt kenarı dörtgenin tabanıdır.
            let k = width / 2. / height;
            (
                (b0.0 + (t0.0 - b0.0) * k, b0.1 + (t0.1 - b0.1) * k),
                (b1.0 + (t1.0 - b1.0) * k, b1.1 + (t1.1 - b1.1) * k),
            )
        };
        ops.push_str(&format!(
            "{width} w {} {} m {} {} l S\n",
            from.0, from.1, to.0, to.1
        ));
    }
    if !ops.is_ascii()
        || [bounds[0], bounds[1], bounds[2], bounds[3]]
            .iter()
            .any(|v| !v.is_finite())
    {
        return None;
    }
    let mut state = dictionary! {"Type" => "ExtGState", "ca" => alpha, "CA" => alpha};
    if highlight {
        // Vurgu metni örtmez, onunla çarpılır (Acrobat ve PDFKit'in yazdığı gibi).
        state.set("BM", "Multiply");
    }
    let bbox: Vec<Object> = bounds.iter().map(|v| Object::Real(*v)).collect();
    let form = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => bbox,
            "Resources" => dictionary! {"ExtGState" => dictionary! {"GDAnnotGS" => state}},
        },
        ops.into_bytes(),
    );
    // Görünüm sayfa koordinatlarında kuruldu: yerleştirme birim matristir.
    Some(Some(Draw {
        source: Source::Built(form),
        matrix: [1., 0., 0., 1., 0., 0.],
    }))
}

/// Sayfanın içerik akışları, sırayla. `/Contents` dolaylı bir diziye çözülebilir.
fn content_items(doc: &LopdfDoc, page: &Dictionary) -> Vec<Object> {
    match page.get(b"Contents") {
        Ok(Object::Array(items)) => items.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(items)) => items.clone(),
            _ => vec![Object::Reference(*id)],
        },
        _ => Vec::new(),
    }
}

/// Açıklama katmanını sayfaya ekler. Sayfa yerleştirilemezse `None`.
fn draw_layer(doc: &mut LopdfDoc, page_id: ObjectId, draws: Vec<Draw>) -> Option<()> {
    let resolved = resolved_page_dictionary(doc, page_id).ok()?;
    let media = rectangle(doc, resolved.get(b"MediaBox").ok())?;
    let visible = rectangle(doc, resolved.get(b"CropBox").ok())
        .map(|c| {
            [
                c[0].max(media[0]),
                c[1].max(media[1]),
                c[2].min(media[2]),
                c[3].min(media[3]),
            ]
        })
        .filter(|v| v[2] > v[0] && v[3] > v[1])
        .unwrap_or(media);
    // Kaynaklar sayfaya KOPYALANIR: miras alınan ya da başka sayfalarla
    // paylaşılan sözlük yerinde değişmez, var olan girdiler korunur.
    let mut resources = dictionary_at(doc, resolved.get(b"Resources").ok())
        .cloned()
        .unwrap_or_default();
    let mut xobjects = dictionary_at(doc, resources.get(b"XObject").ok())
        .cloned()
        .unwrap_or_default();
    let contents = content_items(doc, doc.get_dictionary(page_id).ok()?);
    let mut layer = format!(
        "q {} {} {} {} re W n\n",
        visible[0],
        visible[1],
        visible[2] - visible[0],
        visible[3] - visible[1]
    );
    let mut names: HashMap<ObjectId, Vec<u8>> = HashMap::new();
    let mut counter = 0usize;
    for draw in draws {
        let id = match draw.source {
            Source::Built(form) => doc.add_object(form),
            Source::Stored(id) => as_form(doc, id),
        };
        let name = names.entry(id).or_insert_with(|| loop {
            counter += 1;
            let candidate = format!("GDAnnot{counter}").into_bytes();
            if !xobjects.has(&candidate) {
                xobjects.set(candidate.clone(), Object::Reference(id));
                break candidate;
            }
        });
        let m = draw.matrix;
        layer.push_str(&format!(
            "q {} {} {} {} {} {} cm /{} Do Q\n",
            m[0],
            m[1],
            m[2],
            m[3],
            m[4],
            m[5],
            String::from_utf8_lossy(name)
        ));
    }
    layer.push_str("Q\n");
    resources.set("XObject", xobjects);
    let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let close = doc.add_object(Stream::new(
        Dictionary::new(),
        format!("\nQ\n{layer}").into_bytes(),
    ));
    let mut wrapped = vec![Object::Reference(open)];
    wrapped.extend(contents);
    wrapped.push(Object::Reference(close));
    let page = doc.get_dictionary_mut(page_id).ok()?;
    page.set("Contents", wrapped);
    page.set("Resources", resources);
    Some(())
}

/// Görünüm akışı `/Subtype /Form` taşımıyorsa (bazı üreticiler yazmaz) bir
/// KOPYASI form olarak işaretlenir; belgede paylaşılan akış değişmez.
fn as_form(doc: &mut LopdfDoc, id: ObjectId) -> ObjectId {
    let Ok(stream) = doc.get_object(id).and_then(|o| o.as_stream()) else {
        return id;
    };
    if stream.dict.get(b"Subtype").and_then(|s| s.as_name()).ok() == Some(b"Form".as_slice()) {
        return id;
    }
    let mut copy = stream.clone();
    copy.dict.set("Type", "XObject");
    copy.dict.set("Subtype", "Form");
    doc.add_object(copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECT: [f32; 4] = [100., 600., 300., 650.];

    fn boxed(v: &[f32]) -> Object {
        Object::Array(v.iter().map(|n| Object::Real(*n)).collect())
    }

    fn form(doc: &mut LopdfDoc, bbox: [f32; 4], ops: &str) -> ObjectId {
        doc.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>boxed(&bbox)},
            ops.as_bytes().to_vec(),
        ))
    }

    /// Tek sayfalık belge; sayfanın ek sözlük girdileri `extra` ile.
    fn one_page(annots: impl FnOnce(&mut LopdfDoc) -> Vec<Object>) -> (LopdfDoc, ObjectId) {
        let mut doc = LopdfDoc::with_version("1.7");
        let pages = doc.new_object_id();
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"0 0 0 rg 0 0 10 10 re f".to_vec(),
        ));
        let annots = annots(&mut doc);
        let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,
        "MediaBox"=>boxed(&[0., 0., 595., 842.]),"Contents"=>content,
        "Resources"=>dictionary!{},"Annots"=>annots});
        doc.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages,
        "AcroForm"=>dictionary!{"Fields"=>Vec::<Object>::new(),"NeedAppearances"=>true}});
        doc.trailer.set("Root", root);
        (doc, page)
    }

    fn annot(doc: &mut LopdfDoc, subtype: &str, extra: Dictionary) -> Object {
        let mut d = dictionary! {"Type"=>"Annot","Subtype"=>subtype,"Rect"=>boxed(&RECT)};
        for (k, v) in extra {
            d.set(k, v);
        }
        doc.add_object(d).into()
    }

    fn layer_text(doc: &LopdfDoc, page: ObjectId) -> String {
        let items = content_items(doc, doc.get_dictionary(page).unwrap());
        let last = items.last().unwrap().as_reference().unwrap();
        String::from_utf8(
            doc.get_object(last)
                .unwrap()
                .as_stream()
                .unwrap()
                .content
                .clone(),
        )
        .unwrap()
    }

    fn close(a: [f32; 6], b: [f32; 6]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn placement_maps_the_transformed_bbox_onto_the_rect() {
        // Birim: BBox Rect'in boyutunda, yalnız öteleme.
        assert!(close(
            appearance_placement(RECT, [0., 0., 200., 50.], [1., 0., 0., 1., 0., 0.]).unwrap(),
            [1., 0., 0., 1., 100., 600.]
        ));
        // /Matrix 2× ölçek: BBox 100×25 → 200×50; A ölçeklemez.
        assert!(close(
            appearance_placement(RECT, [0., 0., 100., 25.], [2., 0., 0., 2., 0., 0.]).unwrap(),
            [1., 0., 0., 1., 100., 600.]
        ));
        // 90° dönük /Matrix: BBox 50×200 dönünce [-200,0]..[0,50] olur.
        assert!(close(
            appearance_placement(RECT, [0., 0., 50., 200.], [0., 1., -1., 0., 0., 0.]).unwrap(),
            [1., 0., 0., 1., 300., 600.]
        ));
        // BBox köşeleri ters yazılmış, ofsetli ve Rect'ten küçük: ölçek 2.
        assert!(close(
            appearance_placement(RECT, [10., 10., 110., 35.], [1., 0., 0., 1., 0., 0.]).unwrap(),
            [2., 0., 0., 2., 80., 580.]
        ));
    }

    #[test]
    fn an_annotation_free_document_is_left_untouched() {
        let mut doc = LopdfDoc::with_version("1.7");
        let pages = doc.new_object_id();
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"0 0 0 rg 0 0 10 10 re f".to_vec(),
        ));
        let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,
        "MediaBox"=>boxed(&[0., 0., 595., 842.]),"Contents"=>content});
        doc.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        doc.trailer.set("Root", root);
        let (objects, trailer) = (doc.objects.clone(), doc.trailer.clone());
        let report = flatten_annotations_for_render(&mut doc);
        assert_eq!(report, FlattenReport::default());
        assert!(doc.objects == objects, "açıklamasız belgeye dokunulmamalı");
        assert!(
            doc.trailer == trailer,
            "açıklamasız belgenin kataloğu değişmemeli"
        );
    }

    #[test]
    fn a_drawn_appearance_wraps_content_and_removes_annots_and_acroform() {
        let (mut doc, page) = one_page(|d| {
            let ap = form(d, [0., 0., 200., 50.], "1 1 0 rg 0 0 200 50 re f");
            vec![annot(d, "Square", dictionary! {"AP"=>dictionary!{"N"=>ap}})]
        });
        let original = content_items(&doc, doc.get_dictionary(page).unwrap());
        let report = flatten_annotations_for_render(&mut doc);
        assert_eq!(report.drawn, 1);
        assert_eq!(report.missing_appearance, 0);
        let p = doc.get_dictionary(page).unwrap();
        assert!(!p.has(b"Annots"), "/Annots kopyada kalmamalı");
        assert!(
            !doc.catalog().unwrap().has(b"AcroForm"),
            "/AcroForm kopyada kalmamalı"
        );
        let items = content_items(&doc, p);
        assert_eq!(items.len(), original.len() + 2);
        assert_eq!(&items[1..items.len() - 1], &original[..]);
        let first = items[0].as_reference().unwrap();
        assert_eq!(
            doc.get_object(first).unwrap().as_stream().unwrap().content,
            b"q\n"
        );
        let layer = layer_text(&doc, page);
        assert!(layer.starts_with("\nQ\nq 0 0 595 842 re W n\n"), "{layer}");
        assert!(
            layer.contains("q 1 0 0 1 100 600 cm /GDAnnot1 Do Q"),
            "{layer}"
        );
        assert!(layer.ends_with("Q\n"));
    }

    #[test]
    fn inherited_and_shared_resources_are_copied_never_mutated() {
        let mut doc = LopdfDoc::with_version("1.7");
        let pages = doc.new_object_id();
        let image = doc.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>1,"Height"=>1,
            "ColorSpace"=>"DeviceGray","BitsPerComponent"=>8},
            vec![0],
        ));
        let font =
            doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
        // Paylaşılan, dolaylı XObject sözlüğü — çakışan adla.
        let shared_x = doc.add_object(dictionary! {"GDAnnot1"=>image});
        let shared =
            doc.add_object(dictionary! {"XObject"=>shared_x,"Font"=>dictionary!{"F1"=>font}});
        let ap = form(&mut doc, [0., 0., 200., 50.], "0 0 1 rg 0 0 200 50 re f");
        let a = annot(&mut doc, "Stamp", dictionary! {"AP"=>dictionary!{"N"=>ap}});
        let content = doc.add_object(Stream::new(dictionary! {}, b"q /GDAnnot1 Do Q".to_vec()));
        let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,
        "Contents"=>content,"Annots"=>vec![a]});
        let sibling =
            doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"Contents"=>content});
        // Kaynaklar ve MediaBox sayfa ağacından miras.
        doc.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages","Kids"=>vec![page.into(), sibling.into()],"Count"=>2,
            "Resources"=>shared,"MediaBox"=>boxed(&[0., 0., 595., 842.])}
            .into(),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        doc.trailer.set("Root", root);
        let shared_before = doc.get_object(shared).unwrap().clone();
        let shared_x_before = doc.get_object(shared_x).unwrap().clone();

        assert_eq!(flatten_annotations_for_render(&mut doc).drawn, 1);
        assert_eq!(
            doc.get_object(shared).unwrap(),
            &shared_before,
            "paylaşılan /Resources değişti"
        );
        assert_eq!(
            doc.get_object(shared_x).unwrap(),
            &shared_x_before,
            "paylaşılan /XObject değişti"
        );
        assert!(
            !doc.get_dictionary(sibling).unwrap().has(b"Resources"),
            "komşu sayfa etkilendi"
        );

        let resources = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            resources
                .get(b"Font")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"F1")
                .unwrap(),
            &Object::Reference(font)
        );
        let xobjects = resources.get(b"XObject").unwrap().as_dict().unwrap();
        assert_eq!(
            xobjects.get(b"GDAnnot1").unwrap(),
            &Object::Reference(image),
            "var olan görsel kayboldu"
        );
        assert_eq!(
            xobjects.get(b"GDAnnot2").unwrap(),
            &Object::Reference(ap),
            "yeni ad benzersiz değil"
        );
        assert!(layer_text(&doc, page).contains("/GDAnnot2 Do"));
    }

    #[test]
    fn crop_box_bounds_the_annotation_layer() {
        let (mut doc, page) = one_page(|d| {
            let ap = form(d, [0., 0., 200., 50.], "1 0 0 rg 0 0 200 50 re f");
            vec![annot(d, "Square", dictionary! {"AP"=>dictionary!{"N"=>ap}})]
        });
        doc.get_dictionary_mut(page)
            .unwrap()
            .set("CropBox", boxed(&[50., 700., 700., 100.]));
        flatten_annotations_for_render(&mut doc);
        // CropBox ∩ MediaBox, köşeler normalize: [50 100 595 700].
        assert!(layer_text(&doc, page).contains("q 50 100 545 600 re W n"));
    }

    #[test]
    fn view_semantics_decide_what_is_drawn_skipped_or_missing() {
        let mut layer = None;
        let (mut doc, page) = one_page(|d| {
            let ap = |d: &mut LopdfDoc| form(d, [0., 0., 200., 50.], "0 0 0 rg 0 0 200 50 re f");
            let (a1, a2, a3) = (ap(d), ap(d), ap(d));
            let yes = ap(d);
            let hidden_layer =
                d.add_object(dictionary! {"Type"=>"OCG","Name"=>Object::string_literal("gizli")});
            layer = Some(hidden_layer);
            let parent = d.add_object(
                dictionary! {"FT"=>"Tx","T"=>Object::string_literal("ad"),"V"=>Object::string_literal("Ali")},
            );
            vec![
                Object::Null,
                annot(d, "Square", dictionary! {"F"=>2,"AP"=>dictionary!{"N"=>a1}}),
                annot(
                    d,
                    "Square",
                    dictionary! {"F"=>32,"AP"=>dictionary!{"N"=>a2}},
                ),
                annot(d, "Popup", dictionary! {"Open"=>true}),
                annot(
                    d,
                    "Link",
                    dictionary! {"Border"=>vec![0.into(),0.into(),1.into()]},
                ),
                annot(
                    d,
                    "Square",
                    dictionary! {"Rect"=>boxed(&[0., 0., 0., 0.]),"AP"=>dictionary!{"N"=>a3}},
                ),
                annot(d, "Square", dictionary! {"Rect"=>Object::Null}),
                annot(
                    d,
                    "Widget",
                    dictionary! {"FT"=>"Sig","V"=>Object::Reference((999, 0))},
                ),
                annot(d, "Widget", dictionary! {"FT"=>"Tx"}),
                annot(
                    d,
                    "Widget",
                    dictionary! {"FT"=>"Btn","AS"=>"Off","AP"=>dictionary!{"N"=>dictionary!{"Yes"=>yes}}},
                ),
                annot(
                    d,
                    "Square",
                    dictionary! {"OC"=>hidden_layer,"AP"=>dictionary!{"N"=>Object::Reference((998, 0))}},
                ),
                // Eksik nesneye giden /AP: görünümsüz FreeText sayılır → eksik.
                annot(
                    d,
                    "FreeText",
                    dictionary! {"AP"=>dictionary!{"N"=>Object::Reference((997, 0))}},
                ),
                annot(d, "Ink", dictionary! {}),
                annot(d, "Widget", dictionary! {"Parent"=>parent}),
            ]
        });
        // Katman varsayılan yapılandırmada kapalı.
        let layer = layer.unwrap();
        doc.catalog_mut().unwrap().set(
            "OCProperties",
            dictionary! {"OCGs"=>vec![layer.into()],"D"=>dictionary!{"OFF"=>vec![layer.into()]}},
        );
        let report = flatten_annotations_for_render(&mut doc);
        assert_eq!(
            report,
            FlattenReport {
                drawn: 0,
                skipped: 11,
                missing_appearance: 3
            }
        );
        assert!(!doc.get_dictionary(page).unwrap().has(b"Annots"));
    }

    #[test]
    fn widget_sub_state_follows_as_and_need_appearances_uses_the_stored_ap() {
        for (state, drawn) in [("Yes", 1usize), ("Off", 1), ("Belirsiz", 0)] {
            let (mut doc, page) = one_page(|d| {
                let yes = form(d, [0., 0., 20., 20.], "0 0 0 rg 2 2 16 16 re f");
                let off = form(d, [0., 0., 20., 20.], "");
                vec![annot(
                    d,
                    "Widget",
                    dictionary! {"FT"=>"Btn","V"=>"Yes","AS"=>state,
                    "AP"=>dictionary!{"N"=>dictionary!{"Yes"=>yes,"Off"=>off}}},
                )]
            });
            let yes_id = match doc
                .get_dictionary(page)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap()[0]
            {
                Object::Reference(id) => {
                    let ap = doc
                        .get_dictionary(id)
                        .unwrap()
                        .get(b"AP")
                        .unwrap()
                        .as_dict()
                        .unwrap();
                    ap.get(b"N")
                        .unwrap()
                        .as_dict()
                        .unwrap()
                        .get(b"Yes")
                        .unwrap()
                        .as_reference()
                        .unwrap()
                }
                _ => unreachable!(),
            };
            let report = flatten_annotations_for_render(&mut doc);
            assert_eq!(report.drawn, drawn, "/AS {state}");
            assert_eq!(report.missing_appearance, 0, "/AS {state}");
            if state == "Yes" {
                let resources = doc
                    .get_dictionary(page)
                    .unwrap()
                    .get(b"Resources")
                    .unwrap()
                    .as_dict()
                    .unwrap();
                let x = resources.get(b"XObject").unwrap().as_dict().unwrap();
                assert_eq!(x.get(b"GDAnnot1").unwrap(), &Object::Reference(yes_id));
            }
        }
    }

    #[test]
    fn a_highlight_without_appearance_is_built_with_multiply() {
        let (mut doc, page) = one_page(|d| {
            vec![annot(
                d,
                "Highlight",
                dictionary! {"C"=>vec![1.into(),1.into(),0.into()],
                // Acrobat sırası: sol üst, sağ üst, sol alt, sağ alt.
                "QuadPoints"=>boxed(&[100., 650., 300., 650., 100., 600., 300., 600.])},
            )]
        });
        let report = flatten_annotations_for_render(&mut doc);
        assert_eq!(report.drawn, 1);
        let resources = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap();
        let id = resources
            .get(b"XObject")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"GDAnnot1")
            .unwrap()
            .as_reference()
            .unwrap();
        let built = doc.get_object(id).unwrap().as_stream().unwrap();
        let state = built
            .dict
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"ExtGState")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"GDAnnotGS")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(state.get(b"BM").unwrap().as_name().unwrap(), b"Multiply");
        let ops = String::from_utf8(built.content.clone()).unwrap();
        assert!(ops.contains("1 1 0 rg"), "{ops}");
        // Taban → üst: kendini kesmeyen dörtgen.
        assert!(
            ops.contains("100 600 m 300 600 l 300 650 l 100 650 l h f"),
            "{ops}"
        );
        assert!(layer_text(&doc, page).contains("q 1 0 0 1 0 0 cm /GDAnnot1 Do Q"));
    }

    #[test]
    fn quad_order_is_recognised_in_both_conventions() {
        let acrobat = [(0., 10.), (20., 10.), (0., 0.), (20., 0.)];
        assert_eq!(
            quad_edges(acrobat),
            ((0., 0.), (20., 0.), (0., 10.), (20., 10.))
        );
        // Spec metninin saat yönü tersi sırası: sol alt, sağ alt, sağ üst, sol üst.
        let spec = [(0., 0.), (20., 0.), (20., 10.), (0., 10.)];
        assert_eq!(
            quad_edges(spec),
            ((0., 0.), (20., 0.), (0., 10.), (20., 10.))
        );
    }

    #[test]
    fn underline_and_strikeout_are_drawn_at_the_baseline_and_the_middle() {
        for (subtype, y) in [("Underline", "601.78"), ("StrikeOut", "625 m")] {
            let (mut doc, page) = one_page(|d| {
                vec![annot(
                    d,
                    subtype,
                    dictionary! {"C"=>vec![1.into(),0.into(),0.into()],
                    "QuadPoints"=>boxed(&[100., 650., 300., 650., 100., 600., 300., 600.])},
                )]
            });
            assert_eq!(flatten_annotations_for_render(&mut doc).drawn, 1);
            let resources = doc
                .get_dictionary(page)
                .unwrap()
                .get(b"Resources")
                .unwrap()
                .as_dict()
                .unwrap();
            let id = resources
                .get(b"XObject")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"GDAnnot1")
                .unwrap()
                .as_reference()
                .unwrap();
            let ops = String::from_utf8(
                doc.get_object(id)
                    .unwrap()
                    .as_stream()
                    .unwrap()
                    .content
                    .clone(),
            )
            .unwrap();
            assert!(ops.contains("1 0 0 RG"), "{subtype}: {ops}");
            assert!(ops.contains(&format!("100 {y}")), "{subtype}: {ops}");
            assert!(ops.contains(" l S"), "{subtype}: {ops}");
        }
    }

    #[test]
    fn an_appearance_stream_without_form_subtype_is_copied_not_mutated() {
        let (mut doc, page) = one_page(|d| {
            let ap = d.add_object(Stream::new(
                dictionary! {"BBox"=>boxed(&[0., 0., 200., 50.])},
                b"0 0 1 rg 0 0 200 50 re f".to_vec(),
            ));
            vec![annot(d, "Square", dictionary! {"AP"=>dictionary!{"N"=>ap}})]
        });
        let original: Vec<(ObjectId, Object)> =
            doc.objects.iter().map(|(k, v)| (*k, v.clone())).collect();
        assert_eq!(flatten_annotations_for_render(&mut doc).drawn, 1);
        for (id, obj) in original {
            if let Object::Stream(s) = &obj {
                if s.dict.has(b"BBox") {
                    assert_eq!(
                        doc.get_object(id).unwrap(),
                        &obj,
                        "kayıtlı görünüm akışı değişti"
                    );
                }
            }
        }
        let resources = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap();
        let id = resources
            .get(b"XObject")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"GDAnnot1")
            .unwrap()
            .as_reference()
            .unwrap();
        let copy = doc.get_object(id).unwrap().as_stream().unwrap();
        assert_eq!(
            copy.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Form"
        );
    }
}
