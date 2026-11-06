//! Dilekçe başlığındaki görünmez tablo UDF'te sekmeli satır olur.
//!
//! Saha: "Dilekçeler Word'den UDF'ye çevirirken blok hâlinde oluyor." Başlık
//! bloğu (Dosya No. / Davacılar / Vekili / Konu) Word'de kenarlıksız bir
//! tablodur; UDF'te tablo kalınca UYAP onu blok gibi seçtiriyor ve hücre hücre
//! düzenletiyordu. UYAP'ta yazılan dilekçenin biçimi: `Etiket⇥:⇥Değer`, değer
//! sütununda asılı girinti, duraklar kenar boşluğundan (corpus.rs c50).
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{
    convert_bytes, convert_bytes_with, ConvertOptions, Direction, LayoutTables,
};
use tavzih_core::docx;
use tavzih_core::model::*;
use tavzih_core::warnings::{Severity, WarningCode, WarningSink};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn package(body: &str, numbering: Option<&str>) -> Vec<u8> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1417" w:right="1418" w:bottom="1417" w:left="1418" w:header="708" w:footer="708"/></w:sectPr></w:body></w:document>"#
    );
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        z.start_file("word/document.xml", o).unwrap();
        z.write_all(document.as_bytes()).unwrap();
        if let Some(n) = numbering {
            z.start_file("word/numbering.xml", o).unwrap();
            z.write_all(
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="{W}">{n}</w:numbering>"#
                )
                .as_bytes(),
            )
            .unwrap();
            z.start_file("word/_rels/document.xml.rels", o).unwrap();
            z.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdN" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#).unwrap();
        }
        z.finish().unwrap();
    }
    cur.into_inner()
}

/// Bir hücre paragrafı. `ppr`: paragraf özellikleri.
fn p(ppr: &str, runs: &str) -> String {
    format!("<w:p><w:pPr>{ppr}</w:pPr>{runs}</w:p>")
}
fn r(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}
fn b(text: &str) -> String {
    format!(r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}
fn tc(width: u32, extra: &str, content: &str) -> String {
    format!(r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{extra}</w:tcPr>{content}</w:tc>"#)
}

/// 2381 + 340 + 6350 twip = 119,05 + 17 + 317,5 pt (gerçek dilekçe şablonunun ızgarası).
const GRID: [u32; 3] = [2381, 340, 6350];

fn row(label: &str, value_paragraphs: &[String]) -> String {
    format!(
        "<w:tr>{}{}{}</w:tr>",
        tc(GRID[0], "", &p("", &b(label))),
        tc(GRID[1], "", &p("", &b(":"))),
        tc(GRID[2], "", &value_paragraphs.concat())
    )
}

/// `borders`: tablo düzeyi kenarlık bildirimi (boş: Word'ün kenarlıksız tablosu).
fn table(borders: &str, rows: &[String], grid: &[u32]) -> String {
    let g: String = grid
        .iter()
        .map(|w| format!(r#"<w:gridCol w:w="{w}"/>"#))
        .collect();
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/>{borders}</w:tblPr><w:tblGrid>{g}</w:tblGrid>{}</w:tbl>"#,
        rows.concat()
    )
}

fn petition_header() -> String {
    let justified = |t: &str| p(r#"<w:jc w:val="both"/>"#, &r(t));
    table(
        "",
        &[
            row("Dosya No.", &[justified("2026/0001 E.")]),
            row("DAVACILAR", &[justified("Örnek Kişi, İkinci Örnek")]),
            row("VEKİLİ", &[justified("Av. Örnek Avukat"), justified("Adres antettedir.")]),
            row(
                "KONU",
                &[justified(
                    "Sayın Mahkemenizin 22.10.2026 tarihli ara kararı uyarınca beyanlarımızın sunulmasıdır; bu satır sütuna sığmaz ve alt satıra sarar.",
                )],
            ),
        ],
        &GRID,
    )
}

struct Udf {
    xml: String,
    warns: Vec<(WarningCode, Severity)>,
}

fn to_udf(docx: &[u8], options: &ConvertOptions) -> Udf {
    let mut w = WarningSink::new();
    let udf = convert_bytes_with(docx, Direction::DocxToUdf, options, &mut w).expect("convert");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf)).unwrap();
    let mut xml = String::new();
    z.by_name("content.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    Udf {
        xml,
        warns: w.into_vec().iter().map(|x| (x.code, x.severity)).collect(),
    }
}

/// Gövde paragraflarının açılış etiketleri ve metinleri (sekme `⇥`).
fn lines(xml: &str) -> Vec<(String, String)> {
    let cs = xml.find("<![CDATA[").unwrap() + 9;
    let ce = xml[cs..].find("]]>").unwrap() + cs;
    let buf: Vec<u16> = xml[cs..ce].encode_utf16().collect();
    let body = &xml[xml.find("<elements").unwrap()..];
    let mut out = Vec::new();
    for part in body.split("<paragraph").skip(1) {
        let tag = part[..part.find('>').unwrap()].to_string();
        let end = part.find("</paragraph>").unwrap_or(part.len());
        let mut text = String::new();
        for leaf in part[..end].split("startOffset=\"").skip(1) {
            let so: usize = leaf[..leaf.find('"').unwrap()].parse().unwrap();
            let l = leaf.find("length=\"").unwrap() + 8;
            let len: usize = leaf[l..l + leaf[l..].find('"').unwrap()].parse().unwrap();
            text.push_str(&String::from_utf16_lossy(&buf[so..so + len]));
        }
        out.push((tag, text.trim_end_matches('\n').replace('\t', "⇥")));
    }
    out
}

fn attr(tag: &str, key: &str) -> Option<String> {
    let k = format!(" {key}=\"");
    let at = tag.find(&k)? + k.len();
    Some(tag[at..at + tag[at..].find('"')?].to_string())
}

// ------------------------------------------------------------------ başlık bloğu

#[test]
fn an_invisible_petition_header_becomes_tabbed_lines_not_a_table() {
    let docx = package(&petition_header(), None);
    let udf = to_udf(&docx, &ConvertOptions::default());
    assert!(!udf.xml.contains("<table"), "başlık hâlâ tablo");
    let l = lines(&udf.xml);
    let texts: Vec<&str> = l.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(
        texts[..5],
        [
            "Dosya No.⇥:⇥2026/0001 E.",
            "DAVACILAR⇥:⇥Örnek Kişi, İkinci Örnek",
            "VEKİLİ⇥:⇥Av. Örnek Avukat",
            "Adres antettedir.",
            "KONU⇥:⇥Sayın Mahkemenizin 22.10.2026 tarihli ara kararı uyarınca beyanlarımızın sunulmasıdır; bu satır sütuna sığmaz ve alt satıra sarar.",
        ]
    );
    // UYAP dilekçesinin biçimi: ilk satır kenar boşluğunda, duraklar sütun
    // başlarında, değer sarınca değer sütununun hizasından sürer.
    for (tag, _) in [&l[0], &l[1], &l[2], &l[4]] {
        assert_eq!(attr(tag, "LeftIndent"), None, "{tag}");
        assert_eq!(attr(tag, "Hanging").as_deref(), Some("136.05"), "{tag}");
        assert_eq!(
            attr(tag, "TabSet").as_deref(),
            Some("119.05:0:0,136.05:0:0"),
            "{tag}"
        );
        assert_eq!(attr(tag, "Alignment").as_deref(), Some("3"), "{tag}");
    }
    // Değerin ikinci paragrafı değer sütununda, etiketsiz.
    assert_eq!(attr(&l[3].0, "LeftIndent").as_deref(), Some("136.05"));
    assert_eq!(attr(&l[3].0, "TabSet"), None);
    // Dilekçe aralığı bu satırlara da uygulanır.
    assert!(l[..5]
        .iter()
        .all(|(t, _)| attr(t, "LineSpacing").as_deref() == Some("0.5")));
    // Etiketin kalını korunur.
    assert!(udf.xml.contains(r#"bold="true""#));
    // Söylenir, ama bir kayıp değildir.
    assert!(udf
        .warns
        .contains(&(WarningCode::LayoutTableFlattened, Severity::Info)));
    assert!(
        udf.warns.iter().all(|(_, s)| *s != Severity::Loss),
        "{:?}",
        udf.warns
    );
}

#[test]
fn every_character_of_the_table_survives_in_reading_order() {
    let docx = package(&petition_header(), None);
    let mut w = WarningSink::new();
    let model = docx::reader::read_docx(&docx, &mut w).unwrap();
    let Block::Table(t) = &model.sections[0].blocks[0] else {
        panic!("tablo yok")
    };
    let mut expected = String::new();
    for row in &t.rows {
        for cell in &row.cells {
            for b in &cell.blocks {
                if let Block::Paragraph(p) = b {
                    for r in &p.runs {
                        if let Run::Text { text, .. } = r {
                            expected.push_str(text);
                        }
                    }
                }
            }
        }
    }
    let got: String = lines(&to_udf(&docx, &ConvertOptions::default()).xml)
        .iter()
        .map(|(_, t)| t.replace('⇥', ""))
        .collect();
    let ink = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    assert_eq!(ink(&got), ink(&expected));
}

#[test]
fn the_lines_come_back_to_word_as_tabbed_paragraphs() {
    let docx = package(&petition_header(), None);
    let mut w = WarningSink::new();
    let udf = convert_bytes(&docx, Direction::DocxToUdf, &mut w).unwrap();
    let back = convert_bytes(&udf, Direction::UdfToDocx, &mut w).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(back)).unwrap();
    let mut doc = String::new();
    z.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut doc)
        .unwrap();
    assert!(!doc.contains("<w:tbl>"), "{doc}");
    assert!(
        doc.contains(r#"<w:tab w:val="left" w:pos="2381"/>"#),
        "{doc}"
    );
    assert!(doc.contains(r#"w:hanging="2721""#), "{doc}");
}

#[test]
fn a_word_list_label_inside_the_value_keeps_its_tab() {
    // "1-" Word'de bir liste düzeyidir; UDF'te karşılığı doğrulanmadığı için
    // etiket metin olarak yazılır ve değer sütununda kendi durağına gider.
    let numbering = r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1-"/><w:pPr><w:ind w:left="360" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#;
    let item = |t: &str| {
        p(
            r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#,
            &r(t),
        )
    };
    let body = table(
        "",
        &[row(
            "DAVACILAR",
            &[item("Birinci Davacı"), item("İkinci Davacı")],
        )],
        &GRID,
    );
    let udf = to_udf(&package(&body, Some(numbering)), &ConvertOptions::default());
    assert!(!udf.xml.contains("<table"));
    let l = lines(&udf.xml);
    assert_eq!(l[0].1, "DAVACILAR⇥:⇥1-⇥Birinci Davacı");
    assert_eq!(
        attr(&l[0].0, "TabSet").as_deref(),
        Some("119.05:0:0,136.05:0:0,154.05:0:0")
    );
    assert_eq!(attr(&l[0].0, "Hanging").as_deref(), Some("154.05"));
    // İkinci madde etiketsiz satırda, aynı hizada: satır yine kenar boşluğundan
    // başlar ve bir sekmeyle değer sütununa gider (bütün satırlar aynı
    // duraklarla ölçülür; UYAP'ın sekme tabanı satırın ilk konumudur).
    assert_eq!(l[1].1, "⇥2-⇥İkinci Davacı");
    assert_eq!(
        attr(&l[1].0, "TabSet").as_deref(),
        Some("136.05:0:0,154.05:0:0")
    );
    assert_eq!(attr(&l[1].0, "Hanging").as_deref(), Some("154.05"));
    assert_eq!(attr(&l[1].0, "LeftIndent"), None);
}

#[test]
fn a_centred_signature_block_stays_centred_in_its_column() {
    let centred = |t: &str| p(r#"<w:jc w:val="center"/>"#, &r(t));
    let body = table(
        "",
        &[format!(
            "<w:tr>{}{}</w:tr>",
            tc(4535, "", &p("", "")),
            tc(
                4535,
                "",
                &[centred("Davacılar Vekili"), centred("Av. Örnek Avukat")].concat()
            )
        )],
        &[4535, 4535],
    );
    let udf = to_udf(&package(&body, None), &ConvertOptions::default());
    assert!(!udf.xml.contains("<table"));
    let l = lines(&udf.xml);
    assert_eq!(l[0].1, "Davacılar Vekili");
    assert_eq!(attr(&l[0].0, "Alignment").as_deref(), Some("1"));
    assert_eq!(attr(&l[0].0, "LeftIndent").as_deref(), Some("226.75"));
    assert_eq!(l[1].1, "Av. Örnek Avukat");
}

// ------------------------------------------------------------------ tablo kalanlar

#[test]
fn a_bordered_table_stays_a_table() {
    let borders = r#"<w:tblBorders><w:top w:val="single"/><w:left w:val="single"/><w:bottom w:val="single"/><w:right w:val="single"/><w:insideH w:val="single"/><w:insideV w:val="single"/></w:tblBorders>"#;
    let body = table(
        borders,
        &[row("Kalem", &[p("", &r("1.250.000,00 TL"))])],
        &GRID,
    );
    let udf = to_udf(&package(&body, None), &ConvertOptions::default());
    assert!(udf.xml.contains(r#"border="borderCell""#));
    assert!(!udf
        .warns
        .iter()
        .any(|(c, _)| *c == WarningCode::LayoutTableFlattened));
}

#[test]
fn a_label_too_long_for_its_column_keeps_the_table() {
    // Sekmeli satırda uzun etiket ":" sütununu aşar ve hizayı bozardı.
    let body = table(
        "",
        &[row(
            "DAVALI İDARE VE KATILAN VEKİLİ",
            &[p("", &r("Av. Örnek"))],
        )],
        &GRID,
    );
    let udf = to_udf(&package(&body, None), &ConvertOptions::default());
    assert!(udf.xml.contains("<table"), "taşan etiket satıra çevrildi");
}

#[test]
fn merged_cells_and_wide_text_columns_keep_the_table() {
    // Birleşik hücre.
    let merged = table(
        "",
        &[
            format!(
                "<w:tr>{}{}</w:tr>",
                tc(
                    GRID[0] + GRID[1],
                    r#"<w:gridSpan w:val="2"/>"#,
                    &p("", &r("A"))
                ),
                tc(GRID[2], "", &p("", &r("B")))
            ),
            row("C", &[p("", &r("D"))]),
        ],
        &GRID,
    );
    // Yan yana iki uzun metin sütunu.
    let long = "Birinci sütunda birden çok satıra sarılan uzun bir açıklama metni yer alır.";
    let parallel = table(
        "",
        &[format!(
            "<w:tr>{}{}</w:tr>",
            tc(4535, "", &p("", &r(long))),
            tc(4535, "", &p("", &r(long)))
        )],
        &[4535, 4535],
    );
    for body in [merged, parallel] {
        let udf = to_udf(&package(&body, None), &ConvertOptions::default());
        assert!(udf.xml.contains("<table"), "satıra çevrilmemeliydi");
    }
}

#[test]
fn the_conversion_can_keep_layout_tables_when_asked() {
    let keep = ConvertOptions {
        layout_tables: LayoutTables::Keep,
        ..Default::default()
    };
    let udf = to_udf(&package(&petition_header(), None), &keep);
    assert!(udf.xml.contains(r#"border="borderNone""#));
}

#[test]
fn a_header_template_wider_than_the_margins_is_fitted_to_the_text_width() {
    // Gerçek şablonların çoğunda değer sütunu sağ kenar boşluğuna ~15 pt taşar
    // (2400 + 300 + 6700 twip = 470 pt; yazı alanı 453,5 pt). UDF tabloyu
    // oransal yazdığı için UYAP onu yazı alanına sığdırıyordu; satırlar da öyle.
    let grid = [2400, 300, 6700];
    let body = table(
        "",
        &[format!(
            "<w:tr>{}{}{}</w:tr>",
            tc(grid[0], "", &p("", &b("DAVALI"))),
            tc(grid[1], "", &p("", &b(":"))),
            tc(
                grid[2],
                "",
                &p(r#"<w:jc w:val="both"/>"#, &r("Örnek İdaresi"))
            )
        )],
        &grid,
    );
    let udf = to_udf(&package(&body, None), &ConvertOptions::default());
    assert!(!udf.xml.contains("<table"));
    let l = lines(&udf.xml);
    assert_eq!(l[0].1, "DAVALI⇥:⇥Örnek İdaresi");
    // 120 ve 135 pt, 453,5 / 470 oranıyla: 115,79 ve 130,26.
    assert_eq!(
        attr(&l[0].0, "TabSet").as_deref(),
        Some("115.79:0:0,130.26:0:0")
    );
}

#[test]
fn a_two_column_signature_with_line_breaks_becomes_side_by_side_lines() {
    let centred = |runs: &str| p(r#"<w:jc w:val="center"/>"#, runs);
    let br = "<w:r><w:br/></w:r>";
    let left = centred(&format!("{}{br}{}", r("Davacı"), r("Örnek Kişi")));
    let right = centred(&format!(
        "{}{br}{}",
        r("Davacı Vekili"),
        r("Av. Örnek Avukat")
    ));
    let body = table(
        "",
        &[format!(
            "<w:tr>{}{}</w:tr>",
            tc(4535, "", &left),
            tc(4535, "", &right)
        )],
        &[4535, 4535],
    );
    let udf = to_udf(&package(&body, None), &ConvertOptions::default());
    assert!(!udf.xml.contains("<table"));
    let l = lines(&udf.xml);
    assert_eq!(l[0].1, "⇥Davacı⇥Davacı Vekili");
    assert_eq!(l[1].1, "⇥Örnek Kişi⇥Av. Örnek Avukat");
    // Ortalanmış hücreler ortalanmış sekme duraklarıyla: sütunların ortası.
    assert_eq!(
        attr(&l[0].0, "TabSet").as_deref(),
        Some("113.38:2:0,340.13:2:0")
    );
}

#[test]
fn a_single_column_invisible_box_becomes_plain_paragraphs() {
    let body = table(
        "",
        &[format!(
            "<w:tr>{}</w:tr>",
            tc(
                9070,
                "",
                &[p("", &r("Birinci satır")), p("", &r("İkinci satır"))].concat()
            )
        )],
        &[9070],
    );
    let udf = to_udf(&package(&body, None), &ConvertOptions::default());
    assert!(!udf.xml.contains("<table"));
    let texts: Vec<String> = lines(&udf.xml).into_iter().map(|(_, t)| t).collect();
    assert_eq!(texts[..2], ["Birinci satır", "İkinci satır"]);
}
