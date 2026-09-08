use ekler_core::{
    execute_uyap_preparation, ExecutionContext, LogicalExhibit, Project, SignedPolicy, SourceFile,
    SourceFormat,
};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document as LopdfDoc, Stream};
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

fn create_sample_pdf(pages_cnt: usize) -> tempfile::NamedTempFile {
    let mut doc = LopdfDoc::with_version("1.4");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();

    for _ in 0..pages_cnt {
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
fn test_export_failure_leaves_zero_files_in_destination() {
    let target_dir = TempDir::new().unwrap();
    let ctx = ExecutionContext {
        output_dir: target_dir.path().to_path_buf(),
    };

    let valid_pdf = create_sample_pdf(3);
    let size = std::fs::metadata(valid_pdf.path()).unwrap().len();
    let hash = ekler_core::calculate_sha256(valid_pdf.path()).unwrap();

    let mut project = Project::new("Rollback Testi");

    // Ek 1: Geçerli bir dosya
    let src1 = SourceFile {
        id: "src-valid".to_string(),
        path: valid_pdf.path().to_path_buf(),
        file_name: "Gecerli.pdf".to_string(),
        format: SourceFormat::Pdf,
        size_bytes: size,
        sha256_before: hash,
        mtime: 1000,
        page_count: 3,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };
    project.add_source(src1);

    let mut ex1 = LogicalExhibit::new("ex-1".to_string(), 1, "Belge 1".to_string());
    ex1.sources.push(ekler_core::ExhibitSourceRef {
        source_id: "src-valid".to_string(),
        page_range: None,
    });
    project.exhibits.push(ex1);

    // Ek 2: Hatalı sayfa aralığı (örneğin 3 sayfalık belgeden 50. sayfayı istemek)
    let mut ex2 = LogicalExhibit::new("ex-2".to_string(), 2, "Belge 2".to_string());
    ex2.sources.push(ekler_core::ExhibitSourceRef {
        source_id: "src-valid".to_string(),
        page_range: Some((10, 50)), // Geçersiz aralık! Hata tetikler
    });
    project.exhibits.push(ex2);

    // Dışa aktarmayı çalıştır
    let res = execute_uyap_preparation(&project, &ctx);
    assert!(
        res.is_err(),
        "Hatalı ek nedeniyle dışa aktarma başarısız olmalıdır"
    );

    // KRİTİK P0-2 İNVARYANTI:
    // Ek 1 normalde üretilebilirdi; ANCAK işlem atomik olduğu ve Ek 2 başarısız olduğu için
    // hedef klasörde KESİNLİKLE HİÇBİR DOSYA (0 byte veya kısmi) KALMAMALIDIR!
    let entries: Vec<_> = std::fs::read_dir(target_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();

    assert_eq!(
        entries.len(),
        0,
        "Hata durumunda hedef klasörde hiçbir dosya kalmamalıdır (All-or-Nothing Rollback). Bulunan: {:?}",
        entries.iter().map(|e| e.file_name()).collect::<Vec<_>>()
    );
}

#[test]
fn test_atomic_export_success_verifies_all_invariants() {
    let target_dir = TempDir::new().unwrap();
    let ctx = ExecutionContext {
        output_dir: target_dir.path().to_path_buf(),
    };

    let pdf1 = create_sample_pdf(2);
    let size1 = std::fs::metadata(pdf1.path()).unwrap().len();
    let hash1 = ekler_core::calculate_sha256(pdf1.path()).unwrap();

    let pdf2 = create_sample_pdf(4);
    let size2 = std::fs::metadata(pdf2.path()).unwrap().len();
    let hash2 = ekler_core::calculate_sha256(pdf2.path()).unwrap();

    let mut project = Project::new("Basarili Disa Aktarma Testi");

    let src1 = SourceFile {
        id: "src-1".to_string(),
        path: pdf1.path().to_path_buf(),
        file_name: "Belge1.pdf".to_string(),
        format: SourceFormat::Pdf,
        size_bytes: size1,
        sha256_before: hash1,
        mtime: 1000,
        page_count: 2,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };
    project.add_source(src1);

    let src2 = SourceFile {
        id: "src-2".to_string(),
        path: pdf2.path().to_path_buf(),
        file_name: "Belge2.pdf".to_string(),
        format: SourceFormat::Pdf,
        size_bytes: size2,
        sha256_before: hash2,
        mtime: 1000,
        page_count: 4,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };
    project.add_source(src2);

    let mut ex1 = LogicalExhibit::new("ex-1".to_string(), 1, "Ilk Ek".to_string());
    ex1.sources.push(ekler_core::ExhibitSourceRef {
        source_id: "src-1".to_string(),
        page_range: None,
    });
    project.exhibits.push(ex1);

    let mut ex2 = LogicalExhibit::new("ex-2".to_string(), 2, "Ikinci Ek".to_string());
    ex2.sources.push(ekler_core::ExhibitSourceRef {
        source_id: "src-2".to_string(),
        page_range: None,
    });
    project.exhibits.push(ex2);

    let res = execute_uyap_preparation(&project, &ctx).expect("Dışa aktarma başarılı olmalıdır");

    // Doğrulamalar:
    // 1. En az 2 ek çıktısı + 1 EKLER_LISTESI.txt
    assert!(res.outputs.len() >= 2);
    let list_file = res.package_dir.join("EKLER_LISTESI.txt");
    assert!(list_file.exists());
    assert!(std::fs::metadata(&list_file).unwrap().len() > 0);

    // 2. Her bir fiziksel PDF çıktısının katı invaryant denetimi
    for out in &res.outputs {
        let path = res.package_dir.join(&out.file_name);
        assert!(
            path.exists(),
            "Çıktı dosyası mevcut olmalıdır: {}",
            out.file_name
        );

        let meta = std::fs::metadata(&path).unwrap();
        assert!(
            meta.len() > 0,
            "Çıktı dosyası asla 0 bayt olamaz: {}",
            out.file_name
        );

        let doc = lopdf::Document::load(&path).expect("Üretilen PDF geçerli yapıda olmalıdır");
        assert_eq!(
            doc.get_pages().len(),
            out.page_count,
            "Üretilen PDF'in sayfa sayısı beklenen ile birebir eşleşmelidir"
        );
    }

    // 3. Staging dizini tamamen silinmiş olmalı
    let staging_remnants: Vec<_> = std::fs::read_dir(target_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp_staging"))
        .collect();
    assert_eq!(
        staging_remnants.len(),
        0,
        "Staging dizini geride çöp bırakmamalıdır"
    );
}

#[test]
#[ignore = "External private fixture; run explicitly on the fixture owner machine"]
fn test_real_user_file_full_export_and_artifact_reopen() {
    let real_path =
        std::path::Path::new("/Users/racicetinyuksekbas/Downloads/hakan almalı ihtarname.pdf");
    assert!(real_path.exists(), "Required external fixture is missing");

    let initial_hash = ekler_core::calculate_sha256(real_path).unwrap();
    let initial_meta = std::fs::metadata(real_path).unwrap();

    let target_dir = TempDir::new().unwrap();
    let ctx = ExecutionContext {
        output_dir: target_dir.path().to_path_buf(),
    };

    let mut project = Project::new("Gercek Dosya Dışa Aktarım Testi");
    project.stamp_config.enabled = true;

    // Scan using batch scanner to test full integration
    let scan_res = ekler_core::scan_source_files(&[real_path]);
    assert_eq!(scan_res.sources.len(), 1);
    let src = scan_res.sources[0].clone();
    assert_eq!(src.page_count, 1);
    assert!(
        src.is_repaired,
        "Real file has irregular trailer/xref and should be flagged as repaired"
    );

    let src_id = src.id.clone();
    project.add_source(src);

    let mut ex1 = LogicalExhibit::new("ex-1".to_string(), 1, "Hakan Almalı İhtarname".to_string());
    ex1.sources.push(ekler_core::ExhibitSourceRef {
        source_id: src_id,
        page_range: None,
    });
    project.exhibits.push(ex1);

    let res =
        execute_uyap_preparation(&project, &ctx).expect("Gerçek dosya export'u başarılı olmalıdır");
    assert_eq!(res.outputs.len(), 1);

    let out_file = res.package_dir.join(&res.outputs[0].file_name);
    assert!(out_file.exists());

    let out_meta = std::fs::metadata(&out_file).unwrap();
    assert!(out_meta.len() > 0, "Dışa aktarılan dosya 0 bayt olamaz");

    // Re-open with standard lopdf (no repair needed since it was written out cleanly)
    let reloaded = lopdf::Document::load(&out_file)
        .expect("Dışa aktarılan PDF standart lopdf ile doğrudan açılabilmelidir");
    assert_eq!(reloaded.get_pages().len(), 1, "Sayfa sayısı 1 olmalıdır");

    // Original file immutability check
    let final_hash = ekler_core::calculate_sha256(real_path).unwrap();
    let final_meta = std::fs::metadata(real_path).unwrap();
    assert_eq!(
        initial_hash, final_hash,
        "Orijinal kullanıcı dosyası ASLA değiştirilemez!"
    );
    assert_eq!(
        initial_meta.len(),
        final_meta.len(),
        "Orijinal kullanıcı dosya boyutu değişemez!"
    );
}
