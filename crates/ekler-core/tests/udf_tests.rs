use ekler_core::*;
use std::path::Path;
use tempfile::tempdir;

#[test]
#[ignore = "External private fixture; run explicitly on the fixture owner machine"]
fn test_udf_inspection_and_markdown_conversion() {
    let sample_udf = Path::new("/Users/racicetinyuksekbas/Documents/Tavzih/acceptance/UYAP-Kabul-Adaylari/26_qa_label_layout.udf");
    assert!(sample_udf.exists(), "Required external fixture missing");

    let initial_hash = calculate_sha256(sample_udf).unwrap();

    let info = inspect_udf(sample_udf).unwrap();
    assert!(info.paragraph_count > 0);

    let out_dir = tempdir().unwrap();

    let result = convert_udf_to_markdown(sample_udf, out_dir.path(), true).unwrap();
    let md_content = result.markdown;
    assert!(!md_content.is_empty());
    assert!(result.package_dir.join("belge.md").exists());

    // Türkçe karakterlerin korunduğunu doğrula
    assert!(
        md_content.contains("MAHKEMESİ")
            || md_content.contains("İDARE")
            || md_content.contains("Kırklareli")
    );

    // Kaynak UDF dosyasının asla değişmediğini doğrula
    let final_hash = calculate_sha256(sample_udf).unwrap();
    assert_eq!(initial_hash, final_hash, "KAYNAK UDF ASLA DEĞİŞTİRİLEMEZ!");
}
