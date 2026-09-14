//! Düzenle / PDF çalışma alanı — uçtan uca işlevsel denetim.
//!
//! Her test gerçek kullanıcı zincirini izler:
//!
//!   niyet → ToolOperation → run_tool_with_outcome → safe_io yayını
//!         → çıktıyı YENİDEN AÇ (tolerant loader + validate + scanner)
//!         → değişmezleri ölç (sayfa sayısı, sıra, dönüş, MediaBox, kaynak SHA)
//!
//! "Fonksiyon çağrıldı" PASS değildir; çıktı yeniden açılıp ölçülmeden hiçbir
//! zincir başarılı sayılmaz. Bütün fixture'lar sentetiktir ve yalnız geçici
//! dizine yazılır — gerçek kullanıcı belgesi ya da dizini yoktur.
use ekler_core::{
    calculate_sha256,
    pdf::{self, load_pdf_tolerant},
    scan_source_files,
    toolbox::{run_tool_with_outcome, PageRotation, ToolOperation, ToolOutcome},
    OptimizationLevel as Level,
};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------- fixtures

/// Standart sayfa kutuları (pt).
const A4: [f32; 2] = [595.28, 841.89];
const A4_LANDSCAPE: [f32; 2] = [841.89, 595.28];
const LETTER: [f32; 2] = [612.0, 792.0];

fn media(w: f32, h: f32) -> Object {
    vec![0.into(), 0.into(), w.into(), h.into()].into()
}

/// "Sayfa N" yazan tek bir içerik akışı. Sıra doğrulaması bu işaretle yapılır.
fn text_content(marker: &str) -> Vec<u8> {
    format!("BT /F1 24 Tf 72 720 Td ({marker}) Tj ET").into_bytes()
}

fn font_dict() -> Dictionary {
    dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"}
}

/// n sayfalık vektör PDF; her sayfa kendi kutusu ve "Sayfa i" işaretiyle.
fn vector_pdf(sizes: &[[f32; 2]]) -> Document {
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let font = d.add_object(font_dict());
    let mut kids = Vec::new();
    for (i, [w, h]) in sizes.iter().enumerate() {
        let contents = d.add_object(Stream::new(
            dictionary! {},
            text_content(&format!("Sayfa {}", i + 1)),
        ));
        let page = d.add_object(dictionary! {
            "Type"=>"Page","Parent"=>pages_id,"MediaBox"=>media(*w,*h),
            "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},
            "Contents"=>contents,
        });
        kids.push(Object::Reference(page));
    }
    d.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>kids.clone(),"Count"=>kids.len() as i64}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    d.trailer.set("Root", root);
    d
}

/// Dört sayfa; 3 ve 4 numaralı sayfalar `/Rotate 90` taşıyan bir ara Pages
/// düğümünün altında (MİRAS yoluyla döndürülmüş). Gerçek belgelerde çok
/// yaygın; sayfa kopyalamada mirası kaybeden her yol burada yakalanır.
fn nested_inherited_rotate_pdf() -> Document {
    let mut d = Document::with_version("1.7");
    let root_pages = d.new_object_id();
    let inner_pages = d.new_object_id();
    let font = d.add_object(font_dict());
    let mut page = |parent: ObjectId, i: usize, size: [f32; 2]| {
        let contents = d.add_object(Stream::new(
            dictionary! {},
            text_content(&format!("Sayfa {i}")),
        ));
        d.add_object(dictionary! {
            "Type"=>"Page","Parent"=>parent,"MediaBox"=>media(size[0],size[1]),
            "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},
            "Contents"=>contents,
        })
    };
    let p1 = page(root_pages, 1, A4);
    let p2 = page(root_pages, 2, A4_LANDSCAPE);
    let p3 = page(inner_pages, 3, LETTER);
    let p4 = page(inner_pages, 4, A4);
    d.objects.insert(
        inner_pages,
        dictionary! {"Type"=>"Pages","Parent"=>root_pages,"Rotate"=>90,
        "Kids"=>vec![p3.into(),p4.into()],"Count"=>2}
        .into(),
    );
    d.objects.insert(
        root_pages,
        dictionary! {"Type"=>"Pages",
        "Kids"=>vec![p1.into(),p2.into(),inner_pages.into()],"Count"=>4}
        .into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>root_pages});
    d.trailer.set("Root", root);
    d
}

/// 1 numaralı sayfaya, 3 numaralı sayfayı hedefleyen bir bağlantı açıklaması
/// ekler. İçindekiler tablosu olan her hukuki belgede vardır.
fn add_cross_page_link(d: &mut Document, from: u32, to: u32) {
    let pages = d.get_pages();
    let target = pages[&to];
    let annot = d.add_object(dictionary! {
        "Type"=>"Annot","Subtype"=>"Link","Rect"=>vec![72.into(),700.into(),300.into(),740.into()],
        "Border"=>vec![0.into(),0.into(),0.into()],
        "Dest"=>vec![Object::Reference(target),"Fit".into()],
        "P"=>pages[&from],
    });
    d.get_dictionary_mut(pages[&from])
        .unwrap()
        .set("Annots", vec![Object::Reference(annot)]);
}

/// Tek sayfa + tek görsel XObject. Renk uzayı ve filtre parametrik: gerçek
/// dünyanın ICCBased/Indexed/SMask çeşitleri buradan üretilir.
fn image_pdf(image: Stream, extra_objects: Vec<(ObjectId, Object)>) -> Document {
    let mut d = Document::with_version("1.5");
    for (id, obj) in extra_objects {
        d.objects.insert(id, obj);
        d.max_id = d.max_id.max(id.0);
    }
    let pages = d.new_object_id();
    let img = d.add_object(image);
    let contents = d.add_object(Stream::new(
        dictionary! {},
        b"q 595 0 0 842 0 0 cm /Scan Do Q".to_vec(),
    ));
    let page = d.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,
    "MediaBox"=>media(595.,842.),
    "Resources"=>dictionary!{"XObject"=>dictionary!{"Scan"=>img}},"Contents"=>contents});
    d.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer.set("Root", root);
    d
}

/// Gerçekçi tarama görseli: gürültülü gri zemin, koyu satırlar. 2000×2800
/// piksel RGB (~17 MB ham) → Flate ile ~1–2 MB; JPEG 72 ile çok daha küçük.
fn scan_pixels(w: u32, h: u32) -> image::RgbImage {
    let mut state = 7u32;
    image::RgbImage::from_fn(w, h, |x, y| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let noise = (state % 80) as u8;
        let v = if y % 140 < 8 && x > 150 && x < w - 200 {
            25 + noise / 8
        } else {
            170 + noise
        };
        image::Rgb([v, v, v])
    })
}

fn flate_rgb_stream(px: &image::RgbImage, colorspace: Object) -> Stream {
    let mut s = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>px.width() as i64,
        "Height"=>px.height() as i64,"ColorSpace"=>colorspace,"BitsPerComponent"=>8},
        px.as_raw().clone(),
    );
    s.compress().unwrap();
    s
}

/// `/ColorSpace [/ICCBased n 0 R]` — Word, LibreOffice, Preview ve tarayıcı
/// çıktılarının normali. Profil baytları optimizer için önemsiz (ayrıştırmaz).
fn icc_pdf(px: &image::RgbImage) -> Document {
    let icc_id: ObjectId = (900, 0);
    let icc = Stream::new(
        dictionary! {"N"=>3,"Alternate"=>"DeviceRGB"},
        vec![0u8; 128],
    );
    image_pdf(
        flate_rgb_stream(
            px,
            vec!["ICCBased".into(), Object::Reference(icc_id)].into(),
        ),
        vec![(icc_id, icc.into())],
    )
}

/// Alfa maskesi olan görsel (PNG'den gelen tipik yapı).
fn smask_pdf(px: &image::RgbImage) -> Document {
    let mask_id: ObjectId = (901, 0);
    let mut mask = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>px.width() as i64,
        "Height"=>px.height() as i64,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8},
        vec![255u8; (px.width() * px.height()) as usize],
    );
    mask.compress().unwrap();
    let mut s = flate_rgb_stream(px, "DeviceRGB".into());
    s.dict.set("SMask", Object::Reference(mask_id));
    image_pdf(s, vec![(mask_id, mask.into())])
}

fn device_rgb_flate_pdf(px: &image::RgbImage) -> Document {
    image_pdf(flate_rgb_stream(px, "DeviceRGB".into()), vec![])
}

fn jpeg_q100_pdf(px: &image::RgbImage) -> Document {
    let mut jpeg = vec![];
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
        .encode_image(px)
        .unwrap();
    image_pdf(
        Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>px.width() as i64,
            "Height"=>px.height() as i64,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,
            "Filter"=>"DCTDecode"},
            jpeg,
        ),
        vec![],
    )
}

// ---------------------------------------------------------------- helpers

struct Lab {
    dir: tempfile::TempDir,
}
impl Lab {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn write(&self, name: &str, doc: &mut Document) -> PathBuf {
        let p = self.path(name);
        doc.save(&p).unwrap();
        p
    }
    fn write_bytes(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.path(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

/// Gerçek alım fişi: kabuk çıktıyı tam bu yoldan yeniden açar.
fn reopen(path: &Path) -> Document {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("çıktı okunamadı {path:?}: {e}"));
    assert!(!bytes.is_empty(), "çıktı 0 bayt: {path:?}");
    let receipt = scan_source_files(&[path]);
    assert!(
        receipt.errors.is_empty() && receipt.sources.len() == 1,
        "alım fişi başarısız {path:?}: {:?}",
        receipt.errors
    );
    let loaded = load_pdf_tolerant(&bytes, "cikti.pdf").unwrap();
    assert!(!loaded.is_repaired, "çıktı onarım gerektirdi: {path:?}");
    loaded.document
}

fn markers(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    for (_, id) in doc.get_pages() {
        let content = doc.get_page_content(id).unwrap();
        let text = String::from_utf8_lossy(&content);
        let m = text
            .split("(Sayfa ")
            .nth(1)
            .and_then(|s| s.split(')').next())
            .map(|s| format!("Sayfa {s}"))
            .unwrap_or_default();
        out.push(m);
    }
    out
}

fn rotation(doc: &Document, page: u32) -> i64 {
    pdf::resolved_page_dictionary(doc, doc.get_pages()[&page])
        .unwrap()
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
}

fn boxes(doc: &Document, page: u32) -> ([f32; 4], Option<[f32; 4]>) {
    let d = pdf::resolved_page_dictionary(doc, doc.get_pages()[&page]).unwrap();
    let arr = |k: &[u8]| {
        d.get(k).ok().map(|o| {
            let a = doc.dereference(o).unwrap().1.as_array().unwrap();
            [
                a[0].as_float().unwrap(),
                a[1].as_float().unwrap(),
                a[2].as_float().unwrap(),
                a[3].as_float().unwrap(),
            ]
        })
    };
    (arr(b"MediaBox").unwrap(), arr(b"CropBox"))
}

/// Marka payı: her türetilmiş sayfanın altına eklenen şerit (stamp.rs).
const BRAND_GUTTER: f32 = 8.0 * 2.0 + 8.0 + 10.0;

fn run(
    lab: &Lab,
    sources: &[PathBuf],
    op: ToolOperation,
    out: &str,
    approved: bool,
) -> (ToolOutcome, PathBuf) {
    let out_path = lab.path(out);
    let before: Vec<_> = sources
        .iter()
        .map(|p| calculate_sha256(p).unwrap())
        .collect();
    let outcome = run_tool_with_outcome(sources, &op, &out_path, approved).unwrap();
    let after: Vec<_> = sources
        .iter()
        .map(|p| calculate_sha256(p).unwrap())
        .collect();
    assert_eq!(before, after, "KAYNAK DEĞİŞTİ: {sources:?}");
    // Geçici dosya kalmaz.
    let leftovers: Vec<_> = std::fs::read_dir(lab.dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".tmp") || n.contains(".duzenek-"))
        .collect();
    assert!(leftovers.is_empty(), "geçici dosya kaldı: {leftovers:?}");
    (outcome, out_path)
}

fn published(outcome: &ToolOutcome) -> u64 {
    match outcome {
        ToolOutcome::Published { output_bytes } | ToolOutcome::Compressed { output_bytes, .. } => {
            *output_bytes
        }
        other => panic!("yayın bekleniyordu, gelen: {other:?}"),
    }
}

// ==================================================================== SAVE (P0)

#[test]
fn save_select_pages_publishes_reopenable_pdf_with_exact_order() {
    let lab = Lab::new();
    let src = lab.write("dort.pdf", &mut vector_pdf(&[A4, A4_LANDSCAPE, LETTER, A4]));
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Select { pages: vec![4, 2] },
        "secili.pdf",
        false,
    );
    let bytes = published(&o);
    assert_eq!(bytes, std::fs::metadata(&out).unwrap().len());
    let d = reopen(&out);
    assert_eq!(
        markers(&d),
        ["Sayfa 4", "Sayfa 2"],
        "seçim sırası korunmalı"
    );
    let (m, c) = boxes(&d, 2);
    assert_eq!(
        m[2], A4_LANDSCAPE[0],
        "yatay sayfanın MediaBox genişliği korunmalı"
    );
    assert!(
        c.is_some(),
        "türetilmiş sayfa görünür alanı (CropBox) taşımalı"
    );
}

#[test]
fn save_refuses_existing_target_and_leaves_it_untouched() {
    let lab = Lab::new();
    let src = lab.write("a.pdf", &mut vector_pdf(&[A4]));
    let existing = lab.write_bytes("var.pdf", b"ONCEKI ICERIK");
    let r = run_tool_with_outcome(
        &[src],
        &ToolOperation::Select { pages: vec![1] },
        &existing,
        false,
    );
    assert!(r.is_err(), "mevcut hedefin üzerine yazılmamalı");
    assert_eq!(std::fs::read(&existing).unwrap(), b"ONCEKI ICERIK");
}

#[test]
fn save_into_folder_with_turkish_unicode_and_spaces() {
    let lab = Lab::new();
    let folder = lab.path("Belgeler/Müvekkil Dosyası — İş Sözleşmesi");
    std::fs::create_dir_all(&folder).unwrap();
    let src = lab.write("ğüşiöç kaynak.pdf", &mut vector_pdf(&[A4, LETTER]));
    let out = folder.join("GolgeDosya-select-2026-09-14T10-00-00-000Z.pdf");
    let outcome = run_tool_with_outcome(
        std::slice::from_ref(&src),
        &ToolOperation::Select { pages: vec![2, 1] },
        &out,
        false,
    )
    .unwrap();
    published(&outcome);
    assert_eq!(markers(&reopen(&out)), ["Sayfa 2", "Sayfa 1"]);
}

#[test]
fn receipt_scan_requires_pdf_extension_so_frontend_must_normalise_destination() {
    // Kabuk kaydettiği dosyayı tarayıcıyla yeniden açar; tarayıcı biçimi
    // UZANTIDAN okur. Kullanıcı kaydetme penceresinde uzantıyı silerse dosya
    // yazılır ama fiş düşer ve kullanıcı "kaydedilemedi" görür.
    let lab = Lab::new();
    let src = lab.write("a.pdf", &mut vector_pdf(&[A4]));
    let out = lab.path("uzantisiz");
    run_tool_with_outcome(
        &[src],
        &ToolOperation::Select { pages: vec![1] },
        &out,
        false,
    )
    .unwrap();
    assert!(out.exists(), "dosya gerçekten yazıldı");
    let receipt = scan_source_files(&[&out]);
    assert!(
        !receipt.errors.is_empty(),
        "tarayıcı uzantısız PDF'yi reddeder — kabuk hedefi .pdf ile bitirmek ZORUNDA"
    );
}

// ============================================================ PAGE EDITING (P1)

#[test]
fn reorder_first_to_last_and_last_to_first() {
    let lab = Lab::new();
    let src = lab.write("dort.pdf", &mut vector_pdf(&[A4, A4, A4, A4]));
    let (_, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Reorder {
            pages: vec![2, 3, 4, 1],
        },
        "ilk-sona.pdf",
        false,
    );
    assert_eq!(
        markers(&reopen(&out)),
        ["Sayfa 2", "Sayfa 3", "Sayfa 4", "Sayfa 1"]
    );
    let (_, out) = run(
        &lab,
        &[src],
        ToolOperation::Reorder {
            pages: vec![4, 1, 2, 3],
        },
        "son-basa.pdf",
        false,
    );
    assert_eq!(
        markers(&reopen(&out)),
        ["Sayfa 4", "Sayfa 1", "Sayfa 2", "Sayfa 3"]
    );
}

#[test]
fn reorder_rejects_duplicates_and_partial_permutations() {
    let lab = Lab::new();
    let src = lab.write("dort.pdf", &mut vector_pdf(&[A4, A4, A4, A4]));
    for pages in [vec![1, 1, 2, 3], vec![1, 2, 3], vec![1, 2, 3, 5], vec![]] {
        let out = lab.path("x.pdf");
        assert!(
            run_tool_with_outcome(
                std::slice::from_ref(&src),
                &ToolOperation::Reorder {
                    pages: pages.clone()
                },
                &out,
                false
            )
            .is_err(),
            "kabul edilmemeli: {pages:?}"
        );
        assert!(!out.exists(), "reddedilen işlem dosya bırakmamalı");
    }
}

#[test]
fn delete_single_multiple_and_never_all() {
    let lab = Lab::new();
    let src = lab.write("dort.pdf", &mut vector_pdf(&[A4, A4, A4, A4]));
    let (_, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Delete { pages: vec![2] },
        "tek.pdf",
        false,
    );
    assert_eq!(markers(&reopen(&out)), ["Sayfa 1", "Sayfa 3", "Sayfa 4"]);
    let (_, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Delete { pages: vec![1, 4] },
        "iki.pdf",
        false,
    );
    assert_eq!(markers(&reopen(&out)), ["Sayfa 2", "Sayfa 3"]);
    let out = lab.path("hepsi.pdf");
    assert!(run_tool_with_outcome(
        &[src],
        &ToolOperation::Delete {
            pages: vec![1, 2, 3, 4]
        },
        &out,
        false
    )
    .is_err());
    assert!(!out.exists());
}

#[test]
fn rotate_per_page_left_right_180_and_repeated() {
    let lab = Lab::new();
    let src = lab.write("uc.pdf", &mut vector_pdf(&[A4, A4_LANDSCAPE, A4]));
    let rot = |p: usize, d: i32| PageRotation {
        page: p,
        degrees: d,
    };
    // Kabuk döndürmeyi (delta+360)%360 olarak biriktirir: sol = 270.
    let (_, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::RotatePages {
            rotations: vec![rot(1, 270), rot(2, 90), rot(3, 180)],
        },
        "r1.pdf",
        false,
    );
    let d = reopen(&out);
    assert_eq!(
        [rotation(&d, 1), rotation(&d, 2), rotation(&d, 3)],
        [270, 90, 180]
    );
    // Tekrarlı: çıktıyı yeniden döndür → 270+90=0, 90+90=180, 180+180=0.
    let (_, out2) = run(
        &lab,
        &[out],
        ToolOperation::RotatePages {
            rotations: vec![rot(1, 90), rot(2, 90), rot(3, 180)],
        },
        "r2.pdf",
        false,
    );
    let d2 = reopen(&out2);
    assert_eq!(
        [rotation(&d2, 1), rotation(&d2, 2), rotation(&d2, 3)],
        [0, 180, 0]
    );
    // Geometri: dönüş bir öznitelik olarak taşınır, kutu yeniden örneklenmez.
    // Marka payı görsel alt kenara eklenir: iki türetmede 180° dönük sayfada
    // ilk pay (o an 90°) x eksenine, ikinci pay (o an 180°) üst kenara gelir;
    // toplam alan = özgün + 2 pay.
    let (m, _) = boxes(&d2, 2);
    let area_growth = (m[2] - m[0]) * (m[3] - m[1]) - A4_LANDSCAPE[0] * A4_LANDSCAPE[1];
    let expected = BRAND_GUTTER * A4_LANDSCAPE[1] + BRAND_GUTTER * (A4_LANDSCAPE[0] + BRAND_GUTTER);
    assert!(
        (area_growth - expected).abs() < 1.0,
        "beklenmeyen kutu büyümesi: {area_growth} vs {expected}"
    );
}

#[test]
fn inherited_rotate_survives_select_reorder_delete() {
    // Sayfa 3–4 mirasla 90° dönük. Kopyalama bu mirası sayfaya indirmezse
    // kullanıcı önizlemede dik gördüğü sayfayı çıktıda yatık alır.
    let lab = Lab::new();
    let src = lab.write("miras.pdf", &mut nested_inherited_rotate_pdf());
    let d0 = reopen(&src);
    assert_eq!(
        [rotation(&d0, 3), rotation(&d0, 4)],
        [90, 90],
        "fixture mirası taşımalı"
    );
    let (_, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Select { pages: vec![3, 1] },
        "sec.pdf",
        false,
    );
    let d = reopen(&out);
    assert_eq!(markers(&d), ["Sayfa 3", "Sayfa 1"]);
    assert_eq!(
        [rotation(&d, 1), rotation(&d, 2)],
        [90, 0],
        "mirasla gelen dönüş seçimde korunmalı"
    );
    let (_, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Reorder {
            pages: vec![4, 3, 2, 1],
        },
        "sira.pdf",
        false,
    );
    let d = reopen(&out);
    assert_eq!(
        [
            rotation(&d, 1),
            rotation(&d, 2),
            rotation(&d, 3),
            rotation(&d, 4)
        ],
        [90, 90, 0, 0]
    );
    let (_, out) = run(
        &lab,
        &[src],
        ToolOperation::Delete { pages: vec![1, 2] },
        "sil.pdf",
        false,
    );
    let d = reopen(&out);
    assert_eq!([rotation(&d, 1), rotation(&d, 2)], [90, 90]);
}

#[test]
fn rotate_on_inherited_rotation_accumulates_not_replaces() {
    let lab = Lab::new();
    let src = lab.write("miras.pdf", &mut nested_inherited_rotate_pdf());
    let (_, out) = run(
        &lab,
        &[src],
        ToolOperation::RotatePages {
            rotations: vec![PageRotation {
                page: 3,
                degrees: 90,
            }],
        },
        "r.pdf",
        false,
    );
    let d = reopen(&out);
    assert_eq!(rotation(&d, 3), 180, "90 (miras) + 90 = 180");
    assert_eq!(rotation(&d, 4), 90, "dokunulmayan sayfa mirasını korur");
}

#[test]
fn select_with_cross_page_link_does_not_drag_whole_document_along() {
    // İçindekiler bağlantısı: 1. sayfadan 3. sayfaya. Tek sayfa seçildiğinde
    // çıktı bütün belgeyi yetim nesne olarak taşımamalı.
    let lab = Lab::new();
    let mut big = vector_pdf(&[A4; 40]);
    // 40 sayfayı ağırlaştır: her sayfaya 20 KB'lık benzersiz içerik.
    for (_, id) in big.get_pages() {
        let cid = big
            .get_dictionary(id)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_reference()
            .unwrap();
        let mut s = big.get_object(cid).unwrap().as_stream().unwrap().clone();
        let mut pad = s.content.clone();
        pad.extend(std::iter::repeat_n(b'%', 20_000));
        s.set_content(pad);
        big.objects.insert(cid, s.into());
    }
    add_cross_page_link(&mut big, 1, 3);
    let src = lab.write("toc.pdf", &mut big);
    let src_size = std::fs::metadata(&src).unwrap().len();
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Select { pages: vec![1] },
        "tek.pdf",
        false,
    );
    let size = published(&o);
    let d = reopen(&out);
    assert_eq!(d.get_pages().len(), 1);
    // Tek sayfa 40 sayfalık kaynağın onda birinden büyük olmamalı.
    assert!(
        size * 10 < src_size,
        "tek sayfalık çıktı kaynağın %{}'i: bağlantı hedefi belgeyi sürükledi",
        size * 100 / src_size
    );
}

// =============================================================== COMPRESSION (P0/P1)

#[test]
fn compress_icc_based_scan_must_compress_not_fail() {
    // Gerçek dünyanın normali. DeviceRGB yerine [/ICCBased] olduğu için motor
    // "0 desteklenen görsel" diyorsa kullanıcı "sıkıştırmıyor" der.
    let lab = Lab::new();
    let px = scan_pixels(2000, 2800);
    let src = lab.write("icc.pdf", &mut icc_pdf(&px));
    let src_size = std::fs::metadata(&src).unwrap().len();
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "icc-sik.pdf",
        false,
    );
    match &o {
        ToolOutcome::Compressed {
            output_bytes,
            images_found,
            images_recompressed,
            ..
        } => {
            assert_eq!(*images_found, 1);
            assert_eq!(
                *images_recompressed, 1,
                "ICCBased RGB görsel yeniden kodlanmalı"
            );
            assert!(
                *output_bytes * 100 < src_size * 97,
                "en az %3 küçülmeli: {src_size} → {output_bytes}"
            );
            let d = reopen(&out);
            assert_eq!(d.get_pages().len(), 1);
            println!(
                "compress ICCBased Flate: {src_size} → {output_bytes} (%{})",
                100 - output_bytes * 100 / src_size
            );
        }
        other => panic!("ICCBased tarama sıkıştırılamadı: {other:?}"),
    }
}

#[test]
fn compress_device_rgb_flate_scan_compresses() {
    let lab = Lab::new();
    let px = scan_pixels(2000, 2800);
    let src = lab.write("rgb.pdf", &mut device_rgb_flate_pdf(&px));
    let src_size = std::fs::metadata(&src).unwrap().len();
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "rgb-sik.pdf",
        false,
    );
    let bytes = published(&o);
    assert!(bytes * 100 < src_size * 97);
    let d = reopen(&out);
    let (m, _) = boxes(&d, 1);
    // Marka payı ALT kenara eklenir (y0 aşağı iner); genişlik değişmez.
    assert!(
        (m[2] - m[0] - 595.).abs() < 0.01,
        "genişlik değişmemeli: {m:?}"
    );
    assert!(
        (m[3] - m[1] - (842. + BRAND_GUTTER)).abs() < 0.01,
        "yalnız marka payı eklenir: {m:?}"
    );
    println!(
        "compress DeviceRGB Flate: {src_size} → {bytes} (%{})",
        100 - bytes * 100 / src_size
    );
}

#[test]
fn compress_already_optimized_reports_no_benefit_and_writes_nothing() {
    let lab = Lab::new();
    let src = lab.write("vektor.pdf", &mut vector_pdf(&[A4, A4]));
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "nb.pdf",
        false,
    );
    assert!(
        matches!(o, ToolOutcome::NoBenefit { .. }),
        "vektör belge: {o:?}"
    );
    assert!(!out.exists(), "NoBenefit çıktı bırakmamalı");
}

#[test]
fn compress_jpeg_q100_compresses_and_never_publishes_larger() {
    let lab = Lab::new();
    let px = scan_pixels(2000, 2800);
    let src = lab.write("jpg.pdf", &mut jpeg_q100_pdf(&px));
    let src_size = std::fs::metadata(&src).unwrap().len();
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::GentleCompression,
        },
        "jpg-sik.pdf",
        false,
    );
    match o {
        ToolOutcome::Compressed { output_bytes, .. } => assert!(output_bytes < src_size),
        ToolOutcome::NoBenefit { .. } => assert!(!out.exists()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn compress_smask_image_falls_back_to_cleanup_or_no_benefit_not_failed() {
    // Alfa maskeli görsel güvenli yeniden kodlama kapsamı dışında. Bu bir
    // HATA değil: motor en azından temizlik adayını denemeli ve dürüstçe
    // NoBenefit demeli.
    let lab = Lab::new();
    let px = scan_pixels(800, 1000);
    let src = lab.write("smask.pdf", &mut smask_pdf(&px));
    let (o, _) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "smask-sik.pdf",
        false,
    );
    assert!(
        !matches!(o, ToolOutcome::Failed { .. }),
        "maske taşıyan görsel sıkıştırmayı çökertmemeli: {o:?}"
    );
    println!("compress SMask: {o:?}");
}

#[test]
fn compress_corrupt_input_fails_cleanly_with_no_output() {
    let lab = Lab::new();
    let mut whole = Vec::new();
    vector_pdf(&[A4, A4]).save_to(&mut whole).unwrap();
    let src = lab.write_bytes("bozuk.pdf", &whole[..whole.len() / 2]);
    let out = lab.path("bozuk-sik.pdf");
    let o = run_tool_with_outcome(
        &[src],
        &ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        &out,
        false,
    )
    .unwrap();
    assert!(matches!(o, ToolOutcome::Failed { .. }), "{o:?}");
    assert!(!out.exists());
    if let ToolOutcome::Failed { reason } = o {
        for leak in ["panicked", "src/", ".rs:", "os error"] {
            assert!(!reason.contains(leak), "teknik sızıntı: {reason}");
        }
    }
}

// ================================================================== BRANDING

#[test]
fn branding_growth_is_bounded_for_1_10_100_pages() {
    let lab = Lab::new();
    let mut sizes = Vec::new();
    for n in [1usize, 10, 100] {
        let src = lab.write(&format!("v{n}.pdf"), &mut vector_pdf(&vec![A4; n]));
        let before = std::fs::metadata(&src).unwrap().len();
        let (o, _) = run(
            &lab,
            &[src],
            ToolOperation::Reorder {
                pages: (1..=n).collect(),
            },
            &format!("v{n}-out.pdf"),
            false,
        );
        let after = published(&o);
        sizes.push((n, before, after));
    }
    let per_page = |(n, b, a): &(usize, u64, u64)| (*a as i64 - *b as i64) as f64 / *n as f64;
    let g1 = per_page(&sizes[0]);
    let g100 = per_page(&sizes[2]);
    println!("branding growth: {sizes:?}  per-page: 1→{g1:.0}  100→{g100:.0}");
    // Paylaşılan Form XObject: 100 sayfada sayfa başı büyüme, tek sayfadakinin
    // kesrinde kalmalı (sabit maliyet amortize olur).
    assert!(
        g100 < g1 * 0.5,
        "marka her sayfaya yeniden gömülüyor: sayfa başı {g100:.0} B"
    );
    assert!(
        sizes[2].2 - sizes[2].1 < 60_000,
        "100 sayfa için marka toplam 60 KB'yi aşmamalı"
    );
}

// =================================================================== MERGE

#[test]
fn merge_two_and_three_documents_preserves_order_geometry_rotation() {
    let lab = Lab::new();
    let a = lab.write("a.pdf", &mut vector_pdf(&[A4, LETTER]));
    let b = lab.write("b.pdf", &mut nested_inherited_rotate_pdf());
    let c = lab.write("c.pdf", &mut vector_pdf(&[A4_LANDSCAPE]));
    let (_, out) = run(
        &lab,
        &[a.clone(), b.clone()],
        ToolOperation::Merge,
        "ab.pdf",
        false,
    );
    let d = reopen(&out);
    assert_eq!(d.get_pages().len(), 6);
    assert_eq!(
        markers(&d),
        ["Sayfa 1", "Sayfa 2", "Sayfa 1", "Sayfa 2", "Sayfa 3", "Sayfa 4"]
    );
    assert_eq!(
        [rotation(&d, 5), rotation(&d, 6)],
        [90, 90],
        "birleştirmede miras dönüş korunmalı"
    );
    let (m, _) = boxes(&d, 2);
    assert_eq!(m[2], LETTER[0]);
    let (_, out) = run(&lab, &[a, b, c], ToolOperation::Merge, "abc.pdf", false);
    let d = reopen(&out);
    assert_eq!(d.get_pages().len(), 7);
    let (m, _) = boxes(&d, 7);
    assert_eq!(m[2], A4_LANDSCAPE[0]);
}

#[test]
fn merge_with_encrypted_input_fails_safely() {
    let lab = Lab::new();
    let a = lab.write("a.pdf", &mut vector_pdf(&[A4]));
    let enc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/encrypted.pdf");
    let out = lab.path("m.pdf");
    let r = run_tool_with_outcome(&[a, enc.clone()], &ToolOperation::Merge, &out, false);
    assert!(r.is_err(), "şifreli girdi kabul edilmemeli");
    assert!(!out.exists());
}

// ================================================================= NEGATIVES

#[test]
fn negative_inputs_never_crash_mutate_or_leave_partial_output() {
    let lab = Lab::new();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("bos.pdf", vec![]),
        ("metin.pdf", b"bu bir pdf degil".to_vec()),
        ("yarim.pdf", {
            let mut w = Vec::new();
            vector_pdf(&[A4, A4]).save_to(&mut w).unwrap();
            w.truncate(w.len() * 2 / 3);
            w
        }),
    ];
    for (name, bytes) in cases {
        let src = lab.write_bytes(name, &bytes);
        let hash = calculate_sha256(&src).unwrap();
        for op in [
            ToolOperation::Select { pages: vec![1] },
            ToolOperation::Rotate { degrees: 90 },
            ToolOperation::Compress {
                level: Level::GentleCompression,
            },
        ] {
            let out = lab.path(&format!("{name}.out.pdf"));
            let r = run_tool_with_outcome(std::slice::from_ref(&src), &op, &out, false);
            match r {
                Ok(ToolOutcome::Failed { .. }) | Err(_) => {}
                Ok(other) => panic!("{name} {op:?}: başarı iddiası {other:?}"),
            }
            assert!(!out.exists(), "{name}: kısmi çıktı kaldı");
            assert_eq!(hash, calculate_sha256(&src).unwrap());
        }
    }
    // Şifreli
    let enc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/encrypted.pdf");
    let out = lab.path("enc.pdf");
    assert!(run_tool_with_outcome(
        &[enc],
        &ToolOperation::Select { pages: vec![1] },
        &out,
        false
    )
    .is_err());
    assert!(!out.exists());
}

#[test]
fn signed_source_requires_approval_then_publishes() {
    let lab = Lab::new();
    let signed = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/signed-marker.pdf");
    let out = lab.path("imzali.pdf");
    assert!(run_tool_with_outcome(
        std::slice::from_ref(&signed),
        &ToolOperation::Select { pages: vec![1] },
        &out,
        false
    )
    .is_err());
    assert!(!out.exists());
    let o = run_tool_with_outcome(
        &[signed],
        &ToolOperation::Select { pages: vec![1] },
        &out,
        true,
    )
    .unwrap();
    published(&o);
    reopen(&out);
}

// ================================================================ CHAINS (§16)

#[test]
fn chain_a_open_rotate_reorder_delete_save_reopen_each_step() {
    let lab = Lab::new();
    let src = lab.write(
        "kaynak.pdf",
        &mut vector_pdf(&[A4, A4_LANDSCAPE, LETTER, A4]),
    );
    let (_, s1) = run(
        &lab,
        &[src],
        ToolOperation::RotatePages {
            rotations: vec![PageRotation {
                page: 2,
                degrees: 90,
            }],
        },
        "1-rot.pdf",
        false,
    );
    let (_, s2) = run(
        &lab,
        &[s1],
        ToolOperation::Reorder {
            pages: vec![4, 3, 2, 1],
        },
        "2-sira.pdf",
        false,
    );
    let (_, s3) = run(
        &lab,
        &[s2],
        ToolOperation::Delete { pages: vec![1] },
        "3-sil.pdf",
        false,
    );
    let d = reopen(&s3);
    assert_eq!(markers(&d), ["Sayfa 3", "Sayfa 2", "Sayfa 1"]);
    assert_eq!(
        [rotation(&d, 1), rotation(&d, 2), rotation(&d, 3)],
        [0, 90, 0],
        "döndürülen sayfa sıralama ve silmeden sonra hâlâ 90°"
    );
    // 90° dönük sayfada marka payı görsel alt kenara, yani x eksenine gelir.
    let (m, _) = boxes(&d, 2);
    assert!(
        (m[3] - m[1] - A4_LANDSCAPE[1]).abs() < 0.05,
        "dönük sayfanın yüksekliği korunmalı: {m:?}"
    );
    assert!(
        (m[2] - m[0] - (A4_LANDSCAPE[0] + 3.0 * BRAND_GUTTER)).abs() < 0.05,
        "üç türetme = x ekseninde üç pay: {m:?}"
    );
    // Dik sayfada üç türetme = alt kenarda üç pay.
    let (m1, _) = boxes(&d, 1);
    assert!(
        (m1[3] - m1[1] - (LETTER[1] + 3.0 * BRAND_GUTTER)).abs() < 0.05,
        "üç türetme = üç marka payı: {m1:?}"
    );
}

#[test]
fn chain_e_merge_reorder_compress() {
    let lab = Lab::new();
    let px = scan_pixels(1200, 1600);
    let a = lab.write("a.pdf", &mut icc_pdf(&px));
    let b = lab.write("b.pdf", &mut vector_pdf(&[A4, A4]));
    let (_, m) = run(&lab, &[a, b], ToolOperation::Merge, "m.pdf", false);
    assert_eq!(reopen(&m).get_pages().len(), 3);
    let (_, r) = run(
        &lab,
        &[m],
        ToolOperation::Reorder {
            pages: vec![3, 1, 2],
        },
        "r.pdf",
        false,
    );
    let d = reopen(&r);
    assert_eq!(d.get_pages().len(), 3);
    let size_r = std::fs::metadata(&r).unwrap().len();
    let (o, c) = run(
        &lab,
        &[r],
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "c.pdf",
        false,
    );
    match o {
        ToolOutcome::Compressed { output_bytes, .. } => {
            assert!(output_bytes < size_r);
            assert_eq!(reopen(&c).get_pages().len(), 3);
        }
        other => panic!("zincir sonu sıkıştırılamadı: {other:?}"),
    }
}

// ================================================================ PERFORMANCE

#[test]
fn hundred_page_reorder_completes_reasonably() {
    let lab = Lab::new();
    let src = lab.write("yuz.pdf", &mut vector_pdf(&vec![A4; 100]));
    let t = std::time::Instant::now();
    let (_, out) = run(
        &lab,
        &[src],
        ToolOperation::Reorder {
            pages: (1..=100).rev().collect(),
        },
        "ters.pdf",
        false,
    );
    let elapsed = t.elapsed();
    let d = reopen(&out);
    assert_eq!(markers(&d)[0], "Sayfa 100");
    assert_eq!(markers(&d)[99], "Sayfa 1");
    println!("100 sayfa ters sıralama: {elapsed:?}");
    assert!(elapsed.as_secs() < 20, "100 sayfa {elapsed:?} sürdü");
}

// ============================================ PREVIEW ↔ OUTPUT PARITY (§13)

/// Sayfaları ayırt edilebilir düz renklerle dolduran fixture: raster çıktıda
/// sıra ve dönüş piksel okuyarak doğrulanır.
fn colour_pdf(sizes: &[[f32; 2]]) -> Document {
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let mut kids = Vec::new();
    for (i, [w, h]) in sizes.iter().enumerate() {
        // Sayfa i: kırmızı kanal 40·(i+1); sol üst köşede koyu bir işaret
        // (dönüş yönünü ele verir).
        let r = 0.16 * (i as f32 + 1.0);
        let ops = format!(
            "{r:.3} 0.4 0.8 rg 0 0 {w} {h} re f 0.05 0.05 0.05 rg 0 {} {} {} re f",
            h - h * 0.15,
            w * 0.15,
            h * 0.15
        );
        let contents = d.add_object(Stream::new(dictionary! {}, ops.into_bytes()));
        let page = d.add_object(dictionary! {
            "Type"=>"Page","Parent"=>pages_id,"MediaBox"=>media(*w,*h),
            "Resources"=>dictionary!{},"Contents"=>contents,
        });
        kids.push(Object::Reference(page));
    }
    d.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>kids.clone(),"Count"=>kids.len() as i64}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    d.trailer.set("Root", root);
    d
}

#[cfg(target_os = "macos")]
fn mae_top_left(reference: &image::RgbImage, candidate: &image::RgbImage) -> f64 {
    // Çıktının altına marka payı eklenir; karşılaştırma önizlemenin kapladığı
    // sol-üst bölgeyle sınırlıdır (son iki satır kenar yumuşatması nedeniyle atlanır).
    let w = reference.width().min(candidate.width());
    let h = reference.height().min(candidate.height()).saturating_sub(2);
    let mut delta = 0u64;
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                delta += reference.get_pixel(x, y)[c].abs_diff(candidate.get_pixel(x, y)[c]) as u64;
            }
        }
    }
    delta as f64 / (w as f64 * h as f64 * 3.0)
}

#[cfg(target_os = "macos")]
#[test]
fn preview_matches_saved_output_for_rotation_and_order() {
    use ekler_core::raster;
    let lab = Lab::new();
    let src = lab.write("renk.pdf", &mut colour_pdf(&[A4, A4_LANDSCAPE, LETTER]));
    // Kullanıcı önizlemede 2. sayfayı 90° döndürmüş, sonra sırayı 3-2-1 yapmış
    // ve iki adımda kaydetmiş görsün.
    let (_, rot) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::RotatePages {
            rotations: vec![PageRotation {
                page: 2,
                degrees: 90,
            }],
        },
        "rot.pdf",
        false,
    );
    let (_, ord) = run(
        &lab,
        &[rot],
        ToolOperation::Reorder {
            pages: vec![3, 2, 1],
        },
        "ord.pdf",
        false,
    );
    let folder = raster::pdf_to_images(&ord, lab.dir.path(), "png", 72, false).unwrap();
    let out = |n: usize| {
        image::open(folder.join(format!("sayfa-{n:04}.png")))
            .unwrap()
            .to_rgb8()
    };
    // Çıktı sırası: 3, 2(90°), 1 — önizleme kaynaktan aynı dönüşle üretilir.
    for (n, src_page, rotation) in [(1, 3, 0), (2, 2, 90), (3, 1, 0)] {
        let preview =
            image::load_from_memory(&raster::preview_page(&src, src_page, 72, rotation).unwrap())
                .unwrap()
                .to_rgb8();
        let saved = out(n);
        assert_eq!(
            preview.width(),
            saved.width(),
            "sayfa {n}: genişlik önizlemeyle aynı olmalı"
        );
        let mae = mae_top_left(&preview, &saved);
        assert!(
            mae < 3.0,
            "sayfa {n}: önizleme ile çıktı ayrışıyor (MAE {mae:.2})"
        );
    }
    // Döndürülmüş yatay sayfa çıktıda dik durur (yükseklik > genişlik).
    let p2 = out(2);
    assert!(
        p2.height() > p2.width(),
        "90° dönük yatay sayfa dik olmalı: {}x{}",
        p2.width(),
        p2.height()
    );
}

// ======================================================= PDF → PNG / JPG (§18)

#[cfg(target_os = "macos")]
#[test]
fn export_images_every_page_in_order_with_dpi_and_rotation() {
    use ekler_core::raster;
    let lab = Lab::new();
    let mut doc = colour_pdf(&[A4, A4_LANDSCAPE, LETTER, A4]);
    // 4. sayfa mirasla değil doğrudan 270° dönük.
    let p4 = doc.get_pages()[&4];
    doc.get_dictionary_mut(p4).unwrap().set("Rotate", 270);
    let src = lab.write("dort.pdf", &mut doc);
    let hash = calculate_sha256(&src).unwrap();
    for (format, dpi) in [("png", 72u32), ("jpg", 150)] {
        let folder = raster::pdf_to_images(&src, lab.dir.path(), format, dpi, false).unwrap();
        let names: Vec<_> = {
            let mut v: Vec<_> = std::fs::read_dir(&folder)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        };
        assert_eq!(
            names,
            (1..=4)
                .map(|i| format!("sayfa-{i:04}.{format}"))
                .collect::<Vec<_>>(),
            "her sayfa, sırayla, öngörülebilir adla"
        );
        for (i, [w, h]) in [A4, A4_LANDSCAPE, LETTER, A4].iter().enumerate() {
            let img = image::open(folder.join(&names[i])).unwrap().to_rgb8();
            let scale = dpi as f32 / 72.0;
            let (ew, eh) = if i == 3 { (*h, *w) } else { (*w, *h) };
            // Genişlik sayfa kutusundan; yükseklik marka payı kadar büyük.
            assert_eq!(
                img.width(),
                (ew * scale).ceil() as u32,
                "{format} sayfa {} genişlik",
                i + 1
            );
            assert_eq!(
                img.height(),
                ((eh + BRAND_GUTTER) * scale).ceil() as u32,
                "{format} sayfa {} yükseklik",
                i + 1
            );
            // Sayfa rengi sırayı kanıtlar: kırmızı kanal 0.16·(i+1)·255.
            let px = img.get_pixel(img.width() / 2, img.height() / 2)[0] as i32;
            let expected = (0.16 * (i as f32 + 1.0) * 255.0) as i32;
            assert!(
                (px - expected).abs() < 12,
                "{format} sayfa {}: renk {px}, beklenen ~{expected} — sıra bozuk",
                i + 1
            );
        }
        assert_eq!(calculate_sha256(&src).unwrap(), hash);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn export_images_refuses_annotated_pages_without_partial_output() {
    use ekler_core::raster;
    let lab = Lab::new();
    let mut doc = vector_pdf(&[A4, A4, A4]);
    add_cross_page_link(&mut doc, 1, 3);
    let src = lab.write("ek.pdf", &mut doc);
    let before = std::fs::read_dir(lab.dir.path()).unwrap().count();
    assert!(raster::pdf_to_images(&src, lab.dir.path(), "png", 72, false).is_err());
    let after = std::fs::read_dir(lab.dir.path()).unwrap().count();
    assert_eq!(before, after, "reddedilen dışa aktarma klasör bırakmamalı");
}

// ============================================================ IMAGES → PDF (§19)

#[test]
fn images_to_pdf_png_jpeg_portrait_landscape_fit_a4() {
    let lab = Lab::new();
    let portrait = image::RgbImage::from_fn(600, 900, |x, y| {
        image::Rgb([(x / 3) as u8, (y / 4) as u8, 90])
    });
    let landscape =
        image::RgbImage::from_fn(1200, 500, |x, _| image::Rgb([200, (x / 5) as u8, 40]));
    let p_png = lab.path("dik.png");
    portrait.save(&p_png).unwrap();
    let l_jpg = lab.path("yatay.jpg");
    landscape.save(&l_jpg).unwrap();
    let (_, out) = run(
        &lab,
        &[p_png, l_jpg],
        ToolOperation::Images,
        "gorseller.pdf",
        false,
    );
    let d = reopen(&out);
    assert_eq!(d.get_pages().len(), 2);
    for n in 1..=2 {
        let (m, _) = boxes(&d, n);
        assert!(
            (m[2] - m[0] - A4[0]).abs() < 0.05,
            "sayfa {n} A4 genişliğinde: {m:?}"
        );
        assert!(
            (m[3] - m[1] - (A4[1] + BRAND_GUTTER)).abs() < 0.05,
            "sayfa {n} A4 + marka payı: {m:?}"
        );
    }
    // Çizim matrisi: dik görsel yüksekliği, yatay görsel genişliği doldurur.
    let content = |n: u32| {
        String::from_utf8_lossy(&d.get_page_content(d.get_pages()[&n]).unwrap()).into_owned()
    };
    let avail_w = 595.28 - 72.0;
    let avail_h = 841.89 - 72.0;
    assert!(
        content(1).contains(&format!("{avail_h}"))
            || content(1).contains(&format!("{:.2}", avail_h)),
        "dik görsel yüksekliği doldurmalı: {}",
        content(1)
    );
    assert!(
        content(2).contains(&format!("{avail_w}"))
            || content(2).contains(&format!("{:.2}", avail_w)),
        "yatay görsel genişliği doldurmalı: {}",
        content(2)
    );
}

// ================================================= CHAIN D · CROP · NUMBER · ROTATE ALL

#[test]
fn chain_d_watermark_then_rotate_keeps_both() {
    let lab = Lab::new();
    let src = lab.write("k.pdf", &mut vector_pdf(&[A4, A4]));
    let (_, w) = run(
        &lab,
        &[src],
        ToolOperation::Watermark {
            text: "KOPYA".into(),
        },
        "w.pdf",
        false,
    );
    let (_, r) = run(
        &lab,
        &[w],
        ToolOperation::RotatePages {
            rotations: vec![
                PageRotation {
                    page: 1,
                    degrees: 90,
                },
                PageRotation {
                    page: 2,
                    degrees: 90,
                },
            ],
        },
        "wr.pdf",
        false,
    );
    let d = reopen(&r);
    assert_eq!([rotation(&d, 1), rotation(&d, 2)], [90, 90]);
    for n in 1..=2 {
        let c =
            String::from_utf8_lossy(&d.get_page_content(d.get_pages()[&n]).unwrap()).into_owned();
        assert!(
            c.contains("(KOPYA)"),
            "filigran döndürmeden sonra kaybolmamalı (sayfa {n})"
        );
        assert!(c.contains("(Sayfa "), "kaynak içerik korunmalı");
    }
    let out = lab.path("unicode.pdf");
    assert!(
        run_tool_with_outcome(
            &[r],
            &ToolOperation::Watermark {
                text: "GİZLİ".into()
            },
            &out,
            false
        )
        .is_err(),
        "Unicode filigran bu sürümde açıkça reddedilir"
    );
    assert!(!out.exists());
}

#[test]
fn crop_number_and_rotate_all_publish_reopenable_output() {
    let lab = Lab::new();
    let src = lab.write("k.pdf", &mut vector_pdf(&[A4, A4_LANDSCAPE]));
    let (_, c) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Crop { margin_pt: 28.35 },
        "crop.pdf",
        false,
    );
    let d = reopen(&c);
    let (m, crop) = boxes(&d, 1);
    let crop = crop.expect("kırpma CropBox yazmalı");
    assert!(
        crop[0] >= m[0] + 28.0 && crop[2] <= m[2] - 28.0,
        "CropBox her kenardan içeri: {crop:?} / {m:?}"
    );
    let out = lab.path("hepsi.pdf");
    assert!(
        run_tool_with_outcome(
            std::slice::from_ref(&src),
            &ToolOperation::Crop { margin_pt: 400.0 },
            &out,
            false
        )
        .is_err(),
        "sayfayı yok eden kırpma reddedilir"
    );
    assert!(!out.exists());

    let (_, n) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Number { start: 7 },
        "num.pdf",
        false,
    );
    let d = reopen(&n);
    let c2 = String::from_utf8_lossy(&d.get_page_content(d.get_pages()[&2]).unwrap()).into_owned();
    assert!(
        c2.contains("(8)"),
        "ikinci sayfa 8 numarasını taşımalı: {c2}"
    );

    let (_, r) = run(
        &lab,
        &[src],
        ToolOperation::Rotate { degrees: -90 },
        "rot-all.pdf",
        false,
    );
    let d = reopen(&r);
    assert_eq!(
        [rotation(&d, 1), rotation(&d, 2)],
        [270, 270],
        "tüm sayfalar sola: -90 ≡ 270"
    );
}

#[test]
fn compress_keeps_original_icc_colorspace_object_on_recompressed_image() {
    // Çözmek için Device adına indirgenir; YAZILAN akış özgün ICC profilini
    // korur. Profil düşerse renk yorumu değişir — sessiz fidelity kaybı.
    let lab = Lab::new();
    let px = scan_pixels(1200, 1600);
    let src = lab.write("icc.pdf", &mut icc_pdf(&px));
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::GentleCompression,
        },
        "icc-out.pdf",
        false,
    );
    assert!(
        matches!(
            o,
            ToolOutcome::Compressed {
                images_recompressed: 1,
                ..
            }
        ),
        "{o:?}"
    );
    let d = reopen(&out);
    let image = d
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .find(|s| s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image"))
        .expect("görsel akışı");
    assert_eq!(
        image.dict.get(b"Filter").unwrap().as_name().unwrap(),
        b"DCTDecode"
    );
    let space = image
        .dict
        .get(b"ColorSpace")
        .unwrap()
        .as_array()
        .expect("ICC dizisi korunmalı");
    assert_eq!(space[0].as_name().unwrap(), b"ICCBased");
    assert!(
        space[1].as_reference().is_ok(),
        "profil referansı korunmalı"
    );
}

// ============================================ ŞÜPHECİ TURUNDAN GELEN SÖZLEŞMELER

/// TIFF öngörücüsü (Predictor 2): yatay deltalar. lopdf geri almaz.
fn tiff_predictor_2_rgb(px: &image::RgbImage) -> Vec<u8> {
    let (w, h) = (px.width() as usize, px.height() as usize);
    let raw = px.as_raw();
    let mut out = Vec::with_capacity(raw.len());
    for y in 0..h {
        let row = &raw[y * w * 3..(y + 1) * w * 3];
        for x in 0..w {
            for c in 0..3 {
                let cur = row[x * 3 + c];
                let prev = if x == 0 { 0 } else { row[(x - 1) * 3 + c] };
                out.push(cur.wrapping_sub(prev));
            }
        }
    }
    out
}

/// PNG "Sub" filtresi (tip baytı 1): her satırın önünde bir filtre baytı.
fn png_sub_gray(px: &image::GrayImage) -> Vec<u8> {
    let (w, h) = (px.width() as usize, px.height() as usize);
    let raw = px.as_raw();
    let mut out = Vec::with_capacity(raw.len() + h);
    for y in 0..h {
        out.push(1);
        let row = &raw[y * w..(y + 1) * w];
        for x in 0..w {
            let prev = if x == 0 { 0 } else { row[x - 1] };
            out.push(row[x].wrapping_sub(prev));
        }
    }
    out
}

fn zlib(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

fn first_image(doc: &Document) -> &Stream {
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .find(|s| s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image"))
        .expect("görsel akışı")
}

#[test]
fn compress_never_encodes_undecoded_tiff_predictor_deltas() {
    // ICCBased + Flate + /Predictor 2. Geri alınamayan öngörücü taşıyan görsel
    // dokunulmadan ATLANIR; asla ham delta JPEG'e kodlanmaz.
    let lab = Lab::new();
    let px = scan_pixels(640, 480);
    let icc_id: ObjectId = (900, 0);
    let icc = Stream::new(
        dictionary! {"N"=>3,"Alternate"=>"DeviceRGB"},
        vec![0u8; 128],
    );
    let img = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>640,"Height"=>480,
        "ColorSpace"=>vec!["ICCBased".into(), Object::Reference(icc_id)],
        "BitsPerComponent"=>8,"Filter"=>"FlateDecode",
        "DecodeParms"=>dictionary!{"Predictor"=>2,"Colors"=>3,"Columns"=>640,"BitsPerComponent"=>8}},
        zlib(&tiff_predictor_2_rgb(&px)),
    );
    let mut doc = image_pdf(img, vec![(icc_id, icc.into())]);
    let original = first_image(&doc).content.clone();
    let src = lab.write("tiff2.pdf", &mut doc);
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "tiff2-out.pdf",
        false,
    );
    match o {
        ToolOutcome::NoBenefit {
            images_found,
            images_recompressed,
            ..
        } => {
            assert_eq!((images_found, images_recompressed), (1, 0));
            assert!(!out.exists());
        }
        ToolOutcome::Compressed {
            images_recompressed,
            ..
        } => {
            assert_eq!(
                images_recompressed, 0,
                "öngörücülü görsel yeniden kodlanmamalı"
            );
            let d = reopen(&out);
            assert_eq!(
                first_image(&d).content,
                original,
                "görsel baytları dokunulmamış olmalı"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn compress_decodes_array_form_decodeparms_png_predictor_correctly() {
    // /Filter [/FlateDecode] /DecodeParms [<<...>>] — yasal dizi biçimi. PNG
    // öngörücüsü geri alınabilir; çözüm doğruysa sıkıştırılır, çıktı gerçek
    // piksellere yakın olmalı; çözüm hatalıysa görsel atlanmalı. Çöp asla.
    let lab = Lab::new();
    let gray = image::GrayImage::from_fn(2400, 3200, |x, y| {
        image::Luma([if (x / 7 + y / 11) % 2 == 0 { 40 } else { 200 }])
    });
    let icc_id: ObjectId = (900, 0);
    let icc = Stream::new(
        dictionary! {"N"=>1,"Alternate"=>"DeviceGray"},
        vec![0u8; 64],
    );
    let img = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>2400,"Height"=>3200,
        "ColorSpace"=>vec!["ICCBased".into(), Object::Reference(icc_id)],
        "BitsPerComponent"=>8,"Filter"=>vec![Object::Name(b"FlateDecode".to_vec())],
        "DecodeParms"=>vec![Object::Dictionary(dictionary!{"Predictor"=>15,"Colors"=>1,"Columns"=>2400,"BitsPerComponent"=>8})]},
        zlib(&png_sub_gray(&gray)),
    );
    let mut doc = image_pdf(img, vec![(icc_id, icc.into())]);
    let src = lab.write("pngarr.pdf", &mut doc);
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::AggressiveCompression,
        },
        "pngarr-out.pdf",
        false,
    );
    if let ToolOutcome::Compressed {
        images_recompressed: 1,
        ..
    } = o
    {
        let d = reopen(&out);
        let s = first_image(&d);
        let decoded = image::load_from_memory(&s.content).unwrap().to_luma8();
        let truth = image::imageops::resize(
            &gray,
            decoded.width(),
            decoded.height(),
            image::imageops::FilterType::Lanczos3,
        );
        let mae: f64 = decoded
            .as_raw()
            .iter()
            .zip(truth.as_raw())
            .map(|(a, b)| a.abs_diff(*b) as f64)
            .sum::<f64>()
            / decoded.as_raw().len() as f64;
        assert!(mae < 20.0, "PNG öngörücüsü yanlış çözüldü: MAE {mae:.1}");
    } else {
        assert!(
            matches!(
                o,
                ToolOutcome::NoBenefit {
                    images_recompressed: 0,
                    ..
                }
            ),
            "{o:?}"
        );
    }
}

#[test]
fn compress_broken_image_fails_even_when_cleanup_would_win() {
    // Bozuk JPEG + 400 KB atılabilir Metadata: temizlik %3'ü rahat geçer ama
    // bozuk görseli içinde taşıyan bir çıktı YAYINLANMAZ.
    let lab = Lab::new();
    let mut doc = image_pdf(
        Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>100,"Height"=>100,
            "ColorSpace"=>"DeviceRGB","Filter"=>"DCTDecode","BitsPerComponent"=>8},
            b"broken JPEG".to_vec(),
        ),
        vec![],
    );
    let mut state = 7u32;
    let junk: Vec<u8> = (0..400_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state & 0xff) as u8
        })
        .collect();
    let meta = doc.add_object(Stream::new(dictionary! {}, junk));
    doc.catalog_mut().unwrap().set("Metadata", meta);
    let src = lab.write("bozuk-meta.pdf", &mut doc);
    for level in [
        Level::LowRiskCleanup,
        Level::GentleCompression,
        Level::BalancedCompression,
    ] {
        let out = lab.path(&format!("bozuk-{level:?}.pdf"));
        let o = run_tool_with_outcome(
            std::slice::from_ref(&src),
            &ToolOperation::Compress { level },
            &out,
            false,
        )
        .unwrap();
        if level == Level::LowRiskCleanup {
            // Temizlik görseli çözmez; kazanç dürüst olabilir.
            assert!(!matches!(o, ToolOutcome::Failed { .. }) || !out.exists());
        } else {
            assert!(matches!(o, ToolOutcome::Failed { .. }), "{level:?}: {o:?}");
            assert!(!out.exists(), "{level:?}: bozuk görselli çıktı yazılmamalı");
        }
    }
}

#[test]
fn compress_raw_image_with_incompressible_ballast_is_no_benefit_not_failed() {
    // Filtresiz (ham) küçük görsel + 300 KB sıkıştırılamaz Form XObject:
    // hiçbir ön ayar %3 kazandıramaz; dürüst cevap her düzeyde NoBenefit.
    let lab = Lab::new();
    let mut d = Document::with_version("1.5");
    let pages = d.new_object_id();
    let img = d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>20,"Height"=>20,
        "ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8},
        vec![0xC8u8; 1200],
    ));
    let mut state = 11u32;
    let noise: Vec<u8> = (0..300_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state & 0xff) as u8
        })
        .collect();
    let form = d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),10.into(),10.into()],"Filter"=>"FlateDecode"},
        zlib(&noise),
    ));
    let contents = d.add_object(Stream::new(
        dictionary! {},
        b"q 100 0 0 100 50 50 cm /Im0 Do Q q /Fx0 Do Q".to_vec(),
    ));
    let page = d.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"MediaBox"=>media(595.,842.),
        "Resources"=>dictionary!{"XObject"=>dictionary!{"Im0"=>img,"Fx0"=>form}},"Contents"=>contents});
    d.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer.set("Root", root);
    let src = lab.write("ham-balast.pdf", &mut d);
    for level in [
        Level::LowRiskCleanup,
        Level::GentleCompression,
        Level::BalancedCompression,
    ] {
        let out = lab.path(&format!("ham-{level:?}.pdf"));
        let o = run_tool_with_outcome(
            std::slice::from_ref(&src),
            &ToolOperation::Compress { level },
            &out,
            false,
        )
        .unwrap();
        assert!(
            matches!(o, ToolOutcome::NoBenefit { .. }),
            "{level:?}: {o:?}"
        );
        assert!(!out.exists());
    }
}

// ========================= "BELGE OKUNAMADI" — SAĞLAM BELGENİN REDDİ (2026-09-14)
//
// Kullanıcının gerçek belgesinde Sıkıştır "Belge okunamadı; kopya
// oluşturulamadı" dedi. Belge açılıyor ve önizleniyordu: PDF sağlamdı. Motor
// iki ayrı yerden `InvalidPdf` üretiyordu ve kabuk her `InvalidPdf`'i "belge
// okunamadı" diye çeviriyordu. İkisi de sentetik fixture ile yeniden üretildi:
//
//  1. Marka payı eklenirken açıklama denetimi (her türetilmiş çıktı: kaydetme
//     DA düşüyordu) — yön gözetmiyor, sayfanın sağındaki bir popup'ı bile
//     reddediyor; null girdi, dolaylı /Rect, /Rect'siz açıklama ve 90'ın katı
//     olmayan /Rotate'te lopdf tür hatasıyla düşüyordu.
//  2. Sıkıştırmada görsel çözme — örnek verisinin sonundaki bir satır sonu
//     baytı ve dolaylı /Width, bütün belgeyi ölümcül hatayla düşürüyordu.

fn link_annot(rect: [f32; 4]) -> Dictionary {
    dictionary! {"Type"=>"Annot","Subtype"=>"Link",
    "Rect"=>rect.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
    "Border"=>vec![0.into(),0.into(),0.into()],
    "A"=>dictionary!{"S"=>"URI","URI"=>Object::string_literal("mailto:ornek@ornek.test")}}
}

/// Görünüm akışı olan, yani ekranda GERÇEKTEN çizilen bir açıklama.
fn visible_square(d: &mut Document, rect: [f32; 4], flags: i64) -> ObjectId {
    let ap = d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),20.into()]},
        b"1 0 0 rg 0 0 100 20 re f".to_vec(),
    ));
    d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Square",
    "Rect"=>rect.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
    "F"=>flags,"AP"=>dictionary!{"N"=>ap}})
}

/// İki sayfalık belge; 1. sayfanın /Annots'u `build`'in döndürdüğü nesne.
fn annotated(rotate: i64, build: impl FnOnce(&mut Document) -> Object) -> Document {
    let mut d = vector_pdf(&[A4, A4]);
    let annots = build(&mut d);
    let p = d.get_pages()[&1];
    let page = d.get_dictionary_mut(p).unwrap();
    page.set("Annots", annots);
    if rotate != 0 {
        page.set("Rotate", rotate);
    }
    d
}

fn annot_count(doc: &Document, page: u32) -> usize {
    let p = doc.get_dictionary(doc.get_pages()[&page]).unwrap();
    match p.get(b"Annots").ok().map(|a| doc.dereference(a).unwrap().1) {
        Some(Object::Array(a)) => a.len(),
        _ => 0,
    }
}

/// Sıkıştırma dâhil her türetmede: belge reddedilmez, çıktı yeniden açılır,
/// açıklamalar yerinde kalır.
fn assert_derivable(name: &str, doc: Document, annots_on_page_1: usize) {
    let lab = Lab::new();
    let mut doc = doc;
    let src = lab.write(&format!("{name}.pdf"), &mut doc);
    let (_, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Reorder { pages: vec![1, 2] },
        "kopya.pdf",
        false,
    );
    let d = reopen(&out);
    assert_eq!(d.get_pages().len(), 2, "{name}");
    assert_eq!(
        annot_count(&d, 1),
        annots_on_page_1,
        "{name}: açıklamalar korunmalı"
    );
    let (o, _) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "sikistirilmis.pdf",
        false,
    );
    assert!(
        !matches!(o, ToolOutcome::Failed { .. }),
        "{name}: sağlam belge sıkıştırmada reddedildi: {o:?}"
    );
}

#[test]
fn branding_does_not_refuse_annotations_that_the_gutter_cannot_reveal() {
    // Bağlantı sağ kenarı aşıyor — pay ALTA eklenir, sağdaki taşma görünmez.
    assert_derivable(
        "link-sag",
        annotated(0, |d| {
            vec![Object::Reference(
                d.add_object(link_annot([400., 700., 620., 720.])),
            )]
            .into()
        }),
        1,
    );
    // Vurgu + sayfanın sağındaki popup: gözden geçirilmiş sözleşmelerin tipik yapısı.
    assert_derivable(
        "vurgu-popup",
        annotated(0, |d| {
            let popup = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Popup",
            "Rect"=>vec![600.into(),600.into(),780.into(),720.into()],"Open"=>false});
            let hl = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Highlight",
            "Rect"=>vec![72.into(),690.into(),300.into(),724.into()],
            "QuadPoints"=>vec![72.into(),724.into(),300.into(),724.into(),72.into(),690.into(),300.into(),690.into()],
            "C"=>vec![1.into(),1.into(),0.into()],"Popup"=>popup});
            vec![Object::Reference(hl), Object::Reference(popup)].into()
        }),
        2,
    );
    // Kenarlığı 0 olan, görünümsüz bağlantı alt şeride taşıyor: çizeceği bir şey yok.
    assert_derivable(
        "gorunumsuz-link-alt",
        annotated(0, |d| {
            vec![Object::Reference(
                d.add_object(link_annot([72., -4., 300., 12.])),
            )]
            .into()
        }),
        1,
    );
    // Köşe sırası normalize edilmemiş Rect.
    assert_derivable(
        "ters-rect",
        annotated(0, |d| {
            vec![Object::Reference(
                d.add_object(link_annot([300., 720., 72., 700.])),
            )]
            .into()
        }),
        1,
    );
    // Gizli (F=2) görünür açıklama alt şeride taşıyor: zaten gösterilmez.
    assert_derivable(
        "gizli-kare-alt",
        annotated(0, |d| {
            vec![Object::Reference(visible_square(
                d,
                [72., -4., 300., 12.],
                2,
            ))]
            .into()
        }),
        1,
    );
}

#[test]
fn branding_tolerates_annotation_entries_a_viewer_cannot_place() {
    assert_derivable(
        "annots-null",
        annotated(0, |d| {
            vec![
                Object::Null,
                Object::Reference(d.add_object(link_annot([72., 700., 300., 720.]))),
            ]
            .into()
        }),
        2,
    );
    assert_derivable(
        "rect-dolayli",
        annotated(0, |d| {
            let r = d.add_object(Object::Array(vec![
                72.into(),
                700.into(),
                300.into(),
                720.into(),
            ]));
            vec![Object::Reference(d.add_object(
                dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>r,
                "Border"=>vec![0.into(),0.into(),0.into()]},
            ))]
            .into()
        }),
        1,
    );
    assert_derivable(
        "rect-yok",
        annotated(0, |d| {
            vec![Object::Reference(
                d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link"}),
            )]
            .into()
        }),
        1,
    );
    assert_derivable(
        "annots-dolayli-dizi",
        annotated(0, |d| {
            let a = d.add_object(link_annot([72., 700., 300., 720.]));
            Object::Reference(d.add_object(Object::Array(vec![Object::Reference(a)])))
        }),
        1,
    );
}

#[test]
fn branding_still_refuses_a_visible_annotation_the_gutter_would_reveal() {
    // Değişmez korunuyor: payın açacağı şeride düşen, GERÇEKTEN çizilen bir
    // açıklama varsa türetme durur, çıktı yazılmaz, kaynak değişmez.
    let refuses = |name: &str, doc: Document| {
        let lab = Lab::new();
        let mut doc = doc;
        let src = lab.write(&format!("{name}.pdf"), &mut doc);
        let hash = calculate_sha256(&src).unwrap();
        let out = lab.path("x.pdf");
        let err = run_tool_with_outcome(
            std::slice::from_ref(&src),
            &ToolOperation::Reorder { pages: vec![1, 2] },
            &out,
            false,
        )
        .expect_err(name)
        .to_string();
        assert!(
            !out.exists(),
            "{name}: reddedilen türetme çıktı bırakmamalı"
        );
        assert_eq!(calculate_sha256(&src).unwrap(), hash);
        // Kabuk bu cümleyi "belge okunamadı" değil, gerçek sebep olarak çevirir.
        assert!(
            err.contains("damga payı eklenince görünür"),
            "{name}: {err}"
        );
    };
    // Dik sayfa: pay altta.
    refuses(
        "kare-alt",
        annotated(0, |d| {
            vec![Object::Reference(visible_square(
                d,
                [72., -4., 300., 12.],
                4,
            ))]
            .into()
        }),
    );
    // 90° dönük sayfa: görsel alt kenar x ekseninde, pay SAĞA (x1) eklenir.
    refuses(
        "dik90-kare-sag",
        annotated(90, |d| {
            vec![Object::Reference(visible_square(
                d,
                [590., 300., 610., 400.],
                4,
            ))]
            .into()
        }),
    );
    // Aynı dönük sayfada alt kenarı aşan kare pay şeridinde değildir: geçer.
    assert_derivable(
        "dik90-kare-alt",
        annotated(90, |d| {
            vec![Object::Reference(visible_square(
                d,
                [72., -4., 300., 12.],
                4,
            ))]
            .into()
        }),
        1,
    );
}

#[test]
fn branding_places_mark_on_nonconforming_rotation_instead_of_refusing() {
    // Spec /Rotate'in 90'ın katı olmasını şart koşar; uymayan değeri pdf.js
    // döndürülmemiş sayar. Belge reddedilmez, sayfanın kendi değerine dokunulmaz.
    let lab = Lab::new();
    let mut d = vector_pdf(&[A4, A4]);
    let p = d.get_pages()[&1];
    d.get_dictionary_mut(p).unwrap().set("Rotate", 45);
    let src = lab.write("rotate45.pdf", &mut d);
    let (_, out) = run(
        &lab,
        &[src],
        ToolOperation::Reorder { pages: vec![1, 2] },
        "k.pdf",
        false,
    );
    let d = reopen(&out);
    assert_eq!(rotation(&d, 1), 45, "sayfanın /Rotate değeri korunmalı");
    let (m, _) = boxes(&d, 1);
    assert!(
        (m[3] - m[1] - (A4[1] + BRAND_GUTTER)).abs() < 0.05,
        "pay alta eklenmeli: {m:?}"
    );
}

/// Çıktıdaki tek görselin piksellerini kaynak piksellerle karşılaştırır.
fn output_image_mae(out: &Path, reference: &image::RgbImage) -> f64 {
    let d = reopen(out);
    let stream = d
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .find(|s| s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image"))
        .expect("görsel akışı");
    assert_eq!(
        stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
        b"DCTDecode"
    );
    let got = image::load_from_memory(&stream.content).unwrap().to_rgb8();
    assert_eq!(
        (got.width(), got.height()),
        (reference.width(), reference.height())
    );
    let sum: u64 = got
        .as_raw()
        .iter()
        .zip(reference.as_raw())
        .map(|(a, b)| a.abs_diff(*b) as u64)
        .sum();
    sum as f64 / got.as_raw().len() as f64
}

#[test]
fn compress_tolerates_trailing_eol_and_indirect_dimensions_with_fidelity() {
    let lab = Lab::new();
    let px = scan_pixels(1200, 1600);
    let image_stream =
        |extra: &[u8], flate: bool, width: Object, extra_objects: Vec<(ObjectId, Object)>| {
            let mut raw = px.as_raw().clone();
            raw.extend_from_slice(extra);
            let mut s = Stream::new(
                dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>width,"Height"=>1600,
                "ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8},
                raw,
            );
            if flate {
                s.compress().unwrap();
            }
            image_pdf(s, extra_objects)
        };
    for (name, mut doc) in [
        (
            "flate-sonda-LF",
            image_stream(b"\n", true, 1200.into(), vec![]),
        ),
        (
            "ham-sonda-CRLF",
            image_stream(b"\r\n", false, 1200.into(), vec![]),
        ),
        (
            "width-dolayli",
            image_stream(
                b"",
                true,
                Object::Reference((950, 0)),
                vec![((950, 0), Object::Integer(1200))],
            ),
        ),
    ] {
        let src = lab.write(&format!("{name}.pdf"), &mut doc);
        let (o, out) = run(
            &lab,
            &[src],
            ToolOperation::Compress {
                level: Level::BalancedCompression,
            },
            &format!("{name}-out.pdf"),
            false,
        );
        assert!(
            matches!(
                o,
                ToolOutcome::Compressed {
                    images_recompressed: 1,
                    ..
                }
            ),
            "{name}: sıkıştırılmalıydı, gelen {o:?}"
        );
        let mae = output_image_mae(&out, &px);
        assert!(
            mae < 12.0,
            "{name}: yeniden kodlanan görsel kaynağa sadık değil (MAE {mae:.2})"
        );
    }
}

#[test]
fn compress_skips_image_with_unexplained_length_instead_of_failing_document() {
    // Üç bayt fazlalık satır sonu değildir; örnekleri güvenle yorumlayamayız.
    // Görsel ATLANIR (baytları olduğu gibi kalır) — belge reddedilmez.
    let lab = Lab::new();
    let px = scan_pixels(400, 300);
    let mut raw = px.as_raw().clone();
    raw.extend_from_slice(b"\0\0\0");
    let mut s = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>400,"Height"=>300,
        "ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8},
        raw,
    );
    s.compress().unwrap();
    let original = s.content.clone();
    let src = lab.write("uzunluk.pdf", &mut image_pdf(s, vec![]));
    let (o, out) = run(
        &lab,
        &[src],
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "u.pdf",
        false,
    );
    match o {
        ToolOutcome::Failed { reason } => panic!("belge reddedilmemeliydi: {reason}"),
        ToolOutcome::Compressed {
            images_recompressed,
            ..
        } => {
            assert_eq!(images_recompressed, 0);
            let d = reopen(&out);
            let kept = d
                .objects
                .values()
                .filter_map(|o| o.as_stream().ok())
                .find(|s| {
                    s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image")
                })
                .unwrap();
            assert_eq!(
                kept.content, original,
                "atlanan görselin baytları değişmemeli"
            );
        }
        ToolOutcome::NoBenefit {
            images_recompressed,
            ..
        } => {
            assert_eq!(images_recompressed, 0);
            assert!(!out.exists());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn branding_ignores_closed_popup_but_guards_open_popup_on_rotated_page() {
    // Önizleme ve Acrobat: not simgesi sayfada, popup'ı sayfanın SAĞINDA,
    // kutunun dışında. 90° dönük sayfada pay tam o kenara (sağa) eklenir.
    fn note_with_popup(d: &mut Document, open: Option<bool>) -> Object {
        let popup_id = d.new_object_id();
        let note = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Text",
        "Rect"=>vec![540.into(),780.into(),560.into(),800.into()],
        "Contents"=>Object::string_literal("Not"),"Popup"=>popup_id});
        let mut popup = dictionary! {"Type"=>"Annot","Subtype"=>"Popup",
        "Rect"=>vec![605.into(),580.into(),785.into(),700.into()],"Parent"=>note,"F"=>4};
        if let Some(open) = open {
            popup.set("Open", open);
        }
        d.objects.insert(popup_id, popup.into());
        vec![Object::Reference(note), Object::Reference(popup_id)].into()
    }
    // Kapalı popup (varsayılan ya da açıkça) sayfada hiçbir şey çizmez.
    assert_derivable(
        "kapali-popup-dondurulmus",
        annotated(90, |d| note_with_popup(d, None)),
        2,
    );
    assert_derivable(
        "acikca-kapali-popup-dondurulmus",
        annotated(90, |d| note_with_popup(d, Some(false))),
        2,
    );
    // Açık popup pay şeridine düşüyor: görünür hâle gelirdi — değişmez korunur.
    let lab = Lab::new();
    let mut doc = annotated(90, |d| note_with_popup(d, Some(true)));
    let src = lab.write("acik-popup.pdf", &mut doc);
    let before = calculate_sha256(&src).unwrap();
    let err = run_tool_with_outcome(
        std::slice::from_ref(&src),
        &ToolOperation::Reorder { pages: vec![1, 2] },
        &lab.path("kopya.pdf"),
        false,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("damga payı eklenince görünür"),
        "{err}"
    );
    assert!(!lab.path("kopya.pdf").exists());
    assert_eq!(calculate_sha256(&src).unwrap(), before);
}

// ================= ARTIMLI GÜNCELLENMİŞ BELGE — BAYAT /Prev OFSETİ (2026-09-14)
//
// Gerçek üreticilerin çıktısıyla yapılan taramada (Chrome/Skia, macOS Quartz,
// PDFKit, Word yapısı, Acrobat tarzı güncelleme) kullanıcının belirtisi —
// belge açılıyor, önizleniyor, sayfa seçerek kaydediliyor ama Sıkıştır
// "Belge okunamadı" diyor — birebir yalnız ARTIMLI güncellenmiş dosyalarda
// çıktı. Word'ün "PDF olarak kaydet" çıktısı hibrit başvurulu bir dosyadır ve
// son trailer'ı /Prev taşır; Acrobat'ta "Kaydet", imza ve form doldurma da
// artımlı bölüm ekler. lopdf 0.34 son trailer'ı /Prev ile birlikte tutuyordu;
// belgeyi baştan yazan her yol (sıkıştır, döndür, kırp, filigran, numara)
// eski dosyanın bayt ofsetini yeni dosyaya kopyalıyor, çıktı kendi içinde
// tutarsız kalıyor ve yeniden açma doğrulaması onu haklı olarak reddediyordu.
// Sayfa seçerek kaydetme belgeyi yeniden kurduğu için etkilenmiyordu.
//
// Fixture'lar lopdf'in yazıcısından bağımsız, bayt bayt elle kurulur: test,
// düzeltilen kütüphane davranışının kendisine yaslanmamalı.

#[derive(Clone, Copy, Debug)]
enum Layout {
    /// Klasik xref + artımlı güncelleme bölümü (Acrobat "Kaydet", imza).
    ClassicIncremental,
    /// Klasik xref + /XRefStm + nesne akışında yapı öğeleri, son bölümde
    /// /Prev (Word "PDF olarak kaydet").
    WordHybrid,
    /// PDF 1.5 xref akışı (PNG öngörücülü) + xref akışlı güncelleme.
    XrefStreamIncremental,
}

struct RawPdf {
    bytes: Vec<u8>,
    offsets: std::collections::BTreeMap<u32, usize>,
}

impl RawPdf {
    fn new(version: &str) -> Self {
        let mut bytes = format!("%PDF-{version}\n").into_bytes();
        bytes.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");
        Self {
            bytes,
            offsets: Default::default(),
        }
    }
    fn object(&mut self, id: u32, body: &[u8]) {
        self.offsets.insert(id, self.bytes.len());
        self.bytes
            .extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        self.bytes.extend_from_slice(body);
        self.bytes.extend_from_slice(b"\nendobj\n");
    }
    fn stream(&mut self, id: u32, dict: &str, data: &[u8]) {
        let mut body = format!("<<{dict}/Length {}>>stream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.object(id, &body);
    }
    fn startxref(&mut self, at: usize) {
        self.bytes
            .extend_from_slice(format!("startxref\n{at}\n%%EOF\n").as_bytes());
    }
    /// 0..size aralığında klasik tablo; `live` dışındakiler serbest görünür
    /// (hibrit dosyada nesne akışındaki nesneler böyle yazılır).
    fn classic_table(&mut self, size: u32, live: &[u32], trailer: &str) -> usize {
        let at = self.bytes.len();
        let mut x = format!("xref\n0 {size}\n");
        for n in 0..size {
            match self.offsets.get(&n).filter(|_| live.contains(&n)) {
                Some(off) => x.push_str(&format!("{off:010} 00000 n \n")),
                None => x.push_str("0000000000 65535 f \n"),
            }
        }
        x.push_str(&format!("trailer\n<<{trailer}>>\n"));
        self.bytes.extend_from_slice(x.as_bytes());
        self.startxref(at);
        at
    }
    /// Güncelleme bölümü: yalnız değişen nesneler için alt bölümler.
    fn update_table(&mut self, changed: &[u32], trailer: &str) -> usize {
        let at = self.bytes.len();
        let mut x = String::from("xref\n0 1\n0000000000 65535 f \n");
        for id in changed {
            x.push_str(&format!("{id} 1\n{:010} 00000 n \n", self.offsets[id]));
        }
        x.push_str(&format!("trailer\n<<{trailer}>>\n"));
        self.bytes.extend_from_slice(x.as_bytes());
        self.startxref(at);
        at
    }
    /// W[1 3 1] xref akışı; satırlar Acrobat'ın yazdığı gibi PNG "Up"
    /// öngörücüsüyle. `rows`: (nesne, tür, alan 2, alan 3), artan sırada.
    fn xref_stream(&mut self, id: u32, size: u32, rows: &[(u32, u8, u32, u8)], extra: &str) {
        let mut index: Vec<(u32, u32)> = Vec::new();
        let mut raw = Vec::new();
        let mut prev = [0u8; 5];
        for &(n, kind, f2, f3) in rows {
            match index.last_mut() {
                Some((start, len)) if *start + *len == n => *len += 1,
                _ => index.push((n, 1)),
            }
            let row = [kind, (f2 >> 16) as u8, (f2 >> 8) as u8, f2 as u8, f3];
            raw.push(2);
            raw.extend(row.iter().zip(prev).map(|(b, p)| b.wrapping_sub(p)));
            prev = row;
        }
        let index: String = index.iter().map(|(s, l)| format!("{s} {l} ")).collect();
        self.stream(
            id,
            &format!("/Type/XRef/Size {size}/W[1 3 1]/Index[{index}]/Filter/FlateDecode/DecodeParms<</Columns 5/Predictor 12>>{extra}"),
            &zlib(&raw),
        );
    }
    /// Nesne akışı: `members` sırasıyla.
    fn object_stream(&mut self, id: u32, members: &[(u32, String)]) {
        let (mut header, mut body) = (String::new(), String::new());
        for (n, obj) in members {
            header.push_str(&format!("{n} {} ", body.len()));
            body.push_str(obj);
            body.push('\n');
        }
        self.stream(
            id,
            &format!(
                "/Type/ObjStm/N {}/First {}/Filter/FlateDecode",
                members.len(),
                header.len()
            ),
            &zlib(format!("{header}{body}").as_bytes()),
        );
    }
}

/// `pages` sayfalık sözleşme; 1. sayfada sayfanın İÇİNDE bir not açıklaması
/// vardır. Artımlı düzenlerde bu not güncelleme bölümünde eklenir: çıktıda
/// görünmesi, SON revizyonun okunduğunu kanıtlar.
fn incremental_contract(layout: Layout, pages: u32) -> Vec<u8> {
    let tagged = matches!(layout, Layout::WordHybrid);
    let page_id = |i: u32| 10 + 2 * i;
    let content_id = |i: u32| 11 + 2 * i;
    let note_id = 10 + 2 * pages;
    let kids: String = (0..pages).map(|i| format!("{} 0 R ", page_id(i))).collect();
    let page_dict = |i: u32, with_note: bool| {
        let tags = if tagged {
            format!("/StructParents {i}/Tabs/S/Group<</Type/Group/S/Transparency/CS/DeviceRGB>>")
        } else {
            String::new()
        };
        let annots = if with_note {
            format!("/Annots[{note_id} 0 R]")
        } else {
            String::new()
        };
        format!(
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 595.28 841.89]/Resources<</Font<</F1 3 0 R>>>>/Contents {} 0 R{tags}{annots}>>",
            content_id(i)
        )
    };
    let content = |i: u32| {
        let text = format!("BT /F1 24 Tf 72 720 Td (Sayfa {}) Tj ET", i + 1);
        let text = if tagged {
            format!("/P <</MCID 0>> BDC {text} EMC")
        } else {
            text
        };
        zlib(text.as_bytes())
    };
    let catalog = if tagged {
        "<</Type/Catalog/Pages 2 0 R/Lang(tr-TR)/MarkInfo<</Marked true>>/StructTreeRoot 9 0 R>>"
            .to_string()
    } else {
        "<</Type/Catalog/Pages 2 0 R>>".to_string()
    };
    let pages_dict = format!("<</Type/Pages/Count {pages}/Kids[{kids}]>>");
    let font = "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_string();
    let note = b"<</Type/Annot/Subtype/Text/Rect[500 780 520 800]/Contents(Not)/F 4>>";
    let id = "/ID[<0A1B2C3D4E5F60718293A4B5C6D7E8F9><0A1B2C3D4E5F60718293A4B5C6D7E8F9>]";

    let mut pdf = RawPdf::new(match layout {
        Layout::XrefStreamIncremental => "1.5",
        _ => "1.7",
    });
    match layout {
        Layout::ClassicIncremental => {
            pdf.object(1, catalog.as_bytes());
            pdf.object(2, pages_dict.as_bytes());
            pdf.object(3, font.as_bytes());
            for i in 0..pages {
                pdf.object(page_id(i), page_dict(i, false).as_bytes());
                pdf.stream(content_id(i), "/Filter/FlateDecode", &content(i));
            }
            let live: Vec<u32> = pdf.offsets.keys().copied().collect();
            let base =
                pdf.classic_table(note_id, &live, &format!("/Size {note_id}/Root 1 0 R{id}"));
            // Acrobat "Kaydet": 1. sayfa yeniden yazılır, not eklenir.
            pdf.object(page_id(0), page_dict(0, true).as_bytes());
            pdf.object(note_id, note);
            pdf.update_table(
                &[page_id(0), note_id],
                &format!("/Size {}/Root 1 0 R/Prev {base}{id}", note_id + 1),
            );
        }
        Layout::WordHybrid => {
            pdf.object(1, catalog.as_bytes());
            pdf.object(2, pages_dict.as_bytes());
            pdf.object(3, font.as_bytes());
            for i in 0..pages {
                pdf.object(page_id(i), page_dict(i, i == 0).as_bytes());
                pdf.stream(content_id(i), "/Filter/FlateDecode", &content(i));
            }
            pdf.object(note_id, note);
            pdf.object(8, b"<</Nums[0[6 0 R]]>>");
            pdf.object(
                9,
                b"<</Type/StructTreeRoot/K 5 0 R/ParentTree 8 0 R/ParentTreeNextKey 1>>",
            );
            // Yapı öğeleri nesne akışında; klasik tablo onları serbest gösterir.
            pdf.object_stream(
                7,
                &[
                    (5, "<</Type/StructElem/S/Document/P 9 0 R/K[6 0 R]>>".into()),
                    (
                        6,
                        format!("<</Type/StructElem/S/P/P 5 0 R/Pg {} 0 R/K 0>>", page_id(0)),
                    ),
                ],
            );
            let stm_id = note_id + 1;
            let size = stm_id + 1;
            let stm_at = pdf.bytes.len();
            pdf.xref_stream(
                stm_id,
                size,
                &[(5, 2, 7, 0), (6, 2, 7, 1), (stm_id, 1, stm_at as u32, 0)],
                "",
            );
            let live: Vec<u32> = pdf.offsets.keys().copied().collect();
            let base = pdf.classic_table(size, &live, &format!("/Size {size}/Root 1 0 R{id}"));
            // Word'ün son bölümü: boş tablo, /Prev ve /XRefStm.
            let at = pdf.bytes.len();
            pdf.bytes.extend_from_slice(
                format!("xref\n0 0\ntrailer\n<</Size {size}/Root 1 0 R/Prev {base}/XRefStm {stm_at}{id}>>\n").as_bytes(),
            );
            pdf.startxref(at);
        }
        Layout::XrefStreamIncremental => {
            for i in 0..pages {
                pdf.stream(content_id(i), "/Filter/FlateDecode", &content(i));
            }
            let mut members = vec![(1, catalog), (2, pages_dict), (3, font)];
            for i in 0..pages {
                members.push((page_id(i), page_dict(i, false)));
            }
            let stm_id = note_id + 1;
            pdf.object_stream(stm_id, &members);
            let xs1 = stm_id + 1;
            let at1 = pdf.bytes.len();
            let mut rows = vec![(0, 0, 0, 255)];
            rows.extend(
                members
                    .iter()
                    .enumerate()
                    .map(|(k, (n, _))| (*n, 2, stm_id, k as u8)),
            );
            rows.extend(
                (0..pages).map(|i| (content_id(i), 1, pdf.offsets[&content_id(i)] as u32, 0)),
            );
            rows.push((stm_id, 1, pdf.offsets[&stm_id] as u32, 0));
            rows.push((xs1, 1, at1 as u32, 0));
            rows.sort_by_key(|r| r.0);
            pdf.xref_stream(xs1, xs1 + 1, &rows, &format!("/Root 1 0 R{id}"));
            pdf.startxref(at1);
            // Güncelleme: 1. sayfa düz nesne olarak yeniden yazılır, not eklenir.
            pdf.object(page_id(0), page_dict(0, true).as_bytes());
            pdf.object(note_id, note);
            let xs2 = xs1 + 1;
            let at2 = pdf.bytes.len();
            let rows = [
                (page_id(0), 1, pdf.offsets[&page_id(0)] as u32, 0),
                (note_id, 1, pdf.offsets[&note_id] as u32, 0),
                (xs2, 1, at2 as u32, 0),
            ];
            pdf.xref_stream(xs2, xs2 + 1, &rows, &format!("/Root 1 0 R/Prev {at1}{id}"));
            pdf.startxref(at2);
        }
    }
    pdf.bytes
}

/// Açılan her belge, belgeyi baştan yazan her yoldan geçer: çıktı katı
/// okuyucuyla açılır, bayat ofset taşımaz, son revizyonu ve kimliğini korur.
fn assert_incremental_derivable(name: &str, bytes: &[u8], pages: usize) {
    assert!(
        bytes.windows(5).any(|w| w == b"/Prev"),
        "{name}: fixture artımlı değil — test anlamını yitirir"
    );
    // Kaynak açılıyor ve önizleniyor: kullanıcının gördüğü durum.
    let loaded = load_pdf_tolerant(bytes, "kaynak.pdf").unwrap();
    assert!(!loaded.is_repaired, "{name}");
    assert_eq!(loaded.document.get_pages().len(), pages, "{name}");
    assert_eq!(
        annot_count(&loaded.document, 1),
        1,
        "{name}: son revizyon okunmalı"
    );
    let lab = Lab::new();
    let src = lab.write_bytes(&format!("{name}.pdf"), bytes);
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Compress {
            level: Level::AggressiveCompression,
        },
        "sikistirilmis.pdf",
        false,
    );
    match &o {
        ToolOutcome::NoBenefit { .. } => assert!(!out.exists(), "{name}"),
        ToolOutcome::Compressed { .. } => {
            reopen(&out);
        }
        other => panic!("{name}: açılan belge sıkıştırmada reddedildi: {other:?}"),
    }
    for (label, op) in [
        ("dondur", ToolOperation::Rotate { degrees: 90 }),
        ("numara", ToolOperation::Number { start: 1 }),
    ] {
        let (o, out) = run(
            &lab,
            std::slice::from_ref(&src),
            op,
            &format!("{label}.pdf"),
            false,
        );
        published(&o);
        let raw = std::fs::read(&out).unwrap();
        let strict = Document::load_mem(&raw)
            .unwrap_or_else(|e| panic!("{name}/{label}: çıktı katı okuyucuyla açılmıyor: {e}"));
        assert!(
            !strict.trailer.has(b"Prev") && !strict.trailer.has(b"XRefStm"),
            "{name}/{label}: kaynağın bayt ofseti çıktıya taşındı: {:?}",
            strict.trailer
        );
        assert!(
            strict.trailer.has(b"ID"),
            "{name}/{label}: belge kimliği korunmalı"
        );
        let d = reopen(&out);
        assert_eq!(d.get_pages().len(), pages, "{name}/{label}");
        assert_eq!(
            annot_count(&d, 1),
            1,
            "{name}/{label}: güncellemede eklenen not korunmalı"
        );
        assert_eq!(
            markers(&d),
            (1..=pages)
                .map(|i| format!("Sayfa {i}"))
                .collect::<Vec<_>>(),
            "{name}/{label}"
        );
    }
}

#[test]
fn incrementally_updated_documents_compress_rotate_and_number_like_any_other() {
    for layout in [
        Layout::ClassicIncremental,
        Layout::WordHybrid,
        Layout::XrefStreamIncremental,
    ] {
        assert_incremental_derivable(&format!("{layout:?}"), &incremental_contract(layout, 3), 3);
    }
}
