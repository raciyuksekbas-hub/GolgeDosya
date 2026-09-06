//! Dönüştür modülünün artifact doğrulaması.
//!
//! Birim testleri motorun doğru çalıştığını gösterir; bu dosya **gerçek dosya
//! üretildiğini** gösterir. Kabuğun sarmaladığı yol uçtan uca koşulur:
//! kaynak diskte oluşturulur, dönüştürülür, çıktı diskte aranır, bağımsız
//! olarak yeniden okunur ve kaynağın değişmediği hash'le kanıtlanır.
//!
//! Motor `document-core`'dur ve bu testlerde hiç değiştirilmez; sınanan şey
//! taşımanın kendisidir.

#![cfg(feature = "feature_tavzih")]

use std::path::{Path, PathBuf};
use tavzih_core::convert::{self, Direction, Status};
use tavzih_core::model::{Block, Document, Paragraph, Section};
use tavzih_core::{docx, udf, warnings::WarningSink};

fn workdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "belge-tavzih-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Türkçe metin ve basit biçimlendirme taşıyan bir belge.
fn sample() -> Document {
    let mut d = Document::default();
    let mut s = Section::default();
    s.blocks = vec![
        Block::Paragraph(Paragraph::plain("İSTANBUL NÖBETÇİ ASLİYE HUKUK MAHKEMESİ")),
        Block::Paragraph(Paragraph::plain(
            "Müvekkilim adına, şğüçöıİ karakterlerinin korunduğunu doğrulayan bir paragraf.",
        )),
        Block::Paragraph(Paragraph::plain("Saygılarımla arz ederim.")),
    ];
    d.sections = vec![s];
    d
}

fn write_docx_file(dir: &Path, name: &str) -> PathBuf {
    let mut w = WarningSink::new();
    let bytes = docx::writer::write_docx(&sample(), &mut w).expect("docx yazılmalı");
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn write_udf_file(dir: &Path, name: &str) -> PathBuf {
    let mut w = WarningSink::new();
    // UDF paketleme iki adımdır: içerik XML'i üretilir, sonra ZIP'lenir.
    let content = udf::writer::write_content_xml(&sample(), &mut w).expect("içerik yazılmalı");
    let bytes = udf::writer::package(&content).expect("udf paketlenmeli");
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn text_of(d: &Document) -> String {
    d.sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn docx_to_udf_produces_a_real_file_that_reads_back() {
    let dir = workdir("docx2udf");
    let out = workdir("docx2udf-out");
    let src = write_docx_file(&dir, "dilekce.docx");
    let before = convert::sha256_file(&src).unwrap();

    let result = convert::convert_file_to(&src, &out);

    assert_ne!(
        result.status,
        Status::Failure,
        "dönüşüm başarısız: {:?}",
        result.error
    );
    assert_eq!(result.direction, Direction::DocxToUdf);

    // Çıktı GERÇEKTEN diskte olmalı ve boş olmamalı.
    let produced = PathBuf::from(result.output.as_ref().expect("çıktı yolu"));
    assert!(
        produced.is_file(),
        "çıktı dosyası yok: {}",
        produced.display()
    );
    assert!(std::fs::metadata(&produced).unwrap().len() > 0);
    assert_eq!(produced.extension().and_then(|e| e.to_str()), Some("udf"));

    // Bağımsız olarak yeniden okunmalı ve Türkçe metin korunmalı.
    let bytes = std::fs::read(&produced).unwrap();
    let mut w = WarningSink::new();
    let back = udf::reader::read_udf(&bytes, &mut w).expect("üretilen UDF okunabilmeli");
    let text = text_of(&back);
    assert!(
        text.contains("NÖBETÇİ ASLİYE HUKUK"),
        "başlık kayboldu: {text}"
    );
    assert!(
        text.contains("şğüçöıİ"),
        "Türkçe karakterler kayboldu: {text}"
    );

    // Kaynak asla değişmez ve bu hash'le kanıtlanır.
    assert_eq!(before, convert::sha256_file(&src).unwrap());
    assert_eq!(result.source_sha256_before, result.source_sha256_after);
    assert!(result.source_unchanged);

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&out).ok();
}

#[test]
fn udf_to_docx_produces_a_real_file_that_reads_back() {
    let dir = workdir("udf2docx");
    let out = workdir("udf2docx-out");
    let src = write_udf_file(&dir, "dilekce.udf");
    let before = convert::sha256_file(&src).unwrap();

    let result = convert::convert_file_to(&src, &out);

    assert_ne!(
        result.status,
        Status::Failure,
        "dönüşüm başarısız: {:?}",
        result.error
    );
    assert_eq!(result.direction, Direction::UdfToDocx);

    let produced = PathBuf::from(result.output.as_ref().expect("çıktı yolu"));
    assert!(produced.is_file());
    assert_eq!(produced.extension().and_then(|e| e.to_str()), Some("docx"));

    let bytes = std::fs::read(&produced).unwrap();
    let mut w = WarningSink::new();
    let back = docx::reader::read_docx(&bytes, &mut w).expect("üretilen DOCX okunabilmeli");
    let text = text_of(&back);
    assert!(
        text.contains("NÖBETÇİ ASLİYE HUKUK"),
        "başlık kayboldu: {text}"
    );
    assert!(
        text.contains("şğüçöıİ"),
        "Türkçe karakterler kayboldu: {text}"
    );

    assert_eq!(before, convert::sha256_file(&src).unwrap());
    assert!(result.source_unchanged);

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&out).ok();
}

#[test]
fn an_existing_output_is_never_overwritten() {
    let dir = workdir("clobber");
    let out = workdir("clobber-out");
    let src = write_docx_file(&dir, "aynı.docx");

    let first = convert::convert_file_to(&src, &out);
    let first_path = PathBuf::from(first.output.as_ref().unwrap());
    let first_bytes = std::fs::read(&first_path).unwrap();

    let second = convert::convert_file_to(&src, &out);
    let second_path = PathBuf::from(second.output.as_ref().unwrap());

    assert_ne!(
        first_path, second_path,
        "ikinci çıktı birincinin üzerine yazmamalı"
    );
    assert!(first_path.is_file(), "ilk çıktı hâlâ yerinde olmalı");
    assert_eq!(
        first_bytes,
        std::fs::read(&first_path).unwrap(),
        "ilk çıktının içeriği değişmemeli"
    );

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&out).ok();
}

#[test]
fn a_hostile_archive_is_refused_without_writing_anything() {
    let dir = workdir("hostile");
    let out = workdir("hostile-out");
    // Geçerli bir ZIP değil: motor bunu reddetmeli ve hiçbir şey yazmamalı.
    let src = dir.join("bozuk.docx");
    std::fs::write(&src, b"PK\x03\x04 bu bir DOCX degil").unwrap();

    let result = convert::convert_file_to(&src, &out);

    assert_eq!(result.status, Status::Failure);
    assert!(result.output.is_none(), "hatada çıktı yolu bildirilmemeli");
    let produced: Vec<_> = std::fs::read_dir(&out).unwrap().flatten().collect();
    assert!(produced.is_empty(), "hatada hiçbir dosya yazılmamalı");
    // Kaynak dokunulmamış olmalı ve motor bunu açıkça söylemeli.
    assert!(result.error.as_ref().unwrap().source_untouched);

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&out).ok();
}

#[test]
fn a_batch_converts_every_file_and_leaves_all_sources_unchanged() {
    let dir = workdir("batch");
    let out = workdir("batch-out");
    let a = write_docx_file(&dir, "bir.docx");
    let b = write_udf_file(&dir, "iki.udf");
    let hashes = [
        convert::sha256_file(&a).unwrap(),
        convert::sha256_file(&b).unwrap(),
    ];

    let result = convert::convert_batch_to(&[a.clone(), b.clone()], &out, Some("2026-09-07_0100"));

    assert_eq!(result.total, 2);
    assert_eq!(result.failed, 0, "toplu işte hata olmamalı");
    assert!(result.all_sources_unchanged);
    assert_eq!(hashes[0], convert::sha256_file(&a).unwrap());
    assert_eq!(hashes[1], convert::sha256_file(&b).unwrap());

    // Karışık yön: biri UDF'ye, diğeri DOCX'e gitmeli.
    let dirs: Vec<_> = result.items.iter().map(|i| i.direction).collect();
    assert!(dirs.contains(&Direction::DocxToUdf));
    assert!(dirs.contains(&Direction::UdfToDocx));

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&out).ok();
}
