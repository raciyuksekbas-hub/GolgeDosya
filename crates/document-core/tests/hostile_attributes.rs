//! Tek etikette çok sayıda öznitelik: quick-xml 0.36'nın yinelenen ad denetimi
//! karesel süre alıyordu (RUSTSEC-2026-0194). Ölçüm (release): 16 000
//! öznitelik 1,0 s, 32 000 öznitelik 74 KB'lık bir UDF'de 4,5 s; 1 MB'lık bir
//! dosya dönüştürmeyi dakikalarca kilitlerdi. Okuyucular denetimi kapalı okur;
//! yinelenen öznitelikte anlam değişmez (ilk geçiş kazanır).
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur.

use std::io::Write;
use std::time::{Duration, Instant};

use tavzih_core::model::*;
use tavzih_core::warnings::WarningSink;
use tavzih_core::{convert_bytes, Direction};

fn udf(paragraph_attrs: &str) -> Vec<u8> {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?><template format_id="1.8"><content><![CDATA[merhaba]]></content><properties><pageFormat mediaSizeName="1" leftMargin="70.0" rightMargin="70.0" topMargin="70.0" bottomMargin="70.0" paperOrientation="1" headerFOffset="20.0" footerFOffset="20.0" /></properties><elements resolver="hvl-default"><paragraph{paragraph_attrs}><content startOffset="0" length="7" /></paragraph></elements><styles><style name="hvl-default" family="Dialog" size="12" /></styles></template>"#
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

#[test]
fn thousands_of_attributes_on_one_tag_convert_in_linear_time() {
    let attrs: String = (0..32_000).map(|i| format!(r#" a{i}="""#)).collect();
    let bytes = udf(&attrs);
    let mut w = WarningSink::new();
    let started = Instant::now();
    let out = convert_bytes(&bytes, Direction::UdfToDocx, &mut w);
    let took = started.elapsed();
    assert!(out.is_ok(), "{:?}", out.err());
    // Karesel denetimle optimize derlemede 4,5 s, hata ayıklama derlemesinde
    // bunun birkaç katıydı; doğrusal okumada milisaniyeler.
    assert!(
        took < Duration::from_secs(2),
        "32 000 öznitelik {took:?} sürdü"
    );
}

#[test]
fn duplicated_attribute_keeps_its_first_value() {
    // 1 = ortala, 3 = iki yana yasla. Denetimli okumada ikinci `Alignment`
    // hata olarak atılıyordu; denetimsiz okumada da ilk değer kazanmalı.
    let bytes = udf(r#" Alignment="1" Alignment="3""#);
    let mut w = WarningSink::new();
    let doc = tavzih_core::udf::reader::read_udf(&bytes, &mut w).unwrap();
    let alignment = doc.sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) => Some(p.props.alignment),
            _ => None,
        })
        .unwrap();
    assert_eq!(alignment, Alignment::Center);
}
