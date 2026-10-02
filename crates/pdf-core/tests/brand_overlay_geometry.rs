//! Ürün kuralı: GölgeDosya markası belge GEOMETRİSİNİ DEĞİŞTİRMEZ.
//!
//! Saha: "Hem yapıyı da bozuyor … Büyütmesin aşağıya doğru evrakı." Her PDF
//! çıktısı işareti sayfanın görsel alt kenarına eklenen 34 pt'lik bir payda
//! taşıyordu: A4 595×842 pt → 595×876 pt, döndürülmüş sayfada x ekseni 629 pt.
//!
//! Burada tek kapı (`finalize_pdf_output`) gerçek fixture'larla koşulur ve
//! önce/sonra karşılaştırılır: MediaBox, CropBox, /Rotate, /Annots, sayfa
//! sayısı ve sırası BİREBİR aynı; kaynak içerik akışları bayt bayt aynı ve
//! aynı sırada, yalnız `q … Q` sarmalı ve en üstteki işaret akışı eklenir.
//! İşaret görünür kutunun içinde, görüntülenen sayfanın sağ alt köşesinde,
//! dik ve tam BİR tanedir. Sayım üretim kodundan bağımsızdır.
use lopdf::content::Content;
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use pdf_core::pdf::stamp::{
    apply_stamp_to_document, apply_text_marks, finalize_pdf_output, StampConfig, StampPosition,
};

const BRAND_FORM: &[u8] = b"q /BrandAlpha gs /Mark Do Q";
/// Standart sayfada işaret genişliği ve kenar boşluğu (pt).
const MARK_W: f32 = 56.0;
const INSET: f32 = 12.0;

// ------------------------------------------------------------------ girdiler

#[derive(Clone)]
struct Spec {
    media: [f32; 4],
    crop: Option<[f32; 4]>,
    rotate: Option<i64>,
    annot: bool,
}

fn spec(media: [f32; 4]) -> Spec {
    Spec {
        media,
        crop: None,
        rotate: None,
        annot: false,
    }
}

fn real(v: &[f32]) -> Vec<Object> {
    v.iter().map(|n| Object::Real(*n)).collect()
}

/// Sayfa başına iki kaynak akışı (biri dolaylı dizi değil, düz başvurular):
/// koyu bir çerçeve ve bir metin bloğu.
fn build(specs: &[Spec]) -> Document {
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let mut kids = Vec::new();
    for (i, s) in specs.iter().enumerate() {
        let v = s.crop.unwrap_or(s.media);
        let a = d.add_object(Stream::new(
            dictionary! {},
            format!(
                "0 0 0 RG 1 w {} {} {} {} re S",
                v[0].min(v[2]) + 4.,
                v[1].min(v[3]) + 1.,
                (v[2] - v[0]).abs() - 8.,
                (v[3] - v[1]).abs() - 2.
            )
            .into_bytes(),
        ));
        let b = d.add_object(Stream::new(
            dictionary! {},
            format!("0.2 g {} {} {} 1 re f", v[0] + 10., v[1] + 1., 20 + i).into_bytes(),
        ));
        let mut page = dictionary! {"Type"=>"Page","Parent"=>pages_id,
        "MediaBox"=>real(&s.media),
        "Resources"=>dictionary!{},
        "Contents"=>vec![a.into(), b.into()]};
        if let Some(c) = s.crop {
            page.set("CropBox", real(&c));
        }
        if let Some(r) = s.rotate {
            page.set("Rotate", r);
        }
        if s.annot {
            // Görünür kutunun ALTINDA duran, görünür bir bağlantı. Eski pay bu
            // alanı açacağı için marka işlemi reddediyordu.
            let annot = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Square",
            "Rect"=>real(&[v[0] + 20., v[1] - 30., v[0] + 120., v[1] - 5.]),
            "C"=>real(&[1., 0., 0.])});
            page.set("Annots", vec![annot.into()]);
        }
        kids.push(d.add_object(page).into());
    }
    let n = kids.len() as i64;
    d.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>n}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    d.trailer.set("Root", root);
    d
}

fn finalize(doc: &Document) -> Document {
    let mut work = doc.clone();
    let pdf = finalize_pdf_output(&mut work).unwrap_or_else(|e| panic!("finalize: {e}"));
    Document::load_mem(pdf.bytes()).unwrap()
}

// ------------------------------------------------------------------ ölçüm

fn data(doc: &Document, o: &Object) -> Vec<u8> {
    let (_, o) = doc.dereference(o).unwrap();
    let s = o.as_stream().unwrap();
    if s.dict.has(b"Filter") {
        s.decompressed_content().unwrap()
    } else {
        s.content.clone()
    }
}

/// Sayfa sözlüğünün geometri anahtarları, yazıldıkları HAM sırayla (köşe
/// sırası normalize EDİLMEDEN) ve sayısal değerleriyle.
#[derive(Debug, PartialEq)]
struct Geometry {
    media: Option<Vec<f32>>,
    crop: Option<Vec<f32>>,
    rotate: Option<i64>,
    annots: Option<Object>,
}

fn geometry(doc: &Document, page: ObjectId) -> Geometry {
    let d = doc.get_dictionary(page).unwrap();
    let get = |k: &[u8]| d.get(k).ok().map(|o| doc.dereference(o).unwrap().1.clone());
    let numbers = |o: Object| -> Vec<f32> {
        o.as_array()
            .unwrap()
            .iter()
            .map(|n| number(doc.dereference(n).unwrap().1))
            .collect()
    };
    Geometry {
        media: get(b"MediaBox").map(numbers),
        crop: get(b"CropBox").map(numbers),
        rotate: get(b"Rotate").map(|r| r.as_i64().unwrap()),
        annots: get(b"Annots"),
    }
}

fn contents(doc: &Document, page: ObjectId) -> Vec<Vec<u8>> {
    let d = doc.get_dictionary(page).unwrap();
    let c = d.get(b"Contents").unwrap();
    let items = match doc.dereference(c).unwrap().1 {
        Object::Array(a) => a.clone(),
        _ => vec![c.clone()],
    };
    items.iter().map(|i| data(doc, i)).collect()
}

fn number(o: &Object) -> f32 {
    o.as_float().unwrap()
}

fn norm(v: &[f32]) -> [f32; 4] {
    [
        v[0].min(v[2]),
        v[1].min(v[3]),
        v[0].max(v[2]),
        v[1].max(v[3]),
    ]
}

fn page_box(doc: &Document, page: ObjectId, key: &[u8]) -> Option<[f32; 4]> {
    let mut d = doc.get_dictionary(page).unwrap();
    loop {
        if let Ok(b) = d.get(key) {
            let a = doc.dereference(b).unwrap().1.as_array().unwrap().clone();
            let v: Vec<f32> = a
                .iter()
                .map(|o| number(doc.dereference(o).unwrap().1))
                .collect();
            return Some(norm(&v));
        }
        let parent = d.get(b"Parent").ok()?.as_reference().ok()?;
        d = doc.get_dictionary(parent).ok()?;
    }
}

/// CropBox ∩ MediaBox.
fn visible(doc: &Document, page: ObjectId) -> [f32; 4] {
    let m = page_box(doc, page, b"MediaBox").unwrap();
    match page_box(doc, page, b"CropBox") {
        None => m,
        Some(c) => [
            c[0].max(m[0]),
            c[1].max(m[1]),
            c[2].min(m[2]),
            c[3].min(m[3]),
        ],
    }
}

fn rotation(doc: &Document, page: ObjectId) -> i64 {
    let r = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    if r % 90 == 0 {
        r
    } else {
        0
    }
}

/// Bir marka çizimi, GÖRÜNTÜLENEN sayfa uzayında (sol alt köşe orijin, y
/// yukarı): kutu ve formun x ekseninin ekrandaki yönü.
#[derive(Debug)]
struct Mark {
    /// [x0 y0 x1 y1], görüntülenen sayfada.
    rect: [f32; 4],
    /// Formun +x ekseninin görüntüdeki birim yönü.
    axis: (f32, f32),
    width: f32,
}

/// Kullanıcı uzayı noktasını görüntülenen sayfa uzayına çevirir (/Rotate
/// saat yönünde, ISO 32000-1 §7.7.3.3).
fn to_display(p: (f32, f32), v: [f32; 4], rot: i64) -> (f32, f32) {
    let (x, y) = p;
    match rot {
        0 => (x - v[0], y - v[1]),
        90 => (y - v[1], v[2] - x),
        180 => (v[2] - x, v[3] - y),
        270 => (v[3] - y, x - v[0]),
        _ => unreachable!(),
    }
}

fn brand_forms(doc: &Document) -> Vec<ObjectId> {
    doc.objects
        .iter()
        .filter_map(|(id, o)| {
            let s = o.as_stream().ok()?;
            let d = if s.dict.has(b"Filter") {
                s.decompressed_content().ok()?
            } else {
                s.content.clone()
            };
            (d == BRAND_FORM).then_some(*id)
        })
        .collect()
}

fn marks(doc: &Document, page: ObjectId) -> Vec<Mark> {
    let forms = brand_forms(doc);
    let d = doc.get_dictionary(page).unwrap();
    let res = d
        .get(b"Resources")
        .ok()
        .map(|r| doc.dereference(r).unwrap().1.as_dict().unwrap().clone())
        .unwrap_or_default();
    let names: Vec<Vec<u8>> = res
        .get(b"XObject")
        .ok()
        .map(|x| doc.dereference(x).unwrap().1.as_dict().unwrap().clone())
        .unwrap_or_default()
        .iter()
        .filter(|(_, v)| v.as_reference().is_ok_and(|id| forms.contains(&id)))
        .map(|(k, _)| k.clone())
        .collect();
    let mut joined = Vec::new();
    for c in contents(doc, page) {
        joined.extend(c);
        joined.push(b'\n');
    }
    let v = visible(doc, page);
    let rot = rotation(doc, page);
    let mut ctm = [1f32, 0., 0., 1., 0., 0.];
    let mut stack = Vec::new();
    let mut out = Vec::new();
    for op in Content::decode(&joined).unwrap().operations {
        match op.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => ctm = stack.pop().unwrap_or([1., 0., 0., 1., 0., 0.]),
            "cm" => {
                let m: Vec<f32> = op.operands.iter().map(number).collect();
                ctm = [
                    m[0] * ctm[0] + m[1] * ctm[2],
                    m[0] * ctm[1] + m[1] * ctm[3],
                    m[2] * ctm[0] + m[3] * ctm[2],
                    m[2] * ctm[1] + m[3] * ctm[3],
                    m[4] * ctm[0] + m[5] * ctm[2] + ctm[4],
                    m[4] * ctm[1] + m[5] * ctm[3] + ctm[5],
                ];
            }
            "Do" if names.iter().any(|n| op.operands[0].as_name().unwrap() == n) => {
                let user = |x: f32, y: f32| {
                    (
                        ctm[0] * x + ctm[2] * y + ctm[4],
                        ctm[1] * x + ctm[3] * y + ctm[5],
                    )
                };
                let pts = [(0., 0.), (290., 0.), (0., 72.), (290., 72.)]
                    .map(|(x, y)| to_display(user(x, y), v, rot));
                let o = to_display(user(0., 0.), v, rot);
                let e = to_display(user(1., 0.), v, rot);
                let len = ((e.0 - o.0).powi(2) + (e.1 - o.1).powi(2)).sqrt();
                out.push(Mark {
                    rect: [
                        pts.iter().map(|p| p.0).fold(f32::MAX, f32::min),
                        pts.iter().map(|p| p.1).fold(f32::MAX, f32::min),
                        pts.iter().map(|p| p.0).fold(f32::MIN, f32::max),
                        pts.iter().map(|p| p.1).fold(f32::MIN, f32::max),
                    ],
                    axis: ((e.0 - o.0) / len, (e.1 - o.1) / len),
                    width: len * 290.,
                });
            }
            _ => {}
        }
    }
    out
}

/// Görüntülenen sayfanın boyutu (pt).
fn display_size(doc: &Document, page: ObjectId) -> (f32, f32) {
    let v = visible(doc, page);
    let (w, h) = (v[2] - v[0], v[3] - v[1]);
    if matches!(rotation(doc, page), 90 | 270) {
        (h, w)
    } else {
        (w, h)
    }
}

// ------------------------------------------------------------------ sözleşme

/// Kuralın kendisi: önce/sonra geometri aynı, kaynak akışlar aynı ve
/// sarmalanmış, tam bir işaret görüntünün sağ alt köşesinde ve dik.
fn assert_overlay_contract(before: &Document, after: &Document, label: &str) {
    let (bp, ap) = (before.get_pages(), after.get_pages());
    assert_eq!(bp.len(), ap.len(), "{label}: sayfa sayısı değişti");
    for ((n, b), a) in bp.iter().zip(ap.values()) {
        let at = format!("{label} sayfa {n}");
        assert_eq!(
            geometry(before, *b),
            geometry(after, *a),
            "{at}: sayfa geometrisi değişti"
        );
        assert_eq!(
            display_size(before, *b),
            display_size(after, *a),
            "{at}: görüntülenen boyut değişti"
        );
        // Kaynak akışlar bayt bayt aynı ve aynı sırada; önde `q … re W n`,
        // arkada `Q` ve en sonda işaret akışı.
        let src = contents(before, *b);
        let out = contents(after, *a);
        assert_eq!(out.len(), src.len() + 3, "{at}: içerik akışı sayısı");
        assert_eq!(
            &out[1..out.len() - 2],
            &src[..],
            "{at}: kaynak içerik değişti"
        );
        let prefix = Content::decode(&out[0]).unwrap().operations;
        let ops: Vec<&str> = prefix.iter().map(|o| o.operator.as_str()).collect();
        assert_eq!(ops, ["q", "re", "W", "n"], "{at}: sarmal başı");
        let v = visible(after, *a);
        let clip: Vec<f32> = prefix[1].operands.iter().map(number).collect();
        let clip = norm(&[clip[0], clip[1], clip[0] + clip[2], clip[1] + clip[3]]);
        for i in 0..4 {
            assert!(
                (clip[i] - v[i]).abs() < 0.01,
                "{at}: sarmal görünür kutudan farklı kırpıyor: {clip:?} / {v:?}"
            );
        }
        let suffix = Content::decode(&out[out.len() - 2]).unwrap().operations;
        assert_eq!(suffix.len(), 1);
        assert_eq!(suffix[0].operator, "Q");

        let m = marks(after, *a);
        assert_eq!(m.len(), 1, "{at}: {} işaret", m.len());
        let m = &m[0];
        let (w, h) = display_size(after, *a);
        let inset = m.width * INSET / MARK_W;
        assert!(
            m.rect[0] >= -0.01
                && m.rect[1] >= -0.01
                && m.rect[2] <= w + 0.01
                && m.rect[3] <= h + 0.01,
            "{at}: işaret görünür kutunun dışında: {:?} / {w}x{h}",
            m.rect
        );
        assert!(
            (w - m.rect[2] - inset).abs() < 0.05 && (m.rect[1] - inset).abs() < 0.05,
            "{at}: işaret görüntülenen sağ alt köşede değil: {:?}, kenar {inset}",
            m.rect
        );
        assert!(
            (m.axis.0 - 1.).abs() < 1e-4 && m.axis.1.abs() < 1e-4,
            "{at}: işaret dik değil (eksen {:?})",
            m.axis
        );
        assert!(
            m.width >= 0.08 * w.min(h),
            "{at}: işaret okunmaz: {} pt",
            m.width
        );
    }
}

#[test]
fn branding_never_changes_page_geometry() {
    let a4 = [0., 0., 595., 842.];
    let cases: Vec<(&str, Vec<Spec>)> = vec![
        ("A4 dikey", vec![spec(a4)]),
        ("A4 yatay", vec![spec([0., 0., 842., 595.])]),
        (
            "A4 /Rotate 90",
            vec![Spec {
                rotate: Some(90),
                ..spec(a4)
            }],
        ),
        (
            "A4 /Rotate 270",
            vec![Spec {
                rotate: Some(270),
                ..spec(a4)
            }],
        ),
        (
            "A4 /Rotate 180",
            vec![Spec {
                rotate: Some(180),
                ..spec(a4)
            }],
        ),
        (
            "A4 /Rotate -90",
            vec![Spec {
                rotate: Some(-90),
                ..spec(a4)
            }],
        ),
        (
            "Letter CropBox ≠ MediaBox",
            vec![Spec {
                crop: Some([36., 36., 576., 756.]),
                ..spec([0., 0., 612., 792.])
            }],
        ),
        (
            "CropBox MediaBox'tan taşıyor",
            vec![Spec {
                crop: Some([-20., -20., 615., 700.]),
                ..spec(a4)
            }],
        ),
        ("ters köşeli kutu", vec![spec([595., 842., 0., 0.])]),
        (
            "kaydırılmış kutu, /Rotate 90",
            vec![Spec {
                crop: Some([100., 200., 500., 700.]),
                rotate: Some(90),
                ..spec([50., 50., 600., 900.])
            }],
        ),
        ("çok kısa şerit 595×18", vec![spec([0., 0., 595., 18.])]),
        ("çok kısa şerit 842×12", vec![spec([0., 0., 842., 12.])]),
        ("dar sütun 40×600", vec![spec([0., 0., 40., 600.])]),
        (
            "telefon taraması 1414×2068",
            vec![spec([0., 0., 1414., 2068.])],
        ),
        (
            "çok sayfalı karışık",
            vec![
                spec(a4),
                spec([0., 0., 842., 595.]),
                Spec {
                    crop: Some([36., 36., 576., 756.]),
                    rotate: Some(90),
                    ..spec([0., 0., 612., 792.])
                },
                Spec {
                    rotate: Some(270),
                    ..spec([0., 0., 1414., 2068.])
                },
            ],
        ),
    ];
    for (label, specs) in cases {
        let before = build(&specs);
        let after = finalize(&before);
        assert_overlay_contract(&before, &after, label);
    }
}

/// Standart sayfada kabul edilmiş işaret: 56 pt genişlik, sağdan ve alttan
/// 12 pt (yazıcının basamadığı kenarın içinde).
#[test]
fn the_standard_mark_is_56_pt_and_12_pt_inside_the_corner() {
    for media in [
        [0., 0., 595., 842.],
        [0., 0., 842., 595.],
        [0., 0., 612., 792.],
    ] {
        let before = build(&[spec(media)]);
        let after = finalize(&before);
        let page = *after.get_pages().values().next().unwrap();
        let m = &marks(&after, page)[0];
        let (w, _) = display_size(&after, page);
        assert!((m.width - MARK_W).abs() < 0.01, "{media:?}: {} pt", m.width);
        assert!(
            (w - m.rect[2] - INSET).abs() < 0.01,
            "{media:?}: sağ boşluk"
        );
        assert!((m.rect[1] - INSET).abs() < 0.01, "{media:?}: alt boşluk");
    }
}

/// Görünür kutunun dışında duran bir açıklama, pay eklenmediği için asla
/// görünür hâle gelmez: marka işlemi onu reddetmez ve /Annots'a dokunmaz.
#[test]
fn annotations_outside_the_page_no_longer_block_branding() {
    let before = build(&[Spec {
        annot: true,
        ..spec([0., 0., 595., 842.])
    }]);
    let after = finalize(&before);
    assert_overlay_contract(&before, &after, "açıklamalı sayfa");
}

/// GölgeDosya çıktısı yeniden işlenince ikinci işaret, ikinci sarmal ya da
/// geometri değişikliği OLMAZ.
#[test]
fn reprocessing_an_output_is_idempotent() {
    let before = build(&[
        spec([0., 0., 595., 842.]),
        Spec {
            rotate: Some(90),
            ..spec([0., 0., 842., 595.])
        },
    ]);
    let once = finalize(&before);
    let twice = finalize(&once);
    let thrice = finalize(&twice);
    for (a, b) in [(&once, &twice), (&twice, &thrice)] {
        for (p, q) in a.get_pages().values().zip(b.get_pages().values()) {
            assert_eq!(geometry(a, *p), geometry(b, *q));
            assert_eq!(contents(a, *p), contents(b, *q), "akışlar değişti");
            assert_eq!(marks(b, *q).len(), 1);
        }
        assert_eq!(brand_forms(b).len(), 1, "marka formu çoğaldı");
    }
}

// ------------------------------------------------- kullanıcı işaretleri

/// Helvetica-Bold'un FontBBox'ı, em cinsinden: hiçbir glifin mürekkebi bu
/// kutudan taşmaz. Metin kutusu bununla ve harf başına 1 em genişlikle (her
/// glifin ilerlemesinden büyük) hesaplanır; ölçüm gerçeğinden GENİŞtir, bu
/// yüzden "kesişmiyor" sonucu güvenlidir.
const HB_DESCENT: f32 = 0.228;
const HB_ASCENT: f32 = 0.962;
const HB_LEFT: f32 = 0.170;

/// `m` dönüşümünü `ctm`'nin önüne ekler (`m` sonra `ctm`).
fn then(m: &[f32], ctm: [f32; 6]) -> [f32; 6] {
    [
        m[0] * ctm[0] + m[1] * ctm[2],
        m[0] * ctm[1] + m[1] * ctm[3],
        m[2] * ctm[0] + m[3] * ctm[2],
        m[2] * ctm[1] + m[3] * ctm[3],
        m[4] * ctm[0] + m[5] * ctm[2] + ctm[4],
        m[4] * ctm[1] + m[5] * ctm[3] + ctm[5],
    ]
}

/// `ctm` altındaki `[x0 y0 x1 y1]` kutusunun görüntülenen sayfadaki kutusu.
fn display_rect(ctm: [f32; 6], r: [f32; 4], v: [f32; 4], rot: i64) -> [f32; 4] {
    let pts = [(r[0], r[1]), (r[2], r[1]), (r[0], r[3]), (r[2], r[3])].map(|(x, y)| {
        let user = (
            ctm[0] * x + ctm[2] * y + ctm[4],
            ctm[1] * x + ctm[3] * y + ctm[5],
        );
        to_display(user, v, rot)
    });
    [
        pts.iter().map(|p| p.0).fold(f32::MAX, f32::min),
        pts.iter().map(|p| p.1).fold(f32::MAX, f32::min),
        pts.iter().map(|p| p.0).fold(f32::MIN, f32::max),
        pts.iter().map(|p| p.1).fold(f32::MIN, f32::max),
    ]
}

/// Kullanıcının istediği işaretlerin (Ek rozeti, filigran, sayfa numarası)
/// kutuları, görüntülenen sayfada: rozet dikdörtgeni ve metin. Damga kendi
/// başına bir akıştır (`q cm … Q`), fontu `GolgeDosyaStamp` adıyla bağlıdır.
fn user_marks(doc: &Document, page: ObjectId) -> Vec<[f32; 4]> {
    let v = visible(doc, page);
    let rot = rotation(doc, page);
    let mut out = Vec::new();
    for c in contents(doc, page) {
        let ops = Content::decode(&c).unwrap().operations;
        let stamp = ops.iter().any(|o| {
            o.operator == "Tf"
                && o.operands[0]
                    .as_name()
                    .is_ok_and(|n| n.starts_with(b"GolgeDosyaStamp"))
        });
        if !stamp {
            continue;
        }
        let mut ctm = [1f32, 0., 0., 1., 0., 0.];
        let (mut size, mut origin) = (0f32, (0f32, 0f32));
        for op in &ops {
            let nums = || -> Vec<f32> { op.operands.iter().map(number).collect() };
            match op.operator.as_str() {
                "cm" => ctm = then(&nums(), ctm),
                "Tf" => size = number(&op.operands[1]),
                "Td" => origin = (nums()[0], nums()[1]),
                "re" => {
                    let r = nums();
                    out.push(display_rect(
                        ctm,
                        [r[0], r[1], r[0] + r[2], r[1] + r[3]],
                        v,
                        rot,
                    ));
                }
                "Tj" => {
                    let n = op.operands[0].as_str().unwrap().len() as f32;
                    let (x, y) = origin;
                    let text = [
                        x - HB_LEFT * size,
                        y - HB_DESCENT * size,
                        x + n * size,
                        y + HB_ASCENT * size,
                    ];
                    out.push(display_rect(ctm, text, v, rot));
                }
                _ => {}
            }
        }
    }
    out
}

fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// İşaret tek, görünür kutunun içinde, dik, sağ alt çeyrekte ve okunur;
/// marka, kullanıcı işaretinin eklediği payı (sayfanın kendi geometrisi
/// olmuştur) değiştirmez. Dönüş: işaretin bindiği kullanıcı işaretleri, her
/// biri bir satır (bütün durumlar tek seferde raporlansın diye).
fn brand_collisions(stamped: &Document, after: &Document, label: &str) -> Vec<String> {
    let mut collisions = Vec::new();
    let pages = stamped
        .get_pages()
        .into_values()
        .zip(after.get_pages().into_values());
    for (n, (b, a)) in pages.enumerate() {
        let at = format!("{label}, sayfa {}", n + 1);
        assert_eq!(
            geometry(stamped, b),
            geometry(after, a),
            "{at}: marka geometriyi değiştirdi"
        );
        let users = user_marks(after, a);
        assert!(!users.is_empty(), "{at}: kullanıcı işareti bulunamadı");
        let m = marks(after, a);
        assert_eq!(m.len(), 1, "{at}: {} işaret", m.len());
        let m = &m[0];
        let (w, h) = display_size(after, a);
        assert!(
            m.rect[0] >= -0.01
                && m.rect[1] >= -0.01
                && m.rect[2] <= w + 0.01
                && m.rect[3] <= h + 0.01,
            "{at}: işaret görünür kutunun dışında: {:?} / {w}x{h}",
            m.rect
        );
        assert!(
            (m.axis.0 - 1.).abs() < 1e-4 && m.axis.1.abs() < 1e-4,
            "{at}: işaret dik değil (eksen {:?})",
            m.axis
        );
        assert!(
            m.rect[0] + m.rect[2] > w && m.rect[1] + m.rect[3] < h,
            "{at}: işaret sağ alt çeyrekte değil: {:?} / {w}x{h}",
            m.rect
        );
        assert!(
            m.width >= 0.08 * w.min(h),
            "{at}: işaret okunmaz: {} pt",
            m.width
        );
        for u in users.iter().filter(|u| overlaps(m.rect, **u)) {
            collisions.push(format!(
                "{at}: GölgeDosya işareti {:?} kullanıcı işaretine {u:?} biniyor",
                m.rect
            ));
        }
    }
    collisions
}

fn ek_badge(font_size: f32, margin_pt: f32) -> StampConfig {
    StampConfig {
        enabled: true,
        position: StampPosition::BottomRight,
        font_size,
        margin_pt,
        show_badge: true,
    }
}

type UserTool = (&'static str, fn(&mut Document));

/// Alt kenara işaret basan kullanıcı araçları. Her biri kendi 60 pt'lik
/// payını ekler (ürün kararı, değişmez).
const BOTTOM_USER_TOOLS: [UserTool; 5] = [
    ("sayfa numarası", |d| {
        apply_text_marks(d, None, 12).unwrap()
    }),
    ("dört haneli sayfa numarası", |d| {
        apply_text_marks(d, None, 1234).unwrap()
    }),
    ("filigran", |d| {
        apply_text_marks(d, Some("GİZLİDİR - ÇOĞALTILAMAZ"), 1).unwrap()
    }),
    ("sağ alt Ek rozeti", |d| {
        apply_stamp_to_document(d, 1, 1, &ek_badge(10., 20.)).unwrap()
    }),
    ("sağ alt Ek rozeti, 16 punto, 10 pt kenar", |d| {
        apply_stamp_to_document(d, 12, 345, &ek_badge(16., 10.)).unwrap()
    }),
];

/// Saha bulgusu: Sayfa numarası, filigran ve sağ alt Ek rozeti aynı çıktıda
/// GölgeDosya işaretiyle buluşunca, Letter'dan büyük sayfada işaret numaranın
/// üstüne biniyordu (A3'te "12" işaretin tam içinde, 1414×1885 taramada
/// numaranın üstü işaretin altında, A4'te sağ alt Ek rozeti işarete 6 pt).
/// İşaret artık kullanıcı payının ÜSTÜNDE, özgün sayfanın sağ alt köşesinde.
#[test]
fn the_brand_never_covers_a_bottom_user_mark() {
    let pages: [(&str, Spec); 8] = [
        ("A4", spec([0., 0., 595., 842.])),
        ("Letter", spec([0., 0., 612., 792.])),
        ("A3", spec([0., 0., 842., 1191.])),
        ("A3 yatay", spec([0., 0., 1191., 842.])),
        ("tarama 1414×1885", spec([0., 0., 1414., 1885.])),
        (
            "A3 /Rotate 90",
            Spec {
                rotate: Some(90),
                ..spec([0., 0., 842., 1191.])
            },
        ),
        (
            "A3 /Rotate 270",
            Spec {
                rotate: Some(270),
                ..spec([0., 0., 842., 1191.])
            },
        ),
        (
            "kaydırılmış CropBox, A3",
            Spec {
                crop: Some([30., 40., 812., 1150.]),
                ..spec([0., 0., 842., 1191.])
            },
        ),
    ];
    let mut collisions = Vec::new();
    for (tool, apply) in BOTTOM_USER_TOOLS {
        for (size, s) in &pages {
            let mut stamped = build(std::slice::from_ref(s));
            apply(&mut stamped);
            let after = finalize(&stamped);
            collisions.extend(brand_collisions(
                &stamped,
                &after,
                &format!("{tool}, {size}"),
            ));
        }
        // Çok sayfalı karışık belge: her sayfa kendi payıyla.
        let mut stamped = build(&pages.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>());
        apply(&mut stamped);
        let after = finalize(&stamped);
        collisions.extend(brand_collisions(
            &stamped,
            &after,
            &format!("{tool}, karışık"),
        ));
    }
    assert!(collisions.is_empty(), "{}", collisions.join("\n"));
}

/// Numaralanmış bir çıktı yeniden numaralanır ya da filigranlanır: iki pay,
/// iki kullanıcı işareti. İşaret ikisine de binmez ve tek kalır.
#[test]
fn a_second_bottom_user_mark_keeps_the_brand_clear() {
    let mut collisions = Vec::new();
    for media in [
        [0., 0., 595., 842.],
        [0., 0., 842., 1191.],
        [0., 0., 1414., 1885.],
    ] {
        let mut first = build(&[spec(media)]);
        apply_text_marks(&mut first, None, 7).unwrap();
        let mut stamped = finalize(&first);
        apply_text_marks(&mut stamped, Some("ÖRNEKTİR"), 1).unwrap();
        let after = finalize(&stamped);
        let page = *after.get_pages().values().next().unwrap();
        assert_eq!(user_marks(&after, page).len(), 2, "{media:?}");
        collisions.extend(brand_collisions(
            &stamped,
            &after,
            &format!("iki pay, {media:?}"),
        ));
    }
    assert!(collisions.is_empty(), "{}", collisions.join("\n"));
}

// ------------------------------------------------------- eski (paylı) çıktılar

/// v0.3.0/0.3.1'in bıraktığı paylı sayfa: içerik `q 0 0 w h re W n … Q`
/// ile kırpılmış, kutu alttan 34 pt genişlemiş, işaret o payda. `logo`
/// o dönemin çizimi.
fn banded(w: f32, h: f32, mark_w: f32, logo: &[u8], rotate: Option<i64>) -> Document {
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let body = d.add_object(Stream::new(
        dictionary! {},
        format!("0.15 g 40 {} {} 14 re f", h - 60., w * 0.6).into_bytes(),
    ));
    let prefix = d.add_object(Stream::new(
        dictionary! {},
        format!("q\n0 0 {w} {h} re W n\n").into_bytes(),
    ));
    let suffix = d.add_object(Stream::new(dictionary! {}, b"\nQ\n".to_vec()));
    let font = d.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1",
    "BaseFont"=>"Helvetica-Bold","Encoding"=>"WinAnsiEncoding"});
    let bbox = || vec![0.into(), 0.into(), 290.into(), 72.into()];
    let vector = d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>bbox(),
        "Group"=>dictionary!{"S"=>"Transparency","I"=>true,"CS"=>"DeviceRGB"},
        "Resources"=>dictionary!{"Font"=>dictionary!{"BrandFont"=>font}}},
        logo.to_vec(),
    ));
    let form = d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>bbox(),
        "Resources"=>dictionary!{"XObject"=>dictionary!{"Mark"=>vector},
        "ExtGState"=>dictionary!{"BrandAlpha"=>dictionary!{"Type"=>"ExtGState","ca"=>0.24f32,"CA"=>0.24f32}}}},
        BRAND_FORM.to_vec(),
    ));
    let s = mark_w / 290.;
    let stamp = d.add_object(Stream::new(
        dictionary! {},
        format!(
            "q 1 0 0 1 0 -34 cm {s} 0 0 {s} {} 8 cm /GolgeDosyaBrand Do Q",
            w - 8. - mark_w
        )
        .into_bytes(),
    ));
    let mut page = dictionary! {"Type"=>"Page","Parent"=>pages_id,
    "MediaBox"=>vec![0.into(),(-34).into(),w.into(),h.into()],
    "CropBox"=>vec![0.into(),(-34).into(),w.into(),h.into()],
    "Resources"=>dictionary!{"XObject"=>dictionary!{"GolgeDosyaBrand"=>form}},
    "Contents"=>vec![prefix.into(), body.into(), suffix.into(), stamp.into()]};
    if let Some(r) = rotate {
        page.set("Rotate", r);
    }
    let page = d.add_object(page);
    d.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    d.trailer.set("Root", root);
    d
}

const KAT_2026_09_20: &[u8] = include_bytes!("../assets/brand-logo-2026-09-20.ops");
const CANONICAL: &[u8] = include_bytes!("../assets/brand-logo.ops");

fn logo_bytes(doc: &Document) -> Vec<Vec<u8>> {
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| s.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() == Some(b"Form"))
        .map(|s| {
            if s.dict.has(b"Filter") {
                s.decompressed_content().unwrap()
            } else {
                s.content.clone()
            }
        })
        .filter(|d| d != BRAND_FORM)
        .collect()
}

/// Sahadaki v0.3.x çıktısı yeniden işlenir: girdinin geometrisi (34 pt'lik
/// pay dahil) AYNEN korunur, ikinci pay ve ikinci işaret eklenmez, eski çizim
/// yerinde yeni çizime yükseltilir. Döndürülmüş eski çıktı için de.
#[test]
fn a_legacy_banded_output_keeps_its_geometry_and_one_mark() {
    for rotate in [None, Some(90)] {
        for logo in [KAT_2026_09_20, CANONICAL] {
            let before = banded(595., 842., 56., logo, rotate);
            let after = finalize(&before);
            let (b, a) = (
                *before.get_pages().values().next().unwrap(),
                *after.get_pages().values().next().unwrap(),
            );
            assert_eq!(geometry(&before, b), geometry(&after, a), "{rotate:?}");
            assert_eq!(marks(&after, a).len(), 1, "{rotate:?}");
            assert_eq!(
                contents(&before, b),
                contents(&after, a),
                "{rotate:?}: kurala uyan eski sayfanın akışları değişmemeli"
            );
            assert_eq!(logo_bytes(&after), vec![CANONICAL.to_vec()], "{rotate:?}");
        }
    }
}

/// Eski çıktıda işaret okunmaz küçüklükteyse (büyük sayfada 56 pt) yeniden
/// basılır; eski işaret akışı sökülür ama payı ve kırpma sarmalı KALIR.
/// Eskiden sarmal açılıp CropBox payın öncesine geri yazılıyordu: marka
/// girdinin geometrisini değiştiriyordu.
#[test]
fn an_undersized_legacy_mark_is_replaced_without_restoring_the_crop_box() {
    let before = banded(1414., 2068., 56., KAT_2026_09_20, None);
    let after = finalize(&before);
    let (b, a) = (
        *before.get_pages().values().next().unwrap(),
        *after.get_pages().values().next().unwrap(),
    );
    assert_eq!(geometry(&before, b), geometry(&after, a));
    let m = marks(&after, a);
    assert_eq!(m.len(), 1);
    assert!(m[0].width >= 0.08 * 1414., "{} pt", m[0].width);
    let src = contents(&before, b);
    let out = contents(&after, a);
    // Eski sarmal + gövde + eski `Q` (eski işaret akışı sökülmüş), yeni sarmal.
    assert_eq!(&out[1..out.len() - 2], &src[..src.len() - 1]);
}
