//! Tablo YAPISI iki yönde de korunur.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (199 gerçek Word belgesi ve 293
//! gerçek UDF üzerinde ölçüldü, burada sentetik olarak kurulur):
//! - UYAP'ın kendi dosyaları tabloyu hücrenin içine dört düzeye kadar iç içe
//!   koyar (22 UDF'te 224 iç tablo). Word → UDF yalnız ilk iç düzeyin
//!   paragraflarını yazıyordu; daha derindeki tabloların metni "düzleştirildi"
//!   uyarısının arkasında kayboluyordu (UDF → Word → UDF'te 21 gerçek belge).
//! - Word okuyucusu hücre ve satır özelliklerini (gridSpan, vAlign, vMerge,
//!   tcBorders, tblHeader, trHeight) tek değişkende tutuyordu: hücredeki iç
//!   tablo dış hücrenin değerlerini eziyor, vMerge'li bir iç hücre dış
//!   hücrenin bütün içeriğini siliyordu.
//! - `w:gridBefore`/`w:gridAfter` okunmuyordu: o satırların hücreleri sola
//!   kayıp başka sütunlara oturuyordu. Değişiklik izlemede silinmiş satır
//!   (`w:trPr/w:del`) bütün değişiklikler kabul edilince yoktur; boş bir satır
//!   olarak kalıyor ve tablonun kenarlık kararına da katılıyordu.
//! - UDF okuyucusu, UYAP'ın kendi bir dosyasında satır yerine yazılmış
//!   `<paragraph height=..>` içindeki hücreyi (20 görünür karakter) sessizce
//!   atıyordu; satırsız hücre, bitiş etiketli yaprak ve paragraf içindeki
//!   paragraf da metin kaybettiriyordu.
//! - Birleşik UDF satırının kendi `columnSpans`'ı okunmuyordu: hücreler ızgaraya
//!   eşit dağıtılıyor, birleşik hücre çoğu zaman yanlış sütunlara oturuyordu
//!   (21 gerçek UDF'te 201 satırın 199'u).
//! - Word iki tablonun bitişik durmasına izin vermez; UDF → Word araya zorunlu
//!   boş bir paragraf koyar. O paragraf Word → UDF'te içerik sanılıyor, bitişik
//!   UDF tabloları her turda araya boş bir satır kazanıyordu.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::docx;
use tavzih_core::model::*;
use tavzih_core::udf;
use tavzih_core::warnings::{WarningCode, WarningSink};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

// ------------------------------------------------------------ Word tarafı

fn docx_package(body: &str) -> Vec<u8> {
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

fn p(text: &str) -> String {
    format!(r#"<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

fn tc(tc_pr: &str, content: &str) -> String {
    format!(r#"<w:tc><w:tcPr>{tc_pr}</w:tcPr>{content}</w:tc>"#)
}

fn tr(tr_pr: &str, cells: &[String]) -> String {
    let tr_pr = if tr_pr.is_empty() {
        String::new()
    } else {
        format!("<w:trPr>{tr_pr}</w:trPr>")
    };
    format!("<w:tr>{tr_pr}{}</w:tr>", cells.concat())
}

/// `grid` twip cinsinden sütun genişlikleri.
fn tbl(tbl_pr: &str, grid: &[u32], rows: &[String]) -> String {
    let grid: String = grid
        .iter()
        .map(|w| format!(r#"<w:gridCol w:w="{w}"/>"#))
        .collect();
    format!(
        "<w:tbl><w:tblPr>{tbl_pr}</w:tblPr><w:tblGrid>{grid}</w:tblGrid>{}</w:tbl>",
        rows.concat()
    )
}

fn read_docx(bytes: &[u8]) -> (Document, Vec<WarningCode>) {
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(bytes, &mut w).expect("read docx");
    (d, w.into_vec().iter().map(|x| x.code).collect())
}

// ------------------------------------------------------------ UDF tarafı

/// `content.xml` kurucusu: tampon ve öğe ağacı aynı imleçten üretilir (yazıcı
/// gibi), böylece her `startOffset`/`length` gerçekten metni gösterir.
#[derive(Default)]
struct Udf {
    buf: String,
    units: usize,
}

impl Udf {
    /// Tampona `text` ekler, onu gösteren `<tag ... />` yaprağını döndürür.
    fn leaf(&mut self, tag: &str, text: &str) -> String {
        let start = self.units;
        self.buf.push_str(text);
        let len = text.encode_utf16().count();
        self.units += len;
        format!(r#"<{tag} startOffset="{start}" length="{len}" />"#)
    }

    /// Tek run'lı, sonlandırıcısıyla bir paragraf.
    fn p(&mut self, text: &str) -> String {
        let c = self.leaf("content", &format!("{text}\n"));
        format!("<paragraph>{c}</paragraph>")
    }

    fn build(&self, elements: &str) -> Vec<u8> {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" ?>
<template format_id="1.8" >
<content><![CDATA[{}]]></content>
<properties><pageFormat mediaSizeName="1" leftMargin="70.875" rightMargin="70.875" topMargin="70.875" bottomMargin="70.875" paperOrientation="1" headerFOffset="20.0" footerFOffset="20.0" /></properties>
<elements resolver="hvl-default" >
{elements}
</elements>
<styles><style name="hvl-default" family="Times New Roman" size="12" description="Gövde" /></styles>
</template>"#,
            self.buf
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
}

/// UYAP'ın yazdığı biçimde bir tablo: `columnSpans` tablo ızgarası.
fn utable(spans: &str, rows: &str) -> String {
    let cols = spans.split(',').count();
    format!(
        r#"<table tableName="Tablo" columnCount="{cols}" columnSpans="{spans}" border="borderNone" header="false">{rows}</table>"#
    )
}

fn urow(attrs: &str, cells: &[String]) -> String {
    let cells: String = cells.iter().map(|c| format!("<cell>{c}</cell>")).collect();
    format!(r#"<row rowName="row1" rowType="dataRow"{attrs}>{cells}</row>"#)
}

fn read_udf(bytes: &[u8]) -> (Document, Vec<WarningCode>) {
    let mut w = WarningSink::new();
    let d = udf::reader::read_udf(bytes, &mut w).expect("read udf");
    (d, w.into_vec().iter().map(|x| x.code).collect())
}

fn convert(bytes: &[u8], direction: Direction) -> (Vec<u8>, Vec<WarningCode>) {
    let mut w = WarningSink::new();
    let out = convert_bytes(bytes, direction, &mut w).expect("convert");
    (out, w.into_vec().iter().map(|x| x.code).collect())
}

fn zip_part(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

// ------------------------------------------------------------ model ölçüleri

/// Belgedeki bütün görünen metin, tablo hücreleri (her derinlikte) dâhil.
fn text(d: &Document) -> String {
    fn walk(blocks: &[Block], out: &mut String) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    out.push_str(&p.text());
                    out.push('\n');
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for c in &row.cells {
                            walk(&c.blocks, out);
                        }
                    }
                }
                Block::PageBreak => {}
            }
        }
    }
    let mut out = String::new();
    for s in &d.sections {
        walk(&s.blocks, &mut out);
    }
    out
}

/// Boşluk dışındaki karakterler: paragraf yapısından bağımsız metin karşılaştırması.
fn ink(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Her tablonun iç içelik derinliği, belge sırasıyla (gövde tablosu 0).
fn table_depths(d: &Document) -> Vec<usize> {
    fn walk(blocks: &[Block], depth: usize, out: &mut Vec<usize>) {
        for b in blocks {
            if let Block::Table(t) = b {
                out.push(depth);
                for row in &t.rows {
                    for c in &row.cells {
                        walk(&c.blocks, depth + 1, out);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for s in &d.sections {
        walk(&s.blocks, 0, &mut out);
    }
    out
}

// ------------------------------------------------------------ iç içe tablolar

/// `levels` kez iç içe geçmiş Word tablosu: her düzeyin ikinci hücresi bir alt
/// tabloyu taşır; en içteki tablo düz metindir.
fn nested_docx(levels: usize) -> Vec<u8> {
    let mut inner = String::new();
    for level in (0..=levels).rev() {
        let second = if inner.is_empty() {
            p(&format!("En iç hücre {level}: 2026/129 E."))
        } else {
            format!("{}{inner}<w:p/>", p(&format!("Üst metin {level}")))
        };
        inner = tbl(
            "",
            &[2000, 4000],
            &[tr(
                "",
                &[
                    tc("", &p(&format!("Düzey {level}: 1.250.000,00 TL"))),
                    tc("", &second),
                ],
            )],
        );
    }
    docx_package(&format!("{}{inner}{}", p("Giriş."), p("Saygılarımla.")))
}

#[test]
fn nested_word_tables_reach_udf_whole_at_every_depth() {
    let mut failures = Vec::new();
    for levels in 1..=3 {
        let src = nested_docx(levels);
        let (word, _) = read_docx(&src);
        let (udf_bytes, warns) = convert(&src, Direction::DocxToUdf);
        let (back, _) = read_udf(&udf_bytes);
        let (want, got) = (ink(&text(&word)), ink(&text(&back)));
        if want != got {
            failures.push(format!(
                "{levels} düzey: metin {} -> {} karakter",
                want.chars().count(),
                got.chars().count()
            ));
        }
        if table_depths(&back) != table_depths(&word) {
            failures.push(format!(
                "{levels} düzey: tablo derinlikleri {:?} -> {:?}",
                table_depths(&word),
                table_depths(&back)
            ));
        }
        // Hiçbir şey kaybolmadıysa "düzleştirildi" demek yanlış alarmdır.
        if warns.contains(&WarningCode::TableNestedFlattened) {
            failures.push(format!("{levels} düzey: {warns:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_udf_cell_holding_only_a_table_is_written_as_uyap_writes_it() {
    // Gerçek UYAP dosyalarında iç tablo çoğu zaman hücrenin TEK çocuğudur
    // (<cell><table>…</table></cell>). Word bu hücreyi tablodan sonra zorunlu
    // boş bir paragrafla kapatır; o paragraf yapıdır, içerik değil. Yazıcı
    // UYAP'ın yapısını üretmeli: ne iç tabloyu açmalı ne de uydurma bir
    // paragraf eklemeli.
    let inner = tbl(
        "",
        &[3000],
        &[tr("", &[tc("", &p("İç metin: 2026/129 E."))])],
    );
    let outer = tbl("", &[6000], &[tr("", &[tc("", &format!("{inner}<w:p/>"))])]);
    let src = docx_package(&format!("{outer}{}", p("Son.")));
    let (udf_bytes, _) = convert(&src, Direction::DocxToUdf);
    let xml = zip_part(&udf_bytes, "content.xml");
    let cell = xml.split("<cell>").nth(1).expect("dış hücre");
    assert!(cell.starts_with("<table "), "iç tablo açıldı: {cell}");
    let (back, _) = read_udf(&udf_bytes);
    assert_eq!(table_depths(&back), vec![0, 1]);
    assert!(text(&back).contains("İç metin: 2026/129 E."));
}

#[test]
fn a_document_that_is_only_a_doubly_nested_table_converts() {
    // Düzleştirme iki düzey içteki tabloyu atlayınca belgede adreslenen hiçbir
    // öğe kalmıyor, dönüştürme OUTPUT_VALIDATION_FAILED ile düşüyordu.
    let deep = tbl("", &[2000], &[tr("", &[tc("", &p("Derin: 2026/77 K."))])]);
    let mid = tbl("", &[4000], &[tr("", &[tc("", &format!("{deep}<w:p/>"))])]);
    let outer = tbl("", &[6000], &[tr("", &[tc("", &format!("{mid}<w:p/>"))])]);
    let src = docx_package(&format!("{outer}<w:p/>"));
    let mut w = WarningSink::new();
    let udf_bytes = convert_bytes(&src, Direction::DocxToUdf, &mut w)
        .unwrap_or_else(|e| panic!("dönüştürülemedi: {:?} {}", e.code, e.detail));
    let (back, _) = read_udf(&udf_bytes);
    assert_eq!(ink(&text(&back)), "Derin:2026/77K.");
    assert_eq!(table_depths(&back), vec![0, 1, 2]);
}

#[test]
fn four_level_uyap_tables_survive_udf_to_word_to_udf() {
    // Gerçek icra formlarının şekli: dış tablo → hücre → tablo → … dört düzey,
    // ara hücrelerin tek çocuğu tablo.
    let mut u = Udf::default();
    let top = u.p("İCRA DAİRESİNE");
    let deep = u.p("Alacak: 1.250.000,00 TL – 2026/789 Esas");
    let side = u.p("Faiz %12");
    let tail = u.p("Saygılarımla.");
    let lvl4 = utable("100", &urow("", &[deep]));
    let lvl3 = utable("60,40", &urow("", &[lvl4, side]));
    let lvl2 = utable("100", &urow("", &[lvl3]));
    let lvl1 = utable("100", &urow("", &[format!("{top}{lvl2}")]));
    let src = u.build(&format!("{lvl1}{tail}"));

    let (orig, _) = read_udf(&src);
    assert_eq!(table_depths(&orig), vec![0, 1, 2, 3], "fixture");
    let (word, _) = convert(&src, Direction::UdfToDocx);
    let (again, warns) = convert(&word, Direction::DocxToUdf);
    let (back, _) = read_udf(&again);
    assert_eq!(
        ink(&text(&back)),
        ink(&text(&orig)),
        "metin kayboldu; uyarılar {warns:?}"
    );
    assert_eq!(table_depths(&back), vec![0, 1, 2, 3]);
}

// ------------------------------------------------------------ Word hücre/satır durumu

const SINGLE_ALL: &str = r#"<w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders>"#;
const NONE_CELL: &str = r#"<w:tcBorders><w:top w:val="nil"/><w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/></w:tcBorders>"#;

fn first_table(d: &Document) -> &Table {
    d.sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .expect("tablo yok")
}

fn table_in(c: &TableCell) -> &Table {
    c.blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .expect("hücrede iç tablo yok")
}

fn cell_text(c: &TableCell) -> String {
    text(&Document {
        sections: vec![Section {
            blocks: c.blocks.clone(),
            ..Default::default()
        }],
        ..Default::default()
    })
}

#[test]
fn a_nested_table_leaves_the_outer_cell_and_row_as_word_shows_them() {
    // Dış hücre iki sütunu kaplar ve dikeyde ortalıdır; satır başlık satırıdır
    // ve 100 pt yüksekliktedir. İç tablonun kendi değerleri başkadır; son iç
    // hücresi üstteki hücreyle dikey birleşiktir (vMerge).
    let inner = tbl(
        "",
        &[1000, 1000],
        &[
            tr(
                r#"<w:trHeight w:val="400"/>"#,
                &[
                    tc(r#"<w:vAlign w:val="bottom"/>"#, &p("iç 1")),
                    tc("", &p("iç 2")),
                ],
            ),
            tr("", &[tc("", &p("iç 3")), tc("<w:vMerge/>", "<w:p/>")]),
        ],
    );
    let outer = tbl(
        "",
        &[2000, 2000, 2000],
        &[
            tr(
                r#"<w:tblHeader/><w:trHeight w:val="2000"/>"#,
                &[
                    tc(
                        r#"<w:gridSpan w:val="2"/><w:vAlign w:val="center"/>"#,
                        &format!("{}{inner}<w:p/>", p("Dış hücre: 2026/129 E.")),
                    ),
                    tc("", &p("Komşu")),
                ],
            ),
            tr("", &[tc("", &p("a")), tc("", &p("b")), tc("", &p("c"))]),
        ],
    );
    let (d, _) = read_docx(&docx_package(&outer));
    let t = first_table(&d);
    let row = &t.rows[0];
    let outer_cell = &row.cells[0];
    let mut wrong = Vec::new();
    if !cell_text(outer_cell).contains("Dış hücre: 2026/129 E.") {
        wrong.push(format!("dış hücre metni: {:?}", cell_text(outer_cell)));
    }
    if outer_cell.grid_span != 2 {
        wrong.push(format!("gridSpan {}", outer_cell.grid_span));
    }
    if outer_cell.vertical_align != CellVAlign::Center {
        wrong.push(format!("vAlign {:?}", outer_cell.vertical_align));
    }
    if !row.is_header {
        wrong.push("tblHeader kayboldu".into());
    }
    if row.height_pt != Some(100.0) {
        wrong.push(format!("trHeight {:?}", row.height_pt));
    }
    if row.grid_span_total() != 3 {
        wrong.push(format!("satır {} sütun", row.grid_span_total()));
    }
    if wrong.is_empty() {
        // İç tablo da kendi değerlerini korur.
        let it = table_in(outer_cell);
        assert_eq!(it.rows[0].height_pt, Some(20.0));
        assert!(!it.rows[0].is_header);
        assert_eq!(it.rows[0].cells[0].vertical_align, CellVAlign::Bottom);
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn an_invisible_layout_table_stays_borderless_when_a_cell_holds_a_table() {
    // 3abd129'un tamamlanmamış yanı: dilekçe başlığının görünmez yerleşim
    // tablosu (tablo "single", her hücre "nil"), bir hücresinde iç tablo
    // varsa bütün kenarlıklarıyla çiziliyordu.
    let inner = tbl("", &[1000], &[tr("", &[tc("", &p("iç"))])]);
    let outer = tbl(
        SINGLE_ALL,
        &[2000, 4000],
        &[
            tr(
                "",
                &[
                    tc(NONE_CELL, &p("DAVACI")),
                    tc(NONE_CELL, &format!("{inner}<w:p/>")),
                ],
            ),
            tr(
                "",
                &[tc(NONE_CELL, &p("VEKİLİ")), tc(NONE_CELL, &p("Av. Örnek"))],
            ),
        ],
    );
    let (d, warns) = read_docx(&docx_package(&outer));
    assert_eq!(first_table(&d).border, TableBorder::None, "{warns:?}");
    assert!(
        !warns.contains(&WarningCode::TableBordersApproximated),
        "{warns:?}"
    );
}

// ------------------------------------------------------------ ızgara boşlukları

/// Her satırda metin taşıyan hücrelerin (başlangıç sütunu, metin) listesi.
fn placed(t: &Table) -> Vec<Vec<(usize, String)>> {
    t.rows
        .iter()
        .map(|r| {
            let mut col = 0;
            let mut out = Vec::new();
            for c in &r.cells {
                let s = cell_text(c).trim().to_string();
                if !s.is_empty() {
                    out.push((col, s));
                }
                col += c.grid_span.max(1);
            }
            out
        })
        .collect()
}

fn gap_table(tbl_pr: &str) -> String {
    // Üç sütunlu ızgara: ikinci satır ilk sütunu atlar (gridBefore), üçüncü
    // satır son sütunu boş bırakır (gridAfter).
    tbl(
        tbl_pr,
        &[2000, 2000, 2000],
        &[
            tr("", &[tc("", &p("a1")), tc("", &p("a2")), tc("", &p("a3"))]),
            tr(
                r#"<w:gridBefore w:val="1"/>"#,
                &[tc("", &p("b2")), tc("", &p("b3"))],
            ),
            tr(
                r#"<w:gridAfter w:val="1"/>"#,
                &[tc("", &p("c1")), tc("", &p("c2"))],
            ),
        ],
    )
}

fn s(col: usize, t: &str) -> (usize, String) {
    (col, t.to_string())
}

#[test]
fn grid_before_and_after_keep_real_cells_in_their_columns() {
    let want = vec![
        vec![s(0, "a1"), s(1, "a2"), s(2, "a3")],
        vec![s(1, "b2"), s(2, "b3")],
        vec![s(0, "c1"), s(1, "c2")],
    ];
    let src = docx_package(&gap_table(""));
    let (d, warns) = read_docx(&src);
    let t = first_table(&d);
    assert_eq!(placed(t), want, "Word modeli");
    assert!(
        t.rows.iter().all(|r| r.grid_span_total() == 3),
        "satırlar ızgarayı doldurmuyor"
    );
    // Kenarlıksız tabloda boşluk hücresi de kenarlıksızdır: yaklaşık uyarı yok.
    assert_eq!(t.border, TableBorder::None);
    assert!(
        !warns.contains(&WarningCode::TableBordersApproximated),
        "{warns:?}"
    );
    // UDF'e ve geri: gerçek hücreler yine kendi sütunlarında.
    let (udf_bytes, _) = convert(&src, Direction::DocxToUdf);
    let (back, _) = read_udf(&udf_bytes);
    assert_eq!(placed(first_table(&back)), want, "UDF");
}

#[test]
fn a_grid_gap_in_a_bordered_table_is_reported_not_drawn_silently() {
    // Word boşluk bölgesine kenarlık çizmez; UDF hücre başına kenarlık
    // tutamadığı için boşluk hücresi de çizilir. Bu dürüstçe bildirilmeli.
    let (d, warns) = read_docx(&docx_package(&gap_table(SINGLE_ALL)));
    assert_eq!(first_table(&d).border, TableBorder::Cell);
    assert!(
        warns.contains(&WarningCode::TableBordersApproximated),
        "{warns:?}"
    );
}

// ------------------------------------------------------------ silinmiş satırlar

const REV: &str = r#"w:id="7" w:author="Örnek" w:date="2026-01-01T00:00:00Z""#;

fn deleted_row(cells: &[&str], tc_pr: &str) -> String {
    let cells: Vec<String> = cells
        .iter()
        .map(|t| {
            tc(
                tc_pr,
                &format!(
                    r#"<w:p><w:del {REV}><w:r><w:delText>{t}</w:delText></w:r></w:del></w:p>"#
                ),
            )
        })
        .collect();
    tr(&format!("<w:del {REV}/>"), &cells)
}

#[test]
fn a_tracked_deleted_row_is_gone_once_changes_are_accepted() {
    let body = tbl(
        "",
        &[3000, 3000],
        &[
            tr("", &[tc("", &p("Esas No")), tc("", &p("2026/129"))]),
            deleted_row(&["Karar No", "2026/77"], ""),
            tr("", &[tc("", &p("Faiz")), tc("", &p("%12"))]),
        ],
    );
    let (d, _) = read_docx(&docx_package(&format!("{body}{}", p("Son."))));
    assert_eq!(
        placed(first_table(&d)),
        vec![
            vec![s(0, "Esas No"), s(1, "2026/129")],
            vec![s(0, "Faiz"), s(1, "%12")],
        ]
    );
}

#[test]
fn a_deleted_row_does_not_decide_the_borders_of_the_rows_that_remain() {
    // Görünmez yerleşim tablosu; yalnız silinmiş satırın hücreleri çizgili.
    let single_cell = r#"<w:tcBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/></w:tcBorders>"#;
    let body = tbl(
        "",
        &[3000, 3000],
        &[
            tr(
                "",
                &[tc(NONE_CELL, &p("DAVACI")), tc(NONE_CELL, &p("Örnek A.Ş."))],
            ),
            deleted_row(&["eski", "satır"], single_cell),
            tr(
                "",
                &[tc(NONE_CELL, &p("VEKİLİ")), tc(NONE_CELL, &p("Av. Örnek"))],
            ),
        ],
    );
    let (d, warns) = read_docx(&docx_package(&body));
    assert_eq!(first_table(&d).border, TableBorder::None, "{warns:?}");
    assert!(
        !warns.contains(&WarningCode::TableBordersApproximated),
        "{warns:?}"
    );
}

#[test]
fn a_table_whose_every_row_was_deleted_is_not_left_behind() {
    let body = format!(
        "{}{}{}",
        p("Önce."),
        tbl("", &[3000], &[deleted_row(&["Silinen"], "")]),
        p("Sonra.")
    );
    let (d, _) = read_docx(&docx_package(&body));
    assert!(table_depths(&d).is_empty(), "boş tablo kaldı");
    assert_eq!(ink(&text(&d)), "Önce.Sonra.");
}

// ------------------------------------------------------------ UDF'in alışılmadık yapıları

/// Tampondaki bütün görünen metin (yalnız paragraf sonlandırıcıları değil).
fn buffer_ink(u: &Udf) -> String {
    ink(&u.buf)
}

fn read_udf_full(bytes: &[u8]) -> (Document, Vec<tavzih_core::warnings::Warning>) {
    let mut w = WarningSink::new();
    let d = udf::reader::read_udf(bytes, &mut w).expect("read udf");
    (d, w.into_vec())
}

/// Okuyucunun yerleştiremediği metin için verilen kayıp uyarısı.
const UNPLACED: &str = "UNRECOGNIZED_TEXT_DROPPED";

#[test]
fn a_row_written_as_a_paragraph_keeps_its_cell() {
    // Gerçek bir UYAP dosyasının şekli (D591): tablonun ikinci satırı <row>
    // değil, <paragraph height=".."> olarak yazılmış ve hücreyi o taşıyor.
    // Hücre (beş paragraf, 20 görünür karakter) sessizce düşüyordu.
    let mut u = Udf::default();
    let a = u.p("Borçlu: Örnek Ltd.");
    let b1 = u.p("Alacak: 1.250.000,00 TL");
    let b2 = u.p("Faiz %12");
    let after = u.p("Sonuç.");
    let row1 = urow("", std::slice::from_ref(&a));
    let el = format!(
        r#"{}{after}"#,
        utable(
            "100",
            &format!(r#"{row1}<paragraph height="22.0"><cell>{b1}{b2}</cell></paragraph>"#)
        )
    );
    let src = u.build(&el);
    let (d, warns) = read_udf_full(&src);
    assert_eq!(ink(&text(&d)), buffer_ink(&u), "metin kayboldu: {warns:?}");
    let t = first_table(&d);
    assert_eq!(t.rows.len(), 2, "satır kayboldu");
    assert_eq!(t.rows[1].height_pt, Some(22.0));
    assert!(warns.iter().all(|w| w.code.as_str() != UNPLACED));
    // Word'e dönüşte de iki satır ve bütün metin.
    let (word, _) = convert(&src, Direction::UdfToDocx);
    let (wd, _) = read_docx(&word);
    assert_eq!(first_table(&wd).rows.len(), 2);
    assert_eq!(ink(&text(&wd)), buffer_ink(&u));
}

#[test]
fn a_cell_placed_directly_in_a_table_is_kept() {
    let mut u = Udf::default();
    let a = u.p("Satırsız hücre: 2026/129 E.");
    let tail = u.p("Son.");
    let src = u.build(&format!(
        "{}{tail}",
        utable("100", &format!("<cell>{a}</cell>"))
    ));
    let (d, warns) = read_udf_full(&src);
    assert_eq!(ink(&text(&d)), buffer_ink(&u), "{warns:?}");
    assert_eq!(first_table(&d).rows.len(), 1);
}

#[test]
fn a_paragraph_directly_in_a_table_without_a_cell_stays_a_paragraph() {
    // Koruma: tablonun doğrudan çocuğu olan paragraf yalnız hücre taşıyorsa
    // satırdır; kendi metni olan paragraf eskisi gibi paragraf olarak okunur
    // (eskisi gibi tablonun dışına yazılır; burada yalnız kaybolmadığına bakılır).
    let mut u = Udf::default();
    let a = u.p("Satır");
    let stray = u.p("Tablo içinde serbest paragraf");
    let tail = u.p("Son.");
    let src = u.build(&format!(
        "{}{tail}",
        utable("100", &format!("{}{stray}", urow("", &[a])))
    ));
    let (d, warns) = read_udf_full(&src);
    let got = text(&d);
    assert!(got.contains("Tablo içinde serbest paragraf"), "{warns:?}");
    assert_eq!(
        ink(&got).chars().count(),
        buffer_ink(&u).chars().count(),
        "{warns:?}"
    );
    assert_eq!(first_table(&d).rows.len(), 1);
}

#[test]
fn a_leaf_written_with_an_end_tag_is_read_like_an_empty_one() {
    let mut u = Udf::default();
    let leaf = u
        .leaf("content", "Açık etiketli yaprak.\n")
        .replace(" />", "></content>");
    let tab = u.leaf("tab", "\t").replace(" />", "></tab>");
    let tail = u.leaf("content", "son\n");
    let src = u.build(&format!(
        "<paragraph>{leaf}</paragraph><paragraph>{tab}{tail}</paragraph>"
    ));
    let (d, warns) = read_udf_full(&src);
    assert_eq!(ink(&text(&d)), buffer_ink(&u), "{warns:?}");
    assert!(text(&d).contains("\tson"), "sekme kayboldu: {:?}", text(&d));
}

#[test]
fn a_paragraph_opened_inside_another_keeps_the_outer_text_in_order() {
    let mut u = Udf::default();
    let before = u.leaf("content", "Önce ");
    let inner = u.p("İçteki");
    let after = u.leaf("content", "sonra.\n");
    let src = u.build(&format!(
        r#"<paragraph Alignment="1">{before}{inner}{after}</paragraph>"#
    ));
    let (d, warns) = read_udf_full(&src);
    let lines: Vec<String> = text(&d).lines().map(|l| l.trim().to_string()).collect();
    assert_eq!(lines, vec!["Önce", "İçteki", "sonra."], "{warns:?}");
    // Dıştaki paragrafın biçimi iki parçasında da kalır.
    let aligned = d.sections[0]
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Paragraph(p) if p.props.alignment == Alignment::Center))
        .count();
    assert_eq!(aligned, 2);
}

#[test]
fn text_the_reader_cannot_place_is_reported_as_lost() {
    // Güvenlik ağı: tamponda görünen metin modele ulaşmadıysa sessiz kalınmaz.
    // Burada metni, okuyucunun tanımadığı bir öğe gösteriyor.
    let mut u = Udf::default();
    let known = u.p("Bilinen paragraf.");
    let unknown = u.leaf("ozelAlan", "Bilinmeyen 2026/1");
    let tail = u.leaf("content", "\n");
    let src = u.build(&format!("{known}<paragraph>{unknown}{tail}</paragraph>"));
    let (_, warns) = read_udf_full(&src);
    let w = warns
        .iter()
        .find(|w| w.code.as_str() == UNPLACED)
        .unwrap_or_else(|| panic!("kayıp sessiz kaldı: {warns:?}"));
    assert!(w.data_loss);
    assert_eq!(w.detail.as_deref(), Some("16 karakter"));
}

// ------------------------------------------------------------ birleşik UDF satırları

/// `grid` tablo ızgarası; ikinci satır `row_spans` genişliklerinde
/// `row_spans`'taki kadar hücre taşır (boşsa satır `columnSpans`'sız).
/// Okuyucunun bu satıra verdiği gridSpan listesi döner.
fn merged_row_spans(grid: &str, cells: usize, row_spans: &str) -> Vec<usize> {
    merged_row_read(grid, cells, row_spans).0
}

/// [`merged_row_spans`] ve okuyucunun uyarı kodları.
fn merged_row_read(grid: &str, cells: usize, row_spans: &str) -> (Vec<usize>, Vec<String>) {
    let mut u = Udf::default();
    let cols = grid.split(',').count();
    let full: Vec<String> = (0..cols).map(|i| u.p(&format!("s{i}"))).collect();
    let merged: Vec<String> = (0..cells).map(|i| u.p(&format!("m{i}"))).collect();
    let attrs = if row_spans.is_empty() {
        String::new()
    } else {
        format!(r#" columnSpans="{row_spans}""#)
    };
    let rows = format!("{}{}", urow("", &full), urow(&attrs, &merged));
    let (d, warns) = read_udf(&u.build(&utable(grid, &rows)));
    let spans = first_table(&d).rows[1]
        .cells
        .iter()
        .map(|c| c.grid_span)
        .collect();
    (
        spans,
        warns.iter().map(|c| c.as_str().to_string()).collect(),
    )
}

#[test]
fn a_merged_udf_row_takes_its_columns_from_its_own_widths() {
    let cases = [
        // (ızgara, satır genişlikleri, beklenen gridSpan) — gerçek şekiller:
        // D135 tablosu; sondaki birleşik hücre (chains F3); D296/D555-D572
        // icra formu (en yakın sütun sınırı); D136 (22/78 oranlı satır).
        ("38,302,43,64,85", "38,409,85", vec![1, 3, 1]),
        ("100,100,100", "100,200", vec![1, 2]),
        ("100,5,100", "91,109", vec![1, 2]),
        ("120,419,120,419", "120,419", vec![1, 3]),
    ];
    let mut wrong = Vec::new();
    for (grid, row, want) in cases {
        let n = row.split(',').count();
        let got = merged_row_spans(grid, n, row);
        if got != want {
            wrong.push(format!(
                "ızgara {grid}, satır {row}: {got:?} (beklenen {want:?})"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn a_merged_udf_row_without_widths_is_still_spread_evenly() {
    // Satırın kendi genişlikleri yoksa eski kural sürer.
    assert_eq!(merged_row_spans("100,100,100", 2, ""), vec![2, 1]);
}

const WIDTHS_APPROX: &str = "TABLE_COLUMN_WIDTHS_APPROXIMATED";

#[test]
fn a_merged_row_whose_widths_miss_the_grid_is_reported() {
    // D136 şekli: satırın sınırı hiçbir sütun sınırına yakın değil. En yakın
    // sınır ilk hücreye satırın %11'ini verir, kaynakta %22'dir. Word ızgarası
    // ancak bu yaklaşıklıkla kurulabiliyor; sessiz kalınmaz.
    let (_, warns) = merged_row_read("120,419,120,419", 2, "120,419");
    assert!(warns.iter().any(|c| c == WIDTHS_APPROX), "{warns:?}");
    // Koruma: tam oturan (D135) ve birkaç puanlık yuvarlama farkı olan (icra
    // formu, 3 puan) satırlar için uyarı yok.
    for (grid, row) in [("38,302,43,64,85", "38,409,85"), ("100,5,100", "91,109")] {
        let (_, warns) = merged_row_read(grid, row.split(',').count(), row);
        assert!(
            !warns.iter().any(|c| c == WIDTHS_APPROX),
            "{grid}: {warns:?}"
        );
    }
}

#[test]
fn a_word_merge_at_the_end_of_a_row_survives_word_to_udf_to_word() {
    // Word'de [a][b+c]: UDF'e satır genişlikleriyle yazılır, geri okununca
    // birleşik hücre yine sonda olmalı ([b+c][a] değil).
    let body = tbl(
        "",
        &[2000, 2000, 2000],
        &[
            tr("", &[tc("", &p("1")), tc("", &p("2")), tc("", &p("3"))]),
            tr(
                "",
                &[tc("", &p("a")), tc(r#"<w:gridSpan w:val="2"/>"#, &p("b+c"))],
            ),
        ],
    );
    let (udf_bytes, _) = convert(&docx_package(&body), Direction::DocxToUdf);
    let (word, _) = convert(&udf_bytes, Direction::UdfToDocx);
    let (d, _) = read_docx(&word);
    assert_eq!(
        placed(first_table(&d))[1],
        vec![s(0, "a"), s(1, "b+c")],
        "birleşik hücre yer değiştirdi"
    );
}

// ------------------------------------------------------------ bitişik tablolar

fn adjacent_udf_tables() -> (Vec<u8>, String) {
    let mut u = Udf::default();
    let a = u.p("Birinci tablo");
    let b = u.p("İkinci tablo");
    let tail = u.p("Son.");
    let el = format!(
        "{}{}{tail}",
        utable("100", &urow("", &[a])),
        utable("100", &urow("", &[b]))
    );
    (u.build(&el), buffer_ink(&u))
}

#[test]
fn adjacent_udf_tables_come_back_adjacent_after_word() {
    let (src, ink_src) = adjacent_udf_tables();
    let (word, _) = convert(&src, Direction::UdfToDocx);
    let (again, _) = convert(&word, Direction::DocxToUdf);
    let xml = zip_part(&again, "content.xml");
    let between = &xml[xml.find("</table>").unwrap()..xml.rfind("<table").unwrap()];
    assert!(
        !between.contains("<paragraph"),
        "tabloların arasına paragraf girdi: {between}"
    );
    let (back, _) = read_udf(&again);
    assert_eq!(ink(&text(&back)), ink_src);
}

#[test]
fn the_word_filler_between_adjacent_tables_is_not_shown() {
    // Zorunlu ara paragrafın işareti gizlidir: Word iki tabloyu UYAP'taki gibi
    // bitişik gösterir; araya UDF'te olmayan boş bir satır koymaz.
    let (src, _) = adjacent_udf_tables();
    let (word, _) = convert(&src, Direction::UdfToDocx);
    let doc = zip_part(&word, "word/document.xml");
    let between = &doc[doc.find("</w:tbl>").unwrap()..doc.rfind("<w:tbl>").unwrap()];
    assert!(between.contains("<w:vanish/>"), "{between}");
}

#[test]
fn a_blank_line_the_author_put_between_tables_is_kept() {
    // Koruma: Word belgesinde iki tablo arasındaki görünür boş paragraf
    // yazarındır; UDF'e boş bir satır olarak gider.
    let t = |s: &str| tbl("", &[3000], &[tr("", &[tc("", &p(s))])]);
    let body = format!("{}<w:p/>{}{}", t("A"), t("B"), p("Son."));
    let (udf_bytes, _) = convert(&docx_package(&body), Direction::DocxToUdf);
    let xml = zip_part(&udf_bytes, "content.xml");
    let between = &xml[xml.find("</table>").unwrap()..xml.rfind("<table").unwrap()];
    assert!(between.contains("<paragraph"), "{between}");
}
