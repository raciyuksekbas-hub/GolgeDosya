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
    // Marka payı ilk türetmede görsel alt kenara eklenir (o an 90° → x ekseni).
    // Marka idempotenttir: ikinci türetmede işaret görünür alanda olduğu için
    // yeni pay eklenmez; toplam alan = özgün + 1 pay. (2026-09-15'e kadar her
    // türetme bir pay daha ekliyordu; bu test o birikimi kilitliyordu.)
    let (m, _) = boxes(&d2, 2);
    let area_growth = (m[2] - m[0]) * (m[3] - m[1]) - A4_LANDSCAPE[0] * A4_LANDSCAPE[1];
    let expected = BRAND_GUTTER * A4_LANDSCAPE[1];
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
    // Marka idempotenttir: üç türetme = TEK pay (işaret görünür kaldıkça
    // sonraki türetmeler pay eklemez). 2026-09-15'e kadar üç pay birikiyordu.
    let (m, _) = boxes(&d, 2);
    assert!(
        (m[3] - m[1] - A4_LANDSCAPE[1]).abs() < 0.05,
        "dönük sayfanın yüksekliği korunmalı: {m:?}"
    );
    assert!(
        (m[2] - m[0] - (A4_LANDSCAPE[0] + BRAND_GUTTER)).abs() < 0.05,
        "üç türetme = x ekseninde tek pay: {m:?}"
    );
    let (m1, _) = boxes(&d, 1);
    assert!(
        (m1[3] - m1[1] - (LETTER[1] + BRAND_GUTTER)).abs() < 0.05,
        "üç türetme = tek marka payı: {m1:?}"
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
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
    // Türkçe filigran DESTEKLENİR: ürün Türk hukukçular için ve "GİZLİ"
    // en olağan filigrandır. Metin font kodlamasına çevrilir (mojibake yok).
    let out = lab.path("turkce.pdf");
    assert!(
        run_tool_with_outcome(
            std::slice::from_ref(&r),
            &ToolOperation::Watermark {
                text: "GİZLİ".into()
            },
            &out,
            false
        )
        .is_ok(),
        "Türkçe filigran reddedilmemeli"
    );
    // Fontta karşılığı gerçekten olmayan karakter hâlâ AÇIKÇA reddedilir;
    // sessizce bozuk glif basılmaz.
    let out = lab.path("yazilamaz.pdf");
    assert!(
        run_tool_with_outcome(
            &[r],
            &ToolOperation::Watermark {
                text: "GİZLİ 🔒".into()
            },
            &out,
            false
        )
        .is_err(),
        "yazılamayan karakter açıkça reddedilmeli"
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

// ============ SARKAN BAŞVURU · PAYLAŞILAN KAYNAK · İÇ BAĞLANTI (2026-09-15)
//
// İki açık kusur, yapısal nedeniyle:
//
//  1. Eksik nesneye başvuru. macOS 26 PDFKit'in (Önizleme) yeniden kaydettiği
//     etiketli belgede `/StructTreeRoot /IDTree 71 0 R` yazılı, 71 numaralı
//     nesne dosyada yok (`/Size` 543 içinde, kullanımda değil). Doğrulayıcı
//     Root'tan erişilen HER eksik nesneyi reddediyordu; sayfa görünümü
//     eksiksiz olduğu hâlde belge hiç açılmıyordu. ISO 32000-1 §7.3.10 böyle
//     bir başvuruyu null sayar.
//  2. Paylaşılan kaynak çoğaltması. Sayfa kopyalayıcı eski→yeni id haritasını
//     her sayfada sıfırdan açıyordu: sayfaların paylaştığı font, gömülü font
//     programı, görsel ve ICC profili onu kullanan sayfa sayısı kadar
//     kopyalanıyordu (40 sayfalık Chrome belgesi sıralanınca FontFile2 2 → 80,
//     çıktı 10×). Aynı sebeple kalan sayfaya giden iç bağlantılar da kopuyordu.

/// Çıktıda eksik nesneye başvuru ya da sayfası `null` bir bağlantı hedefi yok.
fn assert_no_dangling_or_dead_destinations(name: &str, doc: &Document) {
    fn visit(doc: &Document, obj: &Object, name: &str, key: Option<&[u8]>) {
        match obj {
            Object::Reference(id) => assert!(
                doc.objects.contains_key(id),
                "{name}: eksik nesneye başvuru {id:?}"
            ),
            Object::Array(items) => {
                if matches!(key, Some(b"Dest") | Some(b"D")) {
                    let first = items.first().map(|f| {
                        let mut value = f.clone();
                        for _ in 0..16 {
                            match value {
                                Object::Reference(id) => {
                                    value = doc.get_object(id).cloned().unwrap_or(Object::Null)
                                }
                                _ => break,
                            }
                        }
                        value
                    });
                    assert!(
                        !matches!(first, Some(Object::Null)),
                        "{name}: sayfası null bir bağlantı hedefi kaldı"
                    );
                }
                for item in items {
                    visit(doc, item, name, None);
                }
            }
            Object::Dictionary(d) => {
                for (k, v) in d.iter() {
                    visit(doc, v, name, Some(k));
                }
            }
            Object::Stream(s) => {
                for (k, v) in s.dict.iter() {
                    visit(doc, v, name, Some(k));
                }
            }
            _ => {}
        }
    }
    for obj in doc.objects.values() {
        visit(doc, obj, name, None);
    }
    for (k, v) in doc.trailer.iter() {
        visit(doc, v, name, Some(k));
    }
}

/// İki sayfalık etiketli belge: sayfa görünümü eksiksiz, belge düzeyindeki
/// yapıda yedi eksik nesneye başvuru. Eksik numaralar klasik xref'te `/Size`
/// içinde ve serbest — PDFKit çıktısındaki biçim. Başvuranlar: yapı ağacının
/// `/IDTree`'si, `/ParentTree` dizisindeki bir öğe, Catalog `/Outlines`, notun
/// `/Popup`ı, bağlantının `/A` eylemi, görselin `/Metadata`sı ve trailer
/// `/Info`. 2. sayfada, değeri Catalog'a kadar uzanan görünmez bir imza alanı.
fn dangling_metadata_pdf() -> Vec<u8> {
    let content = |n: u32, image: bool| {
        let mut text = format!("/P <</MCID 0>> BDC BT /F1 24 Tf 72 720 Td (Sayfa {n}) Tj ET EMC");
        if image {
            text.push_str(" q 40 0 0 40 72 600 cm /Im1 Do Q");
        }
        zlib(text.as_bytes())
    };
    let mut pdf = RawPdf::new("1.7");
    pdf.object(1, b"<</Type/Catalog/Pages 2 0 R/StructTreeRoot 30 0 R/MarkInfo<</Marked true>>/Outlines 40 0 R/Lang(tr-TR)>>");
    pdf.object(2, b"<</Type/Pages/Count 2/Kids[10 0 R 12 0 R]>>");
    pdf.object(3, b"<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>");
    pdf.stream(
        4,
        "/Type/XObject/Subtype/Image/Width 2/Height 2/ColorSpace/DeviceRGB/BitsPerComponent 8/Metadata 41 0 R",
        &[200, 30, 30, 30, 200, 30, 30, 30, 200, 240, 240, 240],
    );
    pdf.object(10, b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 595.28 841.89]/Resources<</Font<</F1 3 0 R>>/XObject<</Im1 4 0 R>>>>/Contents 11 0 R/StructParents 0/Annots[20 0 R 21 0 R]>>");
    pdf.stream(11, "/Filter/FlateDecode", &content(1, true));
    pdf.object(12, b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 595.28 841.89]/Resources<</Font<</F1 3 0 R>>>>/Contents 13 0 R/StructParents 1/Annots[22 0 R]>>");
    pdf.stream(13, "/Filter/FlateDecode", &content(2, false));
    pdf.object(
        20,
        b"<</Type/Annot/Subtype/Text/Rect[500 780 520 800]/Contents(Not)/Popup 42 0 R/F 4>>",
    );
    pdf.object(
        21,
        b"<</Type/Annot/Subtype/Link/Rect[72 700 300 720]/Border[0 0 0]/A 43 0 R>>",
    );
    // Görünmez imza alanı: `/V` imza sözlüğüne, onun `/Reference /Data`sı
    // Catalog'a çıkar. Görünüm grafiği bu yoldan bütün belgeye yayılmamalı.
    pdf.object(
        22,
        b"<</Type/Annot/Subtype/Widget/FT/Sig/T(Imza)/Rect[0 0 0 0]/F 4/V 33 0 R>>",
    );
    pdf.object(33, b"<</Type/Sig/Filter/Adobe.PPKLite/SubFilter/adbe.pkcs7.detached/Reference[<</Type/SigRef/TransformMethod/DocMDP/Data 1 0 R>>]>>");
    pdf.object(
        30,
        b"<</Type/StructTreeRoot/K 31 0 R/ParentTree 32 0 R/IDTree 44 0 R>>",
    );
    pdf.object(
        31,
        b"<</Type/StructElem/S/Document/P 30 0 R/K 0/Pg 10 0 R>>",
    );
    pdf.object(32, b"<</Nums[0[31 0 R] 1[45 0 R]]>>");
    let live: Vec<u32> = pdf.offsets.keys().copied().collect();
    pdf.classic_table(47, &live, "/Size 47/Root 1 0 R/Info 46 0 R");
    pdf.bytes
}

#[test]
fn dangling_reference_outside_page_graph_opens_previews_saves_and_reopens() {
    use ekler_core::pdf::tolerant::RepairStrategy;
    let bytes = dangling_metadata_pdf();
    // Toleranslı oku → normalize et → katı kurallarla yeniden doğrula.
    let loaded = load_pdf_tolerant(&bytes, "onizleme-kaydi.pdf").unwrap();
    assert!(loaded.is_repaired);
    assert_eq!(loaded.strategy, RepairStrategy::DanglingReferences);
    assert!(
        loaded
            .repair_note
            .as_deref()
            .unwrap_or("")
            .contains("7 eksik nesne"),
        "{:?}",
        loaded.repair_note
    );
    assert_eq!(loaded.document.get_pages().len(), 2);
    assert_no_dangling_or_dead_destinations("yüklenen", &loaded.document);
    pdf::validate_document(&loaded.document).unwrap();
    // Görünüm grafiği dokunulmadan kaldı: görsel, font ve iki açıklama yerinde.
    let p1 =
        pdf::resolved_page_dictionary(&loaded.document, loaded.document.get_pages()[&1]).unwrap();
    assert_eq!(annot_count(&loaded.document, 1), 2);
    assert!(format!("{p1:?}").contains("Im1"));

    let lab = Lab::new();
    let src = lab.write_bytes("onizleme-kaydi.pdf", &bytes);
    // Aç: kabuğun alım fişi.
    let receipt = scan_source_files(std::slice::from_ref(&src));
    assert!(receipt.errors.is_empty(), "{:?}", receipt.errors);
    assert_eq!(receipt.sources[0].page_count, 2);
    // İmza alanı taşıdığı için türetilmiş kopya onayla yapılır (ayrı sözleşme).
    assert!(receipt.sources[0].is_signed);
    // Önizle: kabuğun sayfa önizlemesi.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let png = ekler_core::raster::preview_page(&src, 1, 72, 0).unwrap();
        let image = image::load_from_memory(&png).unwrap();
        assert!(image.width() > 100 && image.height() > 100);
    }
    // Kopya kaydet → yeniden aç; sıra, açıklamalar, sarkan başvuru yok.
    for (label, op, expected) in [
        (
            "secim.pdf",
            ToolOperation::Select { pages: vec![2, 1] },
            vec!["Sayfa 2", "Sayfa 1"],
        ),
        (
            "sirala.pdf",
            ToolOperation::Reorder { pages: vec![2, 1] },
            vec!["Sayfa 2", "Sayfa 1"],
        ),
        (
            "dondur.pdf",
            ToolOperation::Rotate { degrees: 90 },
            vec!["Sayfa 1", "Sayfa 2"],
        ),
    ] {
        let (o, out) = run(&lab, std::slice::from_ref(&src), op, label, true);
        published(&o);
        let d = reopen(&out);
        assert_eq!(markers(&d), expected, "{label}");
        assert_no_dangling_or_dead_destinations(label, &d);
    }
    let (o, _) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "sikistir.pdf",
        true,
    );
    assert!(!matches!(o, ToolOutcome::Failed { .. }), "{o:?}");
}

/// Tek bir nesnesi dosyada olmayan iki sayfalık belge. İlk grup sayfa
/// ağacına, içeriğe ya da kaynaklara (katman görünürlüğü dâhil) dokunur:
/// onarılamaz, belge reddedilir. İkinci grup görünüm grafiğinin dışındadır:
/// başvuru null sayılır, belge açılır.
#[derive(Clone, Copy, Debug)]
enum Damage {
    // --- ölümcül
    Contents,
    FontInResources,
    EmbeddedFontProgram,
    ImageXObject,
    PageInKids,
    InheritedResources,
    /// Kaynaklar kök Pages düğümünün `/Parent`ından miras alınıyor.
    InheritedAboveRoot,
    /// Adı `/P` olan font: ad haritasındaki anahtarlar atlanmamalı.
    FontNamedP,
    /// Adı `/A` olan Type3 glifi.
    Type3GlyphNamedA,
    /// Adı `/B` olan Form XObject'in kendi kaynağındaki font.
    FontInsideFormNamedB,
    /// Catalog `/OCProperties /D`: gizli katman görünür olurdu.
    OptionalContentConfig,
    /// Kök Pages düğümünün `/Parent`ı dosyada yok VE 1. sayfanın içeriği eksik.
    DanglingRootParentMissingContents,
    /// Kök Pages düğümünün `/Parent`ı dosyada yok VE sayfa fontu eksik.
    DanglingRootParentMissingFont,
    /// Kökün `/Parent`ı VAR olan bir Pages düğümü, onun `/Parent`ı dosyada yok
    /// VE 1. sayfanın içeriği eksik.
    DanglingGrandparentMissingContents,
    // --- toleranslı
    AnnotationEntry,
    AnnotationAppearance,
    ClosedPopupInAnnots,
    AttachedFile,
    ToUnicode,
    CidSet,
    BoxColorInfo,
    WidgetIcon,
    ReferenceChainToMissing,
}

const FATAL_DAMAGE: [Damage; 14] = [
    Damage::Contents,
    Damage::FontInResources,
    Damage::EmbeddedFontProgram,
    Damage::ImageXObject,
    Damage::PageInKids,
    Damage::InheritedResources,
    Damage::InheritedAboveRoot,
    Damage::FontNamedP,
    Damage::Type3GlyphNamedA,
    Damage::FontInsideFormNamedB,
    Damage::OptionalContentConfig,
    Damage::DanglingRootParentMissingContents,
    Damage::DanglingRootParentMissingFont,
    Damage::DanglingGrandparentMissingContents,
];

const TOLERATED_DAMAGE: [Damage; 9] = [
    Damage::AnnotationEntry,
    Damage::AnnotationAppearance,
    Damage::ClosedPopupInAnnots,
    Damage::AttachedFile,
    Damage::ToUnicode,
    Damage::CidSet,
    Damage::BoxColorInfo,
    Damage::WidgetIcon,
    Damage::ReferenceChainToMissing,
];

fn damaged_page_graph(damage: Damage) -> Vec<u8> {
    let mut d = vector_pdf(&[A4, A4]);
    let pages = d.get_pages();
    let p1 = pages[&1];
    let pages_id = d
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let form = |d: &mut Document, content: &[u8]| {
        d.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),20.into()]},
            content.to_vec(),
        ))
    };
    let set_resources = |d: &mut Document, resources: Dictionary| {
        d.get_dictionary_mut(p1)
            .unwrap()
            .set("Resources", resources);
    };
    let set_annots = |d: &mut Document, annots: Vec<Object>| {
        d.get_dictionary_mut(p1).unwrap().set("Annots", annots);
    };
    let remove = match damage {
        Damage::Contents => d
            .get_dictionary(p1)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_reference()
            .unwrap(),
        Damage::FontInResources => {
            let font = d.add_object(font_dict());
            set_resources(&mut d, dictionary! {"Font"=>dictionary!{"F1"=>font}});
            font
        }
        Damage::EmbeddedFontProgram | Damage::CidSet => {
            let file = d.add_object(Stream::new(
                dictionary! {"Length1"=>8},
                b"TTFDATA!".to_vec(),
            ));
            let cidset = d.add_object(Stream::new(dictionary! {}, vec![0xff]));
            let descriptor = d.add_object(dictionary! {"Type"=>"FontDescriptor","FontName"=>"Gomulu","Flags"=>32,"FontFile2"=>file,"CIDSet"=>cidset});
            let font = d.add_object(dictionary! {"Type"=>"Font","Subtype"=>"TrueType","BaseFont"=>"Gomulu","FontDescriptor"=>descriptor});
            set_resources(&mut d, dictionary! {"Font"=>dictionary!{"F1"=>font}});
            if matches!(damage, Damage::CidSet) {
                cidset
            } else {
                file
            }
        }
        Damage::ImageXObject => {
            let image = d.add_object(Stream::new(
                dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>1,"Height"=>1,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8},
                vec![128],
            ));
            set_resources(&mut d, dictionary! {"XObject"=>dictionary!{"Im1"=>image}});
            image
        }
        Damage::PageInKids => pages[&2],
        Damage::InheritedResources => {
            let font = d.add_object(font_dict());
            let shared = d.add_object(dictionary! {"Font"=>dictionary!{"F1"=>font}});
            for id in pages.values() {
                d.get_dictionary_mut(*id).unwrap().remove(b"Resources");
            }
            d.get_dictionary_mut(pages_id)
                .unwrap()
                .set("Resources", shared);
            shared
        }
        Damage::InheritedAboveRoot => {
            let font = d.add_object(font_dict());
            let upper = d.add_object(dictionary! {"Type"=>"Pages","Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}}});
            for id in pages.values() {
                d.get_dictionary_mut(*id).unwrap().remove(b"Resources");
            }
            d.get_dictionary_mut(pages_id).unwrap().set("Parent", upper);
            font
        }
        Damage::FontNamedP => {
            let font = d.add_object(font_dict());
            set_resources(&mut d, dictionary! {"Font"=>dictionary!{"P"=>font}});
            font
        }
        Damage::Type3GlyphNamedA => {
            let glyph = |d: &mut Document| {
                d.add_object(Stream::new(
                    dictionary! {},
                    b"10 0 0 0 10 10 d1 0 0 10 10 re f".to_vec(),
                ))
            };
            let (a, b) = (glyph(&mut d), glyph(&mut d));
            let font = d.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type3",
                "FontBBox"=>vec![0.into(),0.into(),10.into(),10.into()],
                "FontMatrix"=>vec![0.1.into(),0.into(),0.into(),0.1.into(),0.into(),0.into()],
                "CharProcs"=>dictionary!{"A"=>a,"B"=>b},
                "Encoding"=>dictionary!{"Type"=>"Encoding","Differences"=>vec![65.into(),"A".into(),"B".into()]},
                "FirstChar"=>65,"LastChar"=>66,"Widths"=>vec![10.into(),10.into()],"Resources"=>dictionary!{}});
            set_resources(&mut d, dictionary! {"Font"=>dictionary!{"T3"=>font}});
            a
        }
        Damage::FontInsideFormNamedB => {
            let font = d.add_object(font_dict());
            let header = d.add_object(Stream::new(
                dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),20.into()],
                "Resources"=>dictionary!{"Font"=>dictionary!{"F9"=>font}}},
                b"BT /F9 10 Tf (x) Tj ET".to_vec(),
            ));
            set_resources(&mut d, dictionary! {"XObject"=>dictionary!{"B"=>header}});
            font
        }
        Damage::OptionalContentConfig => {
            let ocg = d.add_object(
                dictionary! {"Type"=>"OCG","Name"=>Object::string_literal("Gizli katman")},
            );
            let config = d.add_object(dictionary! {"OFF"=>vec![Object::Reference(ocg)]});
            d.catalog_mut().unwrap().set(
                "OCProperties",
                dictionary! {"OCGs"=>vec![Object::Reference(ocg)],"D"=>config},
            );
            config
        }
        Damage::DanglingRootParentMissingContents | Damage::DanglingRootParentMissingFont => {
            let missing_parent = d.new_object_id();
            d.get_dictionary_mut(pages_id)
                .unwrap()
                .set("Parent", missing_parent);
            if matches!(damage, Damage::DanglingRootParentMissingContents) {
                d.get_dictionary(p1)
                    .unwrap()
                    .get(b"Contents")
                    .unwrap()
                    .as_reference()
                    .unwrap()
            } else {
                let font = d.add_object(font_dict());
                set_resources(&mut d, dictionary! {"Font"=>dictionary!{"F1"=>font}});
                font
            }
        }
        Damage::DanglingGrandparentMissingContents => {
            let missing_parent = d.new_object_id();
            let upper = d.add_object(dictionary! {"Type"=>"Pages","Parent"=>missing_parent});
            d.get_dictionary_mut(pages_id).unwrap().set("Parent", upper);
            d.get_dictionary(p1)
                .unwrap()
                .get(b"Contents")
                .unwrap()
                .as_reference()
                .unwrap()
        }
        Damage::AnnotationEntry => {
            let annot = d.add_object(visible_square_dict([72., 600., 172., 620.]));
            set_annots(&mut d, vec![Object::Reference(annot)]);
            annot
        }
        Damage::AnnotationAppearance => {
            let mut square = visible_square_dict([72., 600., 172., 620.]);
            let ap = form(&mut d, b"1 0 0 rg 0 0 100 20 re f");
            square.set("AP", dictionary! {"N"=>ap});
            let annot = d.add_object(square);
            set_annots(&mut d, vec![Object::Reference(annot)]);
            ap
        }
        Damage::ClosedPopupInAnnots => {
            let popup = d.new_object_id();
            let note = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Text",
            "Rect"=>vec![540.into(),780.into(),560.into(),800.into()],"Popup"=>popup});
            set_annots(
                &mut d,
                vec![Object::Reference(note), Object::Reference(popup)],
            );
            popup
        }
        Damage::AttachedFile => {
            let file = d.add_object(Stream::new(
                dictionary! {"Type"=>"EmbeddedFile"},
                b"ek dosya".to_vec(),
            ));
            let ap = form(&mut d, b"0 0 1 rg 0 0 20 20 re f");
            let annot = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"FileAttachment",
                "Rect"=>vec![72.into(),600.into(),92.into(),620.into()],
                "FS"=>dictionary!{"Type"=>"Filespec","F"=>Object::string_literal("ek.txt"),"EF"=>dictionary!{"F"=>file}},
                "AP"=>dictionary!{"N"=>ap}});
            set_annots(&mut d, vec![Object::Reference(annot)]);
            file
        }
        Damage::ToUnicode => {
            let cmap = d.add_object(Stream::new(dictionary! {}, b"begincmap endcmap".to_vec()));
            let font = d.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica","ToUnicode"=>cmap});
            set_resources(&mut d, dictionary! {"Font"=>dictionary!{"F1"=>font}});
            cmap
        }
        Damage::BoxColorInfo => {
            let info = d.add_object(
                dictionary! {"CropBox"=>dictionary!{"C"=>vec![0.into(),0.into(),0.into()]}},
            );
            d.get_dictionary_mut(p1).unwrap().set("BoxColorInfo", info);
            info
        }
        Damage::WidgetIcon => {
            let icon = form(&mut d, b"0 1 0 rg 0 0 10 10 re f");
            let ap = form(&mut d, b"0 0 0 rg 0 0 100 20 re f");
            let widget = d.add_object(
                dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Btn",
                "Rect"=>vec![72.into(),600.into(),172.into(),620.into()],
                "MK"=>dictionary!{"I"=>icon},"AP"=>dictionary!{"N"=>ap}},
            );
            set_annots(&mut d, vec![Object::Reference(widget)]);
            icon
        }
        Damage::ReferenceChainToMissing => {
            let missing = d.new_object_id();
            let link = d.add_object(Object::Reference(missing));
            d.catalog_mut().unwrap().set("Outlines", link);
            missing
        }
    };
    d.objects.remove(&remove);
    let mut bytes = Vec::new();
    d.save_to(&mut bytes).unwrap();
    bytes
}

fn visible_square_dict(rect: [f32; 4]) -> Dictionary {
    dictionary! {"Type"=>"Annot","Subtype"=>"Square",
    "Rect"=>rect.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),"F"=>4}
}

#[test]
fn missing_object_in_page_graph_is_still_fatal() {
    for damage in FATAL_DAMAGE {
        let bytes = damaged_page_graph(damage);
        let err = load_pdf_tolerant(&bytes, "hasarli.pdf")
            .map(|_| ())
            .expect_err(&format!(
                "{damage:?}: sayfa ağacı/içerik/kaynaklarda eksik nesne kabul edildi"
            ));
        assert!(
            err.to_string().contains("Eksik nesne referansı"),
            "{damage:?}: {err}"
        );
        let lab = Lab::new();
        let src = lab.write_bytes("hasarli.pdf", &bytes);
        let before = calculate_sha256(&src).unwrap();
        let out = lab.path("kopya.pdf");
        assert!(
            run_tool_with_outcome(
                std::slice::from_ref(&src),
                &ToolOperation::Select { pages: vec![1] },
                &out,
                false
            )
            .is_err(),
            "{damage:?}"
        );
        assert!(!out.exists(), "{damage:?}: kısmi çıktı kaldı");
        assert_eq!(calculate_sha256(&src).unwrap(), before, "{damage:?}");
    }
}

#[test]
fn missing_object_outside_page_graph_opens_and_saves_clean() {
    use ekler_core::pdf::tolerant::RepairStrategy;
    for damage in TOLERATED_DAMAGE {
        let bytes = damaged_page_graph(damage);
        let loaded = load_pdf_tolerant(&bytes, "duzensiz.pdf").unwrap_or_else(|e| {
            panic!("{damage:?}: görünüm dışı eksik nesne belgeyi açtırmadı: {e}")
        });
        assert_eq!(
            loaded.strategy,
            RepairStrategy::DanglingReferences,
            "{damage:?}"
        );
        assert_no_dangling_or_dead_destinations(&format!("{damage:?}"), &loaded.document);
        let lab = Lab::new();
        let src = lab.write_bytes("duzensiz.pdf", &bytes);
        let (o, out) = run(
            &lab,
            std::slice::from_ref(&src),
            ToolOperation::Select { pages: vec![1] },
            "kopya.pdf",
            false,
        );
        published(&o);
        let d = reopen(&out);
        assert_eq!(markers(&d), ["Sayfa 1"], "{damage:?}");
        assert_no_dangling_or_dead_destinations(&format!("{damage:?}/kopya"), &d);
    }
}

// ------------------------------------------------ paylaşılan kaynak kitapları

/// Deterministik, sıkıştırılamaz bayt: gömülü font programı ve tarama görseli
/// gerçek dosyalardaki gibi yer kaplasın.
fn noise(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).max(1);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 24) as u8
        })
        .collect()
}

fn embedded_font(d: &mut Document, name: &str, seed: u32) -> ObjectId {
    let file = d.add_object(Stream::new(
        dictionary! {"Length1"=>40_000},
        noise(40_000, seed),
    ));
    let descriptor = d.add_object(
        dictionary! {"Type"=>"FontDescriptor","FontName"=>name,"Flags"=>32,
        "FontBBox"=>vec![0.into(),(-200).into(),1000.into(),900.into()],"ItalicAngle"=>0,
        "Ascent"=>900,"Descent"=>-200,"CapHeight"=>700,"StemV"=>80,"FontFile2"=>file},
    );
    d.add_object(dictionary! {"Type"=>"Font","Subtype"=>"TrueType","BaseFont"=>name,
        "FirstChar"=>32,"LastChar"=>126,"Widths"=>(32..=126).map(|_| 500.into()).collect::<Vec<Object>>(),
        "FontDescriptor"=>descriptor,"Encoding"=>"WinAnsiEncoding"})
}

/// ICCBased renk uzaylı, SMask'lı Flate görsel.
fn noise_image(d: &mut Document, w: i64, h: i64, seed: u32, icc: ObjectId) -> ObjectId {
    let smask = d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>w,"Height"=>h,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8,"Filter"=>"FlateDecode"},
        zlib(&noise((w * h) as usize, seed.wrapping_mul(7919) ^ 0x5eed)),
    ));
    d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>w,"Height"=>h,
        "ColorSpace"=>vec![Object::Name(b"ICCBased".to_vec()), Object::Reference(icc)],
        "BitsPerComponent"=>8,"Filter"=>"FlateDecode","SMask"=>smask},
        zlib(&noise((w * h * 3) as usize, seed)),
    ))
}

#[derive(Clone, Copy, Debug)]
enum Book {
    /// A — 40 sayfa, aynı gömülü font.
    SameFont,
    /// B — 40 sayfa, aynı logo görseli (ICC + SMask).
    SameImage,
    /// C — 40 sayfa, aynı font + aynı görsel.
    FontAndImage,
    /// D — 40 sayfa, farklı kaynaklar: üç font farklı sayfa kümelerinde, üç
    /// görsel farklı sayfalarda, ortak ExtGState, kendi kaynağı olan ortak
    /// başlık Form XObject'i, ortak ICC profili.
    Mixed,
    /// C'nin kaynakları sayfalarda değil, Pages düğümünde (miras).
    InheritedFontAndImage,
}

fn resource_book(book: Book) -> Document {
    const WORDS: [&str; 16] = [
        "taraflar",
        "sozlesme",
        "hizmet",
        "gizlilik",
        "veri",
        "proje",
        "teklif",
        "madde",
        "yukumluluk",
        "sure",
        "fesih",
        "bildirim",
        "ucret",
        "teslim",
        "kabul",
        "ek",
    ];
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    // Kaynakta yalnız kullanılan nesneler bulunur: yetim nesne kaynağı
    // şişirip oranları olduğundan iyi gösterirdi.
    let uses_image = !matches!(book, Book::SameFont);
    let helvetica = matches!(book, Book::SameImage).then(|| d.add_object(font_dict()));
    let icc = uses_image.then(|| {
        d.add_object(Stream::new(
            dictionary! {"N"=>3,"Alternate"=>"DeviceRGB"},
            noise(3_000, 7),
        ))
    });
    let serif = (!matches!(book, Book::SameImage)).then(|| embedded_font(&mut d, "OrnekSerif", 11));
    let (sans, mono) = match book {
        Book::Mixed => (
            Some(embedded_font(&mut d, "OrnekSans", 12)),
            Some(embedded_font(&mut d, "OrnekMono", 13)),
        ),
        _ => (None, None),
    };
    let logo = icc.map(|icc| noise_image(&mut d, 160, 60, 21, icc));
    let (scan, stamp) = match book {
        Book::Mixed => (
            Some(noise_image(&mut d, 400, 300, 22, icc.unwrap())),
            Some(noise_image(&mut d, 200, 200, 23, icc.unwrap())),
        ),
        _ => (None, None),
    };
    let gstate = matches!(book, Book::Mixed)
        .then(|| d.add_object(dictionary! {"Type"=>"ExtGState","ca"=>0.9f32,"CA"=>0.9f32}));
    let header = match (book, sans) {
        (Book::Mixed, Some(sans)) => Some(d.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),500.into(),30.into()],
            "Resources"=>dictionary!{"Font"=>dictionary!{"F2"=>sans}}},
            b"BT /F2 10 Tf 0 10 Td (ORNEK YAZILIM A.S.) Tj ET".to_vec(),
        ))),
        _ => None,
    };
    let inherited = matches!(book, Book::InheritedFontAndImage);
    if inherited {
        let shared = d.add_object(dictionary! {
        "Font"=>dictionary!{"F1"=>serif.unwrap()},"XObject"=>dictionary!{"Im1"=>logo.unwrap()}});
        d.objects.insert(
            pages_id,
            dictionary! {"Type"=>"Pages","Kids"=>vec![],"Count"=>0,"Resources"=>shared}.into(),
        );
    }
    let mut kids = Vec::new();
    for n in 1..=40u32 {
        let mut fonts = Dictionary::new();
        let mut xobjects = Dictionary::new();
        let mut text = String::new();
        let body_font = "F1";
        fonts.set("F1", helvetica.or(serif).unwrap());
        text.push_str(&format!(
            "BT /{body_font} 24 Tf 72 760 Td (Sayfa {n}) Tj ET\n"
        ));
        let lines = if matches!(book, Book::SameImage) {
            20
        } else {
            60
        };
        for line in 0..lines {
            let words: Vec<&str> = (0..9)
                .map(|w| WORDS[((n as usize * 31 + line * 7 + w * 13) ^ (line * w)) % WORDS.len()])
                .collect();
            let font = match (book, line % 5, line % 2) {
                (Book::Mixed, 0, _) if n % 5 == 0 => "F3",
                (Book::Mixed, _, 0) if n % 2 == 0 => "F2",
                _ => body_font,
            };
            text.push_str(&format!(
                "BT /{font} 9 Tf 72 {} Td ({} {n}-{line}) Tj ET\n",
                730 - line as i64 * 11,
                words.join(" ")
            ));
        }
        let mut draw = |name: &str, id: ObjectId, x: i64, xobjects: &mut Dictionary| {
            xobjects.set(name, id);
            text.push_str(&format!("q 120 0 0 45 {x} 40 cm /{name} Do Q\n"));
        };
        match book {
            Book::SameImage | Book::FontAndImage => draw("Im1", logo.unwrap(), 72, &mut xobjects),
            Book::InheritedFontAndImage => text.push_str("q 120 0 0 45 72 40 cm /Im1 Do Q\n"),
            Book::Mixed => {
                if n == 1 {
                    draw("Im1", logo.unwrap(), 72, &mut xobjects);
                }
                if n % 10 == 0 {
                    draw("Im2", scan.unwrap(), 200, &mut xobjects);
                }
                if n % 8 == 0 {
                    draw("Im3", stamp.unwrap(), 330, &mut xobjects);
                }
                draw("Hdr", header.unwrap(), 72, &mut xobjects);
                if n % 2 == 0 {
                    fonts.set("F2", sans.unwrap());
                }
                if n % 5 == 0 {
                    fonts.set("F3", mono.unwrap());
                }
                text.insert_str(0, "/GS1 gs\n");
            }
            Book::SameFont => {}
        }
        let contents = d.add_object(Stream::new(
            dictionary! {"Filter"=>"FlateDecode"},
            zlib(text.as_bytes()),
        ));
        let mut page = dictionary! {"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>media(A4[0], A4[1]),"Contents"=>contents};
        if !inherited {
            let mut resources = dictionary! {"Font"=>fonts};
            if !xobjects.is_empty() {
                resources.set("XObject", xobjects);
            }
            if matches!(book, Book::Mixed) {
                resources.set("ExtGState", dictionary! {"GS1"=>gstate.unwrap()});
            }
            page.set("Resources", resources);
        }
        kids.push(Object::Reference(d.add_object(page)));
    }
    let mut pages = if inherited {
        d.get_dictionary(pages_id).unwrap().clone()
    } else {
        Dictionary::new()
    };
    pages.set("Type", "Pages");
    pages.set("Kids", kids.clone());
    pages.set("Count", kids.len() as i64);
    d.objects.insert(pages_id, pages.into());
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    d.trailer.set("Root", root);
    d
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Census {
    fonts: usize,
    font_programs: usize,
    images: usize,
    icc_profiles: usize,
    forms: usize,
    gstates: usize,
    stream_bytes: usize,
}

/// Verilen sayfaların (miras dâhil) erişebildiği dolaylı nesnelerin nüfusu.
/// Başka sayfalar ve sayfa ağacı sayılmaz.
fn page_census(doc: &Document, pages: &[ObjectId]) -> Census {
    let mut c = Census::default();
    let mut seen = std::collections::HashSet::new();
    let mut stack: Vec<Object> = pages
        .iter()
        .map(|id| Object::Dictionary(pdf::resolved_page_dictionary(doc, *id).unwrap()))
        .collect();
    let name = |d: &Dictionary, k: &[u8]| {
        d.get(k)
            .ok()
            .and_then(|v| v.as_name().ok())
            .map(<[u8]>::to_vec)
    };
    while let Some(obj) = stack.pop() {
        match obj {
            Object::Reference(id) => {
                if !seen.insert(id) {
                    continue;
                }
                let Ok(found) = doc.get_object(id) else {
                    continue;
                };
                let dict = match found {
                    Object::Dictionary(d) => d,
                    Object::Stream(s) => &s.dict,
                    _ => {
                        stack.push(found.clone());
                        continue;
                    }
                };
                match (
                    name(dict, b"Type").as_deref(),
                    name(dict, b"Subtype").as_deref(),
                ) {
                    (Some(b"Page" | b"Pages"), _) => continue,
                    (Some(b"Font"), _) => c.fonts += 1,
                    (Some(b"ExtGState"), _) => c.gstates += 1,
                    (_, Some(b"Image")) => c.images += 1,
                    (_, Some(b"Form")) => c.forms += 1,
                    _ if dict.has(b"Length1") => c.font_programs += 1,
                    _ if dict.has(b"N") && matches!(found, Object::Stream(_)) => {
                        c.icc_profiles += 1
                    }
                    _ => {}
                }
                if let Object::Stream(s) = found {
                    c.stream_bytes += s.content.len();
                }
                stack.push(found.clone());
            }
            Object::Array(items) => stack.extend(items),
            Object::Dictionary(d) => stack.extend(
                d.iter()
                    .filter(|(k, _)| k.as_slice() != b"Parent")
                    .map(|(_, v)| v.clone()),
            ),
            Object::Stream(s) => stack.extend(s.dict.iter().map(|(_, v)| v.clone())),
            _ => {}
        }
    }
    c
}

/// Aynı baytları taşıyan birden çok akış grubu (kanonik olmayan kopya).
fn duplicate_streams(doc: &Document) -> usize {
    let mut groups: std::collections::HashMap<&[u8], usize> = std::collections::HashMap::new();
    for obj in doc.objects.values() {
        if let Object::Stream(s) = obj {
            *groups.entry(s.content.as_slice()).or_default() += 1;
        }
    }
    groups.values().filter(|n| **n > 1).count()
}

/// A–D kitaplarında Seç 40→1, Seç 40→10, Sırala 40→40, Sil 40→39: paylaşılan
/// her kaynak çıktıda TEK nesne; boyut seçilen sayfaların gerçekten ihtiyaç
/// duyduğu baytla sınırlı. Ölçümler `SIZE_REPORT` ortam değişkeniyle yazılır.
fn assert_shared_resources_stay_single(book: Book) {
    let lab = Lab::new();
    let mut doc = resource_book(book);
    let src = lab.write(&format!("{book:?}.pdf"), &mut doc);
    let source = load_pdf_tolerant(&std::fs::read(&src).unwrap(), "kaynak.pdf")
        .unwrap()
        .document;
    let src_bytes = std::fs::metadata(&src).unwrap().len() as usize;
    assert_eq!(
        duplicate_streams(&source),
        0,
        "{book:?}: fixture'da aynı baytlı akış var"
    );
    let all = source.get_pages();
    let ten: Vec<usize> = (0..10).map(|k| 1 + 4 * k).collect();
    for (label, op, keep) in [
        ("sec-1", ToolOperation::Select { pages: vec![1] }, vec![1]),
        (
            "sec-10",
            ToolOperation::Select { pages: ten.clone() },
            ten.clone(),
        ),
        (
            "sirala-40",
            ToolOperation::Reorder {
                pages: (1..=40).rev().collect(),
            },
            (1..=40).rev().collect(),
        ),
        (
            "sil-39",
            ToolOperation::Delete { pages: vec![20] },
            (1..=40).filter(|p| *p != 20).collect(),
        ),
    ] {
        let (o, out) = run(
            &lab,
            std::slice::from_ref(&src),
            op,
            &format!("{label}.pdf"),
            false,
        );
        let out_bytes = published(&o) as usize;
        let d = reopen(&out);
        let name = format!("{book:?}/{label}");
        assert_eq!(
            markers(&d),
            keep.iter()
                .map(|p| format!("Sayfa {p}"))
                .collect::<Vec<_>>(),
            "{name}"
        );
        let wanted = page_census(
            &source,
            &keep.iter().map(|p| all[&(*p as u32)]).collect::<Vec<_>>(),
        );
        let got = page_census(&d, &d.get_pages().into_values().collect::<Vec<_>>());
        // Boyut: seçilen sayfaların akış baytları + sayfa başına sözlük/marka payı.
        let bound = wanted.stream_bytes + 12_000 + 700 * keep.len();
        if std::env::var("SIZE_REPORT").is_ok() {
            println!(
                "{name:<32} kaynak {src_bytes:>8} B · çıktı {out_bytes:>8} B ({:.2}×) · gereken akış {:>8} B · sınır {bound:>8} B · font programı {} · görsel {}",
                out_bytes as f64 / src_bytes as f64,
                wanted.stream_bytes,
                got.font_programs,
                got.images
            );
        }
        // Marka: ortak vektör + form (2 Form XObject) ve kelime işaretinin TEK
        // paylaşılan base-14 fontu (+1 Font). Başka kaynak eklenmez. "+1"in
        // sayfa sayısından bağımsız olması (1, 10, 39, 40 sayfa) fontun sayfa
        // başına değil bir kez eklendiğini, yani paylaşıldığını kanıtlar.
        let expected = Census {
            forms: wanted.forms + 2,
            fonts: wanted.fonts + 1,
            stream_bytes: got.stream_bytes,
            ..wanted
        };
        assert_eq!(got, expected, "{name}: paylaşılan kaynak tek nesne kalmalı");
        assert_eq!(duplicate_streams(&d), 0, "{name}: aynı baytlı ikinci akış");
        assert!(
            out_bytes <= bound,
            "{name}: çıktı {out_bytes} B, gereken {} B + pay = {bound} B",
            wanted.stream_bytes
        );
        if keep.len() >= 39 {
            assert!(
                out_bytes * 100 <= src_bytes * 115,
                "{name}: tam belge çıktısı kaynaktan anlamsız büyük: {out_bytes} / {src_bytes}"
            );
        }
    }
}

#[test]
fn shared_resources_stay_single_a_same_font() {
    assert_shared_resources_stay_single(Book::SameFont);
}

#[test]
fn shared_resources_stay_single_b_same_image() {
    assert_shared_resources_stay_single(Book::SameImage);
}

#[test]
fn shared_resources_stay_single_c_font_and_image() {
    assert_shared_resources_stay_single(Book::FontAndImage);
}

#[test]
fn shared_resources_stay_single_d_mixed_resources() {
    assert_shared_resources_stay_single(Book::Mixed);
}

#[test]
fn shared_resources_stay_single_with_inherited_resources() {
    assert_shared_resources_stay_single(Book::InheritedFontAndImage);
}

// ---------------------------------------------------------------- iç bağlantılar

/// 40 sayfa; 1. sayfa içindekiler: açık hedef (5), doğrudan GoTo (20), dolaylı
/// GoTo (30), `/Names /Dests` ad ağacında dize ad (35), PDF 1.1 `/Dests`
/// sözlüğünde ad (40), dış URI. 20. sayfadan 1. sayfaya geri bağlantı.
fn linked_book() -> Document {
    let mut d = vector_pdf(&[A4; 40]);
    let pages = d.get_pages();
    let page = |n: u32| Object::Reference(pages[&n]);
    fn link(d: &mut Document, nm: &str, y: i64, key: &str, value: Object) -> Object {
        Object::Reference(d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link",
        "NM"=>Object::string_literal(nm),
        "Rect"=>vec![72.into(),y.into(),300.into(),(y+14).into()],
        "Border"=>vec![0.into(),0.into(),0.into()], key=>value}))
    }
    let goto_30 = d.add_object(dictionary! {"S"=>"GoTo","D"=>vec![page(30),"Fit".into()]});
    let leaf = d.add_object(dictionary! {
    "Limits"=>vec![Object::string_literal("bolum-30"),Object::string_literal("bolum-39")],
    "Names"=>vec![Object::string_literal("bolum-35"),
        vec![page(35),"XYZ".into(),0.into(),800.into(),0.into()].into()]});
    let tree = d.add_object(dictionary! {"Kids"=>vec![Object::Reference(leaf)]});
    let dests = d.add_object(dictionary! {"ek-40"=>dictionary!{"D"=>vec![page(40),"Fit".into()]}});
    let toc = vec![
        link(
            &mut d,
            "L1-5",
            700,
            "Dest",
            vec![page(5), "XYZ".into(), 0.into(), 800.into(), 0.into()].into(),
        ),
        link(
            &mut d,
            "L2-20",
            680,
            "A",
            dictionary! {"S"=>"GoTo","D"=>vec![page(20),"Fit".into()]}.into(),
        ),
        link(&mut d, "L3-30", 660, "A", Object::Reference(goto_30)),
        link(
            &mut d,
            "L4-35",
            640,
            "Dest",
            Object::string_literal("bolum-35"),
        ),
        link(&mut d, "L5-40", 620, "Dest", "ek-40".into()),
        link(
            &mut d,
            "L6-uri",
            600,
            "A",
            dictionary! {"S"=>"URI","URI"=>Object::string_literal("https://ornek.test")}.into(),
        ),
    ];
    d.get_dictionary_mut(pages[&1]).unwrap().set("Annots", toc);
    let back = link(
        &mut d,
        "B20-1",
        700,
        "Dest",
        vec![page(1), "Fit".into()].into(),
    );
    d.get_dictionary_mut(pages[&20])
        .unwrap()
        .set("Annots", vec![back]);
    let catalog = d.catalog_mut().unwrap();
    catalog.set("Names", dictionary! {"Dests"=>tree});
    catalog.set("Dests", dests);
    d
}

/// Sayfadaki bağlantılar: `/NM` → hedef sayfanın işareti. Çıktıda hedef açık
/// dizi olmalı (ad ağacı taşınmaz). Hedefi olmayan bağlantı `None`.
fn link_targets(doc: &Document, page: u32) -> std::collections::BTreeMap<String, Option<String>> {
    let number_of: std::collections::HashMap<ObjectId, u32> =
        doc.get_pages().into_iter().map(|(n, id)| (id, n)).collect();
    let page_markers = markers(doc);
    let dict = doc.get_dictionary(doc.get_pages()[&page]).unwrap();
    let Ok(annots) = dict.get(b"Annots") else {
        return Default::default();
    };
    let annots = doc.dereference(annots).unwrap().1.as_array().unwrap();
    annots
        .iter()
        .map(|a| {
            let a = doc.dereference(a).unwrap().1.as_dict().unwrap();
            let nm = String::from_utf8_lossy(a.get(b"NM").unwrap().as_str().unwrap()).into_owned();
            let action = a
                .get(b"A")
                .ok()
                .map(|x| doc.dereference(x).unwrap().1.as_dict().unwrap());
            let dest = a.get(b"Dest").ok().or_else(|| {
                action
                    .filter(|x| x.get(b"S").unwrap().as_name().unwrap() == b"GoTo")
                    .map(|x| x.get(b"D").expect("GoTo eylemi hedefsiz kaldı"))
            });
            let target = dest.map(|dest| {
                let array = doc
                    .dereference(dest)
                    .unwrap()
                    .1
                    .as_array()
                    .unwrap_or_else(|_| panic!("{nm}: hedef açık diziye çözülmemiş: {dest:?}"));
                let id = array[0]
                    .as_reference()
                    .unwrap_or_else(|_| panic!("{nm}: hedef sayfası başvuru değil: {array:?}"));
                page_markers[number_of[&id] as usize - 1].clone()
            });
            (nm, target)
        })
        .collect()
}

/// `L6-uri` bağlantısı dış URI eylemini koruyor mu?
fn keeps_uri(doc: &Document) -> bool {
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .any(|d| {
            d.get(b"NM").ok().and_then(|n| n.as_str().ok()) == Some(b"L6-uri".as_slice())
                && d.get(b"A")
                    .ok()
                    .and_then(|a| doc.dereference(a).ok())
                    .and_then(|(_, a)| a.as_dict().ok())
                    .and_then(|a| a.get(b"URI").ok())
                    .and_then(|u| u.as_str().ok())
                    == Some(b"https://ornek.test".as_slice())
        })
}

#[test]
fn internal_links_follow_retained_pages_and_leave_nothing_for_removed_ones() {
    let lab = Lab::new();
    let src = lab.write("icindekiler.pdf", &mut linked_book());
    let some = |s: &str| Some(s.to_string());

    // Seç 1, 5, 20, 35: hedefi kalan bağlantılar yeni sayfalara; 30 ve 40'a
    // gidenler hedefsiz kalır, sarkan başvuru bırakmaz.
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Select {
            pages: vec![1, 5, 20, 35],
        },
        "secim.pdf",
        false,
    );
    published(&o);
    let d = reopen(&out);
    assert_eq!(markers(&d), ["Sayfa 1", "Sayfa 5", "Sayfa 20", "Sayfa 35"]);
    let toc = link_targets(&d, 1);
    assert_eq!(toc["L1-5"], some("Sayfa 5"));
    assert_eq!(toc["L2-20"], some("Sayfa 20"));
    assert_eq!(toc["L3-30"], None);
    assert_eq!(
        toc["L4-35"],
        some("Sayfa 35"),
        "ad ağacındaki hedef açık hedefe çözülmeli"
    );
    assert_eq!(toc["L5-40"], None);
    assert_eq!(toc["L6-uri"], None);
    assert!(keeps_uri(&d), "dış bağlantı korunmalı");
    assert_eq!(link_targets(&d, 3)["B20-1"], some("Sayfa 1"));
    assert_eq!(toc.len(), 6, "hedefi çıkarılan bağlantı da sayfada durur");
    assert_no_dangling_or_dead_destinations("secim", &d);

    // Sırala (ters): bütün hedefler kalır ve doğru sayfaya gider.
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Reorder {
            pages: (1..=40).rev().collect(),
        },
        "sirala.pdf",
        false,
    );
    published(&o);
    let d = reopen(&out);
    let toc = link_targets(&d, 40);
    for (nm, target) in [
        ("L1-5", "Sayfa 5"),
        ("L2-20", "Sayfa 20"),
        ("L3-30", "Sayfa 30"),
        ("L4-35", "Sayfa 35"),
        ("L5-40", "Sayfa 40"),
    ] {
        assert_eq!(toc[nm], some(target), "sirala/{nm}");
    }
    assert_eq!(toc["L6-uri"], None);
    assert!(keeps_uri(&d), "sirala: dış bağlantı korunmalı");
    assert_eq!(link_targets(&d, 21)["B20-1"], some("Sayfa 1"));
    assert_no_dangling_or_dead_destinations("sirala", &d);

    // Sil 5: yalnız 5'e giden bağlantı hedefsiz kalır.
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Delete { pages: vec![5] },
        "sil.pdf",
        false,
    );
    published(&o);
    let d = reopen(&out);
    let toc = link_targets(&d, 1);
    assert_eq!(toc["L1-5"], None);
    assert_eq!(toc["L2-20"], some("Sayfa 20"));
    assert_eq!(toc["L3-30"], some("Sayfa 30"));
    assert_eq!(toc["L4-35"], some("Sayfa 35"));
    assert_eq!(toc["L5-40"], some("Sayfa 40"));
    assert_eq!(toc["L6-uri"], None);
    assert!(keeps_uri(&d), "sil: dış bağlantı korunmalı");
    assert_eq!(link_targets(&d, 19)["B20-1"], some("Sayfa 1"));
    assert_no_dangling_or_dead_destinations("sil", &d);

    // Birleştir: her belgenin bağlantısı kendi sayfasına; belgeler karışmaz.
    let second = lab.write("icindekiler-2.pdf", &mut linked_book());
    let (o, out) = run(
        &lab,
        &[src.clone(), second],
        ToolOperation::Merge,
        "birlesik.pdf",
        false,
    );
    published(&o);
    let d = reopen(&out);
    assert_eq!(d.get_pages().len(), 80);
    let number_of: std::collections::HashMap<ObjectId, u32> =
        d.get_pages().into_iter().map(|(n, id)| (id, n)).collect();
    let first_target = |page: u32| {
        let a = d
            .get_dictionary(d.get_pages()[&page])
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .clone();
        let dest = d
            .dereference(&a)
            .unwrap()
            .1
            .as_dict()
            .unwrap()
            .get(b"Dest")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_reference()
            .unwrap();
        number_of[&dest]
    };
    assert_eq!(first_target(1), 5);
    assert_eq!(first_target(41), 45);
    assert_no_dangling_or_dead_destinations("birlesik", &d);
}

// ----------------------------------- şüpheci turu: kopyalayıcının sınırları

/// Çıktıda bu türde bir nesne var mı?
fn has_type(doc: &Document, kind: &[u8]) -> bool {
    doc.objects.values().any(|o| {
        let dict = match o {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &s.dict,
            _ => return false,
        };
        dict.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(kind)
    })
}

#[test]
fn page_copy_carries_nothing_from_removed_pages() {
    // Her sızıntı yolu 2. sayfanın bir sırrına çıkar; Seç 1 çıktısı hiçbirini
    // taşımamalı. Yollar: form alanı hiyerarşisi, imza değeri → DocMDP
    // /Data → Catalog (AcroForm, anahat, ad ağacı), ResetForm /Fields ve
    // /Next Hide /T, makale boncuğu /N, yapı hedefi /SD → yapı ağacı → OBJR.
    let mut d = vector_pdf(&[A4, A4]);
    let pages = d.get_pages();
    let (p1, p2) = (pages[&1], pages[&2]);
    let secret_form = |d: &mut Document, secret: &str| {
        d.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),20.into()]},
            format!("BT /F1 8 Tf ({secret}) Tj ET").into_bytes(),
        ))
    };
    let rect = || vec![72.into(), 600.into(), 172.into(), 620.into()];
    // (a) Alan hiyerarşisi: kardeş widget 2. sayfada, değeri gizli.
    let field = d.new_object_id();
    let ap1 = secret_form(&mut d, "gorunur");
    let w1 = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","Rect"=>rect(),"Parent"=>field,"AP"=>dictionary!{"N"=>ap1},"P"=>p1});
    let ap2 = secret_form(&mut d, "GIZLI-AP2");
    let w2 = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","Rect"=>rect(),"Parent"=>field,
        "T"=>Object::string_literal("tc"),"V"=>Object::string_literal("GIZLI-TCKN-12345678901"),"AP"=>dictionary!{"N"=>ap2},"P"=>p2});
    d.objects.insert(field, dictionary! {"FT"=>"Tx","T"=>Object::string_literal("Ad"),"Kids"=>vec![Object::Reference(w1),Object::Reference(w2)]}.into());
    // (b) İmza alanı: /V → imza → /Reference /Data → Catalog.
    let root = d.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let sig = d.add_object(dictionary! {"Type"=>"Sig","Filter"=>"Adobe.PPKLite",
        "Contents"=>Object::string_literal("imza"),"ByteRange"=>vec![0.into(),0.into(),0.into(),0.into()],
        "Reference"=>vec![dictionary!{"Type"=>"SigRef","TransformMethod"=>"DocMDP","Data"=>root}.into()]});
    let sig_widget = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Sig","Rect"=>vec![0.into(),0.into(),0.into(),0.into()],"V"=>sig});
    let field2 = d.add_object(dictionary! {"FT"=>"Tx","T"=>Object::string_literal("p2"),"V"=>Object::string_literal("GIZLI-P2-DEGER")});
    let outline_item = d.new_object_id();
    let outlines =
        d.add_object(dictionary! {"Type"=>"Outlines","First"=>outline_item,"Last"=>outline_item});
    d.objects.insert(outline_item, dictionary! {"Title"=>Object::string_literal("GIZLI-ANAHAT"),"Parent"=>outlines,"Dest"=>vec![Object::Reference(p2),"Fit".into()]}.into());
    // (c) Eylemler: ResetForm /Fields ve /Next Hide /T 2. sayfanın açıklamasına.
    let ap3 = secret_form(&mut d, "GIZLI-AP3");
    let w3 = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Square","Rect"=>rect(),"AP"=>dictionary!{"N"=>ap3},"P"=>p2});
    let reset = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>rect(),
        "A"=>dictionary!{"S"=>"ResetForm","Fields"=>vec![Object::Reference(field2)],"Next"=>dictionary!{"S"=>"Hide","T"=>w3}}});
    // (d) Makale boncukları: 1. sayfanın boncuğu 2. sayfanınkine bağlı.
    let (bead1, bead2) = (d.new_object_id(), d.new_object_id());
    let thread = d.add_object(dictionary! {"Type"=>"Thread","F"=>bead1,"I"=>dictionary!{"Title"=>Object::string_literal("GIZLI-MAKALE")}});
    d.objects.insert(
        bead1,
        dictionary! {"Type"=>"Bead","T"=>thread,"N"=>bead2,"V"=>bead2,"P"=>p1,"R"=>rect()}.into(),
    );
    d.objects.insert(
        bead2,
        dictionary! {"Type"=>"Bead","N"=>bead1,"V"=>bead1,"P"=>p2,"R"=>rect()}.into(),
    );
    // (e) Yapı hedefi: /SD → yapı öğesi → OBJR → 2. sayfanın bağlantısı.
    let ap4 = secret_form(&mut d, "GIZLI-AP4");
    let w4 = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>rect(),"AP"=>dictionary!{"N"=>ap4},"P"=>p2});
    let tree = d.new_object_id();
    let element = d.add_object(dictionary! {"Type"=>"StructElem","S"=>"Link","P"=>tree,"K"=>vec![dictionary!{"Type"=>"OBJR","Obj"=>w4}.into()]});
    d.objects.insert(
        tree,
        dictionary! {"Type"=>"StructTreeRoot","K"=>vec![Object::Reference(element)]}.into(),
    );
    let sd = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>rect(),
        "A"=>dictionary!{"S"=>"GoTo","D"=>vec![Object::Reference(p1),"Fit".into()],"SD"=>vec![Object::Reference(element),"Fit".into()]}});
    // Sayfalara ve Catalog'a bağla.
    d.get_dictionary_mut(p1).unwrap().set(
        "Annots",
        vec![w1.into(), sig_widget.into(), reset.into(), sd.into()],
    );
    d.get_dictionary_mut(p1)
        .unwrap()
        .set("B", vec![Object::Reference(bead1)]);
    d.get_dictionary_mut(p2)
        .unwrap()
        .set("Annots", vec![w2.into(), w3.into(), w4.into()]);
    d.get_dictionary_mut(p2)
        .unwrap()
        .set("B", vec![Object::Reference(bead2)]);
    let catalog = d.catalog_mut().unwrap();
    catalog.set("AcroForm", dictionary! {"Fields"=>vec![Object::Reference(field), Object::Reference(field2), Object::Reference(sig_widget)]});
    catalog.set("Outlines", outlines);
    catalog.set("StructTreeRoot", tree);
    catalog.set("Threads", vec![Object::Reference(thread)]);
    catalog.set("Names", dictionary! {"Dests"=>dictionary!{"Names"=>vec![Object::string_literal("to-p2"), vec![Object::Reference(p2),"Fit".into()].into()]}});
    catalog.set("Perms", dictionary! {"DocMDP"=>sig});

    let lab = Lab::new();
    let src = lab.write("sizinti.pdf", &mut d);
    // İmza sözlüğü taşıdığı için türetilmiş kopya onayla yapılır.
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Select { pages: vec![1] },
        "tek.pdf",
        true,
    );
    published(&o);
    let raw = std::fs::read(&out).unwrap();
    assert!(
        !raw.windows(5).any(|w| w == b"GIZLI"),
        "çıkarılan sayfanın verisi tek sayfalık çıktıya sızdı"
    );
    let out_doc = reopen(&out);
    for kind in [
        b"StructTreeRoot".as_slice(),
        b"StructElem",
        b"Outlines",
        b"Thread",
        b"Bead",
        b"Sig",
    ] {
        assert!(
            !has_type(&out_doc, kind),
            "{} çıktıya taşındı",
            String::from_utf8_lossy(kind)
        );
    }
    assert_eq!(
        out_doc
            .objects
            .values()
            .filter(|o| o
                .as_dict()
                .ok()
                .and_then(|d| d.get(b"Type").ok())
                .and_then(|t| t.as_name().ok())
                == Some(b"Catalog".as_slice()))
            .count(),
        1,
        "kaynağın Catalog'u çıktıya ikinci kez kopyalandı"
    );
    assert_eq!(
        annot_count(&out_doc, 1),
        4,
        "1. sayfanın kendi açıklamaları yerinde"
    );
    assert_no_dangling_or_dead_destinations("sizinti", &out_doc);
}

#[test]
fn removed_goto_actions_do_not_survive_in_any_position() {
    // Hedefi 2. sayfa olan GoTo eylemleri: URI'nin /Next'inde, bağlantının
    // /AA /E'sinde ve 1. sayfanın /AA /O'sunda. Sil 2 → hiçbiri /D'siz kalmaz.
    let mut d = vector_pdf(&[A4; 6]);
    let pages = d.get_pages();
    let goto2 = || dictionary! {"S"=>"GoTo","D"=>vec![Object::Reference(pages[&2]),"Fit".into()]};
    let rect = || vec![72.into(), 600.into(), 172.into(), 620.into()];
    let uri = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","NM"=>Object::string_literal("uri"),"Rect"=>rect(),
        "A"=>dictionary!{"S"=>"URI","URI"=>Object::string_literal("https://ornek.test"),"Next"=>goto2()}});
    let chained = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","NM"=>Object::string_literal("zincir"),"Rect"=>rect(),
        "A"=>dictionary!{"S"=>"URI","URI"=>Object::string_literal("https://a.test"),
            "Next"=>dictionary!{"S"=>"GoTo","D"=>vec![Object::Reference(pages[&2]),"Fit".into()],
                "Next"=>dictionary!{"S"=>"JavaScript","JS"=>Object::string_literal("KORUNACAK-JS")}}}});
    let enter = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","NM"=>Object::string_literal("aa"),"Rect"=>rect(),
        "AA"=>dictionary!{"E"=>goto2()}});
    let page1 = d.get_dictionary_mut(pages[&1]).unwrap();
    page1.set("Annots", vec![uri.into(), enter.into(), chained.into()]);
    page1.set("AA", dictionary! {"O"=>goto2()});
    let lab = Lab::new();
    let src = lab.write("eylem.pdf", &mut d);

    let incomplete_goto = |doc: &Document| {
        fn visit(obj: &Object, found: &mut usize) {
            match obj {
                Object::Dictionary(d) => {
                    if d.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"GoTo".as_slice())
                        && !d.has(b"D")
                    {
                        *found += 1;
                    }
                    d.iter().for_each(|(_, v)| visit(v, found));
                }
                Object::Array(a) => a.iter().for_each(|v| visit(v, found)),
                Object::Stream(s) => s.dict.iter().for_each(|(_, v)| visit(v, found)),
                _ => {}
            }
        }
        let mut found = 0;
        doc.objects.values().for_each(|o| visit(o, &mut found));
        found
    };

    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Delete { pages: vec![2] },
        "sil.pdf",
        false,
    );
    published(&o);
    let deleted = reopen(&out);
    assert_eq!(incomplete_goto(&deleted), 0, "hedefsiz GoTo eylemi kaldı");
    assert!(
        format!("{:?}", deleted.objects).contains("KORUNACAK-JS"),
        "kaldırılan GoTo'nun ardındaki eylem de düştü"
    );
    assert!(
        format!("{:?}", deleted.objects).contains("https://ornek.test"),
        "URI eylemi korunmalı"
    );
    assert_no_dangling_or_dead_destinations("sil", &deleted);

    // Hedef kalınca üç eylem de yeni 2. sayfaya gider.
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Select { pages: vec![1, 2] },
        "sec.pdf",
        false,
    );
    published(&o);
    let kept = reopen(&out);
    let second = kept.get_pages()[&2];
    let text = format!("{:?}", kept.objects);
    assert_eq!(incomplete_goto(&kept), 0);
    assert_eq!(
        text.matches(&format!("/D [{} {} R /Fit]", second.0, second.1))
            .count(),
        4,
        "kalan hedefe giden dört GoTo yeni sayfaya bağlanmalı: {text}"
    );
}

#[test]
fn null_integer_and_named_destinations_resolve_like_viewers() {
    let mut d = vector_pdf(&[A4; 6]);
    let pages = d.get_pages();
    let page = |n: u32| Object::Reference(pages[&n]);
    let missing = d.new_object_id();
    let via_object = d.add_object(Object::Reference(pages[&2]));
    let chain_end = d.new_object_id();
    let to_missing = d.add_object(Object::Reference(chain_end));
    let link = |d: &mut Document, nm: &str, key: &str, value: Object| {
        Object::Reference(d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link",
            "NM"=>Object::string_literal(nm),"Rect"=>vec![72.into(),600.into(),172.into(),620.into()], key=>value}))
    };
    let annots = vec![
        // Dosyada olmayan sayfa: yükleyici null sayar, kopya hedefi bırakmaz.
        link(
            &mut d,
            "yok-dest",
            "Dest",
            vec![Object::Reference(missing), "Fit".into()].into(),
        ),
        link(
            &mut d,
            "yok-goto",
            "A",
            dictionary! {"S"=>"GoTo","D"=>vec![Object::Reference(missing),"Fit".into()]}.into(),
        ),
        // Sayfaya bir başvuru NESNESİ üzerinden gidilen hedef (`40 0 obj 12 0 R`).
        link(
            &mut d,
            "sayfa-zinciri",
            "Dest",
            vec![Object::Reference(via_object), "Fit".into()].into(),
        ),
        // Dosyada olmayan nesneye zincirlenen sayfa: yükleyici null yapar, hedef düşer.
        link(
            &mut d,
            "null-zinciri",
            "Dest",
            vec![Object::Reference(to_missing), "Fit".into()].into(),
        ),
        // Tamsayı: 0 tabanlı sayfa numarası → 4. sayfa.
        link(
            &mut d,
            "tamsayi",
            "Dest",
            vec![3.into(), "Fit".into()].into(),
        ),
        // Dize ad ağacında, ad /Dests sözlüğünde aranır (ISO 32000-1 12.3.2.3).
        link(&mut d, "dize", "Dest", Object::string_literal("giris")),
        link(&mut d, "ad", "Dest", "giris".into()),
    ];
    d.get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", annots);
    let catalog = d.catalog_mut().unwrap();
    catalog.set("Dests", dictionary! {"giris"=>vec![page(5),"Fit".into()]});
    catalog.set("Names", dictionary! {"Dests"=>dictionary!{"Names"=>vec![Object::string_literal("giris"), vec![page(2),"Fit".into()].into()]}});
    let lab = Lab::new();
    let src = lab.write("hedefler.pdf", &mut d);
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Reorder {
            pages: (1..=6).rev().collect(),
        },
        "ters.pdf",
        false,
    );
    published(&o);
    let reversed = reopen(&out);
    let links = link_targets(&reversed, 6);
    let some = |s: &str| Some(s.to_string());
    assert_eq!(links["yok-dest"], None);
    assert_eq!(links["yok-goto"], None);
    assert_eq!(links["tamsayi"], some("Sayfa 4"));
    assert_eq!(links["sayfa-zinciri"], some("Sayfa 2"));
    assert_eq!(links["null-zinciri"], None);
    assert_eq!(links["dize"], some("Sayfa 2"));
    assert_eq!(links["ad"], some("Sayfa 5"));
    assert_no_dangling_or_dead_destinations("ters", &reversed);
}

#[test]
fn page_listed_through_a_reference_object_keeps_its_links_and_stays_single() {
    // `/Kids [p1 40 0 R p3]`, `40 0 obj 12 0 R`: sayfa ağacı 2. sayfayı bir
    // başvuru NESNESİ üzerinden listeliyor. lopdf'in `get_pages`i zincirin
    // başını (40) verir; bağlantı sayfanın kendi kimliğini (12) ya da zinciri
    // (40) gösterebilir. İkisi de aynı sayfaya gitmeli, sayfa tek kopya kalmalı.
    let mut d = vector_pdf(&[A4; 3]);
    let pages = d.get_pages();
    let via = d.add_object(Object::Reference(pages[&2]));
    let root = d
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    d.get_dictionary_mut(root)
        .unwrap()
        .set("Kids", vec![pages[&1].into(), via.into(), pages[&3].into()]);
    let link = |d: &mut Document, nm: &str, target: ObjectId| {
        Object::Reference(d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link",
        "NM"=>Object::string_literal(nm),"Rect"=>vec![72.into(),600.into(),172.into(),620.into()],
        "Dest"=>vec![Object::Reference(target),"Fit".into()]}))
    };
    let annots = vec![
        link(&mut d, "sayfa-kimligi", pages[&2]),
        link(&mut d, "kids-zinciri", via),
    ];
    d.get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", annots);
    let lab = Lab::new();
    let src = lab.write("kids-zinciri.pdf", &mut d);
    let some = |s: &str| Some(s.to_string());
    let page_objects = |doc: &Document| {
        doc.objects
            .values()
            .filter(|o| {
                o.as_dict()
                    .is_ok_and(|d| d.get(b"Type").and_then(|t| t.as_name()).ok() == Some(b"Page"))
            })
            .count()
    };

    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Reorder {
            pages: vec![3, 2, 1],
        },
        "ters.pdf",
        false,
    );
    published(&o);
    let reversed = reopen(&out);
    assert_eq!(markers(&reversed), ["Sayfa 3", "Sayfa 2", "Sayfa 1"]);
    assert_eq!(page_objects(&reversed), 3, "sayfa iki kez kopyalandı");
    let links = link_targets(&reversed, 3);
    assert_eq!(links["sayfa-kimligi"], some("Sayfa 2"));
    assert_eq!(links["kids-zinciri"], some("Sayfa 2"));
    assert_no_dangling_or_dead_destinations("ters", &reversed);

    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Delete { pages: vec![2] },
        "sil.pdf",
        false,
    );
    published(&o);
    let deleted = reopen(&out);
    assert_eq!(markers(&deleted), ["Sayfa 1", "Sayfa 3"]);
    assert_eq!(page_objects(&deleted), 2, "silinen sayfa çıktıya taşındı");
    let links = link_targets(&deleted, 1);
    assert_eq!(links["sayfa-kimligi"], None);
    assert_eq!(links["kids-zinciri"], None);
    assert_no_dangling_or_dead_destinations("sil", &deleted);
}

#[test]
fn looped_name_tree_cannot_hang_preview_or_select() {
    // `/Kids [7 0 R 7 0 R 7 0 R]`: kendini gösteren ad ağacı düğümü.
    let mut d = vector_pdf(&[A4, A4]);
    let pages = d.get_pages();
    let tree = d.new_object_id();
    d.objects.insert(
        tree,
        dictionary! {"Kids"=>vec![Object::Reference(tree); 3]}.into(),
    );
    d.catalog_mut()
        .unwrap()
        .set("Names", dictionary! {"Dests"=>tree});
    let annot = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>vec![72.into(),600.into(),172.into(),620.into()],"Dest"=>Object::string_literal("yok")});
    d.get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", vec![Object::Reference(annot)]);
    let lab = Lab::new();
    let src = lab.write("dongu.pdf", &mut d);
    let out = lab.path("tek.pdf");
    let (tx, rx) = std::sync::mpsc::channel();
    let job = (src.clone(), out.clone());
    std::thread::spawn(move || {
        let selected = run_tool_with_outcome(
            std::slice::from_ref(&job.0),
            &ToolOperation::Select { pages: vec![1] },
            &job.1,
            false,
        )
        .map(|_| ())
        .map_err(|e| e.to_string());
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let preview = ekler_core::raster::preview_page(&job.0, 1, 72, 0)
            .map(|_| ())
            .map_err(|e| e.to_string());
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let preview: std::result::Result<(), String> = Ok(());
        let _ = tx.send((selected, preview));
    });
    let (selected, preview) = rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("döngülü ad ağacı Seç/önizlemeyi kilitledi");
    selected.unwrap();
    preview.unwrap();
    assert_eq!(reopen(&out).get_pages().len(), 1);
}

#[test]
fn long_object_chains_cannot_overflow_the_stack() {
    // Kabuk araçları ve önizlemeyi 2 MiB yığınlı spawn_blocking iş
    // parçacığında koşar. Görünüm grafiğinde 3.000 halkalı iç içe Form
    // XObject zinciri ve bağlantıda 3.000 halkalı /Next eylem zinciri.
    let mut d = vector_pdf(&[A4, A4]);
    let pages = d.get_pages();
    let mut inner: Option<ObjectId> = None;
    for n in 0..3_000 {
        let mut dict = dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),10.into(),10.into()]};
        let content = match inner {
            Some(child) => {
                dict.set(
                    "Resources",
                    dictionary! {"XObject"=>dictionary!{"X"=>child}},
                );
                b"/X Do".to_vec()
            }
            None => format!("0 0 {n} 1 re f").into_bytes(),
        };
        inner = Some(d.add_object(Stream::new(dict, content)));
    }
    let mut next = dictionary! {"S"=>"URI","URI"=>Object::string_literal("https://ornek.test/son")};
    for _ in 0..3_000 {
        let id = d.add_object(next);
        next =
            dictionary! {"S"=>"URI","URI"=>Object::string_literal("https://ornek.test"),"Next"=>id};
    }
    let link = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>vec![72.into(),600.into(),172.into(),620.into()],"A"=>next});
    let page1 = d.get_dictionary_mut(pages[&1]).unwrap();
    page1.set(
        "Resources",
        dictionary! {"Font"=>dictionary!{"F1"=>d_font_placeholder()},"XObject"=>dictionary!{"Zincir"=>inner.unwrap()}},
    );
    page1.set("Annots", vec![Object::Reference(link)]);
    let lab = Lab::new();
    let src = lab.write("zincir.pdf", &mut d);
    let out = lab.path("tek.pdf");
    let job = (src.clone(), out.clone());
    let result = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            run_tool_with_outcome(
                std::slice::from_ref(&job.0),
                &ToolOperation::Select { pages: vec![1] },
                &job.1,
                false,
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .expect("iş parçacığı düştü");
    result.unwrap();
    assert_eq!(reopen(&out).get_pages().len(), 1);
}

/// `vector_pdf`'in sayfa fontu yerine kullanılan bağımsız Helvetica.
fn d_font_placeholder() -> Object {
    Object::Dictionary(font_dict())
}

#[test]
fn hidden_optional_content_stays_hidden_in_page_copies() {
    // Kaynakta kapalı katman. Catalog `/OCProperties` taşınmazsa kopyada
    // gizli içerik görünür olur. Tek kaynak (Seç) ve farklı taban durumlu iki
    // kaynağın birleşimi (Birleştir) denenir.
    fn layered(base_state_off: bool) -> Document {
        let mut d = vector_pdf(&[A4, A4]);
        let hidden =
            d.add_object(dictionary! {"Type"=>"OCG","Name"=>Object::string_literal("Gizli")});
        let shown =
            d.add_object(dictionary! {"Type"=>"OCG","Name"=>Object::string_literal("Acik")});
        for id in d.get_pages().into_values() {
            let resources = d
                .get_dictionary_mut(id)
                .unwrap()
                .get_mut(b"Resources")
                .unwrap()
                .as_dict_mut()
                .unwrap();
            resources.set("Properties", dictionary! {"L1"=>hidden,"L2"=>shown});
        }
        let config = if base_state_off {
            dictionary! {"BaseState"=>"OFF","ON"=>vec![Object::Reference(shown)]}
        } else {
            dictionary! {"OFF"=>vec![Object::Reference(hidden)]}
        };
        d.catalog_mut().unwrap().set(
            "OCProperties",
            dictionary! {"OCGs"=>vec![Object::Reference(hidden), Object::Reference(shown)],"D"=>config},
        );
        d
    }
    fn off_names(doc: &Document) -> Vec<String> {
        let props = doc
            .catalog()
            .unwrap()
            .get(b"OCProperties")
            .expect("katman yapılandırması taşınmadı");
        let props = doc.dereference(props).unwrap().1.as_dict().unwrap();
        let config = doc
            .dereference(props.get(b"D").unwrap())
            .unwrap()
            .1
            .as_dict()
            .unwrap();
        let mut names: Vec<String> = config
            .get(b"OFF")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|g| {
                let g = doc.dereference(g).unwrap().1.as_dict().unwrap();
                String::from_utf8_lossy(g.get(b"Name").unwrap().as_str().unwrap()).into_owned()
            })
            .collect();
        names.sort();
        names
    }
    let lab = Lab::new();
    let single = lab.write("katman.pdf", &mut layered(false));
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&single),
        ToolOperation::Select { pages: vec![2] },
        "sec.pdf",
        false,
    );
    published(&o);
    let d = reopen(&out);
    assert_eq!(off_names(&d), ["Gizli"]);
    // Sayfanın başvurduğu grup ile kapalı listedeki grup AYNI nesne.
    let page = pdf::resolved_page_dictionary(&d, d.get_pages()[&1]).unwrap();
    let resources = d
        .dereference(page.get(b"Resources").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    let l1 = resources
        .get(b"Properties")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"L1")
        .unwrap()
        .as_reference()
        .unwrap();
    let props = d
        .dereference(d.catalog().unwrap().get(b"OCProperties").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    let off = d
        .dereference(props.get(b"D").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap()
        .get(b"OFF")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .as_reference()
        .unwrap();
    assert_eq!(
        l1, off,
        "sayfadaki katman grubu yapılandırmadakinden farklı nesneye kopyalandı"
    );

    let base_off = lab.write("katman-taban-kapali.pdf", &mut layered(true));
    let (o, out) = run(
        &lab,
        &[single, base_off],
        ToolOperation::Merge,
        "birlesik.pdf",
        false,
    );
    published(&o);
    let d = reopen(&out);
    assert_eq!(
        off_names(&d),
        ["Gizli", "Gizli"],
        "taban durumu OFF olan kaynağın gizli grubu açık listeye çevrilmeli"
    );
    assert_no_dangling_or_dead_destinations("katman", &d);
}

// ================================== İMZALI KAYNAKTAN TÜRETİLEN KOPYA (2026-09-15)
//
// Yeniden yazılan PDF'te imzanın /ByteRange'i artık imzalanan baytları
// göstermez; imza kriptografik olarak geçersizdir. Döndür, Kırp, Numara,
// Filigran ve Sıkıştır belgeyi yerinde yeniden yazdığı için imza sözlüğünü,
// /Perms ve /SigFlags'i çıktıya taşıyordu: kopya görüntüleyicide bozuk imzalı,
// GölgeDosya'nın tarayıcısında "imzalı" görünüyordu. Temizlik ANLAMSALDIR:
// imza sözlüğü ISO 32000-1 §12.8.1 biçimiyle tanınır, anahtar adıyla değil.
// Benzer görünen yapılar — sayfanın /Contents'i, metin alanının /V'si,
// boncuğun /V'si, imza olmayan bir sözlükteki /Reference ve /Contents —
// korunmak zorundadır.

const SIGNATURE_BLOB: &[u8] = b"GIZLI-PKCS7-IMZA-DEGERI";
const TIMESTAMP_BLOB: &[u8] = b"GIZLI-RFC3161-ZAMAN-DAMGASI";
const USAGE_RIGHTS_BLOB: &[u8] = b"GIZLI-UR3-KULLANIM-HAKKI";
const CERTIFICATE_BLOB: &[u8] = b"GIZLI-DSS-SERTIFIKA";
const SIGNATURE_APPEARANCE: &[u8] = b"0 0 1 rg 0 0 150 40 re f";

fn signed_pdf() -> Document {
    let mut d = vector_pdf(&[A4, A4, A4]);
    let pages = d.get_pages();
    // 2. sayfada yeniden kodlanabilir bir tarama: Sıkıştır gerçekten yayınlasın.
    let scan = d.add_object(flate_rgb_stream(
        &scan_pixels(1200, 900),
        "DeviceRGB".into(),
    ));
    let content = d.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 24 Tf 72 720 Td (Sayfa 2) Tj ET q 480 0 0 360 60 200 cm /Im1 Do Q".to_vec(),
    ));
    let page2 = d.get_dictionary_mut(pages[&2]).unwrap();
    page2.set("Contents", content);
    page2.set(
        "Resources",
        dictionary! {"Font"=>dictionary!{"F1"=>font_dict()},"XObject"=>dictionary!{"Im1"=>scan}},
    );
    let hex = |b: &[u8]| Object::String(b.to_vec(), lopdf::StringFormat::Hexadecimal);
    let byte_range = || vec![0.into(), 100.into(), 200.into(), 300.into()];
    // Sertifika imzası (DocMDP), belge zaman damgası, kullanım hakları (UR3).
    let signature = d.add_object(dictionary! {"Type"=>"Sig","Filter"=>"Adobe.PPKLite",
    "SubFilter"=>"adbe.pkcs7.detached","ByteRange"=>byte_range(),"Contents"=>hex(SIGNATURE_BLOB),
    "Reference"=>vec![dictionary!{"Type"=>"SigRef","TransformMethod"=>"DocMDP",
        "TransformParams"=>dictionary!{"Type"=>"TransformParams","P"=>2,"V"=>"1.2"}}.into()]});
    let timestamp = d.add_object(
        dictionary! {"Type"=>"DocTimeStamp","Filter"=>"Adobe.PPKLite",
        "SubFilter"=>"ETSI.RFC3161","ByteRange"=>byte_range(),"Contents"=>hex(TIMESTAMP_BLOB)},
    );
    // /Type'sız imza sözlüğü: yalnız biçimiyle (Filter + ByteRange + Contents) tanınır.
    let usage_rights = d.add_object(dictionary! {"Filter"=>"Adobe.PPKLite",
    "SubFilter"=>"adbe.pkcs7.sha1","ByteRange"=>byte_range(),"Contents"=>hex(USAGE_RIGHTS_BLOB)});
    let certificate = d.add_object(Stream::new(dictionary! {}, CERTIFICATE_BLOB.to_vec()));
    let form = |d: &mut Document, content: &[u8]| {
        d.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),150.into(),40.into()]},
            content.to_vec(),
        ))
    };
    let appearance = form(&mut d, SIGNATURE_APPEARANCE);
    let sig_widget = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Sig",
        "T"=>Object::string_literal("Imza1"),"Rect"=>vec![400.into(),80.into(),550.into(),120.into()],
        "V"=>signature,"Lock"=>dictionary!{"Type"=>"SigFieldLock","Action"=>"All"},
        "AP"=>dictionary!{"N"=>appearance},"P"=>pages[&1]});
    let stamp_widget = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Sig",
        "T"=>Object::string_literal("ZamanDamgasi"),"Rect"=>vec![0.into(),0.into(),0.into(),0.into()],
        "V"=>timestamp,"P"=>pages[&1]});
    // Benzer görünen, imza OLMAYAN yapılar.
    let text_appearance = form(&mut d, b"BT /Helv 10 Tf 2 5 Td (Ali Veli) Tj ET");
    let text_field = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Tx",
        "T"=>Object::string_literal("Ad"),"V"=>Object::string_literal("Ali Veli"),
        "Rect"=>vec![72.into(),600.into(),272.into(),620.into()],"AP"=>dictionary!{"N"=>text_appearance},"P"=>pages[&1]});
    let private = d.add_object(dictionary! {"Type"=>"GolgeTest","Filter"=>"Yok",
        "Reference"=>Object::string_literal("imza-degil"),"Contents"=>Object::string_literal("ozel veri")});
    let (thread, bead) = (d.new_object_id(), d.new_object_id());
    d.objects.insert(
        bead,
        dictionary! {"Type"=>"Bead","T"=>thread,"N"=>bead,"V"=>bead,"P"=>pages[&1],
        "R"=>vec![72.into(),72.into(),500.into(),700.into()]}
        .into(),
    );
    d.objects
        .insert(thread, dictionary! {"Type"=>"Thread","F"=>bead}.into());
    let page1 = d.get_dictionary_mut(pages[&1]).unwrap();
    page1.set(
        "Annots",
        vec![sig_widget.into(), stamp_widget.into(), text_field.into()],
    );
    page1.set(
        "PieceInfo",
        dictionary! {"GolgeTest"=>dictionary!{"Private"=>private,
        "LastModified"=>Object::string_literal("D:20260915")}},
    );
    page1.set("B", vec![Object::Reference(bead)]);
    let acro_form = d.add_object(dictionary! {
    "Fields"=>vec![sig_widget.into(), stamp_widget.into(), text_field.into()],"SigFlags"=>3});
    let catalog = d.catalog_mut().unwrap();
    catalog.set("AcroForm", acro_form);
    catalog.set(
        "Perms",
        dictionary! {"DocMDP"=>signature,"UR3"=>usage_rights},
    );
    catalog.set(
        "DSS",
        dictionary! {"Certs"=>vec![Object::Reference(certificate)]},
    );
    catalog.set("Threads", vec![Object::Reference(thread)]);
    d
}

/// ISO 32000-1 §12.8.1 biçimi, üründen bağımsız yazılmış hâli.
fn looks_like_signature(doc: &Document, dict: &Dictionary) -> bool {
    let get = |k: &[u8]| dict.get(k).ok().map(|v| doc.dereference(v).unwrap().1);
    matches!(get(b"Type"), Some(Object::Name(n)) if n == b"Sig" || n == b"DocTimeStamp")
        || (matches!(get(b"Filter"), Some(Object::Name(_)))
            && matches!(get(b"ByteRange"), Some(Object::Array(_)))
            && matches!(get(b"Contents"), Some(Object::String(..))))
}

/// Trailer'dan erişilen her sözlük (akış sözlükleri dâhil).
fn reachable_dictionaries(doc: &Document) -> Vec<Dictionary> {
    let mut seen = std::collections::HashSet::new();
    let mut stack: Vec<Object> = doc.trailer.iter().map(|(_, v)| v.clone()).collect();
    let mut out = Vec::new();
    while let Some(obj) = stack.pop() {
        match obj {
            Object::Reference(id) => {
                if seen.insert(id) {
                    if let Ok(found) = doc.get_object(id) {
                        stack.push(found.clone());
                    }
                }
            }
            Object::Array(items) => stack.extend(items),
            Object::Dictionary(d) => {
                stack.extend(d.iter().map(|(_, v)| v.clone()));
                out.push(d);
            }
            Object::Stream(s) => {
                stack.extend(s.dict.iter().map(|(_, v)| v.clone()));
                out.push(s.dict);
            }
            _ => {}
        }
    }
    out
}

fn annotations_on(doc: &Document, page: u32) -> Vec<Dictionary> {
    doc.get_dictionary(doc.get_pages()[&page])
        .unwrap()
        .get(b"Annots")
        .map(|a| doc.dereference(a).unwrap().1.as_array().unwrap().clone())
        .unwrap_or_default()
        .into_iter()
        .map(|a| doc.dereference(&a).unwrap().1.as_dict().unwrap().clone())
        .collect()
}

fn stream_bytes(doc: &Document, value: &Object) -> Vec<u8> {
    let stream = doc.dereference(value).unwrap().1.as_stream().unwrap();
    stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone())
}

/// İmzalı kaynaktan türetilen kopyanın on değişmezi.
fn assert_signature_free_copy(name: &str, source: &Document, out: &Path, in_place: bool) {
    let raw = std::fs::read(out).unwrap();
    // PDF yeniden açılıyor (katı + onarımsız) ve tarayıcı imzasız görüyor.
    let d = reopen(out);
    let receipt = scan_source_files(&[out]);
    assert!(
        !receipt.sources[0].is_signed,
        "{name}: tarayıcı kopyayı imzalı saydı"
    );
    assert!(!pdf::detect_signature(&d), "{name}");
    // Eski kriptografik imza, /ByteRange ve imza /Contents'i erişilemez; imza
    // malzemesinin baytları dosyada hiç yok.
    for dict in reachable_dictionaries(&d) {
        assert!(
            !looks_like_signature(&d, &dict),
            "{name}: erişilebilir imza sözlüğü: {dict:?}"
        );
        assert!(!dict.has(b"ByteRange"), "{name}: erişilebilir /ByteRange");
    }
    for blob in [
        SIGNATURE_BLOB,
        TIMESTAMP_BLOB,
        USAGE_RIGHTS_BLOB,
        CERTIFICATE_BLOB,
    ] {
        let hex_upper: Vec<u8> = blob
            .iter()
            .flat_map(|b| format!("{b:02X}").into_bytes())
            .collect();
        let hex_lower = hex_upper.to_ascii_lowercase();
        for needle in [blob.to_vec(), hex_upper, hex_lower] {
            assert!(
                !raw.windows(needle.len()).any(|w| w == needle.as_slice()),
                "{name}: imza malzemesi dosyada kaldı: {}",
                String::from_utf8_lossy(blob)
            );
        }
    }
    // Catalog /Perms ve /DSS yok; /SigFlags imzalı belge bildirmiyor.
    let catalog = d.catalog().unwrap();
    assert!(!catalog.has(b"Perms") && !catalog.has(b"DSS"), "{name}");
    if let Ok(form) = catalog.get(b"AcroForm") {
        let form = d.dereference(form).unwrap().1.as_dict().unwrap();
        assert!(!form.has(b"SigFlags"), "{name}: /SigFlags kaldı");
    }
    // Sayfa içerikleri, görünür imza görünümü, imza alanının boş semantiği ve
    // benzer görünen yapılar: kaynakta "Sayfa N" olan her çıktı sayfasında.
    let source_markers = markers(source);
    for (index, marker) in markers(&d).iter().enumerate() {
        let out_page = index as u32 + 1;
        let src_page = source_markers.iter().position(|m| m == marker).unwrap() as u32 + 1;
        let original = source
            .get_page_content(source.get_pages()[&src_page])
            .unwrap();
        let copied = d.get_page_content(d.get_pages()[&out_page]).unwrap();
        assert!(
            copied
                .windows(original.len())
                .any(|w| w == original.as_slice()),
            "{name}/{marker}: sayfa içeriği değişti"
        );
        if src_page != 1 {
            continue;
        }
        let annotations = annotations_on(&d, out_page);
        let signature_fields: Vec<_> = annotations
            .iter()
            .filter(|a| a.get(b"FT").and_then(|t| t.as_name()).ok() == Some(b"Sig"))
            .collect();
        assert_eq!(
            signature_fields.len(),
            2,
            "{name}: imza alanları sayfada kalmalı"
        );
        for field in &signature_fields {
            assert!(!field.has(b"V"), "{name}: imza alanı değer taşıyor");
        }
        let visible = signature_fields
            .iter()
            .find(|f| f.get(b"T").and_then(|t| t.as_str()).ok() == Some(b"Imza1".as_slice()))
            .unwrap();
        let ap = visible
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap();
        assert_eq!(
            stream_bytes(&d, ap),
            SIGNATURE_APPEARANCE,
            "{name}: imza görünümü değişti"
        );
        let text = annotations
            .iter()
            .find(|a| a.get(b"FT").and_then(|t| t.as_name()).ok() == Some(b"Tx"))
            .expect("metin alanı");
        assert_eq!(
            text.get(b"V").unwrap().as_str().unwrap(),
            b"Ali Veli",
            "{name}: metin alanının /V'si silindi"
        );
        let page = pdf::resolved_page_dictionary(&d, d.get_pages()[&out_page]).unwrap();
        let private = page
            .get(b"PieceInfo")
            .and_then(|p| p.as_dict())
            .and_then(|p| p.get(b"GolgeTest"))
            .and_then(|g| g.as_dict())
            .and_then(|g| g.get(b"Private"))
            .map(|p| d.dereference(p).unwrap().1.as_dict().unwrap().clone())
            .unwrap_or_else(|_| panic!("{name}: sayfanın özel verisi düştü"));
        assert_eq!(
            private.get(b"Reference").unwrap().as_str().unwrap(),
            b"imza-degil",
            "{name}"
        );
        assert_eq!(
            private.get(b"Contents").unwrap().as_str().unwrap(),
            b"ozel veri",
            "{name}"
        );
        if in_place {
            // Yerinde yazımda makale zinciri korunur; boncuğun /V'si silinmez.
            let bead = page.get(b"B").unwrap().as_array().unwrap()[0].clone();
            let bead = d.dereference(&bead).unwrap().1.as_dict().unwrap();
            assert!(bead.has(b"V"), "{name}: boncuğun /V'si silindi");
        }
    }
}

#[test]
fn derived_copies_of_signed_pdfs_carry_no_signature_semantics() {
    let lab = Lab::new();
    let mut source = signed_pdf();
    let src = lab.write("imzali.pdf", &mut source);
    let second = lab.write("imzali-2.pdf", &mut signed_pdf());
    let source = load_pdf_tolerant(&std::fs::read(&src).unwrap(), "imzali.pdf")
        .unwrap()
        .document;
    assert!(scan_source_files(std::slice::from_ref(&src)).sources[0].is_signed);
    for (label, op, in_place) in [
        ("sec", ToolOperation::Select { pages: vec![1, 2] }, false),
        (
            "sirala",
            ToolOperation::Reorder {
                pages: vec![3, 2, 1],
            },
            false,
        ),
        ("sil", ToolOperation::Delete { pages: vec![3] }, false),
        ("dondur", ToolOperation::Rotate { degrees: 90 }, true),
        ("kirp", ToolOperation::Crop { margin_pt: 5.0 }, true),
        ("numara", ToolOperation::Number { start: 1 }, true),
        (
            "filigran",
            ToolOperation::Watermark {
                text: "KOPYA".into(),
            },
            true,
        ),
    ] {
        // `run` kaynak SHA-256'yı işlem öncesi ve sonrası karşılaştırır.
        let (o, out) = run(
            &lab,
            std::slice::from_ref(&src),
            op,
            &format!("{label}.pdf"),
            true,
        );
        published(&o);
        assert_signature_free_copy(label, &source, &out, in_place);
    }
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "sikistir.pdf",
        true,
    );
    assert!(
        matches!(o, ToolOutcome::Compressed { .. }),
        "fixture sıkıştırmada yayın üretmeli: {o:?}"
    );
    assert_signature_free_copy("sikistir", &source, &out, true);
    let (o, out) = run(
        &lab,
        &[src, second],
        ToolOperation::Merge,
        "birlestir.pdf",
        true,
    );
    published(&o);
    assert_signature_free_copy("birlestir", &source, &out, false);
}

#[test]
fn signature_cleanup_keeps_objects_still_used_through_reference_chains() {
    // Sayfaların fontu bir başvuru NESNESİ üzerinden kullanılıyor (`/F1 40 0 R`,
    // `40 0 obj 7 0 R`); imzanın `/Reference /Data`sı aynı fonta doğrudan
    // gidiyor. İmzadan erişilen ama belgede hâlâ kullanılan font boşaltılmamalı.
    let mut d = vector_pdf(&[A4, A4]);
    let pages = d.get_pages();
    let font = d.add_object(font_dict());
    let via = d.add_object(Object::Reference(font));
    for n in [1, 2] {
        d.get_dictionary_mut(pages[&n])
            .unwrap()
            .set("Resources", dictionary! {"Font"=>dictionary!{"F1"=>via}});
    }
    let signature = d.add_object(dictionary! {"Type"=>"Sig","Filter"=>"Adobe.PPKLite",
    "SubFilter"=>"adbe.pkcs7.detached","ByteRange"=>vec![0.into(),100.into(),200.into(),300.into()],
    "Contents"=>Object::String(SIGNATURE_BLOB.to_vec(), lopdf::StringFormat::Hexadecimal),
    "Reference"=>vec![dictionary!{"Type"=>"SigRef","TransformMethod"=>"FieldMDP",
        "TransformParams"=>dictionary!{"Type"=>"TransformParams","Action"=>"All","V"=>"1.2"},
        "Data"=>font}.into()]});
    let widget = d.add_object(
        dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Sig",
        "T"=>Object::string_literal("Imza1"),"Rect"=>vec![0.into(),0.into(),0.into(),0.into()],
        "V"=>signature,"P"=>pages[&1]},
    );
    d.get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", vec![widget.into()]);
    let form = d.add_object(dictionary! {"Fields"=>vec![widget.into()],"SigFlags"=>3});
    d.catalog_mut().unwrap().set("AcroForm", form);
    let lab = Lab::new();
    let src = lab.write("zincirli-imzali.pdf", &mut d);
    assert!(scan_source_files(std::slice::from_ref(&src)).sources[0].is_signed);
    for (label, op) in [
        ("dondur", ToolOperation::Rotate { degrees: 90 }),
        ("sec", ToolOperation::Select { pages: vec![1, 2] }),
    ] {
        let (o, out) = run(
            &lab,
            std::slice::from_ref(&src),
            op,
            &format!("{label}.pdf"),
            true,
        );
        published(&o);
        let copy = reopen(&out);
        assert!(!pdf::detect_signature(&copy), "{label}: imza kaldı");
        for page in [1, 2] {
            let dict = pdf::resolved_page_dictionary(&copy, copy.get_pages()[&page]).unwrap();
            let resolve = |value: &Object| copy.dereference(value).unwrap().1.clone();
            let resources = resolve(dict.get(b"Resources").unwrap());
            let fonts = resolve(resources.as_dict().unwrap().get(b"Font").unwrap());
            let f1 = resolve(fonts.as_dict().unwrap().get(b"F1").unwrap());
            assert_eq!(
                f1.as_dict()
                    .ok()
                    .and_then(|f| f.get(b"BaseFont").ok())
                    .and_then(|b| b.as_name().ok()),
                Some(b"Helvetica".as_slice()),
                "{label}: sayfa {page} fontu boşaltıldı: {f1:?}"
            );
        }
    }
}

// =============================== MARKA İDEMPOTENTTİR — ZİNCİRLEME TÜRETME (2026-09-15)
//
// Kullanıcı kaydettiği kopyayı açıp başka bir araç uygular. Her araç marka
// payı ekliyordu: sırala → döndür → sil zincirinde 1. sayfa iki kez 34 pt
// büyüyor, üç logo ve altı marka formu birikiyordu. İşareti görünür alanda
// duran sayfaya ikinci pay/logo eklenmez; kırpma işareti görünür alanın dışında
// bıraktıysa sayfa yeniden işaretlenir.

/// Sayfanın içeriğinde marka formunu çizen `Do` sayısı.
fn brand_draws(doc: &Document, page: u32) -> usize {
    let content = doc.get_page_content(doc.get_pages()[&page]).unwrap();
    let text = String::from_utf8_lossy(&content);
    text.matches("/GolgeDosyaBrand").count()
}

fn brand_form_count(doc: &Document) -> usize {
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| {
            let data = if s.dict.has(b"Filter") {
                s.decompressed_content().unwrap_or_default()
            } else {
                s.content.clone()
            };
            data == b"q /BrandAlpha gs /Mark Do Q"
        })
        .count()
}

#[test]
fn branding_is_idempotent_across_chained_derivations() {
    let lab = Lab::new();
    let mut d = vector_pdf(&[A4, A4, A4]);
    let pages = d.get_pages();
    let scan = d.add_object(flate_rgb_stream(
        &scan_pixels(1200, 900),
        "DeviceRGB".into(),
    ));
    let content = d.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 24 Tf 72 720 Td (Sayfa 2) Tj ET q 480 0 0 360 60 200 cm /Im1 Do Q".to_vec(),
    ));
    let page2 = d.get_dictionary_mut(pages[&2]).unwrap();
    page2.set("Contents", content);
    page2.set(
        "Resources",
        dictionary! {"Font"=>dictionary!{"F1"=>font_dict()},"XObject"=>dictionary!{"Im1"=>scan}},
    );
    let src = lab.write("kaynak.pdf", &mut d);
    let branded_box = [0.0, -BRAND_GUTTER, A4[0], A4[1]];

    let check = |name: &str, doc: &Document| {
        assert_eq!(brand_form_count(doc), 1, "{name}: marka formu birikti");
        for page in 1..=doc.get_pages().len() as u32 {
            assert_eq!(
                brand_draws(doc, page),
                1,
                "{name}/s.{page}: tek işaret olmalı"
            );
            let (media, crop) = boxes(doc, page);
            for (value, expected) in media.iter().zip(branded_box) {
                assert!(
                    (value - expected).abs() < 0.01,
                    "{name}/s.{page}: MediaBox {media:?}"
                );
            }
            assert_eq!(
                crop.map(|c| c.map(|v| (v * 100.).round())),
                Some(media.map(|v| (v * 100.).round())),
                "{name}/s.{page}"
            );
        }
    };

    let (o, a) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Reorder {
            pages: vec![3, 2, 1],
        },
        "1-sirala.pdf",
        false,
    );
    published(&o);
    check("sirala", &reopen(&a));
    let (o, b) = run(
        &lab,
        std::slice::from_ref(&a),
        ToolOperation::RotatePages {
            rotations: vec![PageRotation {
                page: 1,
                degrees: 90,
            }],
        },
        "2-dondur.pdf",
        false,
    );
    published(&o);
    let rotated = reopen(&b);
    check("dondur", &rotated);
    assert_eq!(rotation(&rotated, 1), 90);
    let (o, c) = run(
        &lab,
        std::slice::from_ref(&b),
        ToolOperation::Delete { pages: vec![3] },
        "3-sil.pdf",
        false,
    );
    published(&o);
    let deleted = reopen(&c);
    check("sil", &deleted);
    assert_eq!(markers(&deleted), ["Sayfa 3", "Sayfa 2"]);
    let (o, e) = run(
        &lab,
        std::slice::from_ref(&c),
        ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        "4-sikistir.pdf",
        false,
    );
    assert!(matches!(o, ToolOutcome::Compressed { .. }), "{o:?}");
    check("sikistir", &reopen(&e));
    // Markalı iki belgenin birleşimi: sayfa başına yine tek işaret.
    let (o, m) = run(
        &lab,
        &[a.clone(), c.clone()],
        ToolOperation::Merge,
        "5-birlestir.pdf",
        false,
    );
    published(&o);
    let merged = reopen(&m);
    for page in 1..=merged.get_pages().len() as u32 {
        assert_eq!(brand_draws(&merged, page), 1, "birlestir/s.{page}");
        let (media, _) = boxes(&merged, page);
        assert!(
            (media[1] + BRAND_GUTTER).abs() < 0.01,
            "birlestir/s.{page}: {media:?}"
        );
    }
    // İşareti görünür alanın dışında bırakan kırpma: yeni işaret eklenir ve
    // GÖRÜNMEZ kalan eski işaret akışı sökülür. Eskiden gizli çizim dosyada
    // kalıyor, sayfa iki işaret çizimi taşıyordu; ürün kuralı artık "tam bir
    // işaret" (bkz. `pdf_branding_invariant.rs`). Kullanıcının kırpması geri
    // ALINMAZ: pay, kırpılmış kutunun altına eklenir.
    let (o, k) = run(
        &lab,
        std::slice::from_ref(&a),
        ToolOperation::Crop { margin_pt: 20.0 },
        "6-kirp.pdf",
        false,
    );
    published(&o);
    let cropped = reopen(&k);
    for page in 1..=3 {
        assert_eq!(
            brand_draws(&cropped, page),
            1,
            "kirp/s.{page}: gizlenen işaretin yerine tek yeni işaret"
        );
        let (_, crop) = boxes(&cropped, page);
        let crop = crop.unwrap();
        assert!(
            (crop[1] - (-BRAND_GUTTER + 20.0 - BRAND_GUTTER)).abs() < 0.01,
            "kirp/s.{page}: {crop:?}"
        );
    }
    assert_eq!(
        brand_form_count(&cropped),
        1,
        "kirp: form yeniden kullanılmalı"
    );
}

// ------------------------------------------ şüpheci turu 2: kalan sızıntılar ve anlam

fn raw_contains(path: &Path, needle: &str) -> bool {
    let raw = std::fs::read(path).unwrap();
    raw.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn document_parts_threads_and_structure_of_removed_pages_do_not_leak() {
    // Tür adı yazılmamış (/Type isteğe bağlı) yapılar da kesilir: PDF 2.0
    // belge parçaları (/DPart, /DPM meta verisi, /AF ilişkili dosyaları),
    // makale zinciri ve boncukları, yapı öğeleri.
    let mut d = vector_pdf(&[A4; 3]);
    let pages = d.get_pages();
    let rect = || vec![72.into(), 600.into(), 172.into(), 620.into()];
    // (1) Belge parçaları: her sayfa kendi kaydına ait.
    let (dpart_root, dpart_node) = (d.new_object_id(), d.new_object_id());
    let leaves: Vec<ObjectId> = (1..=3u32)
        .map(|n| {
            let file = d.add_object(Stream::new(
                dictionary! {"Type"=>"EmbeddedFile"},
                format!("GIZLI-KAYIT-DOSYASI-{n}").into_bytes(),
            ));
            d.add_object(dictionary! {"Type"=>"DPart","Parent"=>dpart_node,"Start"=>pages[&n],"End"=>pages[&n],
                "DPM"=>dictionary!{"Musteri"=>Object::string_literal(format!("GIZLI-MUSTERI-{n}"))},
                "AF"=>vec![dictionary!{"Type"=>"Filespec","F"=>Object::string_literal(format!("kayit-{n}.txt")),"EF"=>dictionary!{"F"=>file}}.into()]})
        })
        .collect();
    d.objects.insert(
        dpart_node,
        dictionary! {"Type"=>"DPart","Parent"=>dpart_root,
        "DParts"=>vec![Object::Array(leaves.iter().map(|l| Object::Reference(*l)).collect())]}
        .into(),
    );
    d.objects.insert(
        dpart_root,
        dictionary! {"Type"=>"DPartRoot","DPartRootNode"=>dpart_node}.into(),
    );
    for n in 1..=3u32 {
        d.get_dictionary_mut(pages[&n])
            .unwrap()
            .set("DPart", leaves[n as usize - 1]);
    }
    // (2) /Type'sız makale zinciri.
    let (thread, bead1, bead2) = (d.new_object_id(), d.new_object_id(), d.new_object_id());
    d.objects.insert(thread, dictionary! {"F"=>bead1,"I"=>dictionary!{"Title"=>Object::string_literal("GIZLI-MAKALE"),"Author"=>Object::string_literal("GIZLI-YAZAR")}}.into());
    d.objects.insert(
        bead1,
        dictionary! {"T"=>thread,"N"=>bead2,"V"=>bead2,"P"=>pages[&1],"R"=>rect()}.into(),
    );
    d.objects.insert(
        bead2,
        dictionary! {"T"=>thread,"N"=>bead1,"V"=>bead1,"P"=>pages[&2],"R"=>rect()}.into(),
    );
    d.get_dictionary_mut(pages[&1])
        .unwrap()
        .set("B", vec![Object::Reference(bead1)]);
    d.get_dictionary_mut(pages[&2])
        .unwrap()
        .set("B", vec![Object::Reference(bead2)]);
    // (3) /Type'sız yapı öğeleri; 1. sayfanın bağlantısı /SD ile birine gider.
    let (document, e1, e2) = (d.new_object_id(), d.new_object_id(), d.new_object_id());
    d.objects.insert(
        e1,
        dictionary! {"S"=>"Link","P"=>document,"Pg"=>pages[&1]}.into(),
    );
    d.objects.insert(e2, dictionary! {"S"=>"P","P"=>document,"Pg"=>pages[&2],
        "ActualText"=>Object::string_literal("GIZLI-P2-METIN"),"Alt"=>Object::string_literal("GIZLI-ALT")}.into());
    d.objects.insert(
        document,
        dictionary! {"S"=>"Document","K"=>vec![Object::Reference(e1), Object::Reference(e2)]}
            .into(),
    );
    let link = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>rect(),
        "A"=>dictionary!{"S"=>"GoTo","D"=>vec![Object::Reference(pages[&1]),"Fit".into()],"SD"=>vec![Object::Reference(e1),"Fit".into()]}});
    let goto_part = d.add_object(
        dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>rect(),
        "A"=>dictionary!{"S"=>"GoToDp","Dp"=>leaves[2]}},
    );
    d.get_dictionary_mut(pages[&1]).unwrap().set(
        "Annots",
        vec![Object::Reference(link), Object::Reference(goto_part)],
    );
    d.catalog_mut().unwrap().set("DPartRoot", dpart_root);

    let lab = Lab::new();
    let src = lab.write("parcalar.pdf", &mut d);
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Select { pages: vec![1] },
        "tek.pdf",
        false,
    );
    published(&o);
    reopen(&out);
    for secret in [
        "GIZLI-MUSTERI-2",
        "GIZLI-MUSTERI-3",
        "GIZLI-KAYIT-DOSYASI-2",
        "GIZLI-KAYIT-DOSYASI-3",
        "GIZLI-MAKALE",
        "GIZLI-YAZAR",
        "GIZLI-P2-METIN",
        "GIZLI-ALT",
    ] {
        assert!(
            !raw_contains(&out, secret),
            "çıkarılan sayfanın verisi sızdı: {secret}"
        );
    }
}

#[test]
fn retained_widgets_keep_their_field_and_removed_fields_do_not_leak() {
    // Alanın iki widget'ı 1. sayfada, biri 2. sayfada. Değer yalnız alan
    // düğümündedir: kalan widget alanını (/FT /T /V /DA) kaybetmemeli; kalmayan
    // widget ve widget'ı olmayan bir alanın değeri tek sayfalık çıktıya sızmamalı.
    let mut d = vector_pdf(&[A4; 3]);
    let pages = d.get_pages();
    let rect = |y: i64| vec![72.into(), y.into(), 272.into(), (y + 20).into()];
    let field = d.new_object_id();
    let appearance = |d: &mut Document, text: &str| {
        d.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),200.into(),20.into()]},
            format!("BT /Helv 10 Tf 2 5 Td ({text}) Tj ET").into_bytes(),
        ))
    };
    let ap1 = appearance(&mut d, "Ali Veli");
    let w1 = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","Rect"=>rect(600),"Parent"=>field,"AP"=>dictionary!{"N"=>ap1},"P"=>pages[&1]});
    let w2 = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","Rect"=>rect(560),"Parent"=>field,"P"=>pages[&1]});
    let ap3 = appearance(&mut d, "GIZLI-W3");
    let w3 = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","Rect"=>rect(600),"Parent"=>field,"AP"=>dictionary!{"N"=>ap3},"P"=>pages[&2]});
    d.objects.insert(field, dictionary! {"FT"=>"Tx","T"=>Object::string_literal("adsoyad"),"V"=>Object::string_literal("Ali Veli"),
        "DA"=>Object::string_literal("/Helv 10 Tf 0 g"),"Kids"=>vec![w1.into(), w2.into(), w3.into()]}.into());
    // Widget'ı olmayan alan (/FT ve /Kids yok), gönderme eyleminin /Fields'ında.
    let person = d.new_object_id();
    let tckn = d.add_object(dictionary! {"T"=>Object::string_literal("tckn"),"V"=>Object::string_literal("GIZLI-TCKN-99"),"Parent"=>person});
    d.objects.insert(person, dictionary! {"FT"=>"Tx","T"=>Object::string_literal("kisi"),"Kids"=>vec![Object::Reference(tckn)]}.into());
    let submit = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>rect(500),
        "A"=>dictionary!{"S"=>"SubmitForm","F"=>dictionary!{"FS"=>"URL","F"=>Object::string_literal("https://ornek.test/gonder")},"Fields"=>vec![Object::Reference(tckn)]}});
    d.get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", vec![w1.into(), w2.into(), submit.into()]);
    d.get_dictionary_mut(pages[&2])
        .unwrap()
        .set("Annots", vec![Object::Reference(w3)]);
    d.catalog_mut().unwrap().set(
        "AcroForm",
        dictionary! {"Fields"=>vec![Object::Reference(field), Object::Reference(person)]},
    );

    /// Sayfadaki widget'ların /Parent'ı üzerinden eriştiği alan sözlükleri.
    fn widget_fields(doc: &Document, page: u32) -> Vec<Dictionary> {
        annotations_on(doc, page)
            .into_iter()
            .filter(|a| a.get(b"Subtype").and_then(|s| s.as_name()).ok() == Some(b"Widget"))
            .map(|w| {
                let parent = w.get(b"Parent").expect("widget alanını kaybetti");
                doc.dereference(parent)
                    .unwrap()
                    .1
                    .as_dict()
                    .expect("alan null oldu")
                    .clone()
            })
            .collect()
    }
    fn assert_field_semantics(name: &str, field: &Dictionary) {
        assert_eq!(
            field.get(b"FT").unwrap().as_name().unwrap(),
            b"Tx",
            "{name}"
        );
        assert_eq!(
            field.get(b"T").unwrap().as_str().unwrap(),
            b"adsoyad",
            "{name}"
        );
        assert_eq!(
            field.get(b"V").unwrap().as_str().unwrap(),
            b"Ali Veli",
            "{name}"
        );
        assert!(field.has(b"DA"), "{name}");
    }

    let lab = Lab::new();
    let src = lab.write("form.pdf", &mut d);
    // Sırala ve Birleştir: hiçbir sayfa çıkmıyor, alan semantiği aynen kalmalı.
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Reorder {
            pages: vec![3, 2, 1],
        },
        "sirala.pdf",
        false,
    );
    published(&o);
    let reordered = reopen(&out);
    let fields = widget_fields(&reordered, 3);
    assert_eq!(fields.len(), 2);
    fields
        .iter()
        .for_each(|f| assert_field_semantics("sirala", f));
    let second = lab.write("form-2.pdf", &mut d.clone());
    let (o, out) = run(
        &lab,
        &[src.clone(), second],
        ToolOperation::Merge,
        "birlestir.pdf",
        false,
    );
    published(&o);
    let merged = reopen(&out);
    for page in [1, 4] {
        let fields = widget_fields(&merged, page);
        assert_eq!(fields.len(), 2, "birlestir/s.{page}");
        fields
            .iter()
            .for_each(|f| assert_field_semantics("birlestir", f));
    }
    // Seç 1: kalan widget'lar alanını korur; alanın /Kids'inde yalnız onlar var.
    let (o, out) = run(
        &lab,
        std::slice::from_ref(&src),
        ToolOperation::Select { pages: vec![1] },
        "tek.pdf",
        false,
    );
    published(&o);
    let single = reopen(&out);
    let fields = widget_fields(&single, 1);
    assert_eq!(fields.len(), 2);
    for field in &fields {
        assert_field_semantics("sec", field);
        let kids = field.get(b"Kids").unwrap().as_array().unwrap();
        assert_eq!(
            kids.len(),
            2,
            "alanın /Kids'inde çıkarılan widget kaldı: {kids:?}"
        );
        assert!(kids.iter().all(|k| matches!(k, Object::Reference(_))));
    }
    for secret in ["GIZLI-W3", "GIZLI-TCKN-99"] {
        assert!(
            !raw_contains(&out, secret),
            "çıkarılan sayfanın form verisi sızdı: {secret}"
        );
    }
    assert_no_dangling_or_dead_destinations("form", &single);
}

#[test]
fn inherited_resource_maps_are_checked_once_not_per_page() {
    // Kök Pages düğümünde DOĞRUDAN, 1.000 girdili bir /XObject haritası; 2.100
    // sayfa onu miras alır. Görünüm denetimi haritayı sayfa başına çoğaltırsa
    // 2 milyonluk bütçe aşılıyor, sağlam belge "nesne sınırı" ile reddediliyordu.
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let font = d.add_object(font_dict());
    let mut xobjects = Dictionary::new();
    for n in 0..1_000 {
        let image = d.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>1,"Height"=>1,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8},
            vec![(n % 251) as u8],
        ));
        xobjects.set(format!("Im{n}"), image);
    }
    let content = d.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 24 Tf 72 720 Td (Sayfa) Tj ET".to_vec(),
    ));
    let kids: Vec<Object> = (0..2_100)
        .map(|_| {
            Object::Reference(
                d.add_object(dictionary! {"Type"=>"Page","Parent"=>pages_id,"Contents"=>content}),
            )
        })
        .collect();
    d.objects.insert(pages_id, dictionary! {"Type"=>"Pages","Kids"=>kids.clone(),"Count"=>kids.len() as i64,
        "MediaBox"=>media(A4[0], A4[1]),"Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font},"XObject"=>xobjects}}.into());
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    d.trailer.set("Root", root);
    let mut bytes = Vec::new();
    d.save_to(&mut bytes).unwrap();
    let started = std::time::Instant::now();
    let loaded = load_pdf_tolerant(&bytes, "miras.pdf").expect("sağlam belge reddedildi");
    assert_eq!(loaded.document.get_pages().len(), 2_100);
    assert!(!loaded.is_repaired);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "yükleme denetimi sayfa × girdi ölçeğinde: {:?}",
        started.elapsed()
    );
}

#[test]
fn page_copy_scales_linearly_with_flat_name_arrays_and_off_layer_lists() {
    // Düz `/Names /Dests` dizisinde (Qt/wkhtmltopdf biçimi) her bağlantı için
    // doğrusal arama ve taban durumu OFF olan katman yapılandırmasında her grup
    // için `/ON` listesinde doğrusal arama sayfa kopyasını KARESEL büyütüyordu
    // (release: 40.000 sayfa 3,6 s, 160.000 grup 14,7 s). Süre değil ölçeklenme
    // denenir: boyut 8 katına çıkınca doğrusal kopya ≈8–10 kat, karesel ≈40–60
    // kat yavaşlar. Küçük ve büyük ölçüm iç içe üç kez alınır, en iyisi sayılır;
    // paralel testlerin yükü iki ölçüme de benzer düşer.
    fn book(pages: usize, groups: usize) -> Document {
        let mut d = vector_pdf(&vec![A4; pages]);
        let ids: Vec<ObjectId> = d.get_pages().into_values().collect();
        let mut names = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            names.push(Object::string_literal(format!("hedef-{i:06}")));
            names.push(vec![Object::Reference(*id), "Fit".into()].into());
        }
        for (i, id) in ids.iter().enumerate() {
            let link = d.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link",
            "Rect"=>vec![72.into(),600.into(),172.into(),620.into()],
            "Dest"=>Object::string_literal(format!("hedef-{:06}", (i + 1) % pages))});
            d.get_dictionary_mut(*id)
                .unwrap()
                .set("Annots", vec![Object::Reference(link)]);
        }
        let ocgs: Vec<Object> = (0..groups)
            .map(|g| {
                Object::Reference(d.add_object(
                    dictionary! {"Type"=>"OCG","Name"=>Object::string_literal(format!("K{g}"))},
                ))
            })
            .collect();
        let on: Vec<Object> = ocgs.iter().step_by(2).cloned().collect();
        let catalog = d.catalog_mut().unwrap();
        catalog.set("Names", dictionary! {"Dests"=>dictionary!{"Names"=>names}});
        if groups > 0 {
            catalog.set(
                "OCProperties",
                dictionary! {"OCGs"=>ocgs,"D"=>dictionary!{"BaseState"=>"OFF","ON"=>on}},
            );
        }
        let mut bytes = Vec::new();
        d.save_to(&mut bytes).unwrap();
        load_pdf_tolerant(&bytes, "olcek.pdf").unwrap().document
    }
    fn growth(small: &Document, large: &Document, pages: impl Fn(&Document) -> Vec<usize>) -> f64 {
        let (small_pages, large_pages) = (pages(small), pages(large));
        let time = |doc: &Document, pages: &[usize]| {
            let started = std::time::Instant::now();
            pdf::extract_pages(doc, pages).unwrap();
            started.elapsed().as_secs_f64()
        };
        let (mut best_small, mut best_large) = (f64::MAX, f64::MAX);
        for _ in 0..3 {
            best_small = best_small.min(time(small, &small_pages));
            best_large = best_large.min(time(large, &large_pages));
        }
        best_large / best_small
    }
    let all_reversed = |doc: &Document| (1..=doc.get_pages().len()).rev().collect::<Vec<_>>();
    let names = growth(&book(1_000, 0), &book(8_000, 0), all_reversed);
    let layers = growth(&book(2, 10_000), &book(2, 80_000), |_| vec![1]);
    println!("ölçeklenme (8× boyut): düz /Names {names:.1}×, BaseState OFF {layers:.1}×");
    assert!(
        names < 20.0,
        "düz /Names dizisiyle sayfa kopyası karesel büyüyor: 8× sayfa → {names:.1}× süre"
    );
    assert!(
        layers < 20.0,
        "BaseState OFF katman listesiyle kopya karesel büyüyor: 8× grup → {layers:.1}× süre"
    );
}
