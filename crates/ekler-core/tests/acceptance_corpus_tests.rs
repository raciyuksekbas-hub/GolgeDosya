//! Checked-in synthetic inputs: a missing fixture is a failure, never an implicit skip.
use ekler_core::pdf::tolerant::RepairStrategy;
use ekler_core::{load_pdf_tolerant, scan_source_files};
use std::path::PathBuf;
// Fikstürler crate'in içinde: bu crate birleşik workspace'e taşınırken
// kendi kendine yeter hâle getirildi. Eskiden workspace kökündeki `qa/`
// altındaydı ve bağımsız DüzenEk deposunun varlığına bağlıydı.
fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name)
}
#[test]
fn checked_in_corpus_import_and_negative_cases() {
    for (name, pages) in [
        ("vector.pdf", 2),
        ("irregular-xref.pdf", 2),
        ("scanned.pdf", 1),
        ("signed-marker.pdf", 2),
        ("photo.jpg", 1),
        ("Türkçe görsel.png", 1),
        ("gray8.tiff", 1),
        ("gray16.tiff", 1),
        ("rgb.tiff", 1),
        ("multipage.tiff", 3),
    ] {
        let scan = scan_source_files(&[corpus(name)]);
        assert!(scan.errors.is_empty(), "{name}: {:?}", scan.errors);
        assert_eq!(scan.sources.len(), 1, "{name}");
        assert_eq!(scan.sources[0].page_count, pages, "{name}");
        if name == "signed-marker.pdf" {
            assert!(scan.sources[0].is_signed);
        }
    }
    let raw = std::fs::read(corpus("irregular-xref.pdf")).unwrap();
    assert_eq!(
        load_pdf_tolerant(&raw, "irregular-xref.pdf")
            .unwrap()
            .strategy,
        RepairStrategy::XrefNormalization
    );
    let scan = scan_source_files(&[corpus("encrypted.pdf")]);
    assert!(scan.sources.is_empty());
    assert_eq!(scan.errors.len(), 1);
    for name in [
        "corpus.doc",
        "corpus.docx",
        "photo.heic",
        "corpus.udf",
        "signed.udf",
    ] {
        assert!(std::fs::metadata(corpus(name)).unwrap().len() > 0, "{name}");
    }
}
