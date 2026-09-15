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
            apply_stamp_to_page(doc, page_id, &stamp_text, font_id, config, None)?;
        }
    }

    Ok(())
}

struct BrandResources {
    form: lopdf::ObjectId,
    streams: std::collections::HashMap<Vec<u8>, lopdf::ObjectId>,
    raster_dpi: Option<u32>,
}
fn add_mark_stream(
    doc: &mut LopdfDoc,
    content: Vec<u8>,
    brand: &mut Option<&mut BrandResources>,
) -> lopdf::ObjectId {
    if let Some(resources) = brand.as_deref_mut() {
        if let Some(id) = resources.streams.get(&content) {
            return *id;
        }
        let id = doc.add_object(Stream::new(Dictionary::new(), content.clone()));
        resources.streams.insert(content, id);
        id
    } else {
        doc.add_object(Stream::new(Dictionary::new(), content))
    }
}

fn apply_stamp_to_page(
    doc: &mut LopdfDoc,
    page_id: lopdf::ObjectId,
    stamp_text: &str,
    font_id: lopdf::ObjectId,
    config: &StampConfig,
    mut brand: Option<&mut BrandResources>,
) -> Result<()> {
    let brand_form = brand.as_ref().map(|b| b.form);
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
    let (mut x0, mut y0, mut x1, mut y1) = (number(0)?, number(1)?, number(2)?, number(3)?);
    let rotation = resolved
        .get(b"Rotate")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    // Spec /Rotate'in 90'ın katı olmasını şart koşar. Uymayan üretici
    // değerinde pdf.js sayfayı döndürülmemiş sayar; belgeyi reddetmek yerine
    // aynısı yapılır. Sayfanın kendi /Rotate değerine dokunulmaz, yalnız
    // damganın yerleşimi bu kuralla hesaplanır.
    let rotation = if rotation % 90 == 0 { rotation } else { 0 };
    let original_box = (x0, y0, x1, y1);

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
    let expanded_media = vec![
        media[0].as_float().map_err(pdf_error)?.min(x0).into(),
        media[1].as_float().map_err(pdf_error)?.min(y0).into(),
        media[2].as_float().map_err(pdf_error)?.max(x1).into(),
        media[3].as_float().map_err(pdf_error)?.max(y1).into(),
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
    let (page_w, page_h) = if rotation == 90 || rotation == 270 {
        (y1 - y0, x1 - x0)
    } else {
        (x1 - x0, y1 - y0)
    };
    let matrix: [f32; 6] = match rotation {
        0 => [1., 0., 0., 1., x0, y0],
        90 => [0., 1., -1., 0., x1, y0],
        180 => [-1., 0., 0., -1., x1, y1],
        270 => [0., -1., 1., 0., x0, y1],
        _ => return Err(EklerError::InvalidPdf("Geçersiz sayfa dönüşü".into())),
    };
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
    let mut font_name = "DuzenEkStamp".to_string();
    while fonts.has(font_name.as_bytes()) {
        font_name.push('_');
    }
    if brand_form.is_none() {
        fonts.set(font_name.clone(), font_id);
        resources.set("Font", fonts);
    }
    doc.get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(pdf_error)?
        .set("Resources", resources);

    let badge_w = if brand_form.is_some() {
        56.0
    } else {
        (stamp_text.len() as f32 * config.font_size * 0.65 + 16.0).max(40.0)
    };
    let badge_h = config.font_size + 10.0;
    if page_w < badge_w + config.margin_pt * 2.0 || page_h < badge_h + config.margin_pt * 2.0 {
        return Err(EklerError::InvalidPdf(
            "Damga bu sayfaya sığmıyor; boyut/kenar boşluğunu azaltın".into(),
        ));
    }
    let margin = config.margin_pt;

    let (badge_x, badge_y, text_x, text_y) = match config.position {
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
        Operation::new("Tj", vec![Object::string_literal(stamp_text)]),
        Operation::new("ET", vec![]),
        Operation::new("Q", vec![]),
    ]);

    if let Some(form_id) = brand_form {
        let mark_width = if let Some(dpi) = brand.as_ref().and_then(|b| b.raster_dpi) {
            (page_w * dpi as f32 / 72. * 0.094).clamp(32., 240.) * 72. / dpi as f32
        } else {
            56.
        };
        let mark_width = mark_width
            .min((page_w - margin * 2.).max(1.))
            .min((gutter - margin * 2.).max(1.) * 290. / 72.);
        let page = doc.get_dictionary_mut(page_id).map_err(pdf_error)?;
        let resources = page
            .get_mut(b"Resources")
            .and_then(|o| o.as_dict_mut())
            .map_err(pdf_error)?;
        let mut objects = Dictionary::new();
        // Existing XObjects may be indirect. Add a unique top-level binding by resolving below.
        if let Ok(Object::Dictionary(existing)) = resources.get(b"XObject") {
            objects = existing.clone();
        }
        // The resolved resource graph was copied above; preserve indirect dictionaries too.
        let existing = resources.get(b"XObject").ok().cloned();
        if let Some(Object::Reference(id)) = existing {
            objects = doc.get_dictionary(id).map_err(pdf_error)?.clone();
        }
        let mut name = "DuzenEkBrand".to_string();
        while objects.has(name.as_bytes()) {
            name.push('_');
        }
        objects.set(name.clone(), form_id);
        doc.get_dictionary_mut(page_id)
            .map_err(pdf_error)?
            .get_mut(b"Resources")
            .and_then(|o| o.as_dict_mut())
            .map_err(pdf_error)?
            .set("XObject", objects);
        ops = vec![
            Operation::new("q", vec![]),
            Operation::new("cm", matrix.into_iter().map(Object::Real).collect()),
            Operation::new(
                "cm",
                vec![
                    (mark_width / 290.0).into(),
                    0.into(),
                    0.into(),
                    (mark_width / 290.0).into(),
                    (page_w - margin - mark_width).into(),
                    margin.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(name.into_bytes())]),
            Operation::new("Q", vec![]),
        ];
    }
    let stamp_content = Content { operations: ops };
    let stamp_stream_id = add_mark_stream(
        doc,
        stamp_content
            .encode()
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?,
        &mut brand,
    );
    // Reuse identical wrappers as well as the vector asset; source streams stay intact.
    let (ox0, oy0, ox1, oy1) = original_box;
    let prefix = add_mark_stream(
        doc,
        format!("q\n{ox0} {oy0} {} {} re W n\n", ox1 - ox0, oy1 - oy0).into_bytes(),
        &mut brand,
    );
    let suffix = add_mark_stream(doc, b"\nQ\n".to_vec(), &mut brand);
    let old = resolved.get(b"Contents").ok().cloned();
    let mut contents = vec![Object::Reference(prefix)];
    if let Some(old) = old {
        match old {
            Object::Array(items) => contents.extend(items),
            other => contents.push(other),
        }
    }
    contents.extend([
        Object::Reference(suffix),
        Object::Reference(stamp_stream_id),
    ]);
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

pub fn apply_text_marks(doc: &mut LopdfDoc, text: Option<&str>, start: usize) -> Result<()> {
    let font=doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica-Bold","Encoding"=>"WinAnsiEncoding"});
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
        apply_stamp_to_page(doc, id, &mark, font, &config, None)?;
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
const BRAND_FORM_CONTENT: &[u8] = b"q /BrandAlpha gs /Mark Do Q";
const BRAND_BBOX: [f32; 4] = [0., 0., 290., 72.];

fn apply_branding_for_output(doc: &mut LopdfDoc, raster_dpi: Option<u32>) -> Result<()> {
    let config = StampConfig {
        enabled: true,
        position: StampPosition::BottomRight,
        font_size: 8.0,
        margin_pt: 8.0,
        show_badge: false,
    };
    // Marka idempotenttir. Daha önce GölgeDosya'nın türettiği bir belgede
    // işareti görünür alanda duran sayfaya ikinci pay ve ikinci logo eklenmez;
    // yeni işaret gerekirse var olan marka formu yeniden kullanılır. Eskiden
    // türetilmiş kopyadan türetilen her kopya sayfayı 34 pt büyütüp bir logo
    // daha ekliyordu (sırala → döndür → sil: iki kat pay, üç logo, altı form).
    // Kırpma işareti görünür alanın dışında bıraktıysa sayfa yeniden işaretlenir.
    let existing = existing_brand_forms(doc);
    let pending: Vec<lopdf::ObjectId> = doc
        .get_pages()
        .into_values()
        .filter(|page| !brand_visible_on_page(doc, *page, &existing))
        .collect();
    if pending.is_empty() {
        return Ok(());
    }
    let form_id = match existing.iter().min() {
        Some(form) => *form,
        None => {
            // One shared, translucent vector form; no raster or unused font per page.
            // Isolate the vector as a transparency group: opacity applies once to the
            // complete mark, not repeatedly where the logo's paths overlap.
            let mut vector = Stream::new(
                dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>BRAND_BBOX.iter().map(|v| Object::Integer(*v as i64)).collect::<Vec<_>>(),
                "Group"=>dictionary! {"S"=>"Transparency","I"=>true,"CS"=>"DeviceRGB"}, "Resources"=>Dictionary::new()},
                BRAND_LOGO.to_vec(),
            );
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
        apply_stamp_to_page(doc, id, "DuzenEk", (0, 0), &config, Some(&mut resources))?;
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

/// Sayfa bir marka formunu çiziyor ve işaret sayfanın görünür kutusunun
/// içinde mi? İşaretin kutusu, çizildiği akıştaki `cm` dönüşümlerinden
/// hesaplanır.
fn brand_visible_on_page(
    doc: &LopdfDoc,
    page_id: lopdf::ObjectId,
    forms: &std::collections::HashSet<lopdf::ObjectId>,
) -> bool {
    if forms.is_empty() {
        return false;
    }
    let Ok(page) = super::resolved_page_dictionary(doc, page_id) else {
        return false;
    };
    let Some(xobjects) = dictionary_at(doc, page.get(b"Resources").ok())
        .and_then(|r| dictionary_at(doc, r.get(b"XObject").ok()))
    else {
        return false;
    };
    let names: Vec<&[u8]> = xobjects
        .iter()
        .filter(|(_, v)| v.as_reference().is_ok_and(|id| forms.contains(&id)))
        .map(|(k, _)| k.as_slice())
        .collect();
    if names.is_empty() {
        return false;
    }
    let number = |o: &Object| o.as_float().ok();
    let Some(visible) = page
        .get(b"CropBox")
        .or_else(|_| page.get(b"MediaBox"))
        .ok()
        .and_then(|b| doc.dereference(b).ok())
        .and_then(|(_, b)| b.as_array().ok())
        .filter(|b| b.len() == 4)
        .and_then(|b| {
            Some([
                number(&b[0])?,
                number(&b[1])?,
                number(&b[2])?,
                number(&b[3])?,
            ])
        })
    else {
        return false;
    };
    let (vx0, vx1) = (visible[0].min(visible[2]), visible[0].max(visible[2]));
    let (vy0, vy1) = (visible[1].min(visible[3]), visible[1].max(visible[3]));
    let streams: Vec<Object> = match page.get(b"Contents") {
        Ok(Object::Array(items)) => items.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(items)) => items.clone(),
            _ => vec![Object::Reference(*id)],
        },
        _ => Vec::new(),
    };
    for item in streams {
        let Some(data) = doc
            .dereference(&item)
            .ok()
            .and_then(|(_, s)| s.as_stream().ok())
            .and_then(stream_data)
        else {
            continue;
        };
        let Ok(content) = Content::decode(&data) else {
            continue;
        };
        // [a b c d e f]; nokta: x' = a·x + c·y + e, y' = b·x + d·y + f.
        let mut ctm = [1f32, 0., 0., 1., 0., 0.];
        let mut saved = Vec::new();
        for op in &content.operations {
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
                    .is_some_and(|n| names.contains(&n)) =>
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
                        ux >= vx0 - TOLERANCE
                            && ux <= vx1 + TOLERANCE
                            && uy >= vy0 - TOLERANCE
                            && uy <= vy1 + TOLERANCE
                    });
                    if inside {
                        return true;
                    }
                }
                _ => {}
            }
        }
    }
    false
}
