//! Değişiklik izleme KAYITLARI belgenin güncel hâli değildir.
//!
//! Word, izlenen bir değişikliğin ÖNCEKİ hâlini belgenin içinde saklar:
//! `w:tblGridChange` eski sütun ızgarasını, `w:tblPrChange` eski tablo
//! özelliklerini, `w:tcPrChange` eski hücre özelliklerini taşır. Okuyucu
//! "bütün değişiklikler kabul edilmiş" hâli okur: eski kayıtlar yok sayılır,
//! silinen metin atılır, eklenen metin kalır.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes_with, ConvertOptions, Direction, LayoutTables};
use tavzih_core::docx;
use tavzih_core::model::*;
use tavzih_core::warnings::WarningSink;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const REV: &str = r#"w:author="Örnek" w:date="2026-01-01T00:00:00Z""#;

fn package(body: &str) -> Vec<u8> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#
    );
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        z.start_file("word/document.xml", o).unwrap();
        z.write_all(document.as_bytes()).unwrap();
        z.finish().unwrap();
    }
    cur.into_inner()
}

fn read(bytes: &[u8]) -> Document {
    let mut w = WarningSink::new();
    docx::reader::read_docx(bytes, &mut w).expect("read docx")
}

fn udf_content(docx: &[u8]) -> String {
    let mut w = WarningSink::new();
    // Bu dosya UDF'teki TABLO yapısını sınar: kenarlıksız tablolar varsayılan
    // olarak sekmeli satıra çevrilir (`layout_tables.rs`), burada tablo kalmalı.
    let keep = ConvertOptions {
        layout_tables: LayoutTables::Keep,
        ..Default::default()
    };
    let udf = convert_bytes_with(docx, Direction::DocxToUdf, &keep, &mut w).expect("convert");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf)).unwrap();
    let mut s = String::new();
    z.by_name("content.xml")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
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

/// Belgedeki bütün görünen metin (tablo hücreleri dâhil), paragraflar `\n` ile.
fn text(d: &Document) -> String {
    fn walk(blocks: &[Block], out: &mut String) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let Run::Text { text, .. } = r {
                            out.push_str(text);
                        }
                    }
                    out.push('\n');
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for c in &row.cells {
                            walk(&c.blocks, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = String::new();
    for s in &d.sections {
        walk(&s.blocks, &mut out);
    }
    out
}

fn p(text: &str) -> String {
    format!(r#"<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

fn tc(text: &str, tc_pr: &str) -> String {
    format!(r#"<w:tc><w:tcPr>{tc_pr}</w:tcPr>{}</w:tc>"#, p(text))
}

/// 2000 ve 4000 twip (100 pt ve 200 pt) iki sütunlu tablo. `grid_extra`
/// `w:tblGrid`'in sonuna, `tc_pr` her hücreye eklenir.
fn two_column_table(grid_extra: &str, tc_pr: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="4000"/>{grid_extra}</w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl>"#,
        tc("Esas No", tc_pr),
        tc("2026/129", tc_pr),
        tc("Karar No", tc_pr),
        tc("2026/77", tc_pr),
    )
}

// ------------------------------------------------------------ sütun ızgarası

#[test]
fn the_current_grid_is_read_as_is() {
    let d = read(&package(&two_column_table("", "")));
    assert_eq!(tables(&d)[0].column_widths, vec![100.0, 200.0]);
    let xml = udf_content(&package(&two_column_table("", "")));
    assert!(
        xml.contains(r#"columnCount="2" columnSpans="100,200""#),
        "{xml}"
    );
}

#[test]
fn an_old_grid_kept_by_track_changes_is_not_the_current_grid() {
    // Word, sütun genişliği değişikliğini izlerken eski ızgarayı
    // `w:tblGrid/w:tblGridChange/w:tblGrid` içinde saklar.
    let old = r#"<w:tblGridChange w:id="7"><w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid></w:tblGridChange>"#;
    let docx = package(&two_column_table(old, ""));
    let d = read(&docx);
    let t = tables(&d)[0];
    assert_eq!(t.column_widths, vec![100.0, 200.0], "eski ızgara eklendi");
    assert_eq!(t.column_count(), 2);
    let xml = udf_content(&docx);
    assert!(
        xml.contains(r#"columnCount="2" columnSpans="100,200""#),
        "{xml}"
    );
    // Satırlar ızgarayı doldurur; satıra özel genişlik gerekmez.
    assert!(!xml.contains(r#"rowType="dataRow" columnSpans="#), "{xml}");
}

#[test]
fn an_old_cell_width_kept_by_track_changes_does_not_change_the_grid() {
    // `w:tcPrChange` eski hücre özelliklerini (genişlik, birleştirme) taşır.
    let old = format!(
        r#"<w:tcW w:w="3000" w:type="dxa"/><w:tcPrChange w:id="8" {REV}><w:tcPr><w:tcW w:w="9000" w:type="dxa"/><w:gridSpan w:val="2"/></w:tcPr></w:tcPrChange>"#
    );
    let d = read(&package(&two_column_table("", &old)));
    let t = tables(&d)[0];
    assert_eq!(t.column_widths, vec![100.0, 200.0]);
    assert!(t
        .rows
        .iter()
        .all(|r| r.cells.len() == 2 && r.cells.iter().all(|c| c.grid_span == 1)));
}

#[test]
fn a_nested_table_keeps_its_own_grid() {
    let inner = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="500"/><w:gridCol w:w="700"/><w:tblGridChange w:id="9"><w:tblGrid><w:gridCol w:w="1200"/></w:tblGrid></w:tblGridChange></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>"#,
        tc("a", ""),
        tc("b", "")
    );
    let outer = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="4000"/></w:tblGrid><w:tr>{}<w:tc><w:tcPr/>{inner}<w:p/></w:tc></w:tr></w:tbl>"#,
        tc("dış", "")
    );
    let d = read(&package(&outer));
    let t = tables(&d)[0];
    assert_eq!(
        t.column_widths,
        vec![100.0, 200.0],
        "iç tablo dış ızgarayı bozdu"
    );
    let nested = t.rows[0].cells[1].blocks.iter().find_map(|b| match b {
        Block::Table(t) => Some(t),
        _ => None,
    });
    // İç tablo ya korunur ya düzleştirilir; korunuyorsa kendi ızgarası doğru.
    if let Some(inner) = nested {
        assert_eq!(inner.column_widths, vec![25.0, 35.0]);
    }
}

#[test]
fn merged_cells_keep_the_current_grid() {
    let body = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:tblGridChange w:id="10"><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid></w:tblGridChange></w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl>"#,
        tc("Başlık", r#"<w:gridSpan w:val="2"/>"#),
        tc("Not", ""),
        tc("1", ""),
        tc("2", ""),
        tc("3", ""),
    );
    let docx = package(&body);
    let t = tables(&read(&docx))[0].clone();
    assert_eq!(t.column_widths, vec![100.0, 100.0, 100.0]);
    assert_eq!(t.rows[0].cells[0].grid_span, 2);
    let xml = udf_content(&docx);
    assert!(
        xml.contains(r#"columnCount="3" columnSpans="100,100,100""#),
        "{xml}"
    );
    assert!(xml.contains(r#"columnSpans="200,100""#), "{xml}");
}

#[test]
fn an_autofit_table_without_widths_still_gets_one_column_per_cell() {
    // `w:tblLayout w:type="autofit"` ve ızgarasız tablo: sütun sayısı
    // hücrelerden gelir.
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblLayout w:type="autofit"/></w:tblPr><w:tblGrid/><w:tr>{}{}</w:tr></w:tbl>"#,
        tc("a", ""),
        tc("b", "")
    );
    let xml = udf_content(&package(&body));
    assert!(xml.contains(r#"columnCount="2""#), "{xml}");
}

// ------------------------------------------------------------ silinen işaretler

#[test]
fn a_deleted_paragraph_mark_does_not_swallow_the_text_after_it() {
    // İki paragrafı birleştirmek (ilkinin sonunda Delete) izlenirken Word
    // yalnız paragraf İŞARETİNİ silinmiş sayar: `w:pPr/w:rPr/w:del` BOŞ bir
    // öğedir. Metin silinmemiştir.
    let body = format!(
        r#"<w:p><w:pPr><w:rPr><w:del w:id="1" {REV}/></w:rPr></w:pPr><w:r><w:t>Birinci paragraf.</w:t></w:r></w:p>{}{}"#,
        p("Davalı 1.250.000,00 TL ödemekle yükümlüdür."),
        p("Saygılarımla.")
    );
    let t = text(&read(&package(&body)));
    assert!(t.contains("Birinci paragraf."), "{t}");
    assert!(
        t.contains("Davalı 1.250.000,00 TL ödemekle yükümlüdür."),
        "{t}"
    );
    assert!(t.contains("Saygılarımla."), "{t}");
}

#[test]
fn a_deleted_row_marker_does_not_swallow_the_text_after_it() {
    let body = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr><w:del w:id="2" {REV}/></w:trPr><w:tc><w:tcPr/><w:p><w:del w:id="3" {REV}><w:r><w:delText>Silinen satır</w:delText></w:r></w:del></w:p></w:tc></w:tr><w:tr><w:trPr><w:ins w:id="4" {REV}/></w:trPr>{}</w:tr></w:tbl>{}"#,
        tc("Kalan satır", ""),
        p("Sonuç: 42 gün.")
    );
    let t = text(&read(&package(&body)));
    assert!(!t.contains("Silinen satır"), "{t}");
    assert!(t.contains("Kalan satır"), "{t}");
    assert!(t.contains("Sonuç: 42 gün."), "{t}");
}

#[test]
fn deleted_text_is_dropped_and_inserted_text_is_kept() {
    let body = format!(
        r#"<w:p><w:r><w:t xml:space="preserve">Faiz oranı </w:t></w:r><w:del w:id="5" {REV}><w:r><w:delText>%9</w:delText></w:r></w:del><w:ins w:id="6" {REV}><w:r><w:t>%12</w:t></w:r></w:ins><w:r><w:t xml:space="preserve"> olarak uygulanır.</w:t></w:r></w:p>"#
    );
    let t = text(&read(&package(&body)));
    assert_eq!(t, "Faiz oranı %12 olarak uygulanır.\n");
}

#[test]
fn moved_text_appears_once_at_its_new_place() {
    // Taşıma izlenirken eski yer `w:moveFrom`, yeni yer `w:moveTo` içindedir.
    // Kabul edilmiş hâlde metin yalnız yeni yerindedir.
    let body = format!(
        r#"<w:p><w:moveFrom w:id="11" {REV}><w:r><w:t>Taşınan cümle.</w:t></w:r></w:moveFrom><w:r><w:t xml:space="preserve"> Kalan.</w:t></w:r></w:p>{}<w:p><w:moveTo w:id="12" {REV}><w:r><w:t>Taşınan cümle.</w:t></w:r></w:moveTo></w:p>"#,
        p("Ara paragraf.")
    );
    let t = text(&read(&package(&body)));
    assert_eq!(t.matches("Taşınan cümle.").count(), 1, "{t}");
    assert!(t.contains("Kalan."), "{t}");
    assert!(t.contains("Ara paragraf."), "{t}");
}

#[test]
fn an_empty_deleted_row_does_not_swallow_the_text_after_it() {
    // Boş bir satır silindiğinde içinde kapanan bir `w:del` yoktur; eskiden
    // `w:trPr/w:del` bayrağı belgenin sonuna kadar takılı kalıyordu.
    let body = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr><w:del w:id="2" {REV}/></w:trPr><w:tc><w:tcPr/><w:p/></w:tc></w:tr><w:tr>{}</w:tr></w:tbl>{}"#,
        tc("Kalan satır", ""),
        p("Sonuç: 42 gün.")
    );
    let t = text(&read(&package(&body)));
    assert!(t.contains("Kalan satır"), "{t}");
    assert!(t.contains("Sonuç: 42 gün."), "{t}");
}

fn paragraphs(d: &Document) -> Vec<String> {
    d.sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(
                p.runs
                    .iter()
                    .filter_map(|r| match r {
                        Run::Text { text, .. } => Some(text.as_str()),
                        Run::Tab { .. } => Some("<tab>"),
                        Run::LineBreak { .. } => Some("<br>"),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

#[test]
fn a_wholly_deleted_paragraph_leaves_no_blank_line() {
    let body = format!(
        r#"{}<w:p><w:pPr><w:rPr><w:del w:id="13" {REV}/></w:rPr></w:pPr><w:del w:id="14" {REV}><w:r><w:delText>Silinen paragraf.</w:delText></w:r></w:del></w:p>{}"#,
        p("Önceki."),
        p("Sonraki.")
    );
    assert_eq!(
        paragraphs(&read(&package(&body))),
        vec!["Önceki.", "Sonraki."]
    );
}

#[test]
fn a_paragraph_whose_mark_alone_was_deleted_keeps_its_text() {
    let body = format!(
        r#"<w:p><w:pPr><w:rPr><w:del w:id="15" {REV}/></w:rPr></w:pPr><w:r><w:t>Birinci</w:t></w:r></w:p>{}"#,
        p("İkinci")
    );
    // Word iki paragrafı birleştirir; burada metin eksiksiz, yapı yaklaşık.
    assert_eq!(
        paragraphs(&read(&package(&body))),
        vec!["Birinci", "İkinci"]
    );
}

#[test]
fn a_moved_paragraph_leaves_no_blank_line_at_its_old_place() {
    let body = format!(
        r#"<w:p><w:pPr><w:rPr><w:moveFrom w:id="16" {REV}/></w:rPr></w:pPr><w:moveFrom w:id="17" {REV}><w:r><w:t>Madde 3.</w:t></w:r></w:moveFrom></w:p>{}<w:p><w:pPr><w:rPr><w:moveTo w:id="18" {REV}/></w:rPr></w:pPr><w:moveTo w:id="19" {REV}><w:r><w:t>Madde 3.</w:t></w:r></w:moveTo></w:p>"#,
        p("Madde 2.")
    );
    assert_eq!(
        paragraphs(&read(&package(&body))),
        vec!["Madde 2.", "Madde 3."]
    );
}

#[test]
fn deleted_tabs_and_line_breaks_are_not_current() {
    let body = format!(
        r#"<w:p><w:r><w:t>Ad</w:t></w:r><w:del w:id="20" {REV}><w:r><w:tab/><w:br/></w:r></w:del><w:ins w:id="21" {REV}><w:r><w:tab/></w:r></w:ins><w:r><w:t>Soyad</w:t></w:r></w:p>"#
    );
    assert_eq!(paragraphs(&read(&package(&body))), vec!["Ad<tab>Soyad"]);
}

#[test]
fn a_deletion_inside_a_move_does_not_reopen_the_moved_text() {
    let body = format!(
        r#"<w:p><w:moveFrom w:id="22" {REV}><w:r><w:t>Eski </w:t></w:r><w:del w:id="23" {REV}><w:r><w:delText>x</w:delText></w:r></w:del><w:r><w:t>yer.</w:t></w:r></w:moveFrom><w:r><w:t>Kalan.</w:t></w:r></w:p>"#
    );
    assert_eq!(paragraphs(&read(&package(&body))), vec!["Kalan."]);
}
