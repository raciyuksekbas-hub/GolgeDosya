//! Bozuk konumlu UDF'ler: metin ya kopyalanmadan bir kez gelir ya da
//! kullanıcıya söylenir; hiçbir zaman sessizce çoğalmaz, kaymaz, düşmez.
//!
//! Denetim bulguları (UF-15, UF-16): üst üste binen iki aralık "abcdcdef"
//! veriyordu; okunamayan bir `startOffset` 0'dan okunup ilk satırı
//! yineliyordu; yasal biçimde bölünmüş bir CDATA "ab]]>" gibi bozuk metin
//! veriyordu; ikinci `<header>` birincisini sessizce siliyordu; tek bir bozuk
//! görsel (base64 değil) bütün belgeyi "geçerli bir UDF değil" diye
//! düşürüyordu. Hepsi uyarısızdı. Gerçek derlemde (293 UDF) bunların hiçbiri
//! yok; bu dosya sağlamlık açığını kapatır.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::Write;

use tavzih_core::model::*;
use tavzih_core::udf;
use tavzih_core::warnings::{Warning, WarningSink};

const TAIL: &str = r#"<properties><pageFormat mediaSizeName="1" leftMargin="70.0" rightMargin="70.0" topMargin="70.0" bottomMargin="70.0" paperOrientation="1" headerFOffset="20.0" footerFOffset="20.0" /></properties>"#;
const STYLES: &str =
    r#"<styles><style name="hvl-default" family="Dialog" size="12" /></styles></template>"#;

/// `content` CDATA sarmalayıcısı dâhil olduğu gibi yazılır (bölünmüş CDATA
/// sınamak için).
fn udf_raw(content: &str, elements: &str) -> Vec<u8> {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?><template format_id="1.8"><content>{content}</content>{TAIL}<elements resolver="hvl-default">{elements}</elements>{STYLES}"#
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

fn udf(buffer: &str, elements: &str) -> Vec<u8> {
    udf_raw(&format!("<![CDATA[{buffer}]]>"), elements)
}

fn read(bytes: &[u8]) -> (Document, Vec<Warning>) {
    let mut w = WarningSink::new();
    let d = udf::reader::read_udf(bytes, &mut w).expect("read udf");
    (d, w.into_vec())
}

fn paragraphs(blocks: &[Block]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p.text()),
            _ => None,
        })
        .collect()
}

fn body(d: &Document) -> Vec<String> {
    paragraphs(&d.sections[0].blocks)
}

fn codes(ws: &[Warning]) -> Vec<&'static str> {
    ws.iter().map(|w| w.code.as_str()).collect()
}

fn leaf(start: &str, len: &str) -> String {
    format!(r#"<content startOffset="{start}" length="{len}"/>"#)
}

// ------------------------------------------------------------ üst üste binen aralık

#[test]
fn overlapping_ranges_are_published_once_and_reported() {
    // [0,4) "abcd" ve [2,7) "cdef\n": ortak "cd" iki kez yazılıyordu.
    let bytes = udf(
        "abcdef\n",
        &format!(
            "<paragraph>{}{}</paragraph>",
            leaf("0", "4"),
            leaf("2", "5")
        ),
    );
    let (d, ws) = read(&bytes);
    assert_eq!(body(&d), vec!["abcdef".to_string()]);
    assert!(
        codes(&ws).contains(&"TEXT_RANGE_OVERLAP"),
        "uyarı yok: {:?}",
        codes(&ws)
    );
}

// ------------------------------------------------------------ okunamayan konum

#[test]
fn an_unreadable_start_offset_is_skipped_and_reported_not_read_from_zero() {
    // Okunamayan konum 0 sayılıyor, ilk satır ikinci paragrafta yineleniyordu.
    for bad in [
        r#"<content startOffset="x" length="13"/>"#,
        r#"<content length="13"/>"#,
    ] {
        let bytes = udf(
            "Birinci satır\nİkinci satır\n",
            &format!(
                "<paragraph>{}</paragraph><paragraph>{bad}</paragraph>",
                leaf("0", "14")
            ),
        );
        let (d, ws) = read(&bytes);
        let all = body(&d).join("\n");
        assert_eq!(all.matches("Birinci satır").count(), 1, "{bad}: {all:?}");
        assert!(
            codes(&ws).contains(&"TEXT_RANGE_INVALID"),
            "{bad}: uyarı yok: {:?}",
            codes(&ws)
        );
    }
}

#[test]
fn a_range_beyond_the_buffer_is_reported() {
    let bytes = udf(
        "Kısa\n",
        &format!(
            "<paragraph>{}</paragraph><paragraph>{}</paragraph>",
            leaf("0", "5"),
            leaf("100", "5")
        ),
    );
    let (d, ws) = read(&bytes);
    assert_eq!(body(&d)[0], "Kısa");
    assert!(
        codes(&ws).contains(&"TEXT_RANGE_INVALID"),
        "uyarı yok: {:?}",
        codes(&ws)
    );
}

#[test]
fn a_range_that_overruns_the_end_by_one_is_still_tolerated() {
    // Gerçek UYAP çıktısı son öğede bazen tamponun bir birim ötesine taşar
    // (okuyucunun bilerek tolere ettiği durum): metin gelir, uyarı yok.
    let bytes = udf(
        "Son satır\n",
        &format!("<paragraph>{}</paragraph>", leaf("0", "11")),
    );
    let (d, ws) = read(&bytes);
    assert_eq!(body(&d), vec!["Son satır".to_string()]);
    assert!(ws.is_empty(), "{:?}", codes(&ws));
}

// ------------------------------------------------------------ CDATA

#[test]
fn a_cdata_buffer_split_into_sections_is_joined() {
    // XML'de bir öğenin metni birden çok CDATA bölümüne yasal olarak
    // bölünebilir; eskiden yalnız "]]]]><![CDATA[>" kaçışı birleştiriliyor,
    // başka bölünme "ab]]>" gibi bozuk metin veriyordu.
    let bytes = udf_raw(
        "<![CDATA[ab]]><![CDATA[cd\n]]>",
        &format!("<paragraph>{}</paragraph>", leaf("0", "5")),
    );
    let (d, _) = read(&bytes);
    assert_eq!(body(&d), vec!["abcd".to_string()]);
}

#[test]
fn an_escaped_cdata_terminator_is_restored() {
    // Metindeki "]]>" yazarken bölünerek kaçırılır; okuyunca geri gelmeli.
    let bytes = udf_raw(
        "<![CDATA[x]]]]><![CDATA[>y\n]]>",
        &format!("<paragraph>{}</paragraph>", leaf("0", "6")),
    );
    let (d, _) = read(&bytes);
    assert_eq!(body(&d), vec!["x]]>y".to_string()]);
}

// ------------------------------------------------------------ ikinci üst/alt bilgi

#[test]
fn a_second_header_or_footer_is_kept_and_reported() {
    // UDF'te bir üst bilgi vardır; ikinci <header> birinciyi sessizce
    // siliyordu. İkisinin metni de kalır, kullanıcıya söylenir.
    let elements = format!(
        "<header><paragraph>{}</paragraph></header><header><paragraph>{}</paragraph></header>\
         <paragraph>{}</paragraph>\
         <footer><paragraph>{}</paragraph></footer><footer><paragraph>{}</paragraph></footer>",
        leaf("0", "6"),
        leaf("6", "6"),
        leaf("12", "6"),
        leaf("18", "6"),
        leaf("24", "6"),
    );
    let bytes = udf("Üst A\nÜst B\nGövde\nAlt A\nAlt B\n", &elements);
    let (d, ws) = read(&bytes);
    let header = paragraphs(&d.sections[0].header.as_ref().expect("üst bilgi").blocks);
    let footer = paragraphs(&d.sections[0].footer.as_ref().expect("alt bilgi").blocks);
    assert_eq!(header, vec!["Üst A".to_string(), "Üst B".to_string()]);
    assert_eq!(footer, vec!["Alt A".to_string(), "Alt B".to_string()]);
    assert_eq!(body(&d), vec!["Gövde".to_string()]);
    assert!(
        codes(&ws).contains(&"HEADER_FOOTER_VARIANT_DROPPED"),
        "uyarı yok: {:?}",
        codes(&ws)
    );
}

// ------------------------------------------------------------ bozuk görsel

#[test]
fn an_undecodable_image_is_skipped_with_a_warning() {
    // Tek bir bozuk görsel bütün dönüştürmeyi düşürüyordu.
    let elements = format!(
        r#"<paragraph>{}<image imageData="@@@ bu base64 değil @@@" width="10" height="10" startOffset="6" length="1"/></paragraph>"#,
        leaf("0", "6")
    );
    let mut w = WarningSink::new();
    let d = udf::reader::read_udf(&udf("Metin \u{00B8}\n", &elements), &mut w)
        .expect("bozuk görsel belgeyi düşürmemeli");
    assert_eq!(body(&d), vec!["Metin ".to_string()]);
    let ws = w.into_vec();
    let img = ws
        .iter()
        .find(|w| w.code.as_str() == "IMAGE_FORMAT_UNSUPPORTED")
        .expect("görsel uyarısı yok");
    assert_eq!(img.detail.as_deref(), Some("bozuk görsel verisi"));
}
