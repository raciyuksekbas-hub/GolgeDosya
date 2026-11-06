//! Word tablosunun GÖRÜNEN kenarlıkları UDF'e doğru taşınır.
//!
//! Saha: "Word'de böyle bir tablo olmadığı hâlde başlıklarda (davacı, vekili,
//! davalı, konu) dönüşüm tablo gibi oluyor." Dilekçe şablonlarındaki başlık
//! bloğu görünmez bir yerleşim tablosudur: `w:tblBorders` "single", ama her
//! hücrede `w:tcBorders` "none". Okuyucu yalnız tablo katmanını okuyup her
//! tabloyu kenarlıklı yazıyordu.
//!
//! UDF bir tabloda yalnız "bütün hücre kenarlıkları" (`borderCell`) ya da "hiç"
//! (`borderNone`) tutabilir. Kural: Word'de hiçbir kenar görünmüyorsa
//! `borderNone`; bazıları görünüyorsa çizgiler kaybolmasın diye `borderCell` ve
//! dürüst bir yaklaşık aktarım uyarısı.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{
    convert_bytes, convert_bytes_with, ConvertOptions, Direction, LayoutTables,
};
use tavzih_core::docx;
use tavzih_core::model::*;
use tavzih_core::warnings::{WarningCode, WarningSink};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn package(document_body: &str, styles: Option<&str>) -> Vec<u8> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body>{document_body}</w:body></w:document>"#
    );
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        z.start_file("word/document.xml", o).unwrap();
        z.write_all(document.as_bytes()).unwrap();
        if let Some(styles) = styles {
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{W}">{styles}</w:styles>"#
            );
            z.start_file("word/styles.xml", o).unwrap();
            z.write_all(xml.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    cur.into_inner()
}

const SINGLE_ALL: &str = r#"<w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/>"#;
const NONE_CELL: &str = r#"<w:tcBorders><w:top w:val="none" w:color="FFFFFF" w:sz="0"/><w:left w:val="none" w:color="FFFFFF" w:sz="0"/><w:bottom w:val="none" w:color="FFFFFF" w:sz="0"/><w:right w:val="none" w:color="FFFFFF" w:sz="0"/></w:tcBorders>"#;

fn cell(text: &str, tc_pr_extra: &str) -> String {
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:type="dxa" w:w="3000"/>{tc_pr_extra}</w:tcPr><w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p></w:tc>"#
    )
}

fn table(tbl_pr: &str, rows: &[Vec<String>]) -> String {
    let cols = rows.first().map(|r| r.len()).unwrap_or(0);
    let grid: String = (0..cols).map(|_| r#"<w:gridCol w:w="3000"/>"#).collect();
    let body: String = rows
        .iter()
        .map(|r| format!("<w:tr>{}</w:tr>", r.concat()))
        .collect();
    format!("<w:tbl><w:tblPr>{tbl_pr}</w:tblPr><w:tblGrid>{grid}</w:tblGrid>{body}</w:tbl>")
}

fn read(bytes: &[u8]) -> (Document, Vec<WarningCode>) {
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(bytes, &mut w).expect("read docx");
    (d, w.into_vec().iter().map(|x| x.code).collect())
}

fn tables(d: &Document) -> Vec<&Table> {
    d.sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn udf_content(docx: &[u8]) -> String {
    // Bu dosya UDF'teki TABLO yapısını sınar: kenarlıksız tablolar varsayılan
    // olarak sekmeli satıra çevrilir (`layout_tables.rs`), burada tablo kalmalı.
    let keep = ConvertOptions {
        layout_tables: LayoutTables::Keep,
        ..Default::default()
    };
    let mut w = WarningSink::new();
    let udf = convert_bytes_with(docx, Direction::DocxToUdf, &keep, &mut w).expect("convert");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf)).unwrap();
    let mut s = String::new();
    z.by_name("content.xml")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

/// Dilekçe başlık bloğu: etiket | ":" | değer, tablo kenarlığı "single", her
/// hücre "none" — Word'de düz, hizalı metin gibi görünür.
fn petition_header() -> String {
    let rows: Vec<Vec<String>> = [
        ("Dosya No.", "2026/1 E."),
        ("DAVACI", "Örnek Yazılım A.Ş."),
        ("VEKİLİ", "Av. Örnek Avukat"),
        ("DAVALI", "Örnek Danışmanlık Ltd. Şti."),
        (
            "KONU",
            "Bilirkişi raporuna karşı beyanlarımızın sunulmasıdır.",
        ),
    ]
    .iter()
    .map(|(label, value)| {
        vec![
            cell(label, NONE_CELL),
            cell(":", NONE_CELL),
            cell(value, NONE_CELL),
        ]
    })
    .collect();
    table(
        &format!("<w:tblW w:type=\"pct\" w:w=\"100%\"/><w:tblBorders>{SINGLE_ALL}</w:tblBorders>"),
        &rows,
    )
}

#[test]
fn a_petition_header_that_is_invisible_in_word_has_no_borders_in_udf() {
    let docx = package(&petition_header(), None);
    let (d, warns) = read(&docx);
    assert_eq!(tables(&d)[0].border, TableBorder::None);
    assert!(
        !warns.contains(&WarningCode::TableBordersApproximated),
        "{warns:?}"
    );
    let xml = udf_content(&docx);
    assert!(
        xml.contains(r#"border="borderNone""#),
        "UDF tablosu kenarlıklı yazıldı"
    );
    assert!(!xml.contains(r#"border="borderCell""#));
    // Varsayılan dönüştürmede bu başlık hiç tablo değildir: sekmeli satırlar
    // (saha: "dilekçeler blok hâlinde oluyor"; ayrıntısı layout_tables.rs).
    let mut w = WarningSink::new();
    let udf = convert_bytes(&docx, Direction::DocxToUdf, &mut w).expect("convert");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf)).unwrap();
    let mut s = String::new();
    z.by_name("content.xml")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    assert!(!s.contains("<table"), "başlık hâlâ tablo: {s}");
}

#[test]
fn a_table_with_its_own_borders_and_no_cell_overrides_keeps_them() {
    let rows = vec![
        vec![cell("A", ""), cell("B", "")],
        vec![cell("C", ""), cell("D", "")],
    ];
    let docx = package(
        &table(&format!("<w:tblBorders>{SINGLE_ALL}</w:tblBorders>"), &rows),
        None,
    );
    let (d, warns) = read(&docx);
    assert_eq!(tables(&d)[0].border, TableBorder::Cell);
    assert!(
        !warns.contains(&WarningCode::TableBordersApproximated),
        "{warns:?}"
    );
    assert!(udf_content(&docx).contains(r#"border="borderCell""#));
}

#[test]
fn a_table_style_with_borders_draws_them() {
    let styles = format!(
        r#"<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders>{SINGLE_ALL}</w:tblBorders></w:tblPr></w:style>"#
    );
    let rows = vec![vec![cell("A", ""), cell("B", "")]];
    let docx = package(
        &table(r#"<w:tblStyle w:val="TableGrid"/>"#, &rows),
        Some(&styles),
    );
    let (d, _) = read(&docx);
    assert_eq!(tables(&d)[0].border, TableBorder::Cell);
}

#[test]
fn a_table_based_on_a_bordered_style_inherits_its_borders() {
    let styles = format!(
        r#"<w:style w:type="table" w:styleId="TableGrid"><w:tblPr><w:tblBorders>{SINGLE_ALL}</w:tblBorders></w:tblPr></w:style><w:style w:type="table" w:styleId="Dilekce"><w:basedOn w:val="TableGrid"/></w:style>"#
    );
    let rows = vec![vec![cell("A", ""), cell("B", "")]];
    let docx = package(
        &table(r#"<w:tblStyle w:val="Dilekce"/>"#, &rows),
        Some(&styles),
    );
    assert_eq!(tables(&read(&docx).0)[0].border, TableBorder::Cell);
}

#[test]
fn a_table_without_style_or_borders_is_borderless_like_words_normal_table() {
    let rows = vec![vec![cell("A", ""), cell("B", "")]];
    let docx = package(&table("", &rows), None);
    assert_eq!(tables(&read(&docx).0)[0].border, TableBorder::None);
}

#[test]
fn the_documents_default_table_style_applies_to_unstyled_tables() {
    let styles = format!(
        r#"<w:style w:type="table" w:default="1" w:styleId="Varsayilan"><w:tblPr><w:tblBorders>{SINGLE_ALL}</w:tblBorders></w:tblPr></w:style>"#
    );
    let rows = vec![vec![cell("A", ""), cell("B", "")]];
    let docx = package(&table("", &rows), Some(&styles));
    assert_eq!(tables(&read(&docx).0)[0].border, TableBorder::Cell);
}

#[test]
fn cell_overrides_win_over_a_bordered_style() {
    let styles = format!(
        r#"<w:style w:type="table" w:styleId="TableGrid"><w:tblPr><w:tblBorders>{SINGLE_ALL}</w:tblBorders></w:tblPr></w:style>"#
    );
    let rows = vec![vec![cell("A", NONE_CELL), cell("B", NONE_CELL)]];
    let docx = package(
        &table(r#"<w:tblStyle w:val="TableGrid"/>"#, &rows),
        Some(&styles),
    );
    assert_eq!(tables(&read(&docx).0)[0].border, TableBorder::None);
}

#[test]
fn an_outer_box_only_is_drawn_in_full_and_reported() {
    let outer = r#"<w:tblBorders><w:top w:val="single"/><w:left w:val="single"/><w:bottom w:val="single"/><w:right w:val="single"/><w:insideH w:val="none"/><w:insideV w:val="none"/></w:tblBorders>"#;
    let rows = vec![
        vec![cell("A", ""), cell("B", "")],
        vec![cell("C", ""), cell("D", "")],
    ];
    let docx = package(&table(outer, &rows), None);
    let (d, warns) = read(&docx);
    assert_eq!(
        tables(&d)[0].border,
        TableBorder::Cell,
        "çizgiler kaybolmamalı"
    );
    assert!(
        warns.contains(&WarningCode::TableBordersApproximated),
        "{warns:?}"
    );
}

#[test]
fn a_line_drawn_by_one_neighbour_is_visible_even_if_the_other_says_none() {
    // İki hücre arasındaki dikey çizgi yalnız soldaki hücrede "single";
    // sağdaki hücre o kenar için "none" diyor. Word çizgiyi çizer.
    let left = r#"<w:tcBorders><w:top w:val="none"/><w:left w:val="none"/><w:bottom w:val="none"/><w:right w:val="single"/></w:tcBorders>"#;
    let rows = vec![vec![cell("A", left), cell("B", NONE_CELL)]];
    let docx = package(&table("", &rows), None);
    let (d, warns) = read(&docx);
    assert_eq!(tables(&d)[0].border, TableBorder::Cell);
    assert!(
        warns.contains(&WarningCode::TableBordersApproximated),
        "{warns:?}"
    );
}

#[test]
fn borders_inside_tracked_property_changes_are_not_the_current_borders() {
    // `w:tcPrChange` revizyondan ÖNCEKİ hücre özelliklerini taşır; güncel
    // hâl kenarlıksızdır.
    let old = format!(
        r#"{NONE_CELL}<w:tcPrChange w:id="1" w:author="x"><w:tcPr><w:tcBorders><w:top w:val="single"/><w:left w:val="single"/><w:bottom w:val="single"/><w:right w:val="single"/></w:tcBorders></w:tcPr></w:tcPrChange>"#
    );
    let rows = vec![vec![cell("A", &old), cell("B", NONE_CELL)]];
    let docx = package(&table("", &rows), None);
    assert_eq!(tables(&read(&docx).0)[0].border, TableBorder::None);
}

#[test]
fn table_borders_inside_a_tracked_table_property_change_are_not_current() {
    // `w:tblPrChange` revizyondan ÖNCEKİ tablo özelliklerini taşır. Güncel
    // tablo stilsiz ve kenarlıksızdır (Word'ün "Normal Table"ı).
    let tbl_pr = format!(
        r#"<w:tblPrChange w:id="2" w:author="x"><w:tblPr><w:tblBorders>{SINGLE_ALL}</w:tblBorders></w:tblPr></w:tblPrChange>"#
    );
    let rows = vec![vec![cell("A", ""), cell("B", "")]];
    let docx = package(&table(&tbl_pr, &rows), None);
    assert_eq!(tables(&read(&docx).0)[0].border, TableBorder::None);
}

#[test]
fn each_table_is_decided_on_its_own() {
    let bordered = vec![vec![cell("X", ""), cell("Y", "")]];
    let body = format!(
        "{}<w:p/>{}",
        petition_header(),
        table(
            &format!("<w:tblBorders>{SINGLE_ALL}</w:tblBorders>"),
            &bordered
        )
    );
    let docx = package(&body, None);
    let (d, _) = read(&docx);
    let kinds: Vec<_> = tables(&d).iter().map(|t| t.border).collect();
    assert_eq!(kinds, vec![TableBorder::None, TableBorder::Cell]);
}
