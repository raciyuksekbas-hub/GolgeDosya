use ekler_core::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_project_exhibits_and_renumbering() {
    let mut project = Project::new("Örnek Proje");
    assert_eq!(project.name, "Örnek Proje");

    let e1 = project.create_exhibit("İş Sözleşmesi");
    assert_eq!(e1.order, 1);
    assert_eq!(e1.display_title(), "Ek-1: İş Sözleşmesi");

    let e2 = project.create_exhibit("İhtarname");
    assert_eq!(e2.order, 2);

    let e3 = project.create_exhibit("Dekontlar");
    assert_eq!(e3.order, 3);

    // Ek-2 silindiğinde, Ek-3 otomatik olarak Ek-2 olmalıdır
    project.remove_exhibit("exhibit-2");
    assert_eq!(project.exhibits.len(), 2);
    assert_eq!(project.exhibits[0].order, 1);
    assert_eq!(project.exhibits[0].name, "İş Sözleşmesi");
    assert_eq!(project.exhibits[1].order, 2);
    assert_eq!(project.exhibits[1].name, "Dekontlar");
}

#[test]
fn test_sanitize_filename() {
    assert_eq!(
        sanitize_for_filename("İş Sözleşmesi (İmzalı)"),
        "Is_Sozlesmesi_Imzali"
    );
    assert_eq!(
        sanitize_for_filename("Kadıköy 12. Noterliği İhtarname"),
        "Kadikoy_12_Noterligi_Ihtarname"
    );
    assert_eq!(sanitize_for_filename("   "), "Ek");
}

#[test]
fn test_file_stem_prefix() {
    let exhibit = LogicalExhibit::new("e1".to_string(), 3, "Banka Dekontu".to_string());
    assert_eq!(exhibit.file_stem_prefix(2), "EK-03_Banka_Dekontu");
    assert_eq!(exhibit.file_stem_prefix(3), "EK-003_Banka_Dekontu");
}

#[test]
fn test_exhibits_list_generation() {
    let mut project = Project::new("Dava Ekleri");
    project.create_exhibit("İş Sözleşmesi");
    project.create_exhibit("İhtarname");

    let counts = vec![("exhibit-1".to_string(), 6), ("exhibit-2".to_string(), 3)];

    let plain = ExhibitsListGenerator::generate_plain_text(&project, &counts);
    assert!(plain.contains("EKLER"));
    assert!(plain.contains("Ek-1: İş Sözleşmesi — 6 sayfa"));
    assert!(plain.contains("Ek-2: İhtarname — 3 sayfa"));

    let md = ExhibitsListGenerator::generate_markdown(&project, &counts);
    assert!(md.contains("### EKLER"));
    assert!(md.contains("* **Ek-1**: İş Sözleşmesi — *6 sayfa*"));
}

#[test]
fn test_strict_page_accounting_success() {
    let pages = vec![
        NormalizedPage {
            source_id: "s1".to_string(),
            source_page_index: 1,
            exhibit_id: "e1".to_string(),
            exhibit_page_number: 1,
            total_exhibit_pages: 3,
            width_pt: 595.28,
            height_pt: 841.89,
            orientation: PageOrientation::Portrait,
            is_blank: false,
        },
        NormalizedPage {
            source_id: "s1".to_string(),
            source_page_index: 2,
            exhibit_id: "e1".to_string(),
            exhibit_page_number: 2,
            total_exhibit_pages: 3,
            width_pt: 595.28,
            height_pt: 841.89,
            orientation: PageOrientation::Portrait,
            is_blank: false,
        },
        NormalizedPage {
            source_id: "s1".to_string(),
            source_page_index: 3,
            exhibit_id: "e1".to_string(),
            exhibit_page_number: 3,
            total_exhibit_pages: 3,
            width_pt: 595.28,
            height_pt: 841.89,
            orientation: PageOrientation::Portrait,
            is_blank: false,
        },
    ];

    let outputs = vec![
        PhysicalOutput {
            file_name: "EK-01_Part1.pdf".to_string(),
            exhibit_id: "e1".to_string(),
            exhibit_order: 1,
            is_continuation: false,
            continuation_index: 0,
            page_start: 1,
            page_end: 2,
            page_count: 2,
            size_bytes: 5_000_000,
            sha256: "hash1".to_string(),
        },
        PhysicalOutput {
            file_name: "EK-01_DEVAM_Part2.pdf".to_string(),
            exhibit_id: "e1".to_string(),
            exhibit_order: 1,
            is_continuation: true,
            continuation_index: 1,
            page_start: 3,
            page_end: 3,
            page_count: 1,
            size_bytes: 2_000_000,
            sha256: "hash2".to_string(),
        },
    ];

    let res = verify_page_accounting("e1", &pages, &outputs);
    assert!(res.is_ok());
    let report = res.unwrap();
    assert!(report.is_valid);
    assert_eq!(report.input_page_count, 3);
    assert_eq!(report.output_page_count, 3);
}

#[test]
fn test_strict_page_accounting_missing_page_fails() {
    let pages = vec![
        NormalizedPage {
            source_id: "s1".to_string(),
            source_page_index: 1,
            exhibit_id: "e1".to_string(),
            exhibit_page_number: 1,
            total_exhibit_pages: 2,
            width_pt: 595.28,
            height_pt: 841.89,
            orientation: PageOrientation::Portrait,
            is_blank: false,
        },
        NormalizedPage {
            source_id: "s1".to_string(),
            source_page_index: 2,
            exhibit_id: "e1".to_string(),
            exhibit_page_number: 2,
            total_exhibit_pages: 2,
            width_pt: 595.28,
            height_pt: 841.89,
            orientation: PageOrientation::Portrait,
            is_blank: false,
        },
    ];

    // Yalnızca 1. sayfa çıktıya eklenmiş olsun (2. sayfa kayıp)
    let outputs = vec![PhysicalOutput {
        file_name: "EK-01.pdf".to_string(),
        exhibit_id: "e1".to_string(),
        exhibit_order: 1,
        is_continuation: false,
        continuation_index: 0,
        page_start: 1,
        page_end: 1,
        page_count: 1,
        size_bytes: 3_000_000,
        sha256: "hash1".to_string(),
    }];

    let res = verify_page_accounting("e1", &pages, &outputs);
    assert!(res.is_err());
}

#[test]
fn test_source_integrity_invariant() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"Orijinal Dokuman Icerigi").unwrap();
    tmp.flush().unwrap();

    let path = tmp.path().to_path_buf();
    let initial_hash = calculate_sha256(&path).unwrap();

    let source = SourceFile {
        id: "src-1".to_string(),
        path: path.clone(),
        file_name: "belge.pdf".to_string(),
        format: SourceFormat::Pdf,
        size_bytes: 24,
        sha256_before: initial_hash,
        mtime: 1000,
        page_count: 1,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    };

    // Değişmediğinde geçerli
    assert!(verify_source_integrity(&source).is_ok());

    // Kaynak değiştirilirse bütünlük kontrolü patlamalıdır
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(b" Yetkisiz Mutasyon").unwrap();
    file.flush().unwrap();

    assert!(verify_source_integrity(&source).is_err());
}

#[test]
fn test_validation_report() {
    let mut project = Project::new("Proje");
    project.create_exhibit("Ek 1");
    let outputs = vec![PhysicalOutput {
        file_name: "EK-01.pdf".to_string(),
        exhibit_id: "exhibit-1".to_string(),
        exhibit_order: 1,
        is_continuation: false,
        continuation_index: 0,
        page_start: 1,
        page_end: 5,
        page_count: 5,
        size_bytes: 4_000_000, // 4 MB < 9.5 MB
        sha256: "hash".to_string(),
    }];

    let rep = validate_project_and_outputs(&project, &outputs);
    assert!(rep.is_ready_for_uyap);

    // 10 MB limitini aşan çıktı durumunda UYAP'a Hazır olmamalıdır
    let oversized = vec![PhysicalOutput {
        file_name: "EK-01.pdf".to_string(),
        exhibit_id: "exhibit-1".to_string(),
        exhibit_order: 1,
        is_continuation: false,
        continuation_index: 0,
        page_start: 1,
        page_end: 20,
        page_count: 20,
        size_bytes: 11_000_000, // 11 MB > 9.5 MB
        sha256: "hash".to_string(),
    }];
    let rep2 = validate_project_and_outputs(&project, &oversized);
    assert!(!rep2.is_ready_for_uyap);
}
#[test]
fn portable_filename_components_avoid_windows_devices_and_long_names() {
    assert_eq!(sanitize_for_filename("CON"), "Belge_CON");
    assert_eq!(sanitize_for_filename("lpt1"), "Belge_lpt1");
    assert!(sanitize_for_filename(&"A".repeat(500)).len() <= 96);
    assert!(!sanitize_for_filename("ödeme. ").ends_with('.'));
}
