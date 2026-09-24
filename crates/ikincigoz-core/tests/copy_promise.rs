//! Ürün metni motorun yapabildiğini aşmaz (GölgeDosya saha maddesi 37).
//!
//! Denetle'nin karşılama metni "Yazım, noktalama ve tutarlılık…" diye
//! başlıyordu; motor genel yazım denetimi yapmaz. Bu test metni DAVRANIŞA
//! bağlar: metin yeniden "yazım" vaat ederse, motor sık görülen yazım
//! yanlışlarının en az %80'ini bulmak zorundadır. Bugün 0/10 bulur, yani metin
//! vaat etmemelidir.
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};
use std::io::{Cursor, Write};
use std::path::Path;

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

/// Bellekte tek paragraflı bir DOCX; gerçek ayrıştırıcı ve motor koşar.
fn orthography_findings(paragraph: &str) -> usize {
    let xml = format!("<w:document {NS}><w:body><w:p><w:r><w:t>{paragraph}</w:t></w:r></w:p></w:body></w:document>");
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buffer);
        w.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        w.write_all(xml.as_bytes()).unwrap();
        w.finish().unwrap();
    }
    let d = parser::parse("dilekçe.docx", &buffer.into_inner()).unwrap();
    let dict = ikincigoz_core::dict::UserDictionary::default();
    let options = LintOptions::default();
    let ctx = Context::new(&d, &dict, None, &options);
    analyze(&ctx)
        .findings
        .iter()
        .filter(|f| f.rule_id.starts_with("ORTHO"))
        .count()
}

fn denetle_hint() -> String {
    let modes =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/belge-shell/src/shell/modes.ts");
    let text = std::fs::read_to_string(modes).expect("modes.ts okunur");
    let at = text.find("ikincigoz: {").expect("Denetle kipi");
    let block = &text[at..];
    let hint = block.find("hint:").expect("açıklama");
    let rest = &block[hint..];
    let open = rest.find('"').unwrap() + 1;
    let close = open + rest[open..].find('"').unwrap();
    rest[open..close].to_string()
}

/// Sık görülen, sözlükte karşılığı olmayan yazım yanlışları (gerçek kelime değil).
const MISSPELLINGS: [&str; 10] = [
    "Davacı yanlız başına dava açmıştır.",
    "Bu durumu herkez bilmektedir.",
    "Malesef davalı ödeme yapmamıştır.",
    "Belgenin orjinali dosyaya sunulmuştur.",
    "Bu yalnış bir değerlendirmedir.",
    "Davacı müracat etmiştir.",
    "Tabiki bu talep kabul edilmelidir.",
    "Olay şöyleki gerçekleşmiştir.",
    "Dosyada pekçok eksiklik vardır.",
    "Talebi yerindedir, sonuçda kabul edilmelidir.",
];

#[test]
fn the_welcome_text_does_not_promise_spelling_the_engine_does_not_check() {
    let hint = denetle_hint();
    let promises = hint.to_lowercase().contains("yazım") || hint.contains("Yazım");
    if !promises {
        return;
    }
    let found = MISSPELLINGS
        .iter()
        .filter(|p| orthography_findings(p) > 0)
        .count();
    assert!(
        found * 10 >= MISSPELLINGS.len() * 8,
        "Denetle metni \"{hint}\" yazım denetimi vaat ediyor ama motor {found}/{} yazım yanlışı buluyor",
        MISSPELLINGS.len()
    );
}

/// Testin kendisi ayırt edici mi: bugünkü motor bu yanlışların hiçbirini bulmaz.
/// (Motor bir gün gerçekten yazım denetimi kazanırsa bu ölçüm değişir ve metin
/// yeniden "yazım" diyebilir.)
#[test]
fn todays_engine_finds_none_of_the_common_misspellings() {
    let found = MISSPELLINGS
        .iter()
        .filter(|p| orthography_findings(p) > 0)
        .count();
    assert_eq!(
        found, 0,
        "motor artık yazım yanlışı buluyor; metni ve bu testi güncelleyin"
    );
}
