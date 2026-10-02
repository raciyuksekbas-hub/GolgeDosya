//! PDF → PNG/JPEG ve Düzenle önizlemesinde açıklama görünümleri.
//!
//! Saha bulgusu (v0.3.2): "Vurgu falan varsa PDF'de, bunu PNG'ye veya JPEG'e
//! dönüştüremiyor." Dışa aktarma `/Annots` taşıyan her sayfayı reddediyordu
//! (hiçbir şey çizmeyen içindekiler bağlantısı dahil); önizleme ise vurgu,
//! not, mürekkep, damga ve form değerlerini hiç göstermiyordu, çünkü yerel
//! renderer (CoreGraphics) açıklama görünüm akışlarını çizmez.
//!
//! Kabul ölçütü GERÇEK PİKSELDİR: bir görüntüleyicinin gösterdiği her açıklama
//! önizlemede ve dışa aktarılan görselde aynı yerde, aynı renkte görünür;
//! görüntüleyicinin göstermediği (gizli, ekranda çizilmeyen, kapalı popup,
//! kenarlıksız bağlantı) hiçbir yerde görünmez. Kaynak bayt bayt aynı kalır.
//!
//! `RASTER_ANNOT_EVIDENCE=<klasör>` verilirse önizleme ve çıktı PNG'leri
//! inceleme için oraya da yazılır.
#![cfg(any(target_os = "macos", target_os = "windows"))]
use ekler_core::{calculate_sha256, raster, EklerError};
use image::RgbImage;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::path::{Path, PathBuf};

const RECT: [f32; 4] = [100., 600., 300., 650.];
const CHECKBOX: [f32; 4] = [100., 500., 120., 520.];
const A4: [f32; 4] = [0., 0., 595., 842.];
/// Vurgunun altındaki sayfa içeriği: siyah kalmalı (vurgu çarpılır, örtmez).
const UNDER_MARK: &str = "0 0 0 rg 150 620 20 10 re f";

fn boxed(v: &[f32]) -> Object {
    Object::Array(v.iter().map(|n| Object::Real(*n)).collect())
}

fn quad(r: [f32; 4]) -> Object {
    // Acrobat/PDFKit sırası: sol üst, sağ üst, sol alt, sağ alt.
    boxed(&[r[0], r[3], r[2], r[3], r[0], r[1], r[2], r[1]])
}

fn form(
    doc: &mut Document,
    bbox: [f32; 4],
    matrix: Option<[f32; 6]>,
    resources: Option<Dictionary>,
    ops: &str,
) -> ObjectId {
    let mut d = dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>boxed(&bbox)};
    if let Some(m) = matrix {
        d.set("Matrix", boxed(&m));
    }
    if let Some(r) = resources {
        d.set("Resources", r);
    }
    doc.add_object(Stream::new(d, ops.as_bytes().to_vec()))
}

fn annot(doc: &mut Document, subtype: &str, extra: Dictionary) -> ObjectId {
    let mut d = dictionary! {"Type"=>"Annot","Subtype"=>subtype,"Rect"=>boxed(&RECT),"F"=>4};
    for (k, v) in extra {
        d.set(k, v);
    }
    doc.add_object(d)
}

/// Sayfa tarifi: içerik, açıklamalar ve sayfa sözlüğüne eklenecek girdiler.
struct PageSpec {
    content: String,
    annots: Vec<ObjectId>,
    extra: Dictionary,
}

fn page(content: &str, annots: Vec<ObjectId>) -> PageSpec {
    PageSpec {
        content: content.into(),
        annots,
        extra: Dictionary::new(),
    }
}

fn save(
    mut doc: Document,
    pages_id: ObjectId,
    specs: Vec<PageSpec>,
    catalog: Dictionary,
    path: &Path,
) {
    let mut kids = Vec::new();
    for spec in specs {
        let content = doc.add_object(Stream::new(dictionary! {}, spec.content.into_bytes()));
        let mut p = dictionary! {"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>boxed(&A4),
        "Resources"=>dictionary!{},"Contents"=>content};
        if !spec.annots.is_empty() {
            p.set(
                "Annots",
                spec.annots
                    .into_iter()
                    .map(Object::Reference)
                    .collect::<Vec<_>>(),
            );
        }
        for (k, v) in spec.extra {
            p.set(k, v);
        }
        kids.push(Object::Reference(doc.add_object(p)));
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>count}.into(),
    );
    let mut cat = dictionary! {"Type"=>"Catalog","Pages"=>pages_id};
    for (k, v) in catalog {
        cat.set(k, v);
    }
    let root = doc.add_object(cat);
    doc.trailer.set("Root", root);
    doc.save(path).unwrap();
}

/// Beklenen çizim: sayfa, alan, renk ve en az piksel sayısı.
struct Expect {
    name: &'static str,
    area: [f32; 4],
    rgb: [u8; 3],
    min: usize,
}

/// Çizilmesi gereken (sayfa, beklenti) ve hiçbir şey çizmemesi gereken
/// (sayfa, ad) listeleri.
type Corpus = (Vec<(usize, Expect)>, Vec<(usize, &'static str)>);

/// Görüntüleyicinin gösterdiği her açıklama türü + gösterilmeyen kontroller.
fn corpus(path: &Path) -> Corpus {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut specs = Vec::new();
    let mut draws = Vec::new();
    let mut blanks = Vec::new();
    let mut fields = Vec::new();
    let multiply = dictionary! {"ExtGState"=>dictionary!{"GS0"=>dictionary!{"Type"=>"ExtGState","BM"=>"Multiply"}}};
    let full = |name, rgb, min| Expect {
        name,
        area: RECT,
        rgb,
        min,
    };

    let ap = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        Some(multiply),
        "/GS0 gs 1 1 0 rg 0 0 200 50 re f",
    );
    let a = annot(
        &mut doc,
        "Highlight",
        dictionary! {"QuadPoints"=>quad(RECT),"C"=>vec![1.into(),1.into(),0.into()],"AP"=>dictionary!{"N"=>ap}},
    );
    specs.push(page(UNDER_MARK, vec![a]));
    draws.push((
        specs.len(),
        full("Highlight /AP (Multiply)", [255, 255, 0], 9000),
    ));

    let ap = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "0 0 1 RG 4 w 0 4 m 200 4 l S",
    );
    let a = annot(
        &mut doc,
        "Underline",
        dictionary! {"QuadPoints"=>quad(RECT),"C"=>vec![0.into(),0.into(),1.into()],"AP"=>dictionary!{"N"=>ap}},
    );
    specs.push(page("", vec![a]));
    draws.push((specs.len(), full("Underline /AP", [0, 0, 255], 600)));

    let ap = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "1 0 0 RG 4 w 0 25 m 200 25 l S",
    );
    let a = annot(
        &mut doc,
        "StrikeOut",
        dictionary! {"QuadPoints"=>quad(RECT),"C"=>vec![1.into(),0.into(),0.into()],"AP"=>dictionary!{"N"=>ap}},
    );
    specs.push(page("", vec![a]));
    draws.push((specs.len(), full("StrikeOut /AP", [255, 0, 0], 600)));

    let ap = form(&mut doc, [0., 0., 200., 50.], None, None, "0 0.6 0 RG 3 w 0 3 m 20 9 l 40 3 l 60 9 l 80 3 l 100 9 l 120 3 l 140 9 l 160 3 l 180 9 l 200 3 l S");
    let a = annot(
        &mut doc,
        "Squiggly",
        dictionary! {"QuadPoints"=>quad(RECT),"AP"=>dictionary!{"N"=>ap}},
    );
    specs.push(page("", vec![a]));
    draws.push((specs.len(), full("Squiggly /AP", [0, 153, 0], 300)));

    let ap = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "0.6 0.8 1 rg 0 0 200 50 re f 0 0 0 RG 2 w 1 1 198 48 re S",
    );
    let a = annot(
        &mut doc,
        "FreeText",
        dictionary! {"Contents"=>Object::string_literal("not"),"DA"=>Object::string_literal("/Helv 12 Tf 0 g"),"AP"=>dictionary!{"N"=>ap}},
    );
    specs.push(page("", vec![a]));
    draws.push((specs.len(), full("FreeText /AP", [153, 204, 255], 8000)));

    let ap = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "1 0 1 RG 5 w 10 10 m 190 40 l S",
    );
    let a = annot(
        &mut doc,
        "Ink",
        dictionary! {"InkList"=>vec![boxed(&[110., 610., 290., 640.])],"AP"=>dictionary!{"N"=>ap}},
    );
    specs.push(page("", vec![a]));
    draws.push((specs.len(), full("Ink /AP", [255, 0, 255], 600)));

    let ap = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "0 0.5 1 RG 4 w 2 2 196 46 re S",
    );
    let a = annot(&mut doc, "Square", dictionary! {"AP"=>dictionary!{"N"=>ap}});
    specs.push(page("", vec![a]));
    draws.push((specs.len(), full("Square /AP", [0, 128, 255], 1500)));

    let ap = form(&mut doc, [0., 0., 200., 50.], None, None, "1 0.5 0 rg 100 25 m 155 25 200 36 200 25 c 200 14 155 0 100 0 c 45 0 0 14 0 25 c 0 36 45 50 100 50 c 155 50 200 36 200 25 c f");
    let a = annot(&mut doc, "Circle", dictionary! {"AP"=>dictionary!{"N"=>ap}});
    specs.push(page("", vec![a]));
    draws.push((specs.len(), full("Circle /AP", [255, 128, 0], 6500)));

    // BBox 100×25, /Matrix 2×: §12.5.5 eşlemesi Rect'i tam doldurmalı.
    let ap = form(
        &mut doc,
        [0., 0., 100., 25.],
        Some([2., 0., 0., 2., 0., 0.]),
        None,
        "0 0.7 0 rg 0 0 100 25 re f",
    );
    let a = annot(
        &mut doc,
        "Stamp",
        dictionary! {"Name"=>"Approved","AP"=>dictionary!{"N"=>ap}},
    );
    specs.push(page("", vec![a]));
    draws.push((
        specs.len(),
        full("Stamp /AP (/Matrix 2×)", [0, 179, 0], 9800),
    ));

    let ap = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "0.8 0.8 1 rg 0 0 200 50 re f 0 0 0 rg 10 20 60 10 re f",
    );
    let tx = annot(
        &mut doc,
        "Widget",
        dictionary! {"FT"=>"Tx","T"=>Object::string_literal("ad"),"V"=>Object::string_literal("Ali"),"DA"=>Object::string_literal("/Helv 12 Tf 0 g"),"AP"=>dictionary!{"N"=>ap}},
    );
    let yes = form(
        &mut doc,
        [0., 0., 20., 20.],
        None,
        None,
        "0 0 0 rg 2 2 16 16 re f",
    );
    let off = form(&mut doc, [0., 0., 20., 20.], None, None, "");
    let cb = annot(
        &mut doc,
        "Widget",
        dictionary! {"Rect"=>boxed(&CHECKBOX),"FT"=>"Btn","T"=>Object::string_literal("onay"),"V"=>"Yes","AS"=>"Yes","AP"=>dictionary!{"N"=>dictionary!{"Yes"=>yes,"Off"=>off}}},
    );
    fields.push(Object::Reference(tx));
    fields.push(Object::Reference(cb));
    specs.push(page("", vec![tx, cb]));
    draws.push((specs.len(), full("Widget Tx /AP", [204, 204, 255], 9000)));
    draws.push((
        specs.len(),
        Expect {
            name: "Widget Btn /AS Yes",
            area: CHECKBOX,
            rgb: [0, 0, 0],
            min: 250,
        },
    ));

    // Görünüm akışı OLMAYAN metin işaretlemeleri: görünüm kurulur.
    let a = annot(
        &mut doc,
        "Highlight",
        dictionary! {"QuadPoints"=>quad(RECT),"C"=>vec![1.into(),1.into(),0.into()]},
    );
    specs.push(page(UNDER_MARK, vec![a]));
    draws.push((specs.len(), full("Highlight /AP'siz", [255, 255, 0], 9000)));
    let a = annot(
        &mut doc,
        "Underline",
        dictionary! {"QuadPoints"=>quad(RECT),"C"=>vec![0.into(),0.into(),1.into()]},
    );
    specs.push(page("", vec![a]));
    draws.push((specs.len(), full("Underline /AP'siz", [0, 0, 255], 300)));
    let a = annot(
        &mut doc,
        "StrikeOut",
        dictionary! {"QuadPoints"=>quad(RECT),"C"=>vec![1.into(),0.into(),0.into()]},
    );
    specs.push(page("", vec![a]));
    draws.push((specs.len(), full("StrikeOut /AP'siz", [255, 0, 0], 300)));

    // Kontroller: görüntüleyici bunları ekranda göstermez.
    for (flags, name) in [(2, "Square /F Hidden"), (32, "Square /F NoView")] {
        let ap = form(
            &mut doc,
            [0., 0., 200., 50.],
            None,
            None,
            "1 0 0 rg 0 0 200 50 re f",
        );
        let a = annot(
            &mut doc,
            "Square",
            dictionary! {"F"=>flags,"AP"=>dictionary!{"N"=>ap}},
        );
        specs.push(page("", vec![a]));
        blanks.push((specs.len(), name));
    }
    let parent = annot(
        &mut doc,
        "Text",
        dictionary! {"F"=>2,"Contents"=>Object::string_literal("not")},
    );
    let popup = annot(
        &mut doc,
        "Popup",
        dictionary! {"Parent"=>parent,"Open"=>true},
    );
    specs.push(page("", vec![parent, popup]));
    blanks.push((specs.len(), "Popup (açık) + gizli üst not"));
    let a = annot(
        &mut doc,
        "Link",
        dictionary! {"A"=>dictionary!{"S"=>"URI","URI"=>Object::string_literal("https://ornek.invalid")}},
    );
    specs.push(page("", vec![a]));
    blanks.push((specs.len(), "Link /AP'siz (varsayılan kenarlık)"));

    let form_dict = doc.add_object(dictionary! {"Fields"=>fields,"NeedAppearances"=>true});
    save(
        doc,
        pages_id,
        specs,
        dictionary! {"AcroForm"=>form_dict},
        path,
    );
    (draws, blanks)
}

fn evidence() -> Option<PathBuf> {
    std::env::var_os("RASTER_ANNOT_EVIDENCE").map(PathBuf::from)
}

fn keep(img: &RgbImage, name: &str) {
    if let Some(dir) = evidence() {
        std::fs::create_dir_all(&dir).unwrap();
        img.save(dir.join(name)).unwrap();
    }
}

fn preview(src: &Path, page: usize, dpi: u32, rotation: i32) -> RgbImage {
    let before = calculate_sha256(src).unwrap();
    let png = raster::preview_page(src, page, dpi, rotation)
        .unwrap_or_else(|e| panic!("s.{page} önizlenemedi: {e}"));
    assert_eq!(
        before,
        calculate_sha256(src).unwrap(),
        "önizleme kaynağı değiştirdi"
    );
    image::load_from_memory(&png).unwrap().to_rgb8()
}

/// PDF alanının (72 DPI, dönüşsüz: 1 px = 1 pt) beklenen renge yakın pikselleri.
fn near(img: &RgbImage, area: [f32; 4], rgb: [u8; 3]) -> usize {
    let h = img.height() as f32;
    let mut n = 0;
    for y in (h - area[3]) as u32..(h - area[1]) as u32 {
        for x in area[0] as u32..area[2] as u32 {
            let p = img.get_pixel(x, y).0;
            if (0..3).all(|c| p[c].abs_diff(rgb[c]) <= 40) {
                n += 1;
            }
        }
    }
    n
}

fn non_white(img: &RgbImage) -> usize {
    img.pixels()
        .filter(|p| p.0.iter().any(|c| *c < 240))
        .count()
}

/// Siyah içerik işareti vurgunun ALTINDA siyah kalır.
fn under_mark_is_dark(img: &RgbImage) -> bool {
    let p = img.get_pixel(160, img.height() - 625).0;
    p.iter().all(|c| *c < 60)
}

#[test]
fn preview_draws_what_a_viewer_shows_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("aciklamalar.pdf");
    let (draws, blanks) = corpus(&src);
    let mut failures = Vec::new();
    for (page, expect) in &draws {
        let img = preview(&src, *page, 72, 0);
        keep(&img, &format!("onizleme-s{page:02}.png"));
        let got = near(&img, expect.area, expect.rgb);
        println!("s.{page} {}: {got} px", expect.name);
        if got < expect.min {
            failures.push(format!(
                "s.{page} {}: {got} px (en az {})",
                expect.name, expect.min
            ));
        }
    }
    for (page, name) in &blanks {
        let img = preview(&src, *page, 72, 0);
        keep(&img, &format!("onizleme-s{page:02}.png"));
        let got = non_white(&img);
        println!("s.{page} {name}: {got} px (kontrol)");
        if got != 0 {
            failures.push(format!(
                "s.{page} {name}: {got} px çizildi, hiç çizilmemeliydi"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "önizleme açıklama görünümleri:\n{}",
        failures.join("\n")
    );
}

#[test]
fn a_highlight_multiplies_over_the_text_instead_of_covering_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("vurgu.pdf");
    let (draws, _) = corpus(&src);
    for (page, expect) in draws
        .iter()
        .filter(|(_, e)| e.name.starts_with("Highlight"))
    {
        let img = preview(&src, *page, 72, 0);
        assert!(
            near(&img, RECT, [255, 255, 0]) >= 9000,
            "s.{page} {}: vurgu yok",
            expect.name
        );
        assert!(
            under_mark_is_dark(&img),
            "s.{page} {}: vurgu alttaki metni örttü",
            expect.name
        );
    }
}

/// Dışa aktarılan görsel, önizlemenin gösterdiğini gösterir: açıklama
/// pikselleri, altta GölgeDosya işareti; kaynak bayt bayt aynı.
#[test]
fn export_contains_the_same_annotations_as_the_preview() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("aciklamalar.pdf");
    let (draws, blanks) = corpus(&src);
    let before = calculate_sha256(&src).unwrap();
    let out = dir.path().join("cikti");
    std::fs::create_dir_all(&out).unwrap();
    let folder = raster::pdf_to_images(&src, &out, "png", 72, false)
        .unwrap_or_else(|e| panic!("açıklamalı PDF görsele dönüştürülemedi: {e}"));
    assert_eq!(before, calculate_sha256(&src).unwrap(), "kaynak değişti");
    let total = draws
        .iter()
        .map(|(p, _)| *p)
        .chain(blanks.iter().map(|(p, _)| *p))
        .max()
        .unwrap();
    for page in 1..=total {
        let got = image::open(folder.join(format!("sayfa-{page:04}.png")))
            .unwrap()
            .to_rgb8();
        keep(&got, &format!("cikti-s{page:02}.png"));
        assert_eq!(
            (got.width(), got.height()),
            (595, 842),
            "s.{page}: görsel sayfa boyutunda (marka sayfayı büyütmez)"
        );
        let top = image::imageops::crop_imm(&got, 0, 0, 595, 842).to_image();
        let shown = preview(&src, page, 72, 0);
        // Marka artık sayfanın üstüne, sağ alt köşeye basılıyor ve yalnız
        // çıktıda var; karşılaştırma açıklamaları ölçsün diye o köşe dışarıda.
        let brand_corner = |x: u32, y: u32| x >= 500 && y >= 800;
        let mut diff = 0u64;
        let mut n = 0u64;
        for ((x, y, a), b) in top.enumerate_pixels().zip(shown.pixels()) {
            if brand_corner(x, y) {
                continue;
            }
            n += 3;
            diff += (0..3)
                .map(|c| u64::from(a.0[c].abs_diff(b.0[c])))
                .sum::<u64>();
        }
        let mae = diff as f64 / n as f64;
        assert!(mae < 0.5, "s.{page}: önizleme ↔ çıktı MAE {mae:.3}");
        let band = got.height() - 16;
        assert!(
            (got.width() * 2 / 3..got.width() - 8).any(|x| got
                .get_pixel(x, band)
                .0
                .iter()
                .any(|c| *c < 235)),
            "s.{page}: sağ alt köşede GölgeDosya işareti yok"
        );
    }
    for (page, expect) in &draws {
        let got = image::open(folder.join(format!("sayfa-{page:04}.png")))
            .unwrap()
            .to_rgb8();
        let top = image::imageops::crop_imm(&got, 0, 0, 595, 842).to_image();
        assert!(
            near(&top, expect.area, expect.rgb) >= expect.min,
            "çıktı s.{page} {}",
            expect.name
        );
    }
    for (page, name) in &blanks {
        let got = image::open(folder.join(format!("sayfa-{page:04}.png")))
            .unwrap()
            .to_rgb8();
        let mut top = image::imageops::crop_imm(&got, 0, 0, 595, 842).to_image();
        // Sağ alt köşedeki marka sayılmaz; açıklamalar o köşede değil.
        for y in 800..842 {
            for x in 500..595 {
                top.put_pixel(x, y, image::Rgb([255, 255, 255]));
            }
        }
        assert_eq!(non_white(&top), 0, "çıktı s.{page} {name}: çizilmemeliydi");
    }
    // JPEG de aynı yoldan geçer.
    let jpg_out = dir.path().join("jpg");
    std::fs::create_dir_all(&jpg_out).unwrap();
    let folder = raster::pdf_to_images(&src, &jpg_out, "jpg", 72, false).unwrap();
    let first = image::open(folder.join("sayfa-0001.jpg"))
        .unwrap()
        .to_rgb8();
    let top = image::imageops::crop_imm(&first, 0, 0, 595, 842).to_image();
    assert!(near(&top, RECT, [255, 255, 0]) >= 9000, "JPEG: vurgu yok");
    assert!(under_mark_is_dark(&top), "JPEG: vurgu metni örttü");
}

/// Dönük sayfa ve CropBox ≠ MediaBox: görünüm sayfayla birlikte döner;
/// CropBox dışındaki (MediaBox içindeki) açıklama ne önizlemede ne de
/// çıktıda görünür.
#[test]
fn rotated_and_cropped_pages_keep_annotations_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("donuk.pdf");
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let red = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "1 0 0 rg 0 0 200 50 re f",
    );
    let inside = annot(
        &mut doc,
        "Square",
        dictionary! {"AP"=>dictionary!{"N"=>red}},
    );
    let green = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "0 1 0 rg 0 0 200 50 re f",
    );
    let below = annot(
        &mut doc,
        "Square",
        dictionary! {"Rect"=>boxed(&[100., 20., 300., 70.]),"AP"=>dictionary!{"N"=>green}},
    );
    let mut spec = page("", vec![inside, below]);
    spec.extra.set("Rotate", 90);
    spec.extra.set("CropBox", boxed(&[0., 100., 595., 842.]));
    save(doc, pages_id, vec![spec], Dictionary::new(), &src);

    let greenish = |img: &RgbImage| {
        img.pixels()
            .filter(|p| p.0[1] > 150 && p.0[0] < 100 && p.0[2] < 100)
            .count()
    };
    // Dönük görünüm: genişlik = CropBox yüksekliği (742), yükseklik = 595.
    // (x, y) noktası saat yönünde 90° dönünce (y − 100, x) pikseline düşer.
    let img = preview(&src, 1, 72, 0);
    keep(&img, "donuk-onizleme.png");
    assert_eq!((img.width(), img.height()), (742, 595));
    let (mut n, mut sx, mut sy) = (0usize, 0f64, 0f64);
    for (x, y, p) in img.enumerate_pixels() {
        if p.0[0] > 200 && p.0[1] < 60 && p.0[2] < 60 {
            n += 1;
            sx += f64::from(x);
            sy += f64::from(y);
        }
    }
    assert!(n >= 9500, "dönük sayfada açıklama çizilmedi: {n} px");
    let (cx, cy) = (sx / n as f64, sy / n as f64);
    assert!(
        (cx - 525.).abs() < 3. && (cy - 200.).abs() < 3.,
        "açıklama yanlış yerde: ({cx:.1}, {cy:.1})"
    );
    assert_eq!(
        greenish(&img),
        0,
        "CropBox dışındaki açıklama önizlemede göründü"
    );

    let out = dir.path().join("cikti");
    std::fs::create_dir_all(&out).unwrap();
    let folder = raster::pdf_to_images(&src, &out, "png", 72, false).unwrap();
    let got = image::open(folder.join("sayfa-0001.png"))
        .unwrap()
        .to_rgb8();
    keep(&got, "donuk-cikti.png");
    assert_eq!(
        greenish(&got),
        0,
        "çıktı CropBox dışındaki açıklamayı açığa çıkardı"
    );
    let red_out = got
        .pixels()
        .filter(|p| p.0[0] > 200 && p.0[1] < 60 && p.0[2] < 60)
        .count();
    assert!(red_out >= 9500, "çıktıda açıklama yok: {red_out} px");
}

/// Sayfa kaynakları sayfa ağacından miras ve sayfada zaten bir görsel var:
/// açıklama eklenince görsel ve metin kaybolmaz.
#[test]
fn inherited_resources_and_existing_images_survive_flattening() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("miras.pdf");
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let image = doc.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>2,"Height"=>2,
        "ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8},
        vec![0, 0, 200, 0, 0, 200, 0, 0, 200, 0, 0, 200],
    ));
    let font = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica-Bold","Encoding"=>"WinAnsiEncoding"});
    let shared = doc.add_object(
        dictionary! {"XObject"=>dictionary!{"GDAnnot1"=>image},"Font"=>dictionary!{"F1"=>font}},
    );
    let ap = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "1 0 0 rg 0 0 200 50 re f",
    );
    let a = annot(&mut doc, "Square", dictionary! {"AP"=>dictionary!{"N"=>ap}});
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"q 100 0 0 100 50 50 cm /GDAnnot1 Do Q 0 0 0 rg BT /F1 40 Tf 300 300 Td (METIN) Tj ET"
            .to_vec(),
    ));
    let p = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages_id,"Contents"=>content,"Annots"=>vec![a.into()]});
    doc.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>vec![p.into()],"Count"=>1,"Resources"=>shared,"MediaBox"=>boxed(&A4)}.into(),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", root);
    doc.save(&src).unwrap();

    let check = |img: &RgbImage, what: &str| {
        assert!(
            near(img, [50., 50., 150., 150.], [0, 0, 200]) >= 9000,
            "{what}: sayfadaki görsel kayboldu"
        );
        assert!(
            near(img, [300., 295., 450., 335.], [0, 0, 0]) >= 300,
            "{what}: sayfa metni kayboldu"
        );
        assert!(
            near(img, RECT, [255, 0, 0]) >= 9800,
            "{what}: açıklama çizilmedi"
        );
    };
    let img = preview(&src, 1, 72, 0);
    keep(&img, "miras-onizleme.png");
    check(&img, "önizleme");
    let out = dir.path().join("cikti");
    std::fs::create_dir_all(&out).unwrap();
    let folder = raster::pdf_to_images(&src, &out, "png", 72, false).unwrap();
    let got = image::open(folder.join("sayfa-0001.png"))
        .unwrap()
        .to_rgb8();
    check(
        &image::imageops::crop_imm(&got, 0, 0, 595, 842).to_image(),
        "çıktı",
    );
}

/// İmzalı PDF: imza alanı sayfada (görünmez widget) olduğu için onay verilse
/// bile reddediliyordu. Onayla artık dışa aktarılır; onaysız ret aynı kalır.
#[test]
fn a_signed_pdf_exports_after_approval() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("imzali.pdf");
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let sig = doc.add_object(dictionary! {"Type"=>"Sig","Filter"=>"Adobe.PPKLite","SubFilter"=>"adbe.pkcs7.detached",
        "ByteRange"=>vec![0.into(),10.into(),20.into(),10.into()],"Contents"=>Object::String(vec![0; 16], lopdf::StringFormat::Hexadecimal)});
    let invisible = annot(
        &mut doc,
        "Widget",
        dictionary! {"FT"=>"Sig","T"=>Object::string_literal("imza"),"V"=>sig,"F"=>132,"Rect"=>boxed(&[0., 0., 0., 0.])},
    );
    let unsigned = annot(
        &mut doc,
        "Widget",
        dictionary! {"FT"=>"Sig","T"=>Object::string_literal("ikinci imza"),"Rect"=>boxed(&[100., 100., 250., 150.])},
    );
    let form_dict = doc
        .add_object(dictionary! {"Fields"=>vec![invisible.into(), unsigned.into()],"SigFlags"=>3});
    save(
        doc,
        pages_id,
        vec![page(
            "0 0 0 rg 100 700 100 20 re f",
            vec![invisible, unsigned],
        )],
        dictionary! {"AcroForm"=>form_dict},
        &src,
    );
    let before = calculate_sha256(&src).unwrap();
    let out = dir.path().join("cikti");
    std::fs::create_dir_all(&out).unwrap();
    match raster::pdf_to_images(&src, &out, "png", 72, false) {
        Err(EklerError::ValidationFailed(m)) => assert!(m.contains("onay"), "{m}"),
        other => panic!("onaysız imzalı PDF reddedilmeliydi: {other:?}"),
    }
    let folder = raster::pdf_to_images(&src, &out, "png", 72, true)
        .unwrap_or_else(|e| panic!("onaylı imzalı PDF dönüştürülemedi: {e}"));
    let got = image::open(folder.join("sayfa-0001.png"))
        .unwrap()
        .to_rgb8();
    assert!(
        near(&got, [100., 700., 200., 720.], [0, 0, 0]) >= 1900,
        "sayfa içeriği yok"
    );
    assert_eq!(before, calculate_sha256(&src).unwrap());
}

/// Görünür bir açıklamanın görünümü belgede yok ve güvenle kurulamıyor:
/// dışa aktarma dar bir cümleyle reddeder, yarım klasör bırakmaz; önizleme
/// ise düşmez, sayfanın geri kalanını gösterir.
#[test]
fn an_appearance_that_cannot_be_built_is_refused_only_on_export() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("gorunumsuz.pdf");
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let free_text = annot(
        &mut doc,
        "FreeText",
        dictionary! {"Contents"=>Object::string_literal("not"),"DA"=>Object::string_literal("/Helv 12 Tf 1 0 0 rg")},
    );
    save(
        doc,
        pages_id,
        vec![
            page("0 0 0 rg 100 100 50 50 re f", vec![]),
            page(UNDER_MARK, vec![free_text]),
        ],
        Dictionary::new(),
        &src,
    );
    let before = calculate_sha256(&src).unwrap();
    let entries = || std::fs::read_dir(dir.path()).unwrap().count();
    let count = entries();
    match raster::pdf_to_images(&src, dir.path(), "png", 72, false) {
        Err(EklerError::UnsupportedFormat(m)) => {
            assert_eq!(m, ekler_core::pdf::flatten::MISSING_APPEARANCE_MESSAGE);
            assert!(!m.contains("::") && !m.contains("Annots"), "teknik iz: {m}");
        }
        other => panic!("görünümü kurulamayan açıklama reddedilmeliydi: {other:?}"),
    }
    assert_eq!(count, entries(), "reddedilen dışa aktarma klasör bıraktı");
    assert_eq!(before, calculate_sha256(&src).unwrap());
    let img = preview(&src, 2, 72, 0);
    assert!(
        under_mark_is_dark(&img),
        "önizleme sayfa içeriğini göstermedi"
    );
}

/// 2026-09-14 düzensizlikleri: `/Annots` içinde null, dolaylı `/Rect`,
/// `/Rect`'siz açıklama, eksik nesneye giden `/AP`. Hiçbiri önizlemeyi ya da
/// dışa aktarmayı düşürmez.
#[test]
fn irregular_annotation_entries_never_break_preview_or_export() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("duzensiz.pdf");
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let rect = doc.add_object(boxed(&RECT));
    let link = annot(
        &mut doc,
        "Link",
        dictionary! {"Rect"=>rect,"Border"=>vec![0.into(),0.into(),0.into()]},
    );
    let no_rect = doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Square"});
    let dangling = annot(
        &mut doc,
        "Link",
        dictionary! {"AP"=>dictionary!{"N"=>Object::Reference((9999, 0))}},
    );
    let ap = form(
        &mut doc,
        [0., 0., 200., 50.],
        None,
        None,
        "0 0 1 rg 0 0 200 50 re f",
    );
    let square = annot(
        &mut doc,
        "Square",
        dictionary! {"Rect"=>rect,"AP"=>dictionary!{"N"=>ap}},
    );
    let mut spec = page("", vec![link, no_rect, dangling, square]);
    let mut annots = vec![Object::Null];
    annots.extend(spec.annots.drain(..).map(Object::Reference));
    spec.extra.set("Annots", annots);
    save(doc, pages_id, vec![spec], Dictionary::new(), &src);
    let img = preview(&src, 1, 72, 0);
    assert!(near(&img, RECT, [0, 0, 255]) >= 9800);
    let out = dir.path().join("cikti");
    std::fs::create_dir_all(&out).unwrap();
    raster::pdf_to_images(&src, &out, "png", 72, false)
        .unwrap_or_else(|e| panic!("dışa aktarma düştü: {e}"));
}
