//! Sekme durakları: hiza her iki yönde taşınır, Word'ün durak mirası doğru çözülür.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (gerçek derlemde ölçüldü, burada
//! sentetik olarak kurulur):
//! - UDF `TabSet` girdisi `konum:hiza:dolgu` biçimindedir (Swing `TabStop`:
//!   0 sol, 1 sağ, 2 orta, 4 ondalık). Okuyucu yalnız konumu alıyor, her durağı
//!   SOL yapıyordu: 69 belgede, sekme içeren 435 paragrafta 813 orta/sağ durak
//!   (çoğu etiket/değer bloklarındaki ortalanmış ":" sütunu) kayıptı.
//! - Yazıcı her durağı `:0:0` yazıyor ve TAB_ALIGNMENT_APPROXIMATED
//!   bildiriyordu: gerçek Word belgelerinin %53'ünde (105/199) gereksiz uyarı.
//! - Word'de durak mirası BİRLEŞİMDİR: stilin durakları ∪ paragrafın durakları,
//!   eksi `w:val="clear"` ile silinen konumlar. Okuyucu paragrafın listesiyle
//!   stilinkini DEĞİŞTİRİYOR, `clear` girdisini de sol durak yapıyordu (gerçek 7
//!   belgede 67 silinmiş durak UDF'e durak olarak gitti).
//! - Kenar payı 70,875 pt olan UYAP sayfasında metin genişliği 9071 twip; tam
//!   sağ kenardaki 9072 twip'lik durak 1 twip yuvarlamayla düşüyordu.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::model::*;
use tavzih_core::warnings::{WarningCode, WarningSink};
use tavzih_core::{docx, udf};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

// ------------------------------------------------------------------ yardımcılar

fn udf_package(tabset: &str, text: &str) -> Vec<u8> {
    let content = format!("{text}\n");
    let len = content.encode_utf16().count();
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<template format_id=\"1.8\" >\n\
         <content><![CDATA[{content}]]></content>\n\
         <properties><pageFormat mediaSizeName=\"1\" leftMargin=\"70.875\" rightMargin=\"70.875\" \
         topMargin=\"70.875\" bottomMargin=\"70.875\" paperOrientation=\"1\" headerFOffset=\"20.0\" \
         footerFOffset=\"20.0\" /></properties>\n\
         <elements resolver=\"hvl-default\" ><paragraph TabSet=\"{tabset}\"><content startOffset=\"0\" length=\"{len}\" /></paragraph></elements>\n\
         <styles><style name=\"hvl-default\" family=\"Times New Roman\" size=\"12\" description=\"Gövde\" /></styles>\n</template>"
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

fn part(zip_bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn codes(w: WarningSink) -> Vec<WarningCode> {
    w.into_vec().iter().map(|x| x.code).collect()
}

fn first_para(d: &Document) -> &Paragraph {
    d.sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) => Some(p),
            _ => None,
        })
        .expect("paragraph")
}

fn stops(p: &Paragraph) -> Vec<(i64, TabAlign)> {
    p.props
        .tab_stops
        .iter()
        .map(|t| ((t.pos_pt * 20.0).round() as i64, t.align))
        .collect()
}

/// Word'ün yerleşik "footer" stili gibi: ortada ve sağ kenarda durak.
const STYLES: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Etiket"><w:name w:val="Etiket"/><w:basedOn w:val="Normal"/><w:pPr><w:tabs><w:tab w:val="left" w:pos="1418"/><w:tab w:val="left" w:pos="2836"/></w:tabs></w:pPr></w:style><w:style w:type="paragraph" w:styleId="AltBilgi"><w:name w:val="footer"/><w:basedOn w:val="Normal"/><w:pPr><w:tabs><w:tab w:val="center" w:pos="4536"/><w:tab w:val="right" w:pos="9072"/></w:tabs></w:pPr></w:style>"#;

// ------------------------------------------------------------------ UDF → Word

#[test]
fn udf_tab_alignment_codes_are_read_as_swing_defines_them() {
    let u = udf_package(
        "100.0:0:0,200.0:2:0,300.0:1:0,350.0:4:0",
        "Ad\tSoyad\t:\tDeğer",
    );
    let mut w = WarningSink::new();
    let d = udf::reader::read_udf(&u, &mut w).unwrap();
    assert_eq!(
        stops(first_para(&d)),
        vec![
            (2000, TabAlign::Left),
            (4000, TabAlign::Center),
            (6000, TabAlign::Right),
            (7000, TabAlign::Decimal),
        ]
    );
}

#[test]
fn udf_center_and_right_stops_reach_word_without_a_warning() {
    let mut w = WarningSink::new();
    let out = convert_bytes(
        &udf_package("200.0:2:0,400.0:1:0", "Ad\t:\tDeğer"),
        Direction::UdfToDocx,
        &mut w,
    )
    .unwrap();
    let x = part(&out, "word/document.xml");
    assert!(x.contains(r#"<w:tab w:val="center" w:pos="4000"/>"#), "{x}");
    assert!(x.contains(r#"<w:tab w:val="right" w:pos="8000"/>"#), "{x}");
    // Word dört hizayı da taşır: yaklaşım değil, kesin aktarım.
    assert!(!codes(w).contains(&WarningCode::TabAlignmentApproximated));
}

#[test]
fn a_stop_on_the_uyap_right_margin_is_kept() {
    // 595,276 − 2×70,875 = 453,526 pt = 9071 twip; durak 453,6 pt = 9072 twip.
    let mut w = WarningSink::new();
    let out = convert_bytes(
        &udf_package("226.8:0:0,453.6:0:0", "Sol\tOrta\tSağ"),
        Direction::UdfToDocx,
        &mut w,
    )
    .unwrap();
    let x = part(&out, "word/document.xml");
    assert!(x.contains(r#"<w:tab w:val="left" w:pos="9072"/>"#), "{x}");
}

// ------------------------------------------------------------------ Word → UDF

#[test]
fn word_center_and_right_stops_are_written_as_udf_codes() {
    let body = r#"<w:p><w:pPr><w:tabs><w:tab w:val="left" w:pos="1440"/><w:tab w:val="center" w:pos="4536"/><w:tab w:val="right" w:pos="9072"/></w:tabs></w:pPr><w:r><w:t>Ad</w:t></w:r><w:r><w:tab/><w:t>:</w:t></w:r></w:p>"#;
    let mut w = WarningSink::new();
    let out = convert_bytes(&docx_package(body, STYLES), Direction::DocxToUdf, &mut w).unwrap();
    let x = part(&out, "content.xml");
    assert!(
        x.contains(r#"TabSet="72.0:0:0,226.8:2:0,453.6:1:0""#),
        "{}",
        &x[x.find("<paragraph").unwrap()..][..200]
    );
    assert!(!codes(w).contains(&WarningCode::TabAlignmentApproximated));
}

#[test]
fn a_decimal_stop_is_still_reported_as_approximated() {
    // UDF derleminde ondalık durak (Swing 4) hiç görülmedi: sol yazılır, söylenir.
    let body = r#"<w:p><w:pPr><w:tabs><w:tab w:val="decimal" w:pos="4536"/></w:tabs></w:pPr><w:r><w:t>Tutar</w:t></w:r></w:p>"#;
    let mut w = WarningSink::new();
    let out = convert_bytes(&docx_package(body, STYLES), Direction::DocxToUdf, &mut w).unwrap();
    assert!(part(&out, "content.xml").contains(r#"TabSet="226.8:0:0""#));
    assert!(codes(w).contains(&WarningCode::TabAlignmentApproximated));
}

#[test]
fn style_and_paragraph_stops_merge_and_clear_removes_one() {
    // Stil: 1418 ve 2836. Paragraf 2836'yı siler, 4536 ekler.
    // Word'ün etkin durakları: 1418 ve 4536. `clear` hiçbir zaman durak değildir.
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Etiket"/><w:tabs><w:tab w:val="clear" w:pos="2836"/><w:tab w:val="left" w:pos="4536"/></w:tabs></w:pPr><w:r><w:t>DAVACI</w:t></w:r></w:p>"#;
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(&docx_package(body, STYLES), &mut w).unwrap();
    assert_eq!(
        stops(first_para(&d)),
        vec![(1418, TabAlign::Left), (4536, TabAlign::Left)]
    );
}

#[test]
fn a_direct_stop_adds_to_the_inherited_footer_stops() {
    // Alt bilgi stilinin orta/sağ durakları, paragrafın kendi durağıyla BİRLİKTE geçerli.
    let body = r#"<w:p><w:pPr><w:pStyle w:val="AltBilgi"/><w:tabs><w:tab w:val="left" w:pos="1134"/></w:tabs></w:pPr><w:r><w:t>Sayfa</w:t></w:r></w:p>"#;
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(&docx_package(body, STYLES), &mut w).unwrap();
    assert_eq!(
        stops(first_para(&d)),
        vec![
            (1134, TabAlign::Left),
            (4536, TabAlign::Center),
            (9072, TabAlign::Right)
        ]
    );
}

#[test]
fn clearing_every_inherited_stop_leaves_none() {
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Etiket"/><w:tabs><w:tab w:val="clear" w:pos="1418"/><w:tab w:val="clear" w:pos="2836"/></w:tabs></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#;
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(&docx_package(body, STYLES), &mut w).unwrap();
    assert_eq!(stops(first_para(&d)), vec![]);
}
