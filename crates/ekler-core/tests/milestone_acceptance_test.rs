use ekler_core::*;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document as LopdfDoc, Object, Stream};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

fn create_mock_pdf(pages: usize, prefix: &str) -> (tempfile::NamedTempFile, u64, String) {
    let mut doc = LopdfDoc::with_version("1.7");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let mut page_ids = Vec::new();
    for i in 1..=pages {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![50.into(), 750.into()]),
                Operation::new(
                    "Tj",
                    vec![Object::string_literal(format!("{} - Sayfa {}", prefix, i))],
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
    {
        let mut f = File::create(tmp.path()).unwrap();
        doc.save_to(&mut f).unwrap();
        f.flush().unwrap();
    }

    let size = std::fs::metadata(tmp.path()).unwrap().len();
    let hash = calculate_sha256(tmp.path()).unwrap();
    (tmp, size, hash)
}

fn create_mock_jpeg(w: u32, h: u32) -> (tempfile::NamedTempFile, u64, String) {
    let img = ::image::DynamicImage::new_rgb8(w, h);
    let tmp = tempfile::Builder::new().suffix(".jpg").tempfile().unwrap();
    img.save(tmp.path()).unwrap();
    let size = std::fs::metadata(tmp.path()).unwrap().len();
    let hash = calculate_sha256(tmp.path()).unwrap();
    (tmp, size, hash)
}

fn create_mock_multipage_tiff(
    frames: usize,
    w: u32,
    h: u32,
) -> (tempfile::NamedTempFile, u64, String) {
    let tmp = tempfile::Builder::new().suffix(".tiff").tempfile().unwrap();
    {
        let f = File::create(tmp.path()).unwrap();
        let mut encoder = tiff::encoder::TiffEncoder::new(f).unwrap();

        for i in 0..frames {
            let buffer: Vec<u8> = (0..(w * h * 3))
                .map(|k| ((k * 31 + (i as u32) * 17) % 256) as u8)
                .collect();
            let image_encoder = encoder
                .new_image::<tiff::encoder::colortype::RGB8>(w, h)
                .unwrap();
            image_encoder.write_data(&buffer).unwrap();
        }
    }

    let size = std::fs::metadata(tmp.path()).unwrap().len();
    let hash = calculate_sha256(tmp.path()).unwrap();
    (tmp, size, hash)
}

#[test]
fn test_milestone_full_scenario() {
    // 1. Kullanıcı kaynak dosyaları bırakır:
    // - PDF (25 sayfa, büyük sözleşme) -> Ek-1'e atanacak
    // - JPG (dekont görseli) -> Ek-2'ye atanacak
    // - Multipage TIFF (3 kareli taranmış evrak) -> Ek-3'e atanacak
    let (pdf_file, pdf_size, pdf_hash) = create_mock_pdf(25, "Hizmet ve Is Sozlesmesi");
    let (jpg_file, jpg_size, jpg_hash) = create_mock_jpeg(640, 480);
    let (tiff_file, tiff_size, tiff_hash) = create_mock_multipage_tiff(3, 400, 300);

    let src1 = SourceFile {
        id: "src-contract".to_string(),
        path: pdf_file.path().to_path_buf(),
        file_name: "Sozlesme.pdf".to_string(),
        format: SourceFormat::Pdf,
        size_bytes: pdf_size,
        sha256_before: pdf_hash.clone(),
        mtime: 1000,
        page_count: 25,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };

    let src2 = SourceFile {
        id: "src-receipt".to_string(),
        path: jpg_file.path().to_path_buf(),
        file_name: "Banka_Dekontu.jpg".to_string(),
        format: SourceFormat::Jpg,
        size_bytes: jpg_size,
        sha256_before: jpg_hash.clone(),
        mtime: 1000,
        page_count: 1,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };

    let src3 = SourceFile {
        id: "src-scans".to_string(),
        path: tiff_file.path().to_path_buf(),
        file_name: "Taranmis_Ihtarname.tiff".to_string(),
        format: SourceFormat::Tiff,
        size_bytes: tiff_size,
        sha256_before: tiff_hash.clone(),
        mtime: 1000,
        page_count: 3,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };

    let mut project = Project::new("Dava Dilekçesi Ekleri");
    project.add_source(src1);
    project.add_source(src2);
    project.add_source(src3);

    // Ek-1: 3 kareli taranmış büyük evrak (~1.1 MB) -> 600 KB sınırını aşarak ikiye bölünecek
    let e1 = project.create_exhibit("Taranmis Evraklar");
    e1.sources.push(ExhibitSourceRef {
        source_id: "src-scans".to_string(),
        page_range: None,
    });

    // Ek-2: Sözleşme metni (25 sayfa PDF, ~6 KB)
    let e2 = project.create_exhibit("Is Sozlesmesi");
    e2.sources.push(ExhibitSourceRef {
        source_id: "src-contract".to_string(),
        page_range: None,
    });

    // Ek-3: Banka Dekontu (1 sayfa JPG, ~10 KB)
    let e3 = project.create_exhibit("Banka Dekontu");
    e3.sources.push(ExhibitSourceRef {
        source_id: "src-receipt".to_string(),
        page_range: None,
    });

    println!(
        "PDF size: {}, JPG size: {}, TIFF size: {}",
        pdf_size, jpg_size, tiff_size
    );
    // Gerçek PDF'e dönüştükten sonraki boyutları ölç:
    let mut pdf_doc = LopdfDoc::load(pdf_file.path()).unwrap();
    let mut buf1 = Vec::new();
    pdf_doc.save_to(&mut buf1).unwrap();
    println!("Candidate PDF doc bytes: {}", buf1.len());

    let mut tiff_doc = image::image_file_to_pdf(tiff_file.path()).unwrap();
    let mut buf2 = Vec::new();
    tiff_doc.save_to(&mut buf2).unwrap();
    println!("Candidate TIFF doc bytes: {}", buf2.len());

    // Ek-1'in bölünmesini sağlamak için target_size_bytes'ı tiff_doc boyutunun yarısı civarına ayarlıyoruz:
    project.target_size_bytes = (buf2.len() as u64 * 3) / 4;
    println!("Setting target_size_bytes = {}", project.target_size_bytes);

    let out_dir = tempdir().unwrap();
    let ctx = ExecutionContext {
        output_dir: out_dir.path().to_path_buf(),
    };

    // BORU HATTINI ÇALIŞTIR
    let result = execute_uyap_preparation(&project, &ctx).unwrap();

    // 1. Ek-1 iki parçaya bölünmüş olmalı: EK-01_Taranmis_Evraklar_S001-... ve EK-01_Taranmis_Evraklar_DEVAM_...
    let ek1_outputs: Vec<&PhysicalOutput> = result
        .outputs
        .iter()
        .filter(|o| o.exhibit_id == "exhibit-1")
        .collect();
    assert!(
        ek1_outputs.len() >= 2,
        "Ek-1 boyut sınırını aştığı için devam dosyalarına bölünmeliydi"
    );
    assert!(!ek1_outputs[0].is_continuation);
    assert!(ek1_outputs[1].is_continuation);
    assert!(ek1_outputs[1].file_name.contains("DEVAM"));

    // 2. Ek-1 sayfa numaralandırması kesintisiz olmalı (1..3)
    let ek1_pages_sum: usize = ek1_outputs.iter().map(|o| o.page_count).sum();
    assert_eq!(ek1_pages_sum, 3);
    assert_eq!(ek1_outputs[0].page_start, 1);
    assert_eq!(ek1_outputs[1].page_start, ek1_outputs[0].page_end + 1);

    // 3. Ek-2 (25 sayfa PDF) ve Ek-3 (1 sayfa JPG) eksiksiz üretilmiş olmalı
    let ek2_output = result
        .outputs
        .iter()
        .find(|o| o.exhibit_id == "exhibit-2")
        .unwrap();
    assert_eq!(ek2_output.page_count, 25);

    let ek3_output = result
        .outputs
        .iter()
        .find(|o| o.exhibit_id == "exhibit-3")
        .unwrap();
    assert_eq!(ek3_output.page_count, 1);

    // 4. Katı Sayfa Muhasebesi (Strict Page Accounting):
    // 3 (Ek-1) + 25 (Ek-2) + 1 (Ek-3) = 29 sayfa girdi == 29 sayfa çıktı!
    let total_pages_output: usize = result.outputs.iter().map(|o| o.page_count).sum();
    assert_eq!(total_pages_output, 29);

    // 5. Tüm fiziksel çıktı PDF'lerinin hedef boyut altında olduğu gerçek bayt ölçümüyle kanıtlanmalı
    for out in &result.outputs {
        assert!(
            out.size_bytes <= project.target_size_bytes,
            "Çıktı {} boyutu ({} bayt) sınırı ({} bayt) aşamaz!",
            out.file_name,
            out.size_bytes,
            project.target_size_bytes
        );
    }

    // 6. Sıfır Mutasyon: Kaynak dosyaların SHA-256'ları işlem öncesi ve sonrası birebir aynı olmalıdır!
    assert_eq!(calculate_sha256(pdf_file.path()).unwrap(), pdf_hash);
    assert_eq!(calculate_sha256(jpg_file.path()).unwrap(), jpg_hash);
    assert_eq!(calculate_sha256(tiff_file.path()).unwrap(), tiff_hash);

    // 7. Ekler Listesi dosyası oluşturulmuş olmalı
    let list_path = result.package_dir.join("EKLER_LISTESI.txt");
    assert!(list_path.exists());
    let list_txt = std::fs::read_to_string(list_path).unwrap();
    assert!(list_txt.contains("Ek-1: Taranmis Evraklar — 3 sayfa"));
    assert!(list_txt.contains("Ek-2: Is Sozlesmesi — 25 sayfa"));
    assert!(list_txt.contains("Ek-3: Banka Dekontu — 1 sayfa"));

    // 8. FINAL VALIDATION: UYAP'A HAZIR kontrolü yeşil (PASS) olmalıdır!
    assert!(result.validation_report.is_ready_for_uyap);
    for item in &result.validation_report.items {
        assert_ne!(item.level, CheckLevel::Error);
    }
}
