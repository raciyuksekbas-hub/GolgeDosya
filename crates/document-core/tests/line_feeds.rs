//! Satır sonu karakterleri: UDF tamponunda yalnız paragraf sonlandırıcısı olur.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (gerçek derlemde ölçüldü, burada
//! sentetik olarak kurulur):
//! - Word → UDF: `w:t` içindeki ham LF/CR UDF tamponuna olduğu gibi gidiyordu
//!   (gerçek 199 Word belgesinin 14'ünde, 43 LF). UDF'te LF paragraf
//!   sonlandırıcısıdır: öğe ağacında olmayan bir paragraf açar. CR ise uyumlu
//!   her XML ayrıştırıcısında (UYAP'ın Java ayrıştırıcısı dahil) satır sonu
//!   normalleştirmesiyle silinir ve sonraki BÜTÜN ofsetleri bir kaydırır.
//!   Word bu karakterleri boşluk olarak gösterir; öyle aktarılır.
//! - UDF → Word: paragraf içindeki LF `w:t` içine ham yazılıyordu (Word'de
//!   boşluk görünür; 64 belgede 89 LF), paragraf sonundaki fazladan LF ise
//!   sonlandırıcıyla birlikte siliniyordu (48 belgede 1–5 LF kayıp). Artık
//!   yalnız BİR sonlandırıcı silinir; kalan her LF gerçek satır sonudur
//!   (`<w:br/>`).
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::model::*;
use tavzih_core::warnings::WarningSink;
use tavzih_core::{docx, udf};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx_package(body: &str) -> Vec<u8> {
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        z.start_file("word/document.xml", o).unwrap();
        z.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#
            )
            .as_bytes(),
        )
        .unwrap();
        z.finish().unwrap();
    }
    cur.into_inner()
}

/// Tek paragraflı UDF; `leaves` metinleri tamponda art arda durur.
fn udf_package(leaves: &[&str]) -> Vec<u8> {
    let mut buf = String::new();
    let mut el = String::new();
    let mut at = 0;
    for t in leaves {
        let len = t.encode_utf16().count();
        el.push_str(&format!(
            "<content startOffset=\"{at}\" length=\"{len}\" />"
        ));
        buf.push_str(t);
        at += len;
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<template format_id=\"1.8\" >\n\
         <content><![CDATA[{buf}]]></content>\n\
         <elements resolver=\"hvl-default\" ><paragraph>{el}</paragraph></elements>\n\
         <styles><style name=\"hvl-default\" family=\"Times New Roman\" size=\"12\" /></styles>\n</template>"
    );
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        z.start_file("content.xml", o).unwrap();
        z.write_all(xml.as_bytes()).unwrap();
        z.finish().unwrap();
    }
    cur.into_inner()
}

fn part(zip_bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn buffer(content_xml: &str) -> String {
    let start = content_xml.find("<![CDATA[").unwrap() + 9;
    let end = content_xml[start..].find("]]></content>").unwrap() + start;
    content_xml[start..end].to_string()
}

/// Gövde paragrafları: metin, satır sonu `⏎`.
fn shapes(d: &Document) -> Vec<String> {
    d.sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(
                p.runs
                    .iter()
                    .map(|r| match r {
                        Run::Text { text, .. } => text.clone(),
                        Run::Tab { .. } => "\t".into(),
                        Run::LineBreak { .. } => "⏎".into(),
                        Run::Image(_) => "[R]".into(),
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

// ------------------------------------------------------------------ Word → UDF

#[test]
fn line_feeds_inside_word_text_never_reach_the_udf_buffer_raw() {
    // Word'ün kendisi bu karakterleri boşluk olarak gösterir.
    let body = "<w:p><w:r><w:t xml:space=\"preserve\">Ad\nSoyad</w:t></w:r></w:p>\
                <w:p><w:r><w:t xml:space=\"preserve\">x&#13;\ny&#13;z</w:t></w:r></w:p>\
                <w:p><w:r><w:t>SONRAKİ</w:t></w:r></w:p>";
    let mut w = WarningSink::new();
    let out = convert_bytes(&docx_package(body), Direction::DocxToUdf, &mut w).unwrap();
    let xml = part(&out, "content.xml");
    let buf = buffer(&xml);
    assert!(!buf.contains('\r'), "CR tamponda: ofsetler kayar");
    assert_eq!(
        buf.matches('\n').count(),
        xml.matches("<paragraph").count(),
        "LF yalnız paragraf sonlandırıcısı olmalı"
    );
    let mut w2 = WarningSink::new();
    let back = udf::reader::read_udf(&out, &mut w2).unwrap();
    assert_eq!(shapes(&back), vec!["Ad Soyad", "x y z", "SONRAKİ"]);
}

#[test]
fn the_word_reader_shows_line_feeds_as_word_does() {
    let body = "<w:p><w:r><w:t xml:space=\"preserve\">Ad\r\nSoyad</w:t></w:r></w:p>";
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(&docx_package(body), &mut w).unwrap();
    assert_eq!(shapes(&d), vec!["Ad Soyad"]);
}

#[test]
fn the_udf_validator_refuses_a_carriage_return_or_a_stray_line_feed() {
    let d = Document {
        meta: Metadata::default(),
        sections: vec![Section {
            blocks: vec![Block::Paragraph(Paragraph::plain("abc"))],
            ..Default::default()
        }],
        lists: vec![],
    };
    let mut w = WarningSink::new();
    let xml = String::from_utf8(udf::writer::write_content_xml(&d, &mut w).unwrap()).unwrap();
    udf::writer::validate(xml.as_bytes()).unwrap();
    // Aynı uzunlukta bozulmalar: döşeme sağlam, ama tampon UYAP'ta yanlış okunur.
    let cr = xml.replacen("abc\n", "a\rc\n", 1);
    assert!(
        udf::writer::validate(cr.as_bytes()).is_err(),
        "CR kabul edildi"
    );
    let lf = xml.replacen("abc\n", "a\nc\n", 1);
    assert!(
        udf::writer::validate(lf.as_bytes()).is_err(),
        "fazla LF kabul edildi"
    );
}

// ------------------------------------------------------------------ UDF → Word

#[test]
fn an_interior_line_feed_becomes_a_real_word_line_break() {
    let mut w = WarningSink::new();
    let u = udf_package(&["Birinci satır\nİkinci satır\n"]);
    let d = udf::reader::read_udf(&u, &mut w).unwrap();
    assert_eq!(shapes(&d), vec!["Birinci satır⏎İkinci satır"]);
    let out = convert_bytes(&u, Direction::UdfToDocx, &mut w).unwrap();
    let x = part(&out, "word/document.xml");
    assert!(x.contains("<w:br/>"), "{x}");
    let raw_lf_in_text = x
        .split("<w:t")
        .skip(1)
        .any(|t| t[..t.find("</w:t>").unwrap()].contains('\n'));
    assert!(!raw_lf_in_text, "w:t içinde ham LF: {x}");
}

#[test]
fn only_one_paragraph_terminator_is_trimmed() {
    // Gerçek UYAP biçimi: fazladan LF kendi yaprağında, sonlandırıcı da öyle.
    let mut w = WarningSink::new();
    let d = udf::reader::read_udf(&udf_package(&["Ad Soyad", "\n", "\n"]), &mut w).unwrap();
    assert_eq!(shapes(&d), vec!["Ad Soyad⏎"]);
    // Yalnız satır sonlarından oluşan paragraf: iki satır, bir satır sonu.
    let d = udf::reader::read_udf(&udf_package(&["\n\n"]), &mut w).unwrap();
    assert_eq!(shapes(&d), vec!["⏎"]);
    // Sıradan paragraf değişmez.
    let d = udf::reader::read_udf(&udf_package(&["Metin\n"]), &mut w).unwrap();
    assert_eq!(shapes(&d), vec!["Metin"]);
}
