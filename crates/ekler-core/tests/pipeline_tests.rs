use ekler_core::*;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document as LopdfDoc, Object, Stream};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

fn create_test_pdf(num_pages: usize, text_prefix: &str) -> (tempfile::NamedTempFile, u64, String) {
    let mut doc = LopdfDoc::with_version("1.7");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let mut page_ids = Vec::new();
    for i in 1..=num_pages {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![50.into(), 750.into()]),
                Operation::new(
                    "Tj",
                    vec![Object::string_literal(format!(
                        "{} Sayfa {}",
                        text_prefix, i
                    ))],
                ),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.encode().unwrap()));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.28.into(), 841.89.into()],
            "Resources" => dictionary! {
                "Font" => dictionary! {
                    "F1" => font_id,
                },
            },
        });
        page_ids.push(page_id);
    }

    let pages_obj = dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
        "Count" => page_ids.len() as i32,
    };
    doc.set_object(pages_id, pages_obj);

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let tmp = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
    let mut file = File::create(tmp.path()).unwrap();
    doc.save_to(&mut file).unwrap();
    file.flush().unwrap();

    let size = std::fs::metadata(tmp.path()).unwrap().len();
    let hash = calculate_sha256(tmp.path()).unwrap();

    (tmp, size, hash)
}

fn create_test_image(width: u32, height: u32) -> (tempfile::NamedTempFile, u64, String) {
    let mut img = ::image::DynamicImage::new_rgb8(width, height);
    if let Some(rgb) = img.as_mut_rgb8() {
        for (x, y, pixel) in rgb.enumerate_pixels_mut() {
            if (x + y) % 20 < 10 {
                *pixel = ::image::Rgb([200, 220, 240]);
            } else {
                *pixel = ::image::Rgb([50, 80, 120]);
            }
        }
    }

    let tmp = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    img.save(tmp.path()).unwrap();

    let size = std::fs::metadata(tmp.path()).unwrap().len();
    let hash = calculate_sha256(tmp.path()).unwrap();

    (tmp, size, hash)
}

#[test]
fn test_pdf_inspection_and_signature_detection() {
    let (pdf_file, _, _) = create_test_pdf(3, "Test Belge");
    let info = pdf::inspect_pdf(pdf_file.path()).unwrap();

    assert_eq!(info.page_count, 3);
    assert!(!info.is_signed);
    assert_eq!(info.pages_dimensions.len(), 3);
    assert_eq!(info.pages_dimensions[0], (595.28, 841.89));
}

#[test]
fn test_image_conversion_to_pdf() {
    let (img_file, _, _) = create_test_image(800, 600);
    let doc = ekler_core::image::image_file_to_pdf(img_file.path()).unwrap();

    let pages = doc.get_pages();
    assert_eq!(pages.len(), 1);
}

#[test]
fn test_smart_split_acceptance_test() {
    // Madde 34: SMART SPLIT ACCEPTANCE TEST
    // 20 sayfalık mantıksal Ek oluştur
    let (pdf_file, size, hash) = create_test_pdf(20, "Hukuki Sozlesme");

    let source = SourceFile {
        id: "src-1".to_string(),
        path: pdf_file.path().to_path_buf(),
        file_name: "Sozlesme.pdf".to_string(),
        format: SourceFormat::Pdf,
        size_bytes: size,
        sha256_before: hash.clone(),
        mtime: 1000,
        page_count: 20,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };

    let mut project = Project::new("Akıllı Bölme Kabul Testi");
    project.add_source(source);

    let exhibit = project.create_exhibit("Sozlesme");
    exhibit.sources.push(ExhibitSourceRef {
        source_id: "src-1".to_string(),
        page_range: None,
    });

    // Akıllı bölmeyi tetiklemek için hedef boyutu ayarla
    // Keep the cap above one stamped page plus the shared transparency resources.
    // It must still force multiple outputs; coverage and cap assertions below remain unchanged.
    project.target_size_bytes = 8_000;

    let out_dir = tempdir().unwrap();
    let ctx = ExecutionContext {
        output_dir: out_dir.path().to_path_buf(),
    };

    let result = execute_uyap_preparation(&project, &ctx).unwrap();

    // 1. İki fiziksel PDF çıktısı oluşmuş olmalı
    assert!(
        result.outputs.len() >= 2,
        "20 sayfa belirlenen hedef sınır nedeniyle bölünmeliydi"
    );

    // 2. İlk çıktı EK-01_Sozlesme_S001-S... olmalı
    let first = &result.outputs[0];
    assert_eq!(first.page_start, 1);
    assert!(!first.is_continuation);
    assert!(first.file_name.starts_with("EK-01_Sozlesme_S001-"));

    // 3. İkinci çıktı DEVAM olmalı ve ilk çıktının bittiği sayfadan devam etmeli
    let second = &result.outputs[1];
    assert!(second.is_continuation);
    assert!(second.file_name.contains("DEVAM"));
    assert_eq!(second.page_start, first.page_end + 1);

    // 4. Toplam sayfa sayısı 20 olmalı (0 kayıp, 0 tekrar)
    let total_out_pages: usize = result.outputs.iter().map(|o| o.page_count).sum();
    assert_eq!(total_out_pages, 20);

    // 5. Her fiziksel dosya hedef boyutun altında olmalı
    for out in &result.outputs {
        assert!(
            out.size_bytes <= project.target_size_bytes,
            "Çıktı boyutu hedefi aşamaz: {} > {}",
            out.size_bytes,
            project.target_size_bytes
        );
    }

    // 6. Kaynak dosya kesinlikle değişmemiş olmalı
    let final_hash = calculate_sha256(pdf_file.path()).unwrap();
    assert_eq!(hash, final_hash, "KAYNAK DOSYA ASLA DEĞİŞTİRİLEMEZ!");

    // 7. Ekler listesi üretilmiş olmalı
    let list_file = result.package_dir.join("EKLER_LISTESI.txt");
    assert!(list_file.exists());
    let list_content = std::fs::read_to_string(&list_file).unwrap();
    assert!(list_content.contains("Ek-1: Sozlesme — 20 sayfa"));

    // 8. Son doğrulama raporu UYAP'a Hazır olmalı
    assert!(result.validation_report.is_ready_for_uyap);
}

#[test]
fn test_mixed_sources_pipeline_pdf_and_image() {
    // Hem PDF hem Görsel içeren karma Ek testi
    let (pdf_file, pdf_size, pdf_hash) = create_test_pdf(2, "Dava Dilekcesi");
    let (img_file, img_size, img_hash) = create_test_image(600, 400);

    let src1 = SourceFile {
        id: "src-pdf".to_string(),
        path: pdf_file.path().to_path_buf(),
        file_name: "Dilekce.pdf".to_string(),
        format: SourceFormat::Pdf,
        size_bytes: pdf_size,
        sha256_before: pdf_hash.clone(),
        mtime: 1000,
        page_count: 2,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };

    let src2 = SourceFile {
        id: "src-img".to_string(),
        path: img_file.path().to_path_buf(),
        file_name: "Dekont.png".to_string(),
        format: SourceFormat::Png,
        size_bytes: img_size,
        sha256_before: img_hash.clone(),
        mtime: 1000,
        page_count: 1,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };

    let mut project = Project::new("Karma Ek Testi");
    project.add_source(src1);
    project.add_source(src2);

    let exhibit = project.create_exhibit("Belgeler");
    exhibit.sources.push(ExhibitSourceRef {
        source_id: "src-pdf".to_string(),
        page_range: None,
    });
    exhibit.sources.push(ExhibitSourceRef {
        source_id: "src-img".to_string(),
        page_range: None,
    });

    let out_dir = tempdir().unwrap();
    let ctx = ExecutionContext {
        output_dir: out_dir.path().to_path_buf(),
    };

    let result = execute_uyap_preparation(&project, &ctx).unwrap();

    assert!(result.validation_report.is_ready_for_uyap);
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.outputs[0].page_count, 3); // 2 sayfa PDF + 1 sayfa Görsel = 3 sayfa

    // Orijinal dosyaların SHA-256'ları değişmemiş olmalı
    assert_eq!(calculate_sha256(pdf_file.path()).unwrap(), pdf_hash);
    assert_eq!(calculate_sha256(img_file.path()).unwrap(), img_hash);
}
