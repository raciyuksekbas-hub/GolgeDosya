//! PDF hız düzeltmeleri sonucu değiştirmemeli.
//!
//! Saha: "PDF düzenleme ve sıkıştırma işlemlerini çok geç yapıyor." Önizleme
//! artık belgeyi bir kez açar; sıkıştırma istenen ön ayardan başlar ve görselleri
//! paralel işler; özet macOS'ta donanımla alınır. Bu dosya bunların karşılığında
//! hiçbir şeyin gevşemediğini denetler. Bütün belgeler sentetiktir.

use ekler_core::toolbox::{run_tool_with_outcome, ToolOperation, ToolOutcome};
use ekler_core::{calculate_sha256, OptimizationLevel};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use sha2::{Digest, Sha256};
use std::path::Path;

fn pdf_with_image(image: Stream, pages: usize, side_pt: i64) -> Document {
    let mut doc = Document::with_version("1.7");
    let parent = doc.new_object_id();
    let image = doc.add_object(image);
    let mut kids: Vec<Object> = Vec::new();
    for _ in 0..pages {
        let content = doc.add_object(Stream::new(
            dictionary! {},
            format!("q {side_pt} 0 0 {side_pt} 0 0 cm /Im0 Do Q").into_bytes(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => parent,
            "MediaBox" => vec![0.into(), 0.into(), side_pt.into(), side_pt.into()],
            "Contents" => content,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image } },
        });
        kids.push(page.into());
    }
    doc.objects.insert(
        parent,
        dictionary! { "Type" => "Pages", "Count" => pages as i64, "Kids" => kids }.into(),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => parent });
    doc.trailer.set("Root", catalog);
    doc
}

/// Filtresiz RGB görsel akışı.
fn raw_rgb(width: i64, height: i64, samples: Vec<u8>) -> Stream {
    Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => width, "Height" => height,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        },
        samples,
    )
    .with_compression(false)
}

/// Deterministik gürültü.
fn noise(len: usize) -> Vec<u8> {
    let mut x = 0x2545_F491u32;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

/// Yumuşak geçişli görsel: her ön ayar kalite kapısından geçer.
fn soft_rgb(width: usize, height: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            out.extend([(x * 255 / width) as u8, (y * 255 / height) as u8, 128]);
        }
    }
    out
}

fn only_image(doc: &Document) -> (ObjectId, &Stream) {
    let mut images = doc.objects.iter().filter_map(|(id, o)| {
        let s = o.as_stream().ok()?;
        (s.dict.get(b"Subtype").ok()?.as_name().ok()? == b"Image").then_some((*id, s))
    });
    let first = images.next().expect("görsel yok");
    assert!(images.next().is_none(), "birden çok görsel");
    first
}

fn compress(src: &Path, out: &Path, level: OptimizationLevel) -> ToolOutcome {
    run_tool_with_outcome(
        &[src.to_path_buf()],
        &ToolOperation::Compress { level },
        out,
        false,
    )
    .unwrap()
}

#[test]
fn the_file_digest_is_standard_sha256() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x");
    for (content, expected) in [
        (
            b"".to_vec(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc".to_vec(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ] {
        std::fs::write(&path, content).unwrap();
        assert_eq!(calculate_sha256(&path).unwrap(), expected);
    }
    // Okuma tamponundan büyük dosya, parça sınırları dâhil.
    let big = noise(1024 * 1024 + 17);
    std::fs::write(&path, &big).unwrap();
    assert_eq!(
        calculate_sha256(&path).unwrap(),
        format!("{:x}", Sha256::digest(&big))
    );
}

/// İstenen ön ayarın kendisi uygulanır; daha hafif bir aday aranmaz.
#[test]
fn compression_applies_the_requested_preset() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("kaynak.pdf");
    // Yalnız uzun kenar sınırların üstünde: dar şerit, test hızlı kalsın.
    pdf_with_image(raw_rgb(2500, 64, soft_rgb(2500, 64)), 1, 600)
        .save(&src)
        .unwrap();
    for (level, side) in [
        (OptimizationLevel::GentleCompression, 2400),
        (OptimizationLevel::BalancedCompression, 2000),
        (OptimizationLevel::AggressiveCompression, 1600),
    ] {
        let out = dir.path().join(format!("{side}.pdf"));
        assert!(
            matches!(
                compress(&src, &out, level),
                ToolOutcome::Compressed {
                    images_recompressed: 1,
                    ..
                }
            ),
            "{level:?}"
        );
        let doc = Document::load(&out).unwrap();
        let (_, image) = only_image(&doc);
        assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), side);
        assert_eq!(
            image.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"DCTDecode"
        );
    }
}

/// Kalite kapısı gevşemedi. Gürültülü görselde JPEG'in ortalama piksel hatası
/// kapının üstünde kalır: "Güçlü" (sınır 20, q65) geçemez ve bir hafifine düşer;
/// "Hafif" (sınır 12, q80) geçemez ve görsele hiç dokunulmaz.
#[test]
fn a_preset_that_fails_the_quality_gate_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("gurultu.pdf");
    let (w, h) = (2500usize, 64usize);
    pdf_with_image(raw_rgb(w as i64, h as i64, noise(w * h * 3)), 1, 600)
        .save(&src)
        .unwrap();

    let strong = dir.path().join("guclu.pdf");
    assert!(matches!(
        compress(&src, &strong, OptimizationLevel::AggressiveCompression),
        ToolOutcome::Compressed {
            images_recompressed: 1,
            ..
        }
    ));
    let doc = Document::load(&strong).unwrap();
    let (_, image) = only_image(&doc);
    assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 2000);

    let gentle = dir.path().join("hafif.pdf");
    match compress(&src, &gentle, OptimizationLevel::GentleCompression) {
        ToolOutcome::NoBenefit {
            images_found: 1,
            images_recompressed: 0,
            ..
        } => assert!(!gentle.exists()),
        other => panic!("temizlik adayı bekleniyordu: {other:?}"),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod preview {
    use super::*;
    use ekler_core::raster::preview_page;

    fn write_pdf(path: &Path, pages: usize, side_pt: i64) {
        pdf_with_image(raw_rgb(4, 4, vec![200; 48]), pages, side_pt)
            .save(path)
            .unwrap();
    }

    fn width_of(png: &[u8]) -> u32 {
        image::load_from_memory(png).unwrap().width()
    }

    #[test]
    fn a_rewritten_file_is_previewed_as_it_is_now() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.pdf");
        write_pdf(&path, 1, 100);
        assert_eq!(width_of(&preview_page(&path, 1, 72, 0).unwrap()), 100);
        assert!(preview_page(&path, 2, 72, 0).is_err());
        write_pdf(&path, 2, 200);
        assert_eq!(width_of(&preview_page(&path, 2, 72, 0).unwrap()), 200);
        std::fs::remove_file(&path).unwrap();
        assert!(preview_page(&path, 1, 72, 0).is_err());
    }

    #[test]
    fn alternating_files_never_show_each_others_pages() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.pdf"), dir.path().join("b.pdf"));
        write_pdf(&a, 2, 100);
        write_pdf(&b, 2, 300);
        for _ in 0..3 {
            assert_eq!(width_of(&preview_page(&a, 2, 72, 0).unwrap()), 100);
            assert_eq!(width_of(&preview_page(&b, 1, 72, 0).unwrap()), 300);
        }
    }

    #[test]
    fn previews_leave_the_source_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.pdf");
        write_pdf(&path, 2, 100);
        let before = calculate_sha256(&path).unwrap();
        let first = preview_page(&path, 1, 72, 0).unwrap();
        assert_eq!(preview_page(&path, 1, 72, 0).unwrap(), first);
        preview_page(&path, 1, 72, 90).unwrap();
        assert_eq!(calculate_sha256(&path).unwrap(), before);
    }
}
