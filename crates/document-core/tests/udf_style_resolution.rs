//! UDF → model: öznitelikler UYAP'ın Swing modeli gibi ÇÖZÜLÜR.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (293 gerçek UDF üzerinde ölçüldü,
//! burada sentetik olarak kurulur):
//! - UDF öznitelikleri Swing `AttributeSet`'idir: bir yaprak (`content`, `tab`,
//!   `field`) kendinde olmayan bir özniteliği önce kendi `resolver` stilinde,
//!   sonra paragrafında arar; paragraf da kendi `resolver` stil zincirinde
//!   (`style@resolver`), en sonda `hvl-default`'ta. Okuyucu yalnız öğenin KENDİ
//!   özniteliklerini okuyordu.
//! - Sonuç: `<paragraph bold="true">` içindeki, kendi `bold`'u olmayan 750
//!   yaprak (24 belgede 17.018 harf; çoğu icra takip talebi formu) Word'de düz
//!   çıkıyordu. `edf_*` stilinden gelen iki yana yaslama 124 paragrafta (21
//!   belge), SpaceAbove/SpaceBelow 82 paragrafta (10 belge) kayboluyordu.
//! - Açık bir öznitelik her zaman kazanır; çözümleme yalnız EKSİK olanı doldurur.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::model::*;
use tavzih_core::udf;
use tavzih_core::warnings::WarningSink;

/// İçerik tamponunu ve öğe ağacını aynı imleçle kuran küçük yardımcı: ofsetler
/// UTF-16 birimidir, paragraf sonlandırıcısı UYAP'taki gibi son yaprağın içindedir.
#[derive(Default)]
struct Udf {
    buf: String,
    units: usize,
    elements: String,
}

impl Udf {
    /// `leaves`: (yaprak öznitelikleri, metin). Son yaprağa `\n` eklenir.
    fn para(&mut self, attrs: &str, leaves: &[(&str, &str)]) -> &mut Self {
        self.elements.push_str(&format!("<paragraph{attrs}>"));
        for (i, (la, text)) in leaves.iter().enumerate() {
            let mut t = text.to_string();
            if i + 1 == leaves.len() {
                t.push('\n');
            }
            let len = t.encode_utf16().count();
            self.elements.push_str(&format!(
                "<content{la} startOffset=\"{}\" length=\"{len}\" />",
                self.units
            ));
            self.buf.push_str(&t);
            self.units += len;
        }
        self.elements.push_str("</paragraph>\n");
        self
    }

    fn package(&self, styles: &str) -> Vec<u8> {
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<template format_id=\"1.8\" >\n\
             <content><![CDATA[{}]]></content>\n\
             <properties><pageFormat mediaSizeName=\"1\" leftMargin=\"70.875\" rightMargin=\"70.875\" \
             topMargin=\"70.875\" bottomMargin=\"70.875\" paperOrientation=\"1\" headerFOffset=\"20.0\" \
             footerFOffset=\"20.0\" /></properties>\n\
             <elements resolver=\"hvl-default\" >\n{}</elements>\n<styles>{styles}</styles>\n</template>",
            self.buf, self.elements
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

/// UYAP'ın yazdığı biçimde iki stil: Swing'in `default`'u (editörün arayüz yazı
/// tipi) ve belgenin gövde stili `hvl-default`.
const BASE_STYLES: &str = "<style name=\"default\" description=\"Geçerli\" family=\"Dialog\" size=\"12\" bold=\"false\" italic=\"false\" foreground=\"-13421773\" FONT_ATTRIBUTE_KEY=\"javax.swing.plaf.FontUIResource[family=Dialog,name=Dialog,style=plain,size=12]\" />\
<style name=\"hvl-default\" family=\"Times New Roman\" size=\"12\" description=\"Gövde\" />";

fn read(bytes: &[u8]) -> Document {
    let mut w = WarningSink::new();
    udf::reader::read_udf(bytes, &mut w).expect("read udf")
}

fn paragraphs(d: &Document) -> Vec<&Paragraph> {
    d.sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p),
            _ => None,
        })
        .collect()
}

fn text_props(p: &Paragraph) -> Vec<(String, RunProps)> {
    p.runs
        .iter()
        .filter_map(|r| match r {
            Run::Text { text, props } => Some((text.clone(), props.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_leaf_inherits_bold_from_its_paragraph() {
    // Gerçek icra formu: başlık paragrafı `bold="true"`, yaprakların kendi
    // `bold`'u yok (bazılarının `resolver="hvl-default"`'u var; o stil `bold`
    // söylemez, arama paragrafa düşer).
    let mut u = Udf::default();
    u.para(
        " Alignment=\"1\" bold=\"true\" resolver=\"hvl-default\"",
        &[
            ("", "TAKİP TALEBİ "),
            (" resolver=\"hvl-default\"", "ÖRNEK 1"),
        ],
    );
    let d = read(&u.package(BASE_STYLES));
    let runs = text_props(paragraphs(&d)[0]);
    assert_eq!(runs.len(), 2);
    for (text, props) in &runs {
        assert!(
            props.bold,
            "paragraf düzeyindeki kalınlık kayboldu: {text:?}"
        );
    }
}

#[test]
fn an_explicit_leaf_attribute_still_wins_over_the_paragraph() {
    let mut u = Udf::default();
    u.para(
        " bold=\"true\" italic=\"true\"",
        &[(" bold=\"false\"", "düz"), ("", "kalın")],
    );
    let d = read(&u.package(BASE_STYLES));
    let runs = text_props(paragraphs(&d)[0]);
    assert!(!runs[0].1.bold, "açık bold=\"false\" çiğnendi");
    assert!(runs[0].1.italic, "eksik italic paragraftan gelmeli");
    assert!(runs[1].1.bold);
}

#[test]
fn paragraph_alignment_and_spacing_come_from_the_resolver_style_chain() {
    // edf_baslik → edf_govde → hvl-default: hiza edf_govde'de, aralıklar
    // edf_baslik'te tanımlı. Paragrafın KENDİ özniteliği yok.
    let styles = format!(
        "{BASE_STYLES}<style name=\"edf_govde\" resolver=\"hvl-default\" Alignment=\"3\" LeftIndent=\"0.0\" />\
         <style name=\"edf_baslik\" resolver=\"edf_govde\" SpaceAbove=\"2.0\" SpaceBelow=\"6.0\" description=\"Başlık\" />"
    );
    let mut u = Udf::default();
    u.para(" resolver=\"edf_baslik\"", &[("", "Gerekçe")]);
    // Açık öznitelik stil zincirini ezer.
    u.para(
        " resolver=\"edf_baslik\" Alignment=\"1\" SpaceAbove=\"0.0\"",
        &[("", "Ortada")],
    );
    let d = read(&u.package(&styles));
    let ps = paragraphs(&d);
    assert_eq!(ps[0].props.alignment, Alignment::Justify);
    assert_eq!(ps[0].props.space_before_pt, 2.0);
    assert_eq!(ps[0].props.space_after_pt, 6.0);
    assert_eq!(ps[1].props.alignment, Alignment::Center);
    assert_eq!(ps[1].props.space_before_pt, 0.0, "açık SpaceAbove çiğnendi");
    assert_eq!(ps[1].props.space_after_pt, 6.0);
}

#[test]
fn hvl_default_is_the_last_resolver() {
    // Gerçek icra şablonlarında hvl-default'un kendisi Alignment="3" taşır.
    let styles = "<style name=\"hvl-default\" family=\"Arial\" size=\"11\" Alignment=\"3\" SpaceAbove=\"0.0\" description=\"Gövde\" />";
    let mut u = Udf::default();
    u.para("", &[("", "Yaslanmış metin")]);
    let d = read(&u.package(styles));
    assert_eq!(paragraphs(&d)[0].props.alignment, Alignment::Justify);
}

#[test]
fn the_swing_default_style_does_not_dictate_the_document_font() {
    // `default` Swing'in `StyleContext.DEFAULT_STYLE`'ıdır: UYAP onu belge açılırken
    // metin alanının ARAYÜZ yazı tipiyle yeniden yazar (FONT_ATTRIBUTE_KEY). Kayıtlı
    // Dialog/koyu gri, yazarın ekranını anlatır; belgenin yazı tipi değildir.
    let mut u = Udf::default();
    u.para(" bold=\"true\"", &[(" resolver=\"default\"", "imza")]);
    let d = read(&u.package(BASE_STYLES));
    let runs = text_props(paragraphs(&d)[0]);
    let props = &runs[0].1;
    assert_eq!(
        props.font_family, None,
        "Dialog belgeye yazı tipi olarak sızdı"
    );
    assert_eq!(props.color, None, "arayüzün koyu grisi metin rengi oldu");
    // Swing'de `default` stilinin kayıtlı bold="false"'u paragraftan ÖNCE okunur.
    assert!(!props.bold);
}

#[test]
fn inherited_bold_and_alignment_reach_the_docx() {
    let styles = format!(
        "{BASE_STYLES}<style name=\"edf_orta\" resolver=\"hvl-default\" Alignment=\"1\" />"
    );
    let mut u = Udf::default();
    u.para(
        " bold=\"true\" resolver=\"edf_orta\"",
        &[("", "İCRA DAİRESİNE")],
    );
    let mut w = WarningSink::new();
    let docx = convert_bytes(&u.package(&styles), Direction::UdfToDocx, &mut w).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
    let mut x = String::new();
    z.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut x)
        .unwrap();
    assert!(x.contains("<w:jc w:val=\"center\"/>"), "hiza yok: {x}");
    assert!(x.contains("<w:b/>"), "kalınlık yok: {x}");
}
