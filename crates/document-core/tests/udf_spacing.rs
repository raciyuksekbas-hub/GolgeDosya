//! Word → UDF: varsayılan dilekçe aralığı.
//!
//! Saha: "Varsayılan olarak dönüştürülen UDF dosyalarında satır aralığı 1,5;
//! öncesinde ve sonrasında boşluk olmamalı." Varsayılan (`UdfSpacing::Petition`)
//! gövdedeki ve tablo hücrelerindeki her paragrafı 1,5 satır aralığına ve
//! sıfır paragraf boşluğuna getirir. Üst/alt bilgi dokunulmadan kalır.
//! `UdfSpacing::Source` Word'deki aralıkları olduğu gibi taşır (sözleşme gibi
//! sayfa akışı korunması gereken belgeler için).
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur.

use std::io::{Read, Write};

use tavzih_core::convert::{
    convert_bytes, convert_bytes_with, ConvertOptions, Direction, UdfSpacing,
};
use tavzih_core::warnings::WarningSink;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// 1,15 satır aralığı, 12 pt önce, 6 pt sonra.
const SPACED: &str =
    r#"<w:pPr><w:spacing w:before="240" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>"#;

fn package(body: &str, header: Option<&str>) -> Vec<u8> {
    let sect = if header.is_some() {
        r#"<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr>"#
    } else {
        ""
    };
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{body}{sect}</w:body></w:document>"#
    );
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        z.start_file("word/document.xml", o).unwrap();
        z.write_all(document.as_bytes()).unwrap();
        if let Some(header) = header {
            z.start_file("word/_rels/document.xml.rels", o).unwrap();
            z.write_all(
                format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH" Type="{R}/header" Target="header1.xml"/></Relationships>"#)
                    .as_bytes(),
            )
            .unwrap();
            z.start_file("word/header1.xml", o).unwrap();
            z.write_all(
                format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr xmlns:w="{W}">{header}</w:hdr>"#).as_bytes(),
            )
            .unwrap();
        }
        z.finish().unwrap();
    }
    cur.into_inner()
}

fn para(text: &str) -> String {
    format!(r#"<w:p>{SPACED}<w:r><w:t>{text}</w:t></w:r></w:p>"#)
}

fn content(udf: Vec<u8>) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf)).unwrap();
    let mut s = String::new();
    z.by_name("content.xml")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

fn to_udf(docx: &[u8], spacing: Option<UdfSpacing>) -> String {
    let mut w = WarningSink::new();
    let udf = match spacing {
        None => convert_bytes(docx, Direction::DocxToUdf, &mut w),
        Some(udf_spacing) => convert_bytes_with(
            docx,
            Direction::DocxToUdf,
            &ConvertOptions {
                udf_spacing,
                ..Default::default()
            },
            &mut w,
        ),
    }
    .expect("convert");
    content(udf)
}

/// `<paragraph …>` açılış etiketleri; `elements` (gövde) ve `header` ayrı.
fn paragraph_tags(xml: &str, part: &str) -> Vec<String> {
    let start = xml
        .find(&format!("<{part}"))
        .unwrap_or_else(|| panic!("{part} yok: {xml}"));
    let end = xml[start..]
        .find(&format!("</{part}>"))
        .map(|e| start + e)
        .unwrap_or(xml.len());
    let mut out = Vec::new();
    let mut rest = &xml[start..end];
    while let Some(i) = rest.find("<paragraph") {
        let close = rest[i..].find('>').expect("kapanmayan etiket") + i + 1;
        out.push(rest[i..close].to_string());
        rest = &rest[close..];
    }
    out
}

#[test]
fn the_default_writes_one_and_a_half_lines_and_no_paragraph_spacing() {
    let xml = to_udf(
        &package(&format!("{}{}", para("Birinci"), para("İkinci")), None),
        None,
    );
    let tags = paragraph_tags(&xml, "elements");
    assert_eq!(tags.len(), 2, "{xml}");
    for tag in &tags {
        assert!(
            tag.contains(r#"LineSpacing="0.5""#),
            "1,5 satır değil: {tag}"
        );
        assert!(
            !tag.contains("SpaceAbove"),
            "paragraf öncesi boşluk kaldı: {tag}"
        );
        assert!(
            !tag.contains("SpaceBelow"),
            "paragraf sonrası boşluk kaldı: {tag}"
        );
    }
}

#[test]
fn paragraphs_inside_tables_follow_the_same_spacing() {
    let table = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr/>{}</w:tc></w:tr></w:tbl>"#,
        para("Hücre")
    );
    let xml = to_udf(&package(&table, None), None);
    let tags = paragraph_tags(&xml, "elements");
    assert!(!tags.is_empty());
    for tag in &tags {
        assert!(tag.contains(r#"LineSpacing="0.5""#), "{tag}");
        assert!(
            !tag.contains("SpaceAbove") && !tag.contains("SpaceBelow"),
            "{tag}"
        );
    }
}

#[test]
fn the_header_keeps_its_own_spacing() {
    let header =
        r#"<w:p><w:pPr><w:spacing w:after="240"/></w:pPr><w:r><w:t>Antet</w:t></w:r></w:p>"#;
    let xml = to_udf(&package(&para("Gövde"), Some(header)), None);
    let tags = paragraph_tags(&xml, "header");
    assert!(!tags.is_empty(), "üst bilgi yazılmadı: {xml}");
    assert!(
        tags.iter().all(|t| t.contains(r#"SpaceBelow="12.0""#)),
        "{tags:?}"
    );
    assert!(tags.iter().all(|t| !t.contains("LineSpacing")), "{tags:?}");
}

#[test]
fn source_spacing_keeps_what_word_had() {
    let xml = to_udf(&package(&para("Kaynak"), None), Some(UdfSpacing::Source));
    let tag = &paragraph_tags(&xml, "elements")[0];
    assert!(tag.contains(r#"SpaceAbove="12.0""#), "{tag}");
    assert!(tag.contains(r#"SpaceBelow="6.0""#), "{tag}");
    assert!(tag.contains(r#"LineSpacing="0.15""#), "{tag}");
}

#[test]
fn the_default_is_the_petition_spacing() {
    assert_eq!(ConvertOptions::default().udf_spacing, UdfSpacing::Petition);
}
