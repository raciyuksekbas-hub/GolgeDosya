use ekler_core::{scan_source_files, SourceFormat};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document as LopdfDoc, Stream};
use std::fs::File;
use std::io::Write;
use tempfile::NamedTempFile;

fn create_valid_test_pdf(pages_cnt: usize) -> NamedTempFile {
    let mut doc = LopdfDoc::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();

    for _i in 0..pages_cnt {
        let content = Content {
            operations: vec![Operation::new("q", vec![]), Operation::new("Q", vec![])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }

    doc.set_object(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => pages_cnt as i64,
        },
    );

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
    tmp
}

#[test]
fn test_scanner_batch_resilience_and_dedup() {
    // 1. Düzgün bir 3 sayfalık PDF
    let valid_pdf = create_valid_test_pdf(3);

    // 2. Bozuk bir PDF (uzantısı .pdf ama içi çöp veri)
    let mut corrupt_pdf = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
    corrupt_pdf
        .write_all(b"BU_GECERSIZ_BIR_PDF_DOSYASIDIR_CORRUPT")
        .unwrap();
    corrupt_pdf.flush().unwrap();

    // 3. Desteklenmeyen bir dosya (ör. .exe veya .bin)
    let mut unsupported_file = tempfile::Builder::new().suffix(".bin").tempfile().unwrap();
    unsupported_file.write_all(b"BINARY_DATA").unwrap();
    unsupported_file.flush().unwrap();

    // 4. Var olmayan bir hayali dosya
    let nonexistent_path = "/tmp/kesinlikle_var_olmayan_dosya_1234567.pdf";

    // 5. Aynı geçerli PDF dosyasının ikinci kopyası (Deduplication testi için)
    let valid_pdf_dup = valid_pdf.path().to_path_buf();

    let paths = vec![
        valid_pdf.path().to_path_buf(),
        corrupt_pdf.path().to_path_buf(),
        unsupported_file.path().to_path_buf(),
        std::path::PathBuf::from(nonexistent_path),
        valid_pdf_dup,
    ];

    let result = scan_source_files(&paths);

    // BATCH RESILIENCE:
    // Bozuk, desteklenmeyen veya var olmayan 3 dosya tüm batch'i ÇÖKERTMEDİ!
    // 2 geçerli tarama başarıyla yapıldı.
    assert_eq!(result.sources.len(), 2);
    assert_eq!(result.errors.len(), 3);

    // Geçerli dosya kontrolleri
    let src1 = &result.sources[0];
    assert_eq!(src1.format, SourceFormat::Pdf);
    assert_eq!(src1.page_count, 3);
    assert!(!src1.is_signed);
    assert!(src1.size_bytes > 0);

    // DEDUPLICATION:
    // Aynı fiziksel dosya için üretilen ID ve SHA-256 birebir aynı olmalı
    let src2 = &result.sources[1];
    assert_eq!(src1.id, src2.id);
    assert_eq!(src1.sha256_before, src2.sha256_before);

    // Hata mesajlarının açıklayıcılığı
    let corrupt_err = result
        .errors
        .iter()
        .find(|e| e.path.ends_with(".pdf"))
        .unwrap();
    assert!(corrupt_err.reason.contains("geçersiz PDF"));

    let unsupported_err = result
        .errors
        .iter()
        .find(|e| e.path.ends_with(".bin"))
        .unwrap();
    assert!(unsupported_err
        .reason
        .contains("desteklenmeyen dosya biçimi"));

    let missing_err = result
        .errors
        .iter()
        .find(|e| e.path.contains("var_olmayan"))
        .unwrap();
    assert!(missing_err.reason.contains("bulunamadı"));
}

#[test]
#[ignore = "External private fixture; run explicitly on the fixture owner machine"]
fn test_load_real_file_with_scanner() {
    let p =
        std::path::PathBuf::from("/Users/racicetinyuksekbas/Downloads/hakan almalı ihtarname.pdf");
    assert!(p.exists(), "Required external fixture is missing");
    let res = scan_source_files(&[p]);
    assert_eq!(res.sources.len(), 1, "Gerçek dosya taranabilmelidir");
    assert_eq!(res.errors.len(), 0, "Gerçek dosya için hata olmamalıdır");

    let src = &res.sources[0];
    assert_eq!(src.page_count, 1);
    assert!(
        src.is_repaired,
        "Dosya toleranslı onarım motoruyla yüklenmiş olmalıdır"
    );
    assert!(src
        .signature_note
        .as_ref()
        .unwrap()
        .contains("çalışma kopyası"));
}
