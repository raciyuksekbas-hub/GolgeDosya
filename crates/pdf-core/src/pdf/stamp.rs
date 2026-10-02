use crate::error::{EklerError, Result};
use serde::{Deserialize, Serialize};

/// Damganın sayfadaki köşesi.
///
/// `ekler-core::model`'den buraya taşındı: bir damga konumu PDF alanının
/// kavramıdır. Taşımadan önce `pdf` modülü yalnız bu iki tip için `model`'e
/// bağlıydı ve crate sınırı çizilince bu bağ döngü doğuruyordu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StampPosition {
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StampConfig {
    pub enabled: bool,
    pub position: StampPosition,
    pub font_size: f32,
    pub margin_pt: f32,
    pub show_badge: bool,
}

impl Default for StampConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: StampPosition::TopRight,
            font_size: 10.0,
            margin_pt: 20.0,
            show_badge: true,
        }
    }
}
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document as LopdfDoc, Object, Stream};

/// Helvetica-Bold (base-14) glif genişlikleri, em'in binde biri, WinAnsi
/// 32..=126. Damga metni bu yazı tipiyle ve bu kodlamayla yazılır.
const HELVETICA_BOLD_WIDTHS: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278,
    278, // ' '..'/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // '0'..'9'
    333, 333, 584, 584, 584, 611, 975, // ':'..'@'
    722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667,
    611, 722, 667, 944, 667, 667, 611, // 'A'..'Z'
    333, 278, 333, 584, 556, 333, // '['..'`'
    556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, 611, 611, 389, 556,
    333, 611, 556, 778, 556, 556, 500, // 'a'..'z'
    389, 280, 389, 584, // '{'..'~'
];

/// Metnin Helvetica-Bold ile basılı genişliği (pt). ASCII dışındaki bir harf
/// ortalama bir harf genişliğiyle sayılır.
fn helvetica_bold_width(text: &str, font_size: f32) -> f32 {
    let units: u32 = text
        .chars()
        .map(|c| {
            let code = c as u32;
            if (32..=126).contains(&code) {
                u32::from(HELVETICA_BOLD_WIDTHS[(code - 32) as usize])
            } else {
                556
            }
        })
        .sum();
    units as f32 * font_size / 1000.0
}

pub fn apply_stamp_to_document(
    doc: &mut LopdfDoc,
    exhibit_order: usize,
    start_page_num: usize,
    config: &StampConfig,
) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let pages = doc.get_pages();
    let total_pages = pages.len();

    // Damga için ortak Helvetica-Bold font nesnesi ekle
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
        "Encoding" => "WinAnsiEncoding",
    });

    for i in 1..=(total_pages as u32) {
        if let Some(&page_id) = pages.get(&i) {
            let page_num = start_page_num + (i as usize - 1);
            let stamp_text = format!("Ek-{} / {}. Sayfa", exhibit_order, page_num);
            apply_stamp_to_page(doc, page_id, &stamp_text, font_id, config)?;
        }
    }

    Ok(())
}

struct BrandResources {
    form: lopdf::ObjectId,
    streams: std::collections::HashMap<Vec<u8>, lopdf::ObjectId>,
    raster_dpi: Option<u32>,
}
/// Marka akışını ekler; aynı baytlı akış (sarmal, işaret) belgede bir kez
/// tutulur ve sayfalar onu paylaşır.
fn add_brand_stream(
    doc: &mut LopdfDoc,
    content: Vec<u8>,
    brand: &mut BrandResources,
) -> lopdf::ObjectId {
    if let Some(id) = brand.streams.get(&content) {
        return *id;
    }
    let id = doc.add_object(Stream::new(Dictionary::new(), content.clone()));
    brand.streams.insert(content, id);
    id
}

/// /Rotate, damganın ve işaretin yerleşimi için: 0, 90, 180 ya da 270.
///
/// Spec /Rotate'in 90'ın katı olmasını şart koşar. Uymayan üretici değerinde
/// pdf.js sayfayı döndürülmemiş sayar; belgeyi reddetmek yerine aynısı
/// yapılır. Sayfanın kendi /Rotate değerine dokunulmaz.
fn placement_rotation(doc: &LopdfDoc, resolved: &Dictionary) -> i64 {
    let rotation = resolved
        .get(b"Rotate")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    if rotation % 90 == 0 {
        rotation
    } else {
        0
    }
}

/// Görsel (döndürülmüş) uzaydan sayfa kullanıcı uzayına dönüşüm: görsel sol
/// alt köşe orijin, x sağa, y yukarı. `bx` kutu `[x0 y0 x1 y1]`.
fn display_matrix(rotation: i64, bx: [f32; 4]) -> Result<[f32; 6]> {
    let [x0, y0, x1, y1] = bx;
    Ok(match rotation {
        0 => [1., 0., 0., 1., x0, y0],
        90 => [0., 1., -1., 0., x1, y0],
        180 => [-1., 0., 0., -1., x1, y1],
        270 => [0., -1., 1., 0., x0, y1],
        _ => return Err(EklerError::InvalidPdf("Geçersiz sayfa dönüşü".into())),
    })
}

/// `display_matrix`'in tersi: kullanıcı uzayındaki nokta, `bx` kutusunun
/// görüntülenen hâlinde nerede (sol alt köşe orijin, x sağa, y yukarı).
fn user_to_display(rotation: i64, bx: [f32; 4], (x, y): (f32, f32)) -> (f32, f32) {
    let [x0, y0, x1, y1] = bx;
    match rotation {
        90 => (y - y0, x1 - x),
        180 => (x1 - x, y1 - y),
        270 => (y1 - y, x - x0),
        _ => (x - x0, y - y0),
    }
}

/// `bx` kutusunun görüntülenen genişliği ve yüksekliği.
fn display_size(rotation: i64, bx: [f32; 4]) -> (f32, f32) {
    let (w, h) = (bx[2] - bx[0], bx[3] - bx[1]);
    if rotation == 90 || rotation == 270 {
        (h, w)
    } else {
        (w, h)
    }
}

/// Kullanıcı uzayındaki nokta, görüntülenen görünür kutunun sağ alt
/// çeyreğinde mi? Sığdırmak için küçültülen işaret kısa kenarda tam ortaya
/// düşer; yarım puntoluk pay bunun içindir.
fn in_bottom_right_quarter(rotation: i64, visible: [f32; 4], point: (f32, f32)) -> bool {
    const TOLERANCE: f32 = 0.5;
    let (w, h) = display_size(rotation, visible);
    let (x, y) = user_to_display(rotation, visible, point);
    x >= w / 2. - TOLERANCE && y <= h / 2. + TOLERANCE
}

/// İki kutunun kesişimi; boşsa `None`.
fn intersect(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let both = [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ];
    (both[0] < both[2] && both[1] < both[3]).then_some(both)
}

/// Kullanıcı işaretlerinin damga fontu bu adla (gerekirse sonuna `_`
/// eklenerek) bağlanır. Marka yerleşimi damga akışını bu adla tanır.
const STAMP_FONT: &str = "GolgeDosyaStamp";

/// Ek damgası, filigran ve sayfa numarası: kullanıcının İSTEDİĞİ işaretler.
/// Bu yol sayfaya pay ekler (işaret kaynak piksele binmez). GölgeDosya
/// markası bu yoldan GEÇMEZ; o, geometriye dokunmayan
/// `place_brand_overlay` ile basılır.
fn apply_stamp_to_page(
    doc: &mut LopdfDoc,
    page_id: lopdf::ObjectId,
    stamp_text: &str,
    font_id: lopdf::ObjectId,
    config: &StampConfig,
) -> Result<bool> {
    // Dönüş: `true` işaret eklendi, `false` sayfa (çok küçük olduğu için)
    // güvenle atlandı. Gerçek hatalar `Err`.
    let resolved = super::resolved_page_dictionary(doc, page_id)?;
    let bounds = resolved
        .get(b"CropBox")
        .or_else(|_| resolved.get(b"MediaBox"))
        .map_err(pdf_error)?;
    let (_, bounds) = doc.dereference(bounds).map_err(pdf_error)?;
    let bounds = bounds.as_array().map_err(pdf_error)?;
    if bounds.len() != 4 {
        return Err(EklerError::InvalidPdf("Geçersiz sayfa kutusu".into()));
    }
    let number = |i: usize| bounds[i].as_float().map_err(pdf_error);
    // Kutu köşe sırası serbesttir (ISO 32000-1 §7.9.5); üreticiler ters köşeli
    // kutu ([urx ury llx lly]) yazabilir. Görüntüleyiciler kutuyu normalize
    // eder. Ters köşe damganın genişliğini negatif yapıp "sığmıyor" hatasıyla
    // sağlam bir belgede bütün araçları düşürüyordu; kutu burada normalize
    // edilir (aynı dikdörtgen, kanonik köşeler).
    let (rx0, ry0, rx1, ry1) = (number(0)?, number(1)?, number(2)?, number(3)?);
    let (mut x0, mut y0, mut x1, mut y1) = (rx0.min(rx1), ry0.min(ry1), rx0.max(rx1), ry0.max(ry1));
    let rotation = placement_rotation(doc, &resolved);
    let original_box = (x0, y0, x1, y1);

    // Metin damgaları (Ek damgası, filigran, sayfa numarası) kullanıcının
    // istediği punto ile basılır.
    let margin = config.margin_pt;
    let gutter = config.margin_pt * 2.0 + config.font_size + 10.0;
    let top = matches!(
        config.position,
        StampPosition::TopRight | StampPosition::TopLeft
    );
    match (rotation, top) {
        (0, true) | (180, false) => y1 += gutter,
        (0, false) | (180, true) => y0 -= gutter,
        (90, true) | (270, false) => x0 -= gutter,
        (90, false) | (270, true) => x1 += gutter,
        _ => return Err(EklerError::InvalidPdf("Geçersiz sayfa dönüşü".into())),
    }

    let (page_w, page_h) = if rotation == 90 || rotation == 270 {
        (y1 - y0, x1 - x0)
    } else {
        (x1 - x0, y1 - y0)
    };
    // Rozet metni ÖLÇÜLÜR (saha maddesi 5: "kutunun ortasında olsa daha iyi
    // olabilir"). Genişlik harf sayısı × 0,65 diye tahmin ediliyor, metin sol
    // kenardan 8 pt'ye sabitleniyordu: "Ek-1 / 1. Sayfa" kutusunda sağda 38 pt,
    // solda 8 pt boşluk kalıyordu. Artık kutu metnin gerçek genişliği + 8 pt
    // iç boşluk, metin kutunun ortasında. Marka işareti bu yoldan geçmez.
    let text_w = helvetica_bold_width(stamp_text, config.font_size);
    let badge_w = (text_w + 16.0).max(40.0);
    let badge_h = config.font_size + 10.0;
    // Sığmazlık denetimi SAYFA DEĞİŞTİRİLMEDEN önce yapılır: sığmıyorsa sayfaya
    // hiç dokunulmaz. Filigran ve sayfa numarası isteğe bağlı işaretlerdir;
    // tek bir küçük sayfa bütün işlemi düşürmemeli, sayfa işaretsiz atlanır.
    // (GölgeDosya işareti isteğe bağlı değildir; o `place_brand_overlay`'den
    // geçer ve sayfaya sığacak kadar küçülür.)
    if page_w < badge_w + margin * 2.0 || page_h < badge_h + margin * 2.0 {
        return Ok(false);
    }

    // Açıklama denetimi PAY EKLENDİKTEN SONRA ve yalnız yeni açılan şeride
    // bakar. Pay tek bir kenara eklenir: sayfanın sağında duran bir vurgu
    // popup'ı ya da sağ kenarı birkaç punto aşan bir bağlantı, alta eklenen
    // şeritle görünür hâle GELMEZ. Eski denetim yönü hiç dikkate almıyor,
    // okunamayan girdide (null, dolaylı /Rect, /Rect'siz açıklama) de lopdf
    // tür hatasıyla düşüyordu: sağlam bir belge "Bozuk veya geçersiz PDF"
    // diye reddediliyor, kullanıcı hem kaydedemiyor hem sıkıştıramıyordu.
    if let Ok(annots) = resolved.get(b"Annots") {
        if let Some(strip) = revealed_strip(original_box, (x0, y0, x1, y1)) {
            let items = match doc.dereference(annots) {
                Ok((_, Object::Array(items))) => items.clone(),
                _ => Vec::new(),
            };
            if items
                .iter()
                .any(|item| annotation_would_be_revealed(doc, item, strip))
            {
                return Err(EklerError::InvalidPdf("Görünür alan dışına taşan açıklama/form alanı damga payı eklenince görünür hâle gelecekti; gizli görünümü açmamak için işlem durduruldu".into()));
            }
        }
    }
    let media = resolved.get(b"MediaBox").map_err(pdf_error)?;
    let (_, media) = doc.dereference(media).map_err(pdf_error)?;
    let media = media.as_array().map_err(pdf_error)?;
    if media.len() != 4 {
        return Err(EklerError::InvalidPdf("Geçersiz MediaBox".into()));
    }
    // MediaBox köşeleri de ters sırada olabilir; pay eklenmiş kutuyla min/max
    // almadan önce normalize edilir, yoksa çıktı MediaBox'ı bozulur.
    let mn = |i: usize| media[i].as_float().map_err(pdf_error);
    let (m0, m1, m2, m3) = (mn(0)?, mn(1)?, mn(2)?, mn(3)?);
    let expanded_media = vec![
        m0.min(m2).min(x0).into(),
        m1.min(m3).min(y0).into(),
        m0.max(m2).max(x1).into(),
        m1.max(m3).max(y1).into(),
    ];
    // The extra strip is outside the old visible page; no source text is covered.
    let page = doc
        .get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(pdf_error)?;
    page.set(
        "CropBox",
        vec![
            Object::Real(x0),
            Object::Real(y0),
            Object::Real(x1),
            Object::Real(y1),
        ],
    );
    page.set("MediaBox", Object::Array(expanded_media));
    let matrix = display_matrix(rotation, [x0, y0, x1, y1])?;
    // Copy resource dictionaries per page; never overwrite a source font binding.
    let mut resources = resolved
        .get(b"Resources")
        .ok()
        .map(|o| doc.dereference(o).and_then(|(_, o)| o.as_dict()).cloned())
        .transpose()
        .map_err(pdf_error)?
        .unwrap_or_default();
    let mut fonts = resources
        .get(b"Font")
        .ok()
        .map(|o| doc.dereference(o).and_then(|(_, o)| o.as_dict()).cloned())
        .transpose()
        .map_err(pdf_error)?
        .unwrap_or_default();
    let mut font_name = STAMP_FONT.to_string();
    while fonts.has(font_name.as_bytes()) {
        font_name.push('_');
    }
    fonts.set(font_name.clone(), font_id);
    resources.set("Font", fonts);
    doc.get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(pdf_error)?
        .set("Resources", resources);

    let (badge_x, badge_y, _, text_y) = match config.position {
        StampPosition::TopRight => {
            let x = page_w - margin - badge_w;
            let y = page_h - margin - badge_h;
            (x, y, x + 8.0, y + 6.0)
        }
        StampPosition::TopLeft => {
            let x = margin;
            let y = page_h - margin - badge_h;
            (x, y, x + 8.0, y + 6.0)
        }
        StampPosition::BottomRight => {
            let x = page_w - margin - badge_w;
            let y = margin;
            (x, y, x + 8.0, y + 6.0)
        }
        StampPosition::BottomLeft => {
            let x = margin;
            let y = margin;
            (x, y, x + 8.0, y + 6.0)
        }
    };

    let text_x = badge_x + (badge_w - text_w) / 2.0;

    let mut ops = vec![
        Operation::new("q", vec![]),
        Operation::new("cm", matrix.into_iter().map(Object::Real).collect()),
    ];

    if config.show_badge {
        ops.extend(vec![
            // Rozet arka planı (açık gri, okunabilirlik için)
            Operation::new("rg", vec![0.94.into(), 0.94.into(), 0.96.into()]),
            Operation::new(
                "re",
                vec![
                    badge_x.into(),
                    badge_y.into(),
                    badge_w.into(),
                    badge_h.into(),
                ],
            ),
            Operation::new("f", vec![]),
            // İnce kenarlık
            Operation::new("RG", vec![0.4.into(), 0.4.into(), 0.4.into()]),
            Operation::new("w", vec![0.6.into()]),
            Operation::new(
                "re",
                vec![
                    badge_x.into(),
                    badge_y.into(),
                    badge_w.into(),
                    badge_h.into(),
                ],
            ),
            Operation::new("S", vec![]),
        ]);
    }

    ops.extend(vec![
        // Metin
        Operation::new("BT", vec![]),
        Operation::new("rg", vec![0.1.into(), 0.1.into(), 0.1.into()]),
        Operation::new(
            "Tf",
            vec![
                Object::Name(font_name.into_bytes()),
                config.font_size.into(),
            ],
        ),
        Operation::new("Td", vec![text_x.into(), text_y.into()]),
        Operation::new(
            "Tj",
            vec![Object::String(
                // Font kodlamasına çevrilmiş baytlar; ham UTF-8 mojibake üretirdi.
                encode_mark_text(stamp_text).ok_or_else(|| {
                    EklerError::ValidationFailed(
                        "Damga metni bu fontla yazılamayan bir karakter içeriyor".into(),
                    )
                })?,
                lopdf::StringFormat::Literal,
            )],
        ),
        Operation::new("ET", vec![]),
        Operation::new("Q", vec![]),
    ]);

    let stamp_content = Content { operations: ops };
    let stamp_stream_id = doc.add_object(Stream::new(
        Dictionary::new(),
        stamp_content
            .encode()
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?,
    ));
    // Source streams stay intact; they are clipped to the old box and wrapped.
    let (ox0, oy0, ox1, oy1) = original_box;
    let prefix = doc.add_object(Stream::new(
        Dictionary::new(),
        format!("q\n{ox0} {oy0} {} {} re W n\n", ox1 - ox0, oy1 - oy0).into_bytes(),
    ));
    let suffix = doc.add_object(Stream::new(Dictionary::new(), b"\nQ\n".to_vec()));
    wrap_contents(doc, page_id, &resolved, prefix, suffix, stamp_stream_id)?;
    Ok(true)
}

/// Sayfanın içerik akışlarını `prefix … suffix` ile sarar ve `overlay`
/// akışını en sona, belgenin üstüne ekler. Kaynak akışlar değişmez.
fn wrap_contents(
    doc: &mut LopdfDoc,
    page_id: lopdf::ObjectId,
    resolved: &Dictionary,
    prefix: lopdf::ObjectId,
    suffix: lopdf::ObjectId,
    overlay: lopdf::ObjectId,
) -> Result<()> {
    let old = resolved.get(b"Contents").ok().cloned();
    let mut contents = vec![Object::Reference(prefix)];
    if let Some(old) = old {
        // `/Contents` dolaylı olup bir DİZİYE çözülebilir (`5 0 R` → `[6 0 R]`).
        // Düzleştirmeden önce çöz; yoksa yeni diziye bir DİZİYE başvuru gömülür
        // ve çıktı "Contents stream eksik" ile doğrulamada düşer (okuma yolu
        // zaten dereference ediyor — bkz. content_items).
        let resolved_old = doc
            .dereference(&old)
            .map(|(_, o)| o.clone())
            .unwrap_or_else(|_| old.clone());
        match resolved_old {
            Object::Array(items) => contents.extend(items),
            // Tek akış (doğrudan ya da dolaylı başvuru): başvuruyu koru.
            _ => contents.push(old),
        }
    }
    contents.extend([Object::Reference(suffix), Object::Reference(overlay)]);
    doc.get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(pdf_error)?
        .set("Contents", contents);
    Ok(())
}
fn pdf_error(e: lopdf::Error) -> EklerError {
    EklerError::InvalidPdf(e.to_string())
}

/// Pay eklendikten sonra YENİ görünür hâle gelen şerit, `[llx, lly, urx, ury]`.
/// Pay tam olarak bir kenara eklenir; önceki ve sonraki kutu yalnız o kenarda
/// ayrışır.
fn revealed_strip(before: (f32, f32, f32, f32), after: (f32, f32, f32, f32)) -> Option<[f32; 4]> {
    let (bx0, by0, bx1, by1) = before;
    let (ax0, ay0, ax1, ay1) = after;
    if ay0 < by0 {
        Some([ax0, ay0, ax1, by0])
    } else if ay1 > by1 {
        Some([ax0, by1, ax1, ay1])
    } else if ax0 < bx0 {
        Some([ax0, ay0, bx0, ay1])
    } else if ax1 > bx1 {
        Some([bx1, ay0, ax1, ay1])
    } else {
        None
    }
}

/// Bir açıklama, yeni açılan şerit yüzünden görünür hâle gelir mi?
///
/// Görüntüleyicinin yerleştiremeyeceği girdi — null, sözlük olmayan, `/Rect`'i
/// eksik ya da sayı olmayan — açığa çıkamaz. Gizli (`/F` Hidden) ya da ekranda
/// çizilmeyen (`/F` NoView) açıklama, kenarlıksız bağlantı ve kapalı popup
/// zaten hiçbir şey çizmez. `/Rect` ve öğeleri dolaylı
/// olabilir ve köşe sırası normalize edilmek zorunda değildir. Şeride saç teli
/// kadar değen kenar sayılmaz.
fn annotation_would_be_revealed(doc: &LopdfDoc, item: &Object, strip: [f32; 4]) -> bool {
    const HIDDEN: i64 = 2;
    const NO_VIEW: i64 = 32;
    const HAIRLINE: f32 = 0.5;
    let Ok((_, Object::Dictionary(annot))) = doc.dereference(item) else {
        return false;
    };
    let flags = annot
        .get(b"F")
        .ok()
        .and_then(|f| doc.dereference(f).ok())
        .and_then(|(_, f)| f.as_i64().ok())
        .unwrap_or(0);
    if flags & (HIDDEN | NO_VIEW) != 0 {
        return false;
    }
    // Bağlantı açıklamasının kendi görünümü yoktur; ekranda çizdiği tek şey
    // kenarlığıdır. Görünüm akışı (/AP) taşımayan ve kenarlık kalınlığı açıkça
    // 0 olan bir bağlantı (Word'ün yazdığı biçim) hiçbir şey çizmez — açığa
    // çıkaracağı bir görünüm yoktur. Kalınlık /BS /W'den, yoksa /Border'ın
    // üçüncü öğesinden okunur; ikisi de yoksa spec varsayılanı 1'dir.
    let subtype = annot.get(b"Subtype").ok().and_then(|t| t.as_name().ok());
    if subtype == Some(b"Link".as_slice()) && !annot.has(b"AP") {
        let number = |o: &Object| doc.dereference(o).ok().and_then(|(_, n)| n.as_float().ok());
        let from_bs = annot
            .get(b"BS")
            .ok()
            .and_then(|bs| doc.dereference(bs).ok())
            .and_then(|(_, bs)| bs.as_dict().ok())
            .and_then(|bs| bs.get(b"W").ok())
            .and_then(number);
        let from_border = annot
            .get(b"Border")
            .ok()
            .and_then(|b| doc.dereference(b).ok())
            .and_then(|(_, b)| b.as_array().ok())
            .and_then(|b| b.get(2))
            .and_then(number);
        if from_bs.or(from_border).unwrap_or(1.0) == 0.0 {
            return false;
        }
    }
    // Popup'ın kendi görünümü yoktur (spec 12.5.6.14): üst açıklaması
    // açıldığında görüntüleyici onu pencere olarak çizer. `/Open true`
    // taşımayan (varsayılan: kapalı) popup sayfada hiçbir şey çizmez.
    // Önizleme ve Acrobat not popup'larını sayfanın sağına, kutunun dışına
    // koyar; 90° dönük sayfada pay tam o kenara eklendiği için kapalı bir
    // popup yüzünden işlem reddediliyordu. Açık popup ya da görünüm akışı
    // taşıyan (spec dışı) popup çizilebilir sayılır ve korunur.
    if subtype == Some(b"Popup".as_slice()) && !annot.has(b"AP") {
        let open = annot
            .get(b"Open")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_bool().ok())
            .unwrap_or(false);
        if !open {
            return false;
        }
    }
    let Some(rect) = annot
        .get(b"Rect")
        .ok()
        .and_then(|r| doc.dereference(r).ok())
        .and_then(|(_, r)| r.as_array().ok())
    else {
        return false;
    };
    if rect.len() != 4 {
        return false;
    }
    let mut v = [0f32; 4];
    for (slot, n) in v.iter_mut().zip(rect) {
        match doc.dereference(n).ok().and_then(|(_, n)| n.as_float().ok()) {
            Some(n) if n.is_finite() => *slot = n,
            _ => return false,
        }
    }
    let (lx, ux) = (v[0].min(v[2]), v[0].max(v[2]));
    let (ly, uy) = (v[1].min(v[3]), v[1].max(v[3]));
    lx < strip[2] - HAIRLINE
        && ux > strip[0] + HAIRLINE
        && ly < strip[3] - HAIRLINE
        && uy > strip[1] + HAIRLINE
}

/// Türkçeye özgü, WinAnsi'de KARŞILIĞI OLMAYAN altı harf. Kullanılmayan düşük
/// kodlara `/Differences` ile bağlanır; glif adları Adobe Glyph List'tendir ve
/// base-14 Helvetica'yı sağlayan her font bunları içerir.
const TR_GLYPHS: [(char, u8, &str); 6] = [
    ('İ', 1, "Idotaccent"),
    ('ı', 2, "dotlessi"),
    ('Ş', 3, "Scedilla"),
    ('ş', 4, "scedilla"),
    ('Ğ', 5, "Gbreve"),
    ('ğ', 6, "gbreve"),
];

/// Damga/filigran metnini font kodlamasına çevir.
///
/// Metin `Tj`'ye HAM UTF-8 olarak veriliyordu; WinAnsi bir fontta çok baytlı
/// karakterler bozuk glif (mojibake) üretirdi. Bu yüzden metin ASCII'ye
/// kısıtlanmıştı — ama ürün Türk hukukçular için: "GİZLİ", "ÖRNEKTİR",
/// "SURETİDİR" gibi en olağan filigranlar reddediliyordu.
///
/// Burada her karakter tek bayta çevrilir: Latin-1 karşılığı olanlar doğrudan
/// (WinAnsi bu aralıkta Latin-1 ile örtüşür), Türkçeye özgü altı harf
/// `/Differences` kodlarına. Çevrilemeyen karakterde `None` döner ki çağıran
/// sessizce bozuk glif basmak yerine açık hata verebilsin.
pub fn encode_mark_text(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        if let Some((_, code, _)) = TR_GLYPHS.iter().find(|(c, _, _)| *c == ch) {
            out.push(*code);
            continue;
        }
        match ch as u32 {
            // Yazdırılabilir ASCII.
            0x20..=0x7E => out.push(ch as u8),
            // Latin-1 ek aralığı; WinAnsi burada Latin-1 ile aynıdır.
            0xA0..=0xFF => out.push(ch as u8),
            _ => return None,
        }
    }
    Some(out)
}

/// Damga fontu: WinAnsi tabanı + Türkçeye özgü glifler için `/Differences`.
fn mark_font(doc: &mut LopdfDoc) -> lopdf::ObjectId {
    let mut differences: Vec<Object> = Vec::new();
    for (_, code, glyph) in TR_GLYPHS {
        differences.push(Object::Integer(code as i64));
        differences.push(Object::Name(glyph.as_bytes().to_vec()));
    }
    let encoding = doc.add_object(dictionary! {
        "Type"=>"Encoding",
        "BaseEncoding"=>"WinAnsiEncoding",
        "Differences"=>differences
    });
    doc.add_object(dictionary! {
        "Type"=>"Font","Subtype"=>"Type1",
        "BaseFont"=>"Helvetica-Bold","Encoding"=>encoding
    })
}

pub fn apply_text_marks(doc: &mut LopdfDoc, text: Option<&str>, start: usize) -> Result<()> {
    let font = mark_font(doc);
    let config = StampConfig {
        enabled: true,
        position: StampPosition::BottomRight,
        font_size: 10.,
        margin_pt: 20.,
        show_badge: false,
    };
    for (i, id) in doc.get_pages().into_values().enumerate() {
        let mark = text
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}", start + i));
        apply_stamp_to_page(doc, id, &mark, font, &config)?;
    }
    Ok(())
}

/// Required mark for derived documents only. Call after transforms and before size validation.
pub fn apply_branding(doc: &mut LopdfDoc) -> Result<()> {
    apply_branding_for_output(doc, None)
}
/// Same shared vector/alpha as PDF output, with deterministic pixel bounds for raster export.
pub fn apply_raster_branding(doc: &mut LopdfDoc, dpi: u32) -> Result<()> {
    apply_branding_for_output(doc, Some(dpi))
}
const BRAND_LOGO: &[u8] = include_bytes!("../../assets/brand-logo.ops");
/// Ürün birleşmeden önceki DüzenEk marka çizimi. ARTIK ÜRETİLMEZ; yalnızca eski
/// sürümle markalanmış belgeleri TANIYIP yükseltmek için tutulur (bkz.
/// `upgrade_legacy_brand_marks`). Tanınmazsa eski belge yeniden işlendiğinde
/// ikinci bir işaret eklenir ve kullanıcı "DüzenEk + GölgeDosya" çift filigranı
/// görürdü.
const LEGACY_BRAND_LOGO: &[u8] = include_bytes!("../../assets/brand-logo-legacy.ops");
/// Birleşme SONRASI üretilmiş ama canonical OLMAYAN marka çizimi.
///
/// `5a399ea` eski DüzenEk kelime işaretini kaldırırken yerine YENİ bir şekil
/// uydurdu ("üst üste iki yaprak") ve eski DüzenEk paletini (#176D73 petrol
/// yeşili) korudu. Oysa ürünün kabul edilmiş işareti
/// `apps/belge-shell/brand/mark.svg` içindeki "Kat"tır: yuvarlatılmış tek
/// sayfa, sağ alt köşesi kalkık, altında pirinç katman. BRAND.md petrol
/// yeşilini açıkça yasaklar. Bu çizimle damgalanmış belgeler sahada üretildi;
/// tanınmazsa üzerlerine ikinci bir işaret eklenir.
const SUPERSEDED_BRAND_LOGO: &[u8] =
    include_bytes!("../../assets/brand-logo-superseded-2026-09-16.ops");
/// v0.3.0 ve v0.3.1'in bastığı "Kat" işareti (2026-09-20). Tek farkı kelime
/// işaretinin taban çizgisi: `76 27 Td`. Büyük harf bandının ortası simgenin
/// ortasından 3,6 birim yukarıda kalıyordu (saha: "simge ile yazı ortada
/// değil"). Sahadaki bütün v0.3.x çıktıları bu baytları taşır; tanınmazsa
/// üzerlerine ikinci bir işaret basılır ve Sıkıştır onları "daha önce
/// işlenmemiş" sayar.
const KAT_2026_09_20_BRAND_LOGO: &[u8] = include_bytes!("../../assets/brand-logo-2026-09-20.ops");

/// Artık üretilmeyen ama TANINMASI gereken bütün marka çizimleri.
const OUTDATED_BRAND_LOGOS: [&[u8]; 3] = [
    LEGACY_BRAND_LOGO,
    SUPERSEDED_BRAND_LOGO,
    KAT_2026_09_20_BRAND_LOGO,
];
/// GölgeDosya'nın KENDİ ürettiği eski çizimler. Bağımsız DüzenEk'in çizimi
/// (`LEGACY_BRAND_LOGO`) burada yoktur: o belgeyi GölgeDosya işlemedi.
const PRIOR_GOLGEDOSYA_LOGOS: [&[u8]; 2] = [SUPERSEDED_BRAND_LOGO, KAT_2026_09_20_BRAND_LOGO];
const BRAND_FORM_CONTENT: &[u8] = b"q /BrandAlpha gs /Mark Do Q";
const BRAND_BBOX: [f32; 4] = [0., 0., 290., 72.];

/// Kabul edilmiş işaretin genişliği: 56 pt (A4/Letter).
const BRAND_MARK_WIDTH: f32 = 56.0;
/// İşaretin görünür kutunun sağ ve alt kenarından uzaklığı, standart sayfada.
/// Ofis yazıcılarının çoğu kenardan 4–5 mm'yi basamaz; 12 pt (≈4,2 mm)
/// işaretin "gerçek boyut" baskıda kesilmemesi içindir. İşaretle orantılıdır.
const BRAND_INSET: f32 = 12.0;
/// Bu kısa kenara kadar (Letter, 612 pt; A4 595 pt) işaret kabul edilmiş
/// boyutunda basılır. Daha büyük sayfada işaret, kenar boşluğuyla birlikte
/// sayfayla orantılı büyür.
///
/// Saha bulgusu: işaret sabit 56 pt basılıyordu. Telefonla çekilmiş belgeler
/// 1414–1851 pt genişlikte (1 px = 1 pt) geliyor; işaret sayfanın %3'üne,
/// sayfaya sığdır görünümünde 5–7 px'lik bir lekeye iniyordu. Nesne olarak
/// oradaydı, kullanıcı için yoktu. Raster dışa aktarma bu oranı zaten
/// uyguluyordu (`page_w · 0,094` = 56/595); PDF yolu uygulamıyordu.
const BRAND_REFERENCE_SHORT_SIDE: f32 = 612.0;
/// Doğrulamanın okunurluk eşiği: görünür kutunun kısa kenarına oranla işaret
/// genişliği. Kabul edilmiş işaret A4'te %9,4, en kötü standart sayfada
/// (Letter/Legal) %9,2'dir; v0.3.x'in paylı çıktısında yatay Letter %8,7.
/// Sahadaki okunmaz işaret %3–4.
pub const MIN_BRAND_MARK_RATIO: f32 = 0.08;
/// Bundan küçük ölçek, işaretin fiziksel olarak basılamadığı (genişliği
/// ~3 pt'nin altında) dejenere bir sayfa demektir.
const MIN_BRAND_SCALE: f32 = 0.05;

/// PDF işaretinin sayfaya göre ölçeği.
///
/// Büyük sayfada `kısa kenar / 612` ile büyür, standart sayfada 1'dir. İşaret
/// taşıyamayacak kadar dar ya da KISA sayfada sığacak kadar küçülür: işaret
/// artık sayfanın içine bindirildiği için yükseklik de sınırdır (eskiden
/// işaret alta eklenen payda durduğundan yalnız genişliğe bakılıyordu).
fn brand_scale(display_w: f32, display_h: f32) -> f32 {
    let grow = (display_w.min(display_h) / BRAND_REFERENCE_SHORT_SIDE).max(1.0);
    grow.min(brand_fit_width(display_w, display_h) / BRAND_MARK_WIDTH)
}

/// Görsel boyutu `w × h` olan kutuya, iki yandaki orantılı kenar boşluğuyla
/// birlikte sığan en geniş işaret (pt).
fn brand_fit_width(display_w: f32, display_h: f32) -> f32 {
    let inset = BRAND_INSET / BRAND_MARK_WIDTH;
    (display_w / (1.0 + 2.0 * inset)).min(display_h / (brand_aspect() + 2.0 * inset))
}

/// İşaretin yüksekliği / genişliği.
fn brand_aspect() -> f32 {
    (BRAND_BBOX[3] - BRAND_BBOX[1]) / (BRAND_BBOX[2] - BRAND_BBOX[0])
}

/// `width` genişliğindeki işaretin `bx` kutusunun görüntülenen sağ alt
/// köşesindeki sol alt noktası, kutunun görsel uzayında: sağdan ve alttan
/// orantılı kenar boşluğu kadar içeride.
fn mark_origin(rotation: i64, bx: [f32; 4], width: f32) -> (f32, f32) {
    let (page_w, _) = display_size(rotation, bx);
    let inset = width * BRAND_INSET / BRAND_MARK_WIDTH;
    (page_w - inset - width, inset)
}

/// `[a b c d e f]` dönüşümü; nokta: x' = a·x + c·y + e, y' = b·x + d·y + f.
type Matrix = [f32; 6];
const IDENTITY: Matrix = [1., 0., 0., 1., 0., 0.];

/// `m`, `ctm`'nin ÖNÜNE eklenir (`cm` işleci): önce `m`, sonra `ctm`.
fn concat(m: Matrix, ctm: Matrix) -> Matrix {
    [
        m[0] * ctm[0] + m[1] * ctm[2],
        m[0] * ctm[1] + m[1] * ctm[3],
        m[2] * ctm[0] + m[3] * ctm[2],
        m[2] * ctm[1] + m[3] * ctm[3],
        m[4] * ctm[0] + m[5] * ctm[2] + ctm[4],
        m[4] * ctm[1] + m[5] * ctm[3] + ctm[5],
    ]
}

/// `cm` işlecinin altı sayısı; sayı olmayan ya da eksik işlenen `None`.
fn operation_matrix(op: &Operation) -> Option<Matrix> {
    let m: Vec<f32> = op
        .operands
        .iter()
        .filter_map(|o| o.as_float().ok())
        .collect();
    m.try_into().ok()
}

/// `q` ardından gelen `x y w h re W n`: bir kırpma sarmalının dikdörtgeni.
fn clip_rect(after_q: &[Operation]) -> Option<[f32; 4]> {
    let [re, clip, end, ..] = after_q else {
        return None;
    };
    if re.operator != "re" || clip.operator != "W" || end.operator != "n" {
        return None;
    }
    let v: Vec<f32> = re
        .operands
        .iter()
        .filter_map(|o| o.as_float().ok())
        .collect();
    let &[x, y, w, h] = v.as_slice() else {
        return None;
    };
    Some([x.min(x + w), y.min(y + h), x.max(x + w), y.max(y + h)])
}

/// Kullanıcı işaretlerinin (Ek damgası, filigran, sayfa numarası) eklediği
/// payların DIŞINDA kalan kutu: sayfanın o işaretlerden önceki görünür alanı.
///
/// `apply_stamp_to_page` payı ekledikten sonra kaynak içeriği eski kutuya
/// kırpan bir sarmalla (`q x y w h re W n … Q`) sarar ve damga akışını
/// (`q cm … /GolgeDosyaStamp Tf … Q`) HEMEN ardından ekler. Damgadan hemen
/// önce kapanan sarmalın kırpma dikdörtgeni, payı eklemeden önceki kutudur.
/// Birden çok damgada (numara üstüne filigran) kutular kesiştirilir.
/// Akışlar sırayla, PDF'in tek içerik akışı gibi yürünür: sarmalın açılışı ve
/// kapanışı ayrı akışlardadır. Damga yoksa ya da yapı tanınmazsa `None`.
fn unstamped_box(doc: &LopdfDoc, page: &Dictionary) -> Option<[f32; 4]> {
    struct Level {
        /// Bu `q` bir kırpma sarmalı açıyorsa dikdörtgeni (kullanıcı uzayı).
        clip: Option<[f32; 4]>,
        /// Bu `q`'dan hemen önce kapanan sarmalın dikdörtgeni.
        after_clip: Option<[f32; 4]>,
        ctm: Matrix,
    }
    let mut stack: Vec<Level> = Vec::new();
    let mut ctm = IDENTITY;
    let mut just_closed = None;
    let mut original: Option<[f32; 4]> = None;
    for item in content_items(doc, page) {
        let Some(operations) = stream_operations(doc, &item) else {
            continue;
        };
        for (i, op) in operations.iter().enumerate() {
            let mut closed = None;
            match op.operator.as_str() {
                "q" => stack.push(Level {
                    clip: (ctm == IDENTITY)
                        .then(|| clip_rect(&operations[i + 1..]))
                        .flatten(),
                    after_clip: just_closed,
                    ctm,
                }),
                "Q" => {
                    if let Some(level) = stack.pop() {
                        ctm = level.ctm;
                        closed = level.clip;
                    }
                }
                "cm" => {
                    if let Some(m) = operation_matrix(op) {
                        ctm = concat(m, ctm);
                    }
                }
                "Tf" if op
                    .operands
                    .first()
                    .and_then(|n| n.as_name().ok())
                    .is_some_and(|n| n.starts_with(STAMP_FONT.as_bytes())) =>
                {
                    if let Some(clip) = stack.last().and_then(|level| level.after_clip) {
                        original = Some(match original {
                            Some(known) => intersect(known, clip)?,
                            None => clip,
                        });
                    }
                }
                _ => {}
            }
            just_closed = closed;
        }
    }
    original
}

/// GölgeDosya işaretini sayfanın ÜSTÜNE bindirir; belge geometrisine dokunmaz.
///
/// Ürün kuralı: marka belge geometrisini DEĞİŞTİRMEZ. MediaBox, CropBox,
/// /Rotate, /Annots, sayfa sayısı ve içeriğin konumu aynen kalır. Eskiden
/// işaret görsel alt kenara eklenen 34 pt'lik bir payda duruyordu; A4 sayfa
/// 595×876 pt'ye, PNG dışa aktarma 34 pt uzuna çıkıyordu (saha: "büyütmesin
/// aşağıya doğru evrakı").
///
/// İşaret görünür kutunun (CropBox ∩ MediaBox) içinde, sayfanın /Rotate ile
/// görüntülenen hâlinin sağ alt köşesine, dik basılır. Saydamdır (marka
/// formunun ExtGState'i, %24, normal karışım) ve belge içeriğinin üstünde
/// çizilir. Kaynak içerik `q … Q` ile sarılır ki işaret temiz grafik
/// durumundan başlasın; kırpma dikdörtgeni görünür kutunun kendisidir,
/// görünen hiçbir şeyi kesmez.
///
/// Sayfa numarası, filigran ya da Ek damgası sayfaya kendi payını ekler ve
/// işaretini o paya basar. İşaret bu durumda payın DIŞINDAKİ özgün kutunun
/// (`unstamped_box`) sağ alt köşesine basılır: ikisi asla üst üste binmez.
/// Pay sayfanın kendi geometrisidir, dokunulmaz. Özgün kutuda işaret okunur
/// boyutta ya da görünür sayfanın sağ alt çeyreğinde kalamıyorsa (yalnız
/// birkaç punto yüksekliğinde dejenere bir sayfa) görünür kutuya dönülür.
///
/// Dönüş: `true` işaret basıldı; `false` raster dışa aktarmada sayfa işareti
/// taşıyamayacak kadar küçük olduğu için atlandı. PDF çıktısında bu durum
/// hatadır: her PDF çıktısı işareti taşır.
fn place_brand_overlay(
    doc: &mut LopdfDoc,
    page_id: lopdf::ObjectId,
    brand: &mut BrandResources,
) -> Result<bool> {
    let resolved = super::resolved_page_dictionary(doc, page_id)?;
    let visible = visible_box(doc, &resolved)
        .ok_or_else(|| EklerError::InvalidPdf("Geçersiz sayfa kutusu".into()))?;
    let rotation = placement_rotation(doc, &resolved);
    let short_side = (visible[2] - visible[0]).min(visible[3] - visible[1]);
    let raster_dpi = brand.raster_dpi;
    let mark_width_in = |bx: [f32; 4]| {
        let (page_w, page_h) = display_size(rotation, bx);
        // Raster dışa aktarmanın kendi ölçüsü var: piksel olarak sayfa
        // genişliğinin %9,4'ü (A4'te 56 pt), 32–240 px arası.
        let wanted = match raster_dpi {
            Some(dpi) => (page_w * dpi as f32 / 72. * 0.094).clamp(32., 240.) * 72. / dpi as f32,
            None => BRAND_MARK_WIDTH * brand_scale(page_w, page_h),
        };
        wanted.min(brand_fit_width(page_w, page_h))
    };
    let anchor = unstamped_box(doc, &resolved)
        .and_then(|original| intersect(original, visible))
        .filter(|original| {
            let width = mark_width_in(*original);
            let legible = match raster_dpi {
                Some(_) => width > BRAND_MARK_WIDTH * MIN_BRAND_SCALE,
                None => width >= MIN_BRAND_MARK_RATIO * short_side,
            };
            legible
                && display_matrix(rotation, *original).is_ok_and(|m| {
                    let (x, y) = mark_origin(rotation, *original, width);
                    let (cx, cy) = (x + width / 2., y + width * brand_aspect() / 2.);
                    let centre = (m[0] * cx + m[2] * cy + m[4], m[1] * cx + m[3] * cy + m[5]);
                    in_bottom_right_quarter(rotation, visible, centre)
                })
        })
        .unwrap_or(visible);
    let mark_width = mark_width_in(anchor);
    if !(mark_width.is_finite() && mark_width > BRAND_MARK_WIDTH * MIN_BRAND_SCALE) {
        if brand.raster_dpi.is_some() {
            return Ok(false);
        }
        return Err(EklerError::InvalidPdf(
            "Sayfa GölgeDosya işaretini taşıyamayacak kadar küçük".into(),
        ));
    }
    let matrix = display_matrix(rotation, anchor)?;
    let (mark_x, mark_y) = mark_origin(rotation, anchor, mark_width);

    // Kaynakları sayfaya kopyala (kalıtımla gelenler dahil) ve formu benzersiz
    // bir adla bağla; belgenin kendi XObject adlarının üstüne yazılmaz.
    let mut resources = resolved
        .get(b"Resources")
        .ok()
        .map(|o| doc.dereference(o).and_then(|(_, o)| o.as_dict()).cloned())
        .transpose()
        .map_err(pdf_error)?
        .unwrap_or_default();
    let mut objects = resources
        .get(b"XObject")
        .ok()
        .map(|o| doc.dereference(o).and_then(|(_, o)| o.as_dict()).cloned())
        .transpose()
        .map_err(pdf_error)?
        .unwrap_or_default();
    let mut name = "GolgeDosyaBrand".to_string();
    while objects.has(name.as_bytes()) {
        name.push('_');
    }
    objects.set(name.clone(), brand.form);
    resources.set("XObject", objects);
    doc.get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(pdf_error)?
        .set("Resources", resources);

    let k = mark_width / (BRAND_BBOX[2] - BRAND_BBOX[0]);
    let ops = vec![
        Operation::new("q", vec![]),
        Operation::new("cm", matrix.into_iter().map(Object::Real).collect()),
        Operation::new(
            "cm",
            vec![
                k.into(),
                0.into(),
                0.into(),
                k.into(),
                mark_x.into(),
                mark_y.into(),
            ],
        ),
        Operation::new("Do", vec![Object::Name(name.into_bytes())]),
        Operation::new("Q", vec![]),
    ];
    let overlay = add_brand_stream(
        doc,
        Content { operations: ops }
            .encode()
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?,
        brand,
    );
    let [vx0, vy0, vx1, vy1] = visible;
    let prefix = add_brand_stream(
        doc,
        format!("q\n{vx0} {vy0} {} {} re W n\n", vx1 - vx0, vy1 - vy0).into_bytes(),
        brand,
    );
    let suffix = add_brand_stream(doc, b"\nQ\n".to_vec(), brand);
    let resolved = super::resolved_page_dictionary(doc, page_id)?;
    wrap_contents(doc, page_id, &resolved, prefix, suffix, overlay)?;
    Ok(true)
}

/// Marka kelime işareti için base-14 Helvetica-Bold. Damga yolunun zaten
/// kullandığı mekanizma; gömülü font yok, her uyumlu görüntüleyicide bulunur.
fn brand_font(doc: &mut LopdfDoc) -> lopdf::ObjectId {
    doc.add_object(dictionary! {
        "Type"=>"Font","Subtype"=>"Type1",
        "BaseFont"=>"Helvetica-Bold","Encoding"=>"WinAnsiEncoding"
    })
}

fn brand_vector_dict(font: lopdf::ObjectId) -> Dictionary {
    dictionary! {
        "Type"=>"XObject","Subtype"=>"Form",
        "BBox"=>BRAND_BBOX.iter().map(|v| Object::Integer(*v as i64)).collect::<Vec<_>>(),
        "Group"=>dictionary! {"S"=>"Transparency","I"=>true,"CS"=>"DeviceRGB"},
        "Resources"=>dictionary! {"Font"=>dictionary! {"BrandFont"=>font}}
    }
}

/// Eskimiş marka çizimlerini YERİNDE canonical GölgeDosya çizimine yükselt.
///
/// Marka formu baytlarıyla tanınır; asset değiştiği için eski bir belgedeki
/// işaret aksi hâlde "marka yok" sayılır ve üstüne ikinci bir GölgeDosya işareti
/// eklenirdi. Çizimi yerinde değiştirmek hem çift filigranı önler hem de eski
/// ürün kimliğinin kullanıcı çıktısında kalmamasını garanti eder.
fn upgrade_legacy_brand_marks(doc: &mut LopdfDoc) -> Result<()> {
    let legacy: Vec<lopdf::ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let stream = obj.as_stream().ok()?;
            if stream.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() != Some(b"Form") {
                return None;
            }
            let data = stream_data(stream)?;
            OUTDATED_BRAND_LOGOS
                .iter()
                .any(|outdated| data == *outdated)
                .then_some(*id)
        })
        .collect();
    if legacy.is_empty() {
        return Ok(());
    }
    let font = brand_font(doc);
    for id in legacy {
        let mut upgraded = Stream::new(brand_vector_dict(font), BRAND_LOGO.to_vec());
        upgraded.compress().map_err(pdf_error)?;
        doc.objects.insert(id, Object::Stream(upgraded));
    }
    Ok(())
}

fn apply_branding_for_output(doc: &mut LopdfDoc, raster_dpi: Option<u32>) -> Result<()> {
    // Marka idempotenttir. Daha önce GölgeDosya'nın türettiği bir belgede
    // işareti görünür alanda duran sayfaya ikinci logo eklenmez; yeni işaret
    // gerekirse var olan marka formu yeniden kullanılır. Eskiden türetilmiş
    // kopyadan türetilen her kopya bir logo daha ekliyordu (sırala → döndür →
    // sil: üç logo, altı form). Kırpma işareti görünür alanın dışında
    // bıraktıysa sayfa yeniden işaretlenir.
    //
    // v0.3.0/0.3.1 çıktıları işareti alta eklenmiş 34 pt'lik bir payda taşır.
    // O pay artık belgenin kendi geometrisidir: korunur, geri alınmaz ve
    // ikinci bir pay eklenmez.
    // Eski DüzenEk işareti taşıyan belgeler ÖNCE yükseltilir: aksi hâlde işaret
    // tanınmaz, sayfaya ikinci bir işaret eklenir ve çift filigran oluşur.
    //
    // "İşaretli" artık "tam bir, görünür ve OKUNUR işaret" demektir. Düzeltmeden
    // önce büyük sayfaya basılmış küçük işaret, kırpma yüzünden görünmez kalmış
    // işaret ya da çift işaret: sayfa yeniden işaretlenir ve GölgeDosya'nın
    // KENDİ yazdığı eski işaret akışı önce sökülür (`detach_brand_stamps`).
    // Böylece yükseltme ikinci bir işaret DEĞİL, tek bir doğru işaret bırakır.
    upgrade_legacy_brand_marks(doc)?;
    let existing = existing_brand_forms(doc);
    let pending: Vec<lopdf::ObjectId> = doc
        .get_pages()
        .into_values()
        .filter(|page| !page_mark_is_canonical(doc, *page, &existing))
        .collect();
    if pending.is_empty() {
        return Ok(());
    }
    if !existing.is_empty() {
        for page in &pending {
            detach_brand_stamps(doc, *page, &existing)?;
        }
    }
    let form_id = match existing.iter().min() {
        Some(form) => *form,
        None => {
            // One shared, translucent vector form; no raster or unused font per page.
            // Isolate the vector as a transparency group: opacity applies once to the
            // complete mark, not repeatedly where the logo's paths overlap.
            let font = brand_font(doc);
            let mut vector = Stream::new(brand_vector_dict(font), BRAND_LOGO.to_vec());
            vector.compress().map_err(pdf_error)?;
            let vector_id = doc.add_object(vector);
            let mut form = Stream::new(
                dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>BRAND_BBOX.iter().map(|v| Object::Integer(*v as i64)).collect::<Vec<_>>(),
                "Resources"=>dictionary! {"XObject"=>dictionary! {"Mark"=>vector_id},"ExtGState"=>dictionary! {"BrandAlpha"=>dictionary! {"Type"=>"ExtGState","ca"=>0.24f32,"CA"=>0.24f32}}}},
                BRAND_FORM_CONTENT.to_vec(),
            );
            form.compress().map_err(pdf_error)?;
            doc.add_object(form)
        }
    };
    let mut resources = BrandResources {
        form: form_id,
        streams: Default::default(),
        raster_dpi,
    };
    for id in pending {
        let stamped = place_brand_overlay(doc, id, &mut resources)?;
        if !stamped && raster_dpi.is_none() {
            return Err(EklerError::InvalidPdf(
                "GölgeDosya işareti sayfaya basılamadı".into(),
            ));
        }
    }
    Ok(())
}

fn dictionary_at<'o>(doc: &'o LopdfDoc, value: Option<&'o Object>) -> Option<&'o Dictionary> {
    value
        .and_then(|v| doc.dereference(v).ok())
        .and_then(|(_, v)| v.as_dict().ok())
}

fn stream_data(stream: &Stream) -> Option<Vec<u8>> {
    if stream.dict.has(b"Filter") {
        stream.decompressed_content().ok()
    } else {
        Some(stream.content.clone())
    }
}

/// Belge daha önce GölgeDosya'dan çıkmış mı?
///
/// Ada ya da üst veriye değil, yalnız GölgeDosya'nın KENDİ marka çizimine
/// bayt bayt bakılır: canonical işaret ya da birleşme sonrası üretilmiş eski
/// çizim. Bağımsız DüzenEk'in işareti sayılmaz; o belgeyi GölgeDosya
/// işlememiştir. Bu, belgenin SIKIŞTIRILDIĞINI söylemez — GölgeDosya her
/// çıktısını markalar.
pub fn is_golgedosya_output(doc: &LopdfDoc) -> bool {
    !existing_brand_forms(doc).is_empty()
        || doc.objects.values().any(|obj| {
            obj.as_stream().is_ok_and(|s| {
                s.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() == Some(b"Form")
                    && stream_data(s)
                        .is_some_and(|d| PRIOR_GOLGEDOSYA_LOGOS.contains(&d.as_slice()))
            })
        })
}

/// Belgedeki GölgeDosya marka formları: içeriği ve logo akışı birebir
/// eşleşen Form XObject'ler. Ada değil baytlara bakılır.
fn existing_brand_forms(doc: &LopdfDoc) -> std::collections::HashSet<lopdf::ObjectId> {
    doc.objects
        .iter()
        .filter_map(|(id, obj)| {
            let form = obj.as_stream().ok()?;
            if form.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() != Some(b"Form")
                || stream_data(form)? != BRAND_FORM_CONTENT
            {
                return None;
            }
            let resources = form.dict.get(b"Resources").ok()?;
            let (_, resources) = doc.dereference(resources).ok()?;
            let xobjects = resources.as_dict().ok()?.get(b"XObject").ok()?;
            let (_, xobjects) = doc.dereference(xobjects).ok()?;
            let mark = xobjects.as_dict().ok()?.get(b"Mark").ok()?;
            let (_, mark) = doc.dereference(mark).ok()?;
            (stream_data(mark.as_stream().ok()?)? == BRAND_LOGO).then_some(*id)
        })
        .collect()
}

/// Sayfadaki bir marka çizimi: görünür kutunun içinde mi, kaç pt genişlikte.
#[derive(Debug, Clone, Copy)]
struct BrandDraw {
    inside: bool,
    width: f32,
}

/// Sayfa kutusu (`CropBox`, `MediaBox` …), köşeleri normalize.
fn page_box(doc: &LopdfDoc, page: &Dictionary, key: &[u8]) -> Option<[f32; 4]> {
    let number = |o: &Object| {
        doc.dereference(o)
            .ok()
            .and_then(|(_, o)| o.as_float().ok())
            .filter(|n| n.is_finite())
    };
    page.get(key)
        .ok()
        .and_then(|b| doc.dereference(b).ok())
        .and_then(|(_, b)| b.as_array().ok())
        .filter(|b| b.len() == 4)
        .and_then(|b| {
            let v = [
                number(&b[0])?,
                number(&b[1])?,
                number(&b[2])?,
                number(&b[3])?,
            ];
            Some([
                v[0].min(v[2]),
                v[1].min(v[3]),
                v[0].max(v[2]),
                v[1].max(v[3]),
            ])
        })
}

/// Görünür kutu: CropBox ∩ MediaBox (ISO 32000-1 §14.11.2: görüntüleyici
/// CropBox'ı MediaBox'a kırpar), CropBox yoksa MediaBox. Kesişim boşsa
/// (bozuk üretici kutusu) CropBox'ın kendisi.
fn visible_box(doc: &LopdfDoc, page: &Dictionary) -> Option<[f32; 4]> {
    let media = page_box(doc, page, b"MediaBox");
    let Some(crop) = page_box(doc, page, b"CropBox") else {
        return media;
    };
    let Some(m) = media else {
        return Some(crop);
    };
    let both = [
        crop[0].max(m[0]),
        crop[1].max(m[1]),
        crop[2].min(m[2]),
        crop[3].min(m[3]),
    ];
    Some(if both[0] < both[2] && both[1] < both[3] {
        both
    } else {
        crop
    })
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

/// Sayfanın kaynaklarında bir marka formuna bağlı XObject adları.
fn brand_names(
    doc: &LopdfDoc,
    page: &Dictionary,
    forms: &std::collections::HashSet<lopdf::ObjectId>,
) -> Vec<Vec<u8>> {
    dictionary_at(doc, page.get(b"Resources").ok())
        .and_then(|r| dictionary_at(doc, r.get(b"XObject").ok()))
        .map(|xobjects| {
            xobjects
                .iter()
                .filter(|(_, v)| v.as_reference().is_ok_and(|id| forms.contains(&id)))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn stream_operations(doc: &LopdfDoc, item: &Object) -> Option<Vec<Operation>> {
    let (_, obj) = doc.dereference(item).ok()?;
    let data = stream_data(obj.as_stream().ok()?)?;
    Content::decode(&data).ok().map(|c| c.operations)
}

/// Sayfadaki bütün marka çizimleri ve görünür kutunun kısa kenarı.
///
/// İşaretin kutusu, çizildiği akıştaki `cm` dönüşümlerinden hesaplanır;
/// genişliği dönüşümün x ekseni uzunluğu × form kutusu. Her akış kendi
/// başına yürünür: GölgeDosya belge içeriğini `q … Q` ile sarmaladığı için
/// işaret akışı temiz grafik durumundan başlar.
fn brand_draws_on_page(
    doc: &LopdfDoc,
    page_id: lopdf::ObjectId,
    forms: &std::collections::HashSet<lopdf::ObjectId>,
) -> Option<(Vec<BrandDraw>, f32)> {
    let page = super::resolved_page_dictionary(doc, page_id).ok()?;
    let visible = visible_box(doc, &page)?;
    let short_side = (visible[2] - visible[0]).min(visible[3] - visible[1]);
    let names = brand_names(doc, &page, forms);
    if names.is_empty() {
        return Some((Vec::new(), short_side));
    }
    let number = |o: &Object| o.as_float().ok();
    let mut draws = Vec::new();
    for item in content_items(doc, &page) {
        let Some(operations) = stream_operations(doc, &item) else {
            continue;
        };
        // [a b c d e f]; nokta: x' = a·x + c·y + e, y' = b·x + d·y + f.
        let mut ctm = [1f32, 0., 0., 1., 0., 0.];
        let mut saved = Vec::new();
        for op in &operations {
            match op.operator.as_str() {
                "q" => saved.push(ctm),
                "Q" => ctm = saved.pop().unwrap_or([1., 0., 0., 1., 0., 0.]),
                "cm" if op.operands.len() == 6 => {
                    let m: Vec<f32> = op.operands.iter().filter_map(number).collect();
                    if m.len() == 6 {
                        ctm = [
                            m[0] * ctm[0] + m[1] * ctm[2],
                            m[0] * ctm[1] + m[1] * ctm[3],
                            m[2] * ctm[0] + m[3] * ctm[2],
                            m[2] * ctm[1] + m[3] * ctm[3],
                            m[4] * ctm[0] + m[5] * ctm[2] + ctm[4],
                            m[4] * ctm[1] + m[5] * ctm[3] + ctm[5],
                        ];
                    }
                }
                "Do" if op
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .is_some_and(|n| names.iter().any(|m| m == n)) =>
                {
                    let corners = [
                        (BRAND_BBOX[0], BRAND_BBOX[1]),
                        (BRAND_BBOX[2], BRAND_BBOX[1]),
                        (BRAND_BBOX[0], BRAND_BBOX[3]),
                        (BRAND_BBOX[2], BRAND_BBOX[3]),
                    ];
                    const TOLERANCE: f32 = 0.5;
                    let inside = corners.iter().all(|(x, y)| {
                        let (ux, uy) = (
                            ctm[0] * x + ctm[2] * y + ctm[4],
                            ctm[1] * x + ctm[3] * y + ctm[5],
                        );
                        ux >= visible[0] - TOLERANCE
                            && ux <= visible[2] + TOLERANCE
                            && uy >= visible[1] - TOLERANCE
                            && uy <= visible[3] + TOLERANCE
                    });
                    let width = (ctm[0] * ctm[0] + ctm[1] * ctm[1]).sqrt()
                        * (BRAND_BBOX[2] - BRAND_BBOX[0]);
                    draws.push(BrandDraw { inside, width });
                }
                _ => {}
            }
        }
    }
    Some((draws, short_side))
}

/// Sayfa kuralı sağlıyor mu: tam BİR görünür işaret ve okunur boyutta.
///
/// Görünür alanın dışında kalmış (ör. sonradan kırpılmış) bir çizim sayılmaz:
/// kullanıcı onu görmez. Görünür iki işaret, okunmaz tek işaret ya da hiç
/// işaret kuralı bozar.
fn page_mark_is_canonical(
    doc: &LopdfDoc,
    page_id: lopdf::ObjectId,
    forms: &std::collections::HashSet<lopdf::ObjectId>,
) -> bool {
    brand_draws_on_page(doc, page_id, forms).is_some_and(|(draws, short_side)| {
        let mut visible = draws.iter().filter(|d| d.inside);
        match (visible.next(), visible.next()) {
            (Some(only), None) => only.width >= MIN_BRAND_MARK_RATIO * short_side,
            _ => false,
        }
    })
}

/// GölgeDosya'nın bu sayfaya daha önce yazdığı işaret akışlarını söker.
///
/// Yalnız GölgeDosya'nın yazdığı BİREBİR kalıp sökülür: kendi başına bir
/// akış, içeriği tam olarak `q cm cm Do Q`, `Do` bir marka formuna. Bu kalıp
/// DüzenEk döneminden bugüne değişmedi (sahadaki eski çıktılarda ölçüldü).
/// Belge içeriği hiçbir koşulda değişmez; kalıba uymayan bir çizim yerinde
/// bırakılır ve `verify_canonical_branding` onu yakalar.
///
/// Sayfa kutularına DOKUNULMAZ. Eskiden işaret en dış sarmalsa sarmal da
/// açılıyor ve CropBox işaretlenmeden önceki hâline geri yazılıyordu (yeni
/// işaret eski payın altına ikinci bir pay eklemesin diye). Marka artık pay
/// eklemediği için bu gerekmez; v0.3.x çıktısının payı girdinin kendi
/// geometrisidir ve aynen korunur. Sarmal (`q … re W n … Q`) da yerinde
/// kalır: içeriği o gün görünen alana kırpar, kaldırılırsa paya taşan
/// kaynak çizimi görünür hâle gelebilirdi.
fn detach_brand_stamps(
    doc: &mut LopdfDoc,
    page_id: lopdf::ObjectId,
    forms: &std::collections::HashSet<lopdf::ObjectId>,
) -> Result<()> {
    let page = super::resolved_page_dictionary(doc, page_id)?;
    let names = brand_names(doc, &page, forms);
    if names.is_empty() {
        return Ok(());
    }
    let items = content_items(doc, &page);
    let is_stamp = |ops: &[Operation]| match ops {
        [q, a, b, draw, end] => {
            q.operator == "q"
                && a.operator == "cm"
                && a.operands.len() == 6
                && b.operator == "cm"
                && b.operands.len() == 6
                && draw.operator == "Do"
                && draw
                    .operands
                    .first()
                    .and_then(|n| n.as_name().ok())
                    .is_some_and(|n| names.iter().any(|m| m == n))
                && end.operator == "Q"
        }
        _ => false,
    };
    let before = items.len();
    let kept: Vec<Object> = items
        .into_iter()
        .filter(|item| !stream_operations(doc, item).is_some_and(|ops| is_stamp(&ops)))
        .collect();
    if kept.len() == before {
        return Ok(());
    }
    doc.get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(pdf_error)?
        .set("Contents", kept);
    Ok(())
}

/// Ürün kuralının doğrulaması: her sayfada tam bir görünür GölgeDosya işareti,
/// okunur boyutta; eski dönem çizimi yok. Yayımdan önceki son söz budur.
pub fn verify_canonical_branding(doc: &LopdfDoc) -> Result<()> {
    let outdated = doc.objects.values().any(|obj| {
        obj.as_stream().is_ok_and(|s| {
            s.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() == Some(b"Form")
                && stream_data(s).is_some_and(|d| OUTDATED_BRAND_LOGOS.iter().any(|o| d == *o))
        })
    });
    if outdated {
        return Err(EklerError::ValidationFailed(
            "Çıktıda eski bir ürün işareti kaldı; güvenlik için yayımlanmadı".into(),
        ));
    }
    let forms = existing_brand_forms(doc);
    for (number, id) in doc.get_pages() {
        if !page_mark_is_canonical(doc, id, &forms) {
            return Err(EklerError::ValidationFailed(format!(
                "Sayfa {number}: çıktı GölgeDosya işaretini doğru taşımıyor; güvenlik için yayımlanmadı"
            )));
        }
    }
    Ok(())
}

/// GölgeDosya'nın ÜRETTİĞİ ya da DEĞİŞTİRDİĞİ bir PDF'in yayına hazır hâli.
///
/// Yalnız `finalize_pdf_output` üretir: işaretlenmiş, yeniden açılıp
/// doğrulanmış baytlar. `ekler_core::safe_io::publish_pdf` yalnız bunu kabul
/// eder ve genel yazıcı ham PDF baytını reddeder. İşaret böylece araç
/// geliştiricisinin hatırlamasına bağlı değildir: unutulursa yayım olmaz.
#[derive(Debug)]
pub struct BrandedPdf {
    bytes: Vec<u8>,
}

impl BrandedPdf {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Kullanıcıya teslim edilecek her PDF'in TEK kapısı: işaretle → serileştir →
/// yeniden aç → yapı, sayfa sayısı ve işaret doğrulaması.
///
/// İdempotenttir: zaten kurala uyan bir belgeye ikinci işaret eklemez.
pub fn finalize_pdf_output(doc: &mut LopdfDoc) -> Result<BrandedPdf> {
    let expected = doc.get_pages().len();
    if expected == 0 {
        return Err(EklerError::InvalidPdf(
            "PDF en az bir sayfa içermeli".into(),
        ));
    }
    apply_branding(doc)?;
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
    let reopened = LopdfDoc::load_mem(&bytes).map_err(pdf_error)?;
    super::validate_document(&reopened)?;
    if reopened.get_pages().len() != expected {
        return Err(EklerError::InvalidPdf("Sayfa sayısı değişti".into()));
    }
    verify_canonical_branding(&reopened)?;
    Ok(BrandedPdf { bytes })
}
