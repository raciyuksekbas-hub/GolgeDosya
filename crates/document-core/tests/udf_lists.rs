//! UDF listeleri Word'de UYAP'taki işaretleriyle ve kendi sayaçlarıyla çıkmalı.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (293 gerçek UDF üzerinde ölçüldü,
//! burada sentetik olarak kurulur):
//! - `numbering.xml` her düzeye "%1." yazıyordu. `%1` birinci düzeyin
//!   sayacıdır: gerçek 5 UDF'te 291 alt madde Word'de üst maddenin
//!   numarasını gösteriyordu ("1. / 1. / 1. / 2.").
//! - Gerçek dosyalarda görülen yedi liste türü ("I.", "a.", "1)", "1-", ok ve
//!   üçgen imi…) sessizce "1." ya da yuvarlak madde imi oluyordu; 21 belgede
//!   220 paragraf. "a) bendi" gibi atıflar boşa düşüyordu.
//! - Tür listenin ilk paragrafından bir kez seçiliyordu; UDF'te tür
//!   paragrafın, Word'de düzeyin özelliğidir.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::warnings::{Warning, WarningCode, WarningSink};

/// (paragraf öznitelikleri, metin) satırlarından bir UDF.
fn udf(paragraphs: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = String::new();
    let mut elems = String::new();
    for (attrs, text) in paragraphs {
        let start = buf.encode_utf16().count();
        buf.push_str(text);
        buf.push('\n');
        let len = text.encode_utf16().count() + 1;
        elems.push_str(&format!(
            r#"<paragraph Alignment="0" {attrs}><content startOffset="{start}" length="{len}" /></paragraph>"#
        ));
    }
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?><template format_id="1.8"><content><![CDATA[{buf}]]></content><properties><pageFormat mediaSizeName="1" leftMargin="70.875" rightMargin="70.875" topMargin="70.875" bottomMargin="70.875" paperOrientation="1" headerFOffset="20.0" footerFOffset="20.0" /></properties><elements resolver="hvl-default">{elems}</elements><styles><style name="hvl-default" family="Times New Roman" size="12" description="Gövde" /></styles></template>"#
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

/// UDF → Word: `numbering.xml` ve uyarılar.
fn to_docx(udf: &[u8]) -> (String, Vec<Warning>) {
    let mut w = WarningSink::new();
    let d = convert_bytes(udf, Direction::UdfToDocx, &mut w).expect("udf->docx");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(d)).unwrap();
    let mut s = String::new();
    z.by_name("word/numbering.xml")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    (s, w.into_vec())
}

fn list_warnings(w: &[Warning]) -> Vec<String> {
    w.iter()
        .filter(|w| w.code == WarningCode::ListNumberingApproximated)
        .map(|w| w.detail.clone().unwrap_or_default())
        .collect()
}

/// numbering.xml'den bir soyut listenin bir düzeyi: (numFmt, lvlText).
fn word_level(numbering: &str, abstract_id: u32, ilvl: u32) -> (String, String) {
    let a = &numbering[numbering
        .find(&format!(r#"w:abstractNumId="{abstract_id}""#))
        .expect("abstractNum")..];
    let l = &a[a.find(&format!(r#"<w:lvl w:ilvl="{ilvl}""#)).expect("lvl")..];
    let l = &l[..l.find("</w:lvl>").unwrap()];
    let val = |tag: &str| {
        let i = l.find(&format!("<w:{tag} w:val=\"")).expect(tag) + tag.len() + 11;
        l[i..i + l[i..].find('"').unwrap()].to_string()
    };
    (val("numFmt"), val("lvlText"))
}

#[test]
fn a_nested_udf_item_counts_on_its_own_level() {
    // Gerçek 5 UDF'te 291 alt madde: numbering.xml her düzeye "%1." yazdığı
    // için Word alt maddede üst maddenin numarasını gösteriyordu.
    let n = r#"ListId="1" Numbered="true" NumberType="NUMBER_TYPE_NUMBER_DOT""#;
    let l1 = format!(r#"{n} ListLevel="1""#);
    let l2 = format!(r#"{n} ListLevel="2""#);
    let (numbering, _) = to_docx(&udf(&[(&l1, "Üst"), (&l2, "Alt bir"), (&l2, "Alt iki")]));
    assert_eq!(word_level(&numbering, 1, 0).1, "%1.");
    assert_eq!(word_level(&numbering, 1, 1).1, "%2.");
    assert_eq!(word_level(&numbering, 1, 2).1, "%3.");
}

#[test]
fn every_list_type_seen_in_real_uyap_files_keeps_its_marker() {
    // Gerçek UDF'lerde görülen türler. "1-" (`NUMBER_TRE`, 118 paragraf) ve
    // `NUMBER_DOT_3` (40 paragraf) için UYAP görüntüsü kanıtlanamadı: en yakın
    // karşılık yazılır ve bildirilir.
    let rows = [
        (
            "1",
            "Numbered=\"true\" NumberType=\"NUMBER_TYPE_NUMBER_PARANTHESE\"",
        ),
        (
            "2",
            "Numbered=\"true\" NumberType=\"NUMBER_TYPE_CHAR_SMALL_DOT\"",
        ),
        (
            "3",
            "Numbered=\"true\" NumberType=\"NUMBER_TYPE_ROMAN_BIG_DOT\"",
        ),
        (
            "4",
            "Numbered=\"true\" NumberType=\"NUMBER_TYPE_NUMBER_TRE\"",
        ),
        (
            "5",
            "Numbered=\"true\" NumberType=\"NUMBER_TYPE_NUMBER_DOT_3\"",
        ),
        ("6", "Bulleted=\"true\" BulletType=\"BULLET_TYPE_ARROW\""),
        ("7", "Bulleted=\"true\" BulletType=\"BULLET_TYPE_TRIANGLE\""),
        (
            "8",
            "Numbered=\"true\" NumberType=\"NUMBER_TYPE_CHAR_SMALL_PARANTHESE\"",
        ),
    ];
    let attrs: Vec<String> = rows
        .iter()
        .map(|(id, a)| format!(r#"ListId="{id}" ListLevel="1" {a}"#))
        .collect();
    let paras: Vec<(&str, &str)> = attrs.iter().map(|a| (a.as_str(), "Madde")).collect();
    let (numbering, w) = to_docx(&udf(&paras));
    let lv = |id| word_level(&numbering, id, 0);
    assert_eq!(lv(1), ("decimal".into(), "%1)".into()));
    assert_eq!(lv(2), ("lowerLetter".into(), "%1.".into()));
    assert_eq!(lv(3), ("upperRoman".into(), "%1.".into()));
    assert_eq!(lv(4), ("decimal".into(), "%1-".into()));
    assert_eq!(lv(5), ("decimal".into(), "%1.".into()));
    assert_eq!(lv(6), ("bullet".into(), "\u{F0D8}".into()));
    assert_eq!(lv(7), ("bullet".into(), "\u{25BA}".into()));
    assert_eq!(lv(8), ("lowerLetter".into(), "%1)".into()));
    let mut lw = list_warnings(&w);
    lw.sort();
    assert_eq!(
        lw,
        vec![
            "NUMBER_TYPE_NUMBER_DOT_3".to_string(),
            "NUMBER_TYPE_NUMBER_TRE".to_string()
        ]
    );
}

#[test]
fn a_udf_list_with_different_types_per_level_keeps_each() {
    let (numbering, _) = to_docx(&udf(&[
        (
            r#"ListId="3" ListLevel="1" Numbered="true" NumberType="NUMBER_TYPE_NUMBER_DOT""#,
            "Üst",
        ),
        (
            r#"ListId="3" ListLevel="2" Numbered="true" NumberType="NUMBER_TYPE_CHAR_SMALL_PARANTHESE""#,
            "Alt",
        ),
    ]));
    assert_eq!(
        word_level(&numbering, 3, 0),
        ("decimal".into(), "%1.".into())
    );
    assert_eq!(
        word_level(&numbering, 3, 1),
        ("lowerLetter".into(), "%2)".into())
    );
}

#[test]
fn a_udf_start_number_whose_meaning_is_unproven_is_reported() {
    // `NumberSetted` gerçek 2 UDF'te 4 paragrafta var; değerlerin (1, 2, 3)
    // başlangıç numarası mı, önceki madde sayısı mı olduğu örneklerden
    // çıkarılamıyor. Tahmin edilmez, bildirilir.
    let (_, w) = to_docx(&udf(&[(
        r#"ListId="1" ListLevel="1" Numbered="true" NumberType="NUMBER_TYPE_NUMBER_DOT" NumberSetted="3""#,
        "Madde",
    )]));
    assert_eq!(list_warnings(&w), vec!["NumberSetted".to_string()]);
}
