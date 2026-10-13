//! Tablo YAPISI iki yönde de korunur.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (199 gerçek Word belgesi ve 293
//! gerçek UDF üzerinde ölçüldü, burada sentetik olarak kurulur):
//! - UYAP'ın kendi dosyaları tabloyu hücrenin içine dört düzeye kadar iç içe
//!   koyar (22 UDF'te 224 iç tablo). Word → UDF yalnız ilk iç düzeyin
//!   paragraflarını yazıyordu; daha derindeki tabloların metni "düzleştirildi"
//!   uyarısının arkasında kayboluyordu (UDF → Word → UDF'te 21 gerçek belge).
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
