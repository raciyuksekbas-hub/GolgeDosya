//! Vurgulama (metin arka planı) her iki yönde taşınır; kesirli punto söylenir.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (gerçek derlemde ölçüldü, burada
//! sentetik olarak kurulur):
//! - Word'ün vurgulaması (`w:highlight`) ve run gölgelemesi (`w:rPr/w:shd`)
//!   sessizce düşüyordu: 12 gerçek Word belgesinde 64 vurgulu run. UDF bunu
//!   taşıyabilir: imzalı UYAP belgelerinde `content@background` var (-256 =
//!   sarı, Swing `StyleConstants.Background`). UDF → Word yönünde de okunmuyordu.
//! - Kesirli punto (11,5) UDF'e sessizce tam sayıya yuvarlanıyordu. Gerçek
//!   293 UDF'te tek bir kesirli `size` yok ve Swing'in yazı boyutu tamsayıdır;
//!   UYAP'ın "10.5"i okuyacağı doğrulanmadan yazılmaz: yuvarlanır ve SÖYLENİR.
//! - UDF paragraf kenarlığı (`paragraph@border="borderCell"` + `borderSpec`,
//!   `borderStyle`, `borderWidth`, `borderColor`; gerçek bir belgede 111
//!   paragraf) sessizce düşüyordu. `borderSpec`'in kodlaması tek örnekten
//!   çözülemez; tahminle `w:pBdr` yazılmaz, kayıp SÖYLENİR.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::model::*;
use tavzih_core::warnings::{WarningCode, WarningSink};
use tavzih_core::{docx, udf};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx_package(body: &str, styles: &str) -> Vec<u8> {
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
        z.start_file("word/styles.xml", o).unwrap();
        z.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{W}">{styles}</w:styles>"#
            )
            .as_bytes(),
        )
        .unwrap();
        z.finish().unwrap();
    }
    cur.into_inner()
}

fn udf_package(leaves: &[(&str, &str)]) -> Vec<u8> {
    udf_package_with("", leaves)
}

/// `para_attrs`: tek paragrafın öznitelikleri.
fn udf_package_with(para_attrs: &str, leaves: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = String::new();
    let mut el = String::new();
    let mut at = 0;
    for (i, (attrs, text)) in leaves.iter().enumerate() {
        let mut t = text.to_string();
        if i + 1 == leaves.len() {
            t.push('\n');
        }
        let len = t.encode_utf16().count();
        el.push_str(&format!(
            "<content{attrs} startOffset=\"{at}\" length=\"{len}\" />"
        ));
        buf.push_str(&t);
        at += len;
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<template format_id=\"1.8\" >\n\
         <content><![CDATA[{buf}]]></content>\n\
         <elements resolver=\"hvl-default\" ><paragraph{para_attrs}>{el}</paragraph></elements>\n\
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

/// `<content ...>` etiketleri, sırayla.
fn content_tags(content_xml: &str) -> Vec<String> {
    let el = &content_xml[content_xml.find("<elements").unwrap()..];
    el.split("<content")
        .skip(1)
        .map(|t| t[..t.find("/>").unwrap()].to_string())
        .collect()
}

const STYLES: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Kutulu"><w:name w:val="Kutulu"/><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="D9D9D9"/></w:pPr></w:style><w:style w:type="character" w:styleId="Isaretli"><w:name w:val="Isaretli"/><w:rPr><w:highlight w:val="green"/></w:rPr></w:style>"#;

// ------------------------------------------------------------------ Word → UDF

#[test]
fn word_highlight_and_run_shading_become_udf_background() {
    let body = r#"<w:p><w:r><w:rPr><w:highlight w:val="yellow"/></w:rPr><w:t>sarı</w:t></w:r><w:r><w:t xml:space="preserve"> düz </w:t></w:r><w:r><w:rPr><w:shd w:val="clear" w:color="auto" w:fill="FFC000"/></w:rPr><w:t>turuncu</w:t></w:r><w:r><w:rPr><w:rStyle w:val="Isaretli"/></w:rPr><w:t>yeşil</w:t></w:r></w:p>"#;
    let mut w = WarningSink::new();
    let out = convert_bytes(&docx_package(body, STYLES), Direction::DocxToUdf, &mut w).unwrap();
    let tags = content_tags(&part(&out, "content.xml"));
    assert!(tags[0].contains(" background=\"-256\""), "{tags:?}");
    assert!(!tags[1].contains("background"), "{tags:?}");
    let orange = Color::from_hex("FFC000").unwrap().to_java_argb();
    assert!(
        tags[2].contains(&format!(" background=\"{orange}\"")),
        "{tags:?}"
    );
    let green = Color::from_hex("00FF00").unwrap().to_java_argb();
    assert!(
        tags[3].contains(&format!(" background=\"{green}\"")),
        "{tags:?}"
    );
}

#[test]
fn paragraph_shading_is_not_a_run_background_and_none_clears() {
    // `w:pPr/w:shd` paragraf kutusudur, metin vurgulaması değil. `none` vurgu yok demektir.
    // Beyaz run gölgesi (webden yapıştırılmış metin) sayfanın kendi rengidir.
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Kutulu"/></w:pPr><w:r><w:rPr><w:highlight w:val="none"/></w:rPr><w:t>metin</w:t></w:r><w:r><w:rPr><w:shd w:val="clear" w:color="auto" w:fill="FFFFFF"/></w:rPr><w:t xml:space="preserve"> beyaz</w:t></w:r></w:p>"#;
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(&docx_package(body, STYLES), &mut w).unwrap();
    let Block::Paragraph(p) = &d.sections[0].blocks[0] else {
        panic!()
    };
    assert_eq!(p.runs[0].props().unwrap().background, None);
    assert_eq!(p.runs[1].props().unwrap().background, None);
}

#[test]
fn a_fractional_point_size_is_rounded_and_said() {
    let body = r#"<w:p><w:r><w:rPr><w:sz w:val="23"/></w:rPr><w:t>11,5 punto</w:t></w:r></w:p>"#;
    let mut w = WarningSink::new();
    let out = convert_bytes(&docx_package(body, STYLES), Direction::DocxToUdf, &mut w).unwrap();
    assert!(content_tags(&part(&out, "content.xml"))[0].contains(" size=\"12\""));
    let warns = w.into_vec();
    assert!(
        warns
            .iter()
            .any(|x| x.code == WarningCode::RunFeatureDropped
                && x.detail.as_deref().is_some_and(|d| d.contains("kesirli"))),
        "{warns:?}"
    );
}

#[test]
fn a_fractional_default_size_is_said_too() {
    let styles = format!(
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="21"/></w:rPr></w:rPrDefault></w:docDefaults>{STYLES}"#
    );
    let body = r#"<w:p><w:r><w:t>10,5 punto gövde</w:t></w:r></w:p>"#;
    let mut w = WarningSink::new();
    convert_bytes(&docx_package(body, &styles), Direction::DocxToUdf, &mut w).unwrap();
    assert!(w
        .into_vec()
        .iter()
        .any(|x| x.code == WarningCode::RunFeatureDropped
            && x.detail.as_deref().is_some_and(|d| d.contains("kesirli"))));
}

#[test]
fn a_double_underline_is_kept_as_single_and_said() {
    // Swing'in (UDF) tek alt çizgisi var: çift/kalın/noktalı çizgi tek çizgi olur.
    // Altı çizili kalır, ama biçim farkı söylenir. Düz tek çizgi uyarı üretmez.
    let mut w = WarningSink::new();
    let body = r#"<w:p><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>tek</w:t></w:r></w:p>"#;
    convert_bytes(&docx_package(body, STYLES), Direction::DocxToUdf, &mut w).unwrap();
    assert!(w.is_empty(), "{:?}", w.into_vec());

    let mut w = WarningSink::new();
    let body = r#"<w:p><w:r><w:rPr><w:u w:val="double"/></w:rPr><w:t>çift</w:t></w:r></w:p>"#;
    let out = convert_bytes(&docx_package(body, STYLES), Direction::DocxToUdf, &mut w).unwrap();
    assert!(content_tags(&part(&out, "content.xml"))[0].contains(" underline=\"true\""));
    let warns = w.into_vec();
    assert!(
        warns
            .iter()
            .any(|x| x.code == WarningCode::RunFeatureDropped
                && x.detail.as_deref().is_some_and(|d| d.contains("alt çizgi"))),
        "{warns:?}"
    );
}

// ------------------------------------------------------------------ UDF → Word

#[test]
fn udf_background_becomes_word_highlight_or_shading() {
    let grey = Color::from_hex("CCCCCC").unwrap().to_java_argb();
    let u = udf_package(&[
        (" background=\"-256\"", "sarı"),
        ("", " düz "),
        (&format!(" background=\"{grey}\""), "gri"),
    ]);
    let mut w = WarningSink::new();
    let d = udf::reader::read_udf(&u, &mut w).unwrap();
    let Block::Paragraph(p) = &d.sections[0].blocks[0] else {
        panic!()
    };
    assert_eq!(
        p.runs[0].props().unwrap().background,
        Color::from_hex("FFFF00")
    );
    let x = part(
        &convert_bytes(&u, Direction::UdfToDocx, &mut w).unwrap(),
        "word/document.xml",
    );
    // Word'ün adlı vurgu rengi tam eşleşirse vurgulayıcı, değilse gölgeleme.
    assert!(x.contains(r#"<w:highlight w:val="yellow"/>"#), "{x}");
    assert!(x.contains(r#"w:fill="CCCCCC""#), "{x}");
    assert_eq!(x.matches("<w:highlight").count(), 1);
}

#[test]
fn a_udf_paragraph_border_is_reported_not_silently_dropped() {
    let boxed = udf_package_with(
        " border=\"borderCell\" borderSpec=\"31\" borderStyle=\"borderStyle-plain\" borderWidth=\"1.0\" borderColor=\"-16777216\"",
        &[("", "Kutulu paragraf")],
    );
    let mut w = WarningSink::new();
    convert_bytes(&boxed, Direction::UdfToDocx, &mut w).unwrap();
    let warns = w.into_vec();
    assert!(
        warns
            .iter()
            .any(|x| x.code == WarningCode::RunFeatureDropped
                && x.detail.as_deref() == Some("paragraf kenarlığı")),
        "{warns:?}"
    );
    // Kenarlıksız paragraf uyarı üretmez.
    let plain = udf_package_with(" border=\"borderNone\"", &[("", "Düz paragraf")]);
    let mut w = WarningSink::new();
    convert_bytes(&plain, Direction::UdfToDocx, &mut w).unwrap();
    assert!(w.is_empty(), "{:?}", w.into_vec());
}

#[test]
fn a_half_point_size_from_udf_is_not_rounded_for_word() {
    // Koruma: UDF'te 10.5 yazılmışsa Word w:sz=21 alır (Word yarım puntoyu taşır).
    let u = udf_package(&[(" size=\"10.5\"", "küçük")]);
    let mut w = WarningSink::new();
    let x = part(
        &convert_bytes(&u, Direction::UdfToDocx, &mut w).unwrap(),
        "word/document.xml",
    );
    assert!(x.contains(r#"<w:sz w:val="21"/>"#), "{x}");
}
