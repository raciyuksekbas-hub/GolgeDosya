//! Düzenle — standalone ↔ birleşik davranış eşitliği.
//!
//! Bu dosya, taşınan komut yüzeyinin gerçekten çalıştığını ve DüzenEk'in
//! sözleşmelerini koruduğunu **ölçer**. Ekran görüntüsü yoktur, piksel yoktur:
//! işlemler birleşik komut fonksiyonları üzerinden çalıştırılır ve üretilen
//! PDF'ler yeniden açılarak sayfa sayısı, sayfa sırası, dönüş, ölçü, boyut,
//! kaynak SHA-256'sı ve marka kaynağı üzerinden denetlenir.
//!
//! Eşitliğin ikinci yarısı `scripts/check-command-parity.py` tarafından
//! kanıtlanır: bağımsız uygulamanın 21 komut gövdesi ile buradakiler anlamca
//! aynıdır. Bağımsız uygulamanın komutları `pub` değildir ve baseline
//! dondurulmuştur, bu yüzden iki komut katmanı yan yana ÇAĞRILAMAZ. Aynı kaynak,
//! aynı motor (`ekler-core`, tek kopya) ve burada ölçülen davranış birlikte
//! eşitliği verir. Bu sınır bilerek böyle kaydedilmiştir.
//!
//! Fikstürler sentetiktir. Gerçek müvekkil belgesi kullanılmaz.
#![cfg(feature = "feature_duzenek")]

use belge_shell_lib::modules::duzenek::*;
use ekler_core::toolbox::{PageRotation, ToolOperation, ToolOutcome};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Bağımsız uygulamanın `crates/ekler-core/tests/toolbox_tests.rs` dosyasındaki
/// `make_pdf` ile aynı fikstür: her sayfada "Sayfa N" yazar, böylece çıktıdaki
/// sayfa SIRASI metinden okunabilir.
fn make_pdf(pages: usize) -> tempfile::NamedTempFile {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut page_ids = Vec::new();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    for i in 1..=pages {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![50.into(), 750.into()]),
                Operation::new("Tj", vec![Object::string_literal(format!("Sayfa {i}"))]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.28.into(), 841.89.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        page_ids.push(page_id);
    }
    doc.set_object(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as i32,
        },
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let tmp = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
    doc.save_to(&mut std::fs::File::create(tmp.path()).unwrap())
        .unwrap();
    tmp
}

/// Fikstürün sayfa ölçüsü (A4, punto).
const PAGE_W: f64 = 595.28;
const PAGE_H: f64 = 841.89;
/// Marka bandının içerik dışına eklediği pay: `margin_pt * 2 + font_size + 10`
/// (stamp.rs:109). Varsayılan yapılandırmada 34 punto.
const GUTTER: f64 = 34.0;

fn sha256(path: &Path) -> String {
    let mut h = Sha256::new();
    h.update(std::fs::read(path).unwrap());
    format!("{:x}", h.finalize())
}

/// Yeniden açılan çıktının ölçülebilir her özelliği.
struct Measured {
    pages: usize,
    /// Her sayfanın metninden okunan kaynak sayfa numaraları — SIRA budur.
    order: Vec<usize>,
    rotations: Vec<i64>,
    boxes: Vec<Vec<f64>>,
    bytes: u64,
}

fn measure(path: &Path) -> Measured {
    let doc = Document::load(path).expect("çıktı yeniden açılamadı");
    let page_map = doc.get_pages();
    let mut order = Vec::new();
    let mut rotations = Vec::new();
    let mut boxes = Vec::new();
    for (number, id) in &page_map {
        let text = doc.extract_text(&[*number]).unwrap_or_default();
        order.push(
            text.split_whitespace()
                .last()
                .and_then(|t| t.trim().parse::<usize>().ok())
                .unwrap_or(0),
        );
        let dict = ekler_core::pdf::resolved_page_dictionary(&doc, *id).unwrap();
        rotations.push(
            dict.get(b"Rotate")
                .ok()
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(0),
        );
        boxes.push(
            dict.get(b"MediaBox")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_float().ok())
                        .map(f64::from)
                        .collect()
                })
                .unwrap_or_default(),
        );
    }
    Measured {
        pages: page_map.len(),
        order,
        rotations,
        boxes,
        bytes: std::fs::metadata(path).unwrap().len(),
    }
}

/// Bir aracı birleşik komut üzerinden çalıştırır.
fn run(paths: &[&Path], op: ToolOperation, out: &Path, approved: bool) -> ToolOutcome {
    tauri::async_runtime::block_on(duzenek_run_pdf_tool(
        paths.iter().map(|p| p.display().to_string()).collect(),
        op,
        out.display().to_string(),
        approved,
    ))
    .expect("araç çalışmadı")
}

fn out_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

// ---------------------------------------------------------------- seçim / sıra

#[test]
fn page_selection_keeps_only_the_named_pages_in_the_named_order() {
    let src = make_pdf(5);
    let before = sha256(src.path());
    let d = out_dir();
    let out = d.path().join("secim.pdf");
    run(
        &[src.path()],
        ToolOperation::Select {
            pages: vec![4, 2, 5],
        },
        &out,
        false,
    );
    let m = measure(&out);
    assert_eq!(m.pages, 3);
    assert_eq!(m.order, vec![4, 2, 5], "seçim sırası korunmalı");
    assert_eq!(sha256(src.path()), before, "kaynak değişti");
    assert!(m.bytes > 0);
}

#[test]
fn reorder_must_contain_every_page_exactly_once() {
    let src = make_pdf(4);
    let d = out_dir();
    let out = d.path().join("sira.pdf");
    run(
        &[src.path()],
        ToolOperation::Reorder {
            pages: vec![3, 1, 4, 2],
        },
        &out,
        false,
    );
    assert_eq!(measure(&out).order, vec![3, 1, 4, 2]);

    // Eksik sıralama reddedilir; bu bir sözleşmedir, kolaylık değil.
    let out2 = d.path().join("eksik.pdf");
    let refused = tauri::async_runtime::block_on(duzenek_run_pdf_tool(
        vec![src.path().display().to_string()],
        ToolOperation::Reorder { pages: vec![1, 2] },
        out2.display().to_string(),
        false,
    ));
    assert!(refused.is_err(), "eksik sıralama kabul edildi");
    assert!(!out2.exists(), "reddedilen işlem dosya bıraktı");
}

#[test]
fn delete_removes_exactly_the_named_pages() {
    let src = make_pdf(5);
    let d = out_dir();
    let out = d.path().join("sil.pdf");
    run(
        &[src.path()],
        ToolOperation::Delete { pages: vec![2, 4] },
        &out,
        false,
    );
    let m = measure(&out);
    assert_eq!(m.pages, 3);
    assert_eq!(m.order, vec![1, 3, 5]);
}

// ---------------------------------------------------------------- döndürme

#[test]
fn rotate_left_and_right_land_on_the_expected_angles() {
    for (delta, expected) in [(-90, 270), (90, 90), (180, 180)] {
        let src = make_pdf(2);
        let d = out_dir();
        let out = d.path().join(format!("don{delta}.pdf"));
        run(
            &[src.path()],
            ToolOperation::RotatePages {
                rotations: vec![PageRotation {
                    page: 1,
                    degrees: delta,
                }],
            },
            &out,
            false,
        );
        let m = measure(&out);
        assert_eq!(m.rotations[0], expected, "{delta}° yanlış açıya indi");
        assert_eq!(m.rotations[1], 0, "işaretsiz sayfa döndürülmüş");

        // Döndürme /Rotate ile yapılır: sayfanın kendi ölçüsü değişmez. Ama
        // marka bandı içeriğin DIŞINA bir pay ekler ve bu payın hangi kenara
        // düştüğü sayfanın dönüşüne göre seçilir (stamp.rs:114-119) — böylece
        // bant, sayfa döndürülse de görsel olarak altta kalır. İki sayfanın
        // kutuları bu yüzden aynı OLMAZ; aynı olan, payın büyüklüğü ve özgün
        // sayfa alanının hiç kırpılmamasıdır.
        for (page, b) in m.boxes.iter().enumerate() {
            assert_eq!(b.len(), 4, "sayfa {page}: MediaBox okunamadı");
            let grown_x = (b[2] - b[0]) - PAGE_W;
            let grown_y = (b[3] - b[1]) - PAGE_H;
            assert!(
                b[0] <= 0.0 && b[1] <= 0.0 && b[2] >= PAGE_W && b[3] >= PAGE_H,
                "sayfa {page}: özgün sayfa alanı kırpıldı: {b:?}"
            );
            // Tam bir kenar büyür, diğeri hiç.
            let (a, c) = (grown_x.abs(), grown_y.abs());
            assert!(
                (a < 0.01 && c > 0.01) || (c < 0.01 && a > 0.01),
                "sayfa {page}: pay iki eksende birden büyümüş: {b:?}"
            );
            assert!(
                (a.max(c) - GUTTER).abs() < 0.01,
                "sayfa {page}: pay {} bekleniyordu, {} bulundu",
                GUTTER,
                a.max(c)
            );
        }
        // Döndürülen sayfada pay yatay eksene, döndürülmeyende dikey eksene düşer.
        let rotated_grew_horizontally = ((m.boxes[0][2] - m.boxes[0][0]) - PAGE_W).abs() > 0.01;
        let upright_grew_vertically = ((m.boxes[1][3] - m.boxes[1][1]) - PAGE_H).abs() > 0.01;
        assert!(
            upright_grew_vertically,
            "dönmemiş sayfada pay dikeyde beklenirdi: {:?}",
            m.boxes[1]
        );
        assert_eq!(
            rotated_grew_horizontally,
            expected == 90 || expected == 270,
            "{expected}° sayfada pay yanlış kenara düştü: {:?}",
            m.boxes[0]
        );
    }
}

#[test]
fn rotation_accumulates_across_pages_like_the_preview() {
    // Önizleme durumu (`rotatePages`) 90'lık adımları toplar; çıktı da toplamalı.
    let src = make_pdf(3);
    let d = out_dir();
    let out = d.path().join("coklu.pdf");
    run(
        &[src.path()],
        ToolOperation::RotatePages {
            rotations: vec![
                PageRotation {
                    page: 1,
                    degrees: 90,
                },
                PageRotation {
                    page: 2,
                    degrees: 270,
                },
                PageRotation {
                    page: 3,
                    degrees: 180,
                },
            ],
        },
        &out,
        false,
    );
    assert_eq!(measure(&out).rotations, vec![90, 270, 180]);

    // İkinci tur, ilkinin üstüne eklenir: 90 + 90 = 180.
    let out2 = d.path().join("coklu2.pdf");
    run(
        &[&out],
        ToolOperation::RotatePages {
            rotations: vec![PageRotation {
                page: 1,
                degrees: 90,
            }],
        },
        &out2,
        false,
    );
    assert_eq!(measure(&out2).rotations[0], 180, "dönüş birikmedi");
}

// ---------------------------------------------------------------- birleştirme

#[test]
fn merge_concatenates_in_the_given_order() {
    let a = make_pdf(2);
    let b = make_pdf(3);
    let d = out_dir();
    let out = d.path().join("birlesik.pdf");
    run(&[b.path(), a.path()], ToolOperation::Merge, &out, false);
    let m = measure(&out);
    assert_eq!(m.pages, 5);
    assert_eq!(m.order, vec![1, 2, 3, 1, 2], "verilen sıra korunmadı");
}

// ---------------------------------------------------------------- sıkıştırma

#[test]
fn compression_reports_no_benefit_and_writes_nothing_when_it_cannot_help() {
    // Sentetik metin PDF'i zaten küçüktür: kazanç yok. Sözleşme, bu durumda
    // BAŞARISIZLIK değil `NoBenefit` bildirmek ve dosya bırakmamaktır.
    let src = make_pdf(3);
    let before = sha256(src.path());
    let d = out_dir();
    let out = d.path().join("sikistir.pdf");
    let outcome = run(
        &[src.path()],
        ToolOperation::Compress {
            level: ekler_core::OptimizationLevel::BalancedCompression,
        },
        &out,
        false,
    );
    match outcome {
        ToolOutcome::NoBenefit {
            source_bytes,
            candidate_bytes,
            ..
        } => {
            assert!(source_bytes > 0);
            assert!(candidate_bytes > 0);
            assert!(!out.exists(), "NoBenefit çıktı bıraktı");
        }
        ToolOutcome::Compressed { output_bytes, .. } => {
            assert!(out.exists());
            assert_eq!(measure(&out).bytes, output_bytes);
        }
        other => panic!("beklenmeyen sonuç: {other:?}"),
    }
    assert_eq!(sha256(src.path()), before, "sıkıştırma kaynağı değiştirdi");
}

/// Sıkıştırılabilir fikstür: tek sayfaya gömülü, gürültülü gri tonlamalı bir
/// tarama. Bağımsız uygulamanın `compression_regression_tests.rs` dosyasındaki
/// "A-scan-300dpi" ile aynı fikir, daha küçük ölçekte.
fn make_scan_pdf() -> tempfile::NamedTempFile {
    let (w, h) = (1200u32, 1600u32);
    let mut state = 7u32;
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = (state % 80) as u8;
            // Metin satırlarını taklit eden koyu şeritler: düz renk değil, gerçek
            // bir taramaya benzer bir dağılım.
            let v = if y % 56 < 4 && x > 60 && x < 920 {
                25 + noise / 8
            } else {
                170u8.saturating_add(noise)
            };
            rgb.extend_from_slice(&[v, v, v]);
        }
    }
    let mut image = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => w as i64, "Height" => h as i64,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        },
        rgb,
    );
    image.compress().unwrap();

    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let img = doc.add_object(image);
    let contents = doc.add_object(Stream::new(
        Dictionary::new(),
        b"q 595 0 0 842 0 0 cm /Scan Do Q".to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Resources" => dictionary! { "XObject" => dictionary! { "Scan" => img } },
        "Contents" => contents,
    });
    doc.objects.insert(
        pages,
        dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }.into(),
    );
    let root = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", root);
    let tmp = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
    doc.save_to(&mut std::fs::File::create(tmp.path()).unwrap())
        .unwrap();
    tmp
}

/// Sözleşmenin üçüncü durumu: gerçekten kazanç varsa `Compressed`.
///
/// Metin fikstürü bunu asla tetikleyemez (`images_found: 0`), bu yüzden ayrı bir
/// görsel fikstür gerekiyor. Üç durumun üçü de böylece birleşik komut yolunda
/// GÖZLENMİŞ olur; hiçbiri varsayılmaz.
#[test]
fn compression_reports_compressed_and_actually_shrinks_the_copy() {
    let src = make_scan_pdf();
    let before = sha256(src.path());
    let d = out_dir();
    let out = d.path().join("kucultuldu.pdf");
    let outcome = run(
        &[src.path()],
        ToolOperation::Compress {
            level: ekler_core::OptimizationLevel::BalancedCompression,
        },
        &out,
        false,
    );
    let ToolOutcome::Compressed {
        source_bytes,
        body_bytes,
        output_bytes,
        images_found,
        images_recompressed,
    } = outcome
    else {
        panic!("görsel taşıyan belgede `Compressed` bekleniyordu: {outcome:?}");
    };
    println!(
        "sıkıştırma: kaynak {source_bytes} → gövde {body_bytes} → marka dahil \
         {output_bytes} bayt · {images_recompressed}/{images_found} görsel"
    );
    assert!(images_found >= 1 && images_recompressed >= 1);
    // Bildirilen küçülme gerçek olmalı: gövde kaynaktan küçük ve dosya diskte.
    assert!(
        body_bytes < source_bytes,
        "başarı bildirildi ama küçülme yok"
    );
    assert!(out.is_file(), "`Compressed` dosya bırakmadı");
    let m = measure(&out);
    assert_eq!(
        m.bytes, output_bytes,
        "bildirilen boyut diskteki boyut değil"
    );
    assert_eq!(m.pages, 1, "sıkıştırma sayfa kaybetti");
    assert_eq!(sha256(src.path()), before, "sıkıştırma kaynağı değiştirdi");
}

#[test]
fn a_failing_compression_is_reported_as_failed_not_as_success() {
    let d = out_dir();
    let bogus = d.path().join("bu-pdf-degil.pdf");
    std::fs::write(&bogus, b"bu bir PDF degil").unwrap();
    let outcome = run(
        &[&bogus],
        ToolOperation::Compress {
            level: ekler_core::OptimizationLevel::BalancedCompression,
        },
        &d.path().join("cikti.pdf"),
        false,
    );
    assert!(
        matches!(outcome, ToolOutcome::Failed { .. }),
        "bozuk girdi başarı gibi raporlandı: {outcome:?}"
    );
}

// ---------------------------------------------------------------- rasterleştirme

#[test]
fn pdf_to_png_and_jpeg_produce_one_readable_file_per_page() {
    for (format, magic) in [
        ("png", &[0x89u8, b'P', b'N', b'G'][..]),
        ("jpg", &[0xFF, 0xD8, 0xFF][..]),
    ] {
        let src = make_pdf(3);
        let before = sha256(src.path());
        let d = out_dir();
        let produced = tauri::async_runtime::block_on(duzenek_pdf_to_images(
            src.path().display().to_string(),
            d.path().display().to_string(),
            format.into(),
            150,
            false,
        ))
        .unwrap_or_else(|e| panic!("{format} üretilemedi: {e}"));

        let mut files: Vec<PathBuf> = std::fs::read_dir(&produced)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        assert_eq!(files.len(), 3, "{format}: sayfa başına bir dosya beklenir");
        for f in &files {
            let bytes = std::fs::read(f).unwrap();
            assert!(bytes.len() > 100, "{format}: boş görsel");
            assert!(bytes.starts_with(magic), "{format}: imza tutmuyor: {f:?}");
        }
        assert_eq!(sha256(src.path()), before, "{format}: kaynak değişti");
    }
}

// ---------------------------------------------------------------- marka

/// Marka sözleşmesi: her sayfada aynı Form XObject, düşük alfa, gömülü görsel yok.
#[test]
fn branding_uses_one_shared_form_across_every_page() {
    for pages in [1usize, 10, 100] {
        let src = make_pdf(pages);
        let d = out_dir();
        let out = d.path().join(format!("marka{pages}.pdf"));
        run(
            &[src.path()],
            ToolOperation::Select {
                pages: (1..=pages).collect(),
            },
            &out,
            false,
        );
        let doc = Document::load(&out).unwrap();
        let mut refs = HashSet::new();
        for id in doc.get_pages().values() {
            let resolved = ekler_core::pdf::resolved_page_dictionary(&doc, *id).unwrap();
            let xobjects = resolved
                .get(b"Resources")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"XObject")
                .unwrap()
                .as_dict()
                .unwrap();
            let (_, value) = xobjects
                .iter()
                .find(|(k, _)| k.starts_with(b"DuzenEkBrand"))
                .expect("sayfada marka kaynağı yok");
            refs.insert(value.as_reference().unwrap());
        }
        assert_eq!(refs.len(), 1, "{pages} sayfa: marka kaynağı paylaşılmıyor");

        let form = doc
            .get_object(*refs.iter().next().unwrap())
            .unwrap()
            .as_stream()
            .unwrap();
        assert_eq!(
            form.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Form"
        );
        let alpha = form
            .dict
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"ExtGState")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"BrandAlpha")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"ca")
            .unwrap()
            .as_float()
            .unwrap();
        assert!((alpha - 0.24).abs() < 0.001, "{pages} sayfa: alfa {alpha}");

        // Marka vektördür: çıktı gömülü görsel taşımaz.
        let images = doc
            .objects
            .values()
            .filter(|v| {
                v.as_stream()
                    .ok()
                    .and_then(|s| s.dict.get(b"Subtype").ok())
                    .and_then(|v| v.as_name().ok())
                    == Some(b"Image")
            })
            .count();
        assert_eq!(images, 0, "{pages} sayfa: markadan görsel sızdı");
    }
}

/// Paylaşılan Form XObject'in ölçülebilir sonucu: sayfa başına maliyet sayfa
/// sayısıyla ARTMAZ.
///
/// Bağımsız uygulamanın `watermark_size_growth_is_bounded` testi mutlak bir
/// tavan kullanır (`kaynak + 4096 + n*100`), ama orada ölçülen şey tek bir
/// belge üzerinde `apply_branding` çağrısıdır. Burada ölçülen, komut yolunun
/// tamamıdır: sayfa çıkarma, birleştirme, marka ve yayın. O yol nesne akışını
/// yeniden yazdığı için mutlak tavan anlamını yitirir; marka sözleşmesinin
/// gerçek imzası olan "sayfa başına sabit maliyet" ise aynen ölçülebilir.
/// Marka sayfa başına yeniden gömülseydi bu eğri yukarı kırılırdı.
#[test]
fn branding_cost_per_page_does_not_grow_with_page_count() {
    let mut measured = Vec::new();
    for pages in [1usize, 10, 100] {
        let src = make_pdf(pages);
        let source_bytes = std::fs::metadata(src.path()).unwrap().len();
        let d = out_dir();
        let out = d.path().join(format!("buyume{pages}.pdf"));
        run(
            &[src.path()],
            ToolOperation::Select {
                pages: (1..=pages).collect(),
            },
            &out,
            false,
        );
        let m = measure(&out);
        assert_eq!(m.pages, pages);
        measured.push((pages, source_bytes, m.bytes));
    }
    for (pages, before, after) in &measured {
        println!(
            "marka {pages} sayfa: {before} → {after} bayt ({:.1} bayt/sayfa)",
            *after as f64 / *pages as f64
        );
    }
    // Sabit marka payı bir kez ödeniyorsa toplam boyut, sayfa sayısında
    // DOĞRUSAL bir terim artı sabit bir terimdir; yani ek sayfa başına maliyet
    // sabit kalır. Marka sayfa başına yeniden gömülseydi bu marjinal maliyet
    // 1→10 ile 10→100 arasında belirgin biçimde artardı.
    let growth: Vec<f64> = measured
        .iter()
        .map(|(_, before, after)| (*after as f64) - (*before as f64))
        .collect();
    let marginal_10 = (growth[1] - growth[0]) / 9.0;
    let marginal_100 = (growth[2] - growth[1]) / 90.0;
    println!(
        "marjinal büyüme: 1→10 {marginal_10:.1} bayt/sayfa, 10→100 {marginal_100:.1} bayt/sayfa"
    );
    assert!(
        marginal_100 <= marginal_10 * 1.25,
        "ek sayfa başına maliyet artıyor — marka paylaşılmıyor olabilir: \
         1→10 {marginal_10:.1}, 10→100 {marginal_100:.1}"
    );
    // Ek sayfa maliyeti, sayfanın kendi içeriğinin maliyetinden küçük kalmalı:
    // marka ek sayfaya kayda değer bir yük bindirmiyor.
    let source_per_page = measured[2].1 as f64 / measured[2].0 as f64;
    assert!(
        marginal_100 < source_per_page,
        "marka ek sayfa başına içerikten fazla yer kaplıyor: \
         {marginal_100:.1} ≥ {source_per_page:.1}"
    );
}

// ------------------------------------------------- güvenli yayın / no-clobber

#[test]
fn publication_never_overwrites_an_existing_file() {
    let src = make_pdf(2);
    let d = out_dir();
    let out = d.path().join("var.pdf");
    std::fs::write(&out, b"onceki icerik").unwrap();
    let refused = tauri::async_runtime::block_on(duzenek_run_pdf_tool(
        vec![src.path().display().to_string()],
        ToolOperation::Select { pages: vec![1] },
        out.display().to_string(),
        false,
    ));
    assert!(refused.is_err(), "var olan dosyanın üstüne yazıldı");
    assert_eq!(
        std::fs::read(&out).unwrap(),
        b"onceki icerik",
        "var olan dosya bozuldu"
    );
}

#[test]
fn publication_refuses_to_write_over_its_own_source() {
    let src = make_pdf(2);
    let before = sha256(src.path());
    let refused = tauri::async_runtime::block_on(duzenek_run_pdf_tool(
        vec![src.path().display().to_string()],
        ToolOperation::Select { pages: vec![1] },
        src.path().display().to_string(),
        false,
    ));
    assert!(refused.is_err(), "kaynağın üstüne yazıldı");
    assert_eq!(sha256(src.path()), before);
}

// ---------------------------------------------------------------- tarama

#[test]
fn scanning_reports_page_count_size_and_hash_without_touching_the_file() {
    let src = make_pdf(7);
    let before = sha256(src.path());
    let result = tauri::async_runtime::block_on(duzenek_scan_source_files(vec![src
        .path()
        .display()
        .to_string()]))
    .unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.sources.len(), 1);
    let s = &result.sources[0];
    assert_eq!(s.page_count, 7);
    assert_eq!(s.size_bytes, std::fs::metadata(src.path()).unwrap().len());
    assert_eq!(s.sha256_before, before);
    assert_eq!(sha256(src.path()), before, "tarama kaynağı değiştirdi");
}

#[test]
fn a_signed_source_needs_approval_before_a_derived_copy_is_made() {
    // İmzalı kaynak `approved` olmadan reddedilir ve kaynak baytları korunur.
    // Bu, imzalı belgeye dokunmama sözleşmesinin komut yüzeyindeki karşılığıdır.
    let src = make_pdf(2);
    let mut doc = Document::load(src.path()).unwrap();
    let sig = doc.add_object(dictionary! {
        "Type" => "Sig", "Filter" => "Adobe.PPKLite", "SubFilter" => "adbe.pkcs7.detached",
    });
    let field = doc.add_object(dictionary! {
        "FT" => "Sig", "T" => Object::string_literal("İmza"), "V" => Object::Reference(sig),
    });
    let catalog_id = doc.catalog().unwrap().get(b"Pages").unwrap().as_reference();
    let _ = catalog_id;
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.set(
            "AcroForm",
            dictionary! { "Fields" => vec![Object::Reference(field)], "SigFlags" => 3 },
        );
    }
    let signed = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
    doc.save_to(&mut std::fs::File::create(signed.path()).unwrap())
        .unwrap();

    let scanned = tauri::async_runtime::block_on(duzenek_scan_source_files(vec![signed
        .path()
        .display()
        .to_string()]))
    .unwrap();
    assert!(
        scanned.sources[0].is_signed,
        "imza işareti tespit edilemedi — fikstür geçersiz"
    );

    let before = sha256(signed.path());
    let d = out_dir();
    let refused = tauri::async_runtime::block_on(duzenek_run_pdf_tool(
        vec![signed.path().display().to_string()],
        ToolOperation::Select { pages: vec![1] },
        d.path().join("turetilmis.pdf").display().to_string(),
        false,
    ));
    assert!(refused.is_err(), "imzalı kaynak onaysız işlendi");
    assert_eq!(sha256(signed.path()), before, "imzalı kaynak DEĞİŞTİ");

    // Onay verildiğinde türetilmiş kopya üretilir, imzalı özgün yine korunur.
    let out = d.path().join("onayli.pdf");
    run(
        &[signed.path()],
        ToolOperation::Select { pages: vec![1] },
        &out,
        true,
    );
    assert_eq!(measure(&out).pages, 1);
    assert_eq!(
        sha256(signed.path()),
        before,
        "onaylı türetme imzalı özgünü değiştirdi"
    );
}
