//! Terim düzeltme önerisi gerçekten yazılır (GölgeDosya saha maddesi 36).
//!
//! Kuralın kendi önerisi alınır, kopyaya yazılır ve kopya yeniden okunur.
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};
use std::io::{Cursor, Write};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

fn docx(paragraphs: &[&str]) -> Vec<u8> {
    let body: String = paragraphs
        .iter()
        .map(|p| format!("<w:p><w:r><w:t>{p}</w:t></w:r></w:p>"))
        .collect();
    let xml = format!("<w:document {NS}><w:body>{body}</w:body></w:document>");
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buffer);
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("word/document.xml", o).unwrap();
        w.write_all(xml.as_bytes()).unwrap();
        w.finish().unwrap();
    }
    buffer.into_inner()
}

#[test]
fn the_suggested_spelling_is_written_to_the_copy() {
    let bytes = docx(&[
        "Taraflar arasındaki sözleşme feshedilmiştir.",
        "Ancak sozlesme hükümleri uygulanmamıştır.",
    ]);
    let d = parser::parse("dilekçe.docx", &bytes).unwrap();
    let dict = ikincigoz_core::dict::UserDictionary::default();
    let options = LintOptions::default();
    let ctx = Context::new(&d, &dict, None, &options);
    let fixes: Vec<_> = analyze(&ctx)
        .findings
        .into_iter()
        .filter(|f| f.rule_id == "CONSISTENCY_TERM_SPELLING")
        .filter_map(|f| f.fix)
        .collect();
    assert_eq!(fixes.len(), 1);
    let written =
        ikincigoz_core::writeback::apply("dilekçe.docx", &bytes, &fixes).expect("yazılabilir");
    let copy = parser::parse("dilekçe.docx", &written.bytes).unwrap();
    assert_eq!(
        copy.blocks[1].text,
        "Ancak sözleşme hükümleri uygulanmamıştır."
    );
    // Kaynak değişmedi; yeniden okunan KOPYADIR.
    assert_eq!(
        parser::parse("dilekçe.docx", &bytes).unwrap().blocks[1].text,
        "Ancak sozlesme hükümleri uygulanmamıştır."
    );
}
