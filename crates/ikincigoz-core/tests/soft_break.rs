//! Bir satır sonu "çok boşluk" değildir (GölgeDosya saha maddesi 35).
//!
//! `<w:br/>` ayrıştırıcıda sentetik bir boşluk olur. Önündeki gerçek boşlukla
//! birlikte "2 boşluk" diye hata veriyordu: Word'de kullanıcı orada bir SATIR
//! SONU görür. Önerilen düzeltme yazılamıyordu (Unaddressable) ve yazım hep-ya-da-
//! hiç olduğu için seçilen bütün düzeltmeleri birlikte düşürüyordu.
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};
use std::io::{Cursor, Write};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

fn docx(body: &str) -> Vec<u8> {
    let xml = format!("<w:document {NS}><w:body>{body}</w:body></w:document>");
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buffer);
        let o = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        w.start_file("word/document.xml", o).unwrap();
        w.write_all(xml.as_bytes()).unwrap();
        w.finish().unwrap();
    }
    buffer.into_inner()
}

fn multiple_spaces(bytes: &[u8]) -> Vec<ikincigoz_core::finding::Finding> {
    let d = parser::parse("dilekçe.docx", bytes).unwrap();
    let dict = ikincigoz_core::dict::UserDictionary::default();
    let options = LintOptions::default();
    let ctx = Context::new(&d, &dict, None, &options);
    analyze(&ctx)
        .findings
        .into_iter()
        .filter(|f| f.rule_id == "TYPO_MULTIPLE_SPACES")
        .collect()
}

#[test]
fn a_space_before_a_line_break_is_not_a_double_space() {
    let bytes = docx(
        r#"<w:p><w:r><w:t xml:space="preserve">Arz ederim. Saygıyla sunarız. </w:t><w:br/><w:t>Saygılarımızla</w:t></w:r></w:p>"#,
    );
    assert!(multiple_spaces(&bytes).is_empty());
}

#[test]
fn a_real_double_space_is_still_found_and_its_fix_writes_back() {
    let bytes = docx(
        r#"<w:p><w:r><w:t xml:space="preserve">Davacı  vekili sunar. </w:t><w:br/><w:t>Saygılarımızla</w:t></w:r></w:p>"#,
    );
    let found = multiple_spaces(&bytes);
    assert_eq!(found.len(), 1, "{found:#?}");
    let fix = found[0]
        .fix
        .clone()
        .expect("gerçek çift boşluğun düzeltmesi var");
    let written =
        ikincigoz_core::writeback::apply("dilekçe.docx", &bytes, &[fix]).expect("yazılabilir");
    let reparsed = parser::parse("dilekçe.docx", &written.bytes).unwrap();
    assert!(reparsed.blocks[0].text.starts_with("Davacı vekili sunar."));
}
