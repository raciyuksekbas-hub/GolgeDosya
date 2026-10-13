//! Açılamayan girdide kullanıcıya DOĞRU sebep söylenir.
//!
//! Denetim bulguları:
//! - OR-06: content.xml'i bozuk (CRC tutmayan) bir .udf "Bu dosya geçerli bir
//!   Word (.docx) belgesi değil" diye bildiriliyordu.
//! - OR-07: Word'ün parola koruması .docx'i bir OLE bileşik dosyasına
//!   (D0 CF 11 E0 A1 B1 1A E1) çevirir; eski .doc da aynı kaptır. Motor "Word
//!   belgesi değil ya da bozulmuş" diyordu, parolayı hiç anmıyordu; ZipCrypto
//!   ile şifrelenmiş arşiv de öyle.
//! - 0 baytlık dosya "ZIP değil" diye düşüyordu; "dosya boş" denmeliydi.
//! - UF-13 / F-12: gerçek bir UDF (D573) sıkıştırılmamış, düz bir content.xml
//!   idi; "geçerli bir UDF değil" deniyordu. Aynı baytlar zip'e konunca
//!   sorunsuz dönüşüyor. Aynı boyut ve DOCTYPE korumalarıyla kabul edilir.
//!
//! Bütün girdiler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::Write;
use std::path::PathBuf;

use tavzih_core::convert::{self, convert_bytes, Direction};
use tavzih_core::model::*;
use tavzih_core::udf;
use tavzih_core::warnings::WarningSink;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn code(bytes: &[u8], dir: Direction) -> String {
    let mut w = WarningSink::new();
    match convert_bytes(bytes, dir, &mut w) {
        Ok(_) => "OK".into(),
        Err(e) => e.code.as_str().into(),
    }
}

fn zip_of(entries: &[(&str, &[u8])], method: zip::CompressionMethod) -> Vec<u8> {
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(method);
        for (name, bytes) in entries {
            z.start_file(*name, o).unwrap();
            z.write_all(bytes).unwrap();
        }
        z.finish().unwrap();
    }
    cur.into_inner()
}

fn docx() -> Vec<u8> {
    let doc = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body><w:p><w:r><w:t>Dilekçe</w:t></w:r></w:p></w:body></w:document>"#
    );
    zip_of(
        &[("word/document.xml", doc.as_bytes())],
        zip::CompressionMethod::Deflated,
    )
}

fn content_xml(prolog: &str) -> String {
    format!(
        r#"{prolog}<?xml version="1.0" encoding="UTF-8" ?>

<template format_id="1.8"><content><![CDATA[Sayın Mahkemeye
]]></content><properties><pageFormat mediaSizeName="1" leftMargin="70.0" rightMargin="70.0" topMargin="70.0" bottomMargin="70.0" paperOrientation="1" headerFOffset="20.0" footerFOffset="20.0" /></properties><elements resolver="hvl-default"><paragraph><content startOffset="0" length="16"/></paragraph></elements><styles><style name="hvl-default" family="Dialog" size="12" /></styles></template>"#
    )
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tavzih-input-errors-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ------------------------------------------------------------ bozuk UDF

#[test]
fn a_corrupt_udf_is_called_an_invalid_udf_not_a_word_file() {
    let xml = content_xml("");
    let mut b = zip_of(
        &[("content.xml", xml.as_bytes())],
        zip::CompressionMethod::Stored,
    );
    // Saklanan verinin içinde bir bayt değişir: CRC tutmaz.
    let at = b.windows(9).position(|w| w == b"<content>").unwrap();
    b[at + 1] ^= 0x20;
    assert_eq!(code(&b, Direction::UdfToDocx), "INVALID_UDF");
}

// ------------------------------------------------------------ parola / eski Word

#[test]
fn a_password_protected_or_legacy_word_file_says_so() {
    // OLE bileşik dosyası: parola korumalı .docx ya da eski .doc.
    let mut ole = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    ole.resize(4096, 0);
    assert_eq!(code(&ole, Direction::DocxToUdf), "PROTECTED_OR_LEGACY_DOCX");

    // ZipCrypto: girdilerin "şifreli" bayrağı (genel amaçlı bayrakların 0. biti)
    // hem yerel başlıkta hem merkezi dizinde açık.
    let mut enc = docx();
    let mut p = 0;
    while p + 4 <= enc.len() {
        match enc[p..p + 4] {
            [0x50, 0x4b, 0x03, 0x04] => enc[p + 6] |= 1,
            [0x50, 0x4b, 0x01, 0x02] => enc[p + 8] |= 1,
            _ => {}
        }
        p += 1;
    }
    assert_eq!(code(&enc, Direction::DocxToUdf), "PROTECTED_OR_LEGACY_DOCX");

    // Kullanıcının gördüğü cümle parolayı ve çözümü söyler.
    let d = tmpdir("ole");
    let src = d.join("Şifreli dilekçe.docx");
    std::fs::write(&src, &ole).unwrap();
    let r = convert::convert_file_to(&src, &d.join("cikti"));
    let e = r.error.expect("hata bekleniyordu");
    assert_eq!(e.code, "PROTECTED_OR_LEGACY_DOCX");
    assert!(e.message.contains("parola"), "{}", e.message);
    assert!(e.message.contains(".docx"), "{}", e.message);
    std::fs::remove_dir_all(&d).ok();
}

// ------------------------------------------------------------ boş dosya

#[test]
fn an_empty_file_says_it_is_empty() {
    assert_eq!(code(&[], Direction::DocxToUdf), "INPUT_EMPTY");
    assert_eq!(code(&[], Direction::UdfToDocx), "INPUT_EMPTY");
    let d = tmpdir("empty");
    let src = d.join("Boş.udf");
    std::fs::write(&src, b"").unwrap();
    let r = convert::convert_file_to(&src, &d.join("cikti"));
    let e = r.error.expect("hata bekleniyordu");
    assert_eq!(e.code, "INPUT_EMPTY");
    assert!(e.message.contains("boş"), "{}", e.message);
    assert!(r.source_unchanged);
    std::fs::remove_dir_all(&d).ok();
}

// ------------------------------------------------------------ düz content.xml

#[test]
fn a_bare_content_xml_udf_is_accepted() {
    // Gerçek D573'ün biçimi: "<?xml … ?> \n\n<template …" ile başlayan düz XML.
    // BOM'lu ve baştaki boşluklu biçimler de aynı.
    for prolog in ["", "\u{FEFF}", " \n"] {
        let bytes = content_xml(prolog).into_bytes();
        let mut w = WarningSink::new();
        let d = udf::reader::read_udf(&bytes, &mut w).expect("düz content.xml okunmalı");
        let Block::Paragraph(p) = &d.sections[0].blocks[0] else {
            panic!("paragraf yok")
        };
        assert_eq!(p.text(), "Sayın Mahkemeye", "{prolog:?}");
        assert_eq!(code(&bytes, Direction::UdfToDocx), "OK");
    }
}

#[test]
fn a_bare_content_xml_keeps_the_archive_guards() {
    // DOCTYPE'lı düz XML, zip içindeki gibi reddedilir.
    let xml = content_xml("").replace(
        "<template",
        "<!DOCTYPE template [<!ENTITY x \"y\">]><template",
    );
    assert_eq!(code(xml.as_bytes(), Direction::UdfToDocx), "UNSAFE_ARCHIVE");
    // UDF olmayan XML hâlâ geçerli bir UDF değildir.
    assert_eq!(
        code(
            br#"<?xml version="1.0"?><html><body/></html>"#,
            Direction::UdfToDocx
        ),
        "INVALID_UDF"
    );
    // XML bir Word belgesi yerine geçmez.
    assert_eq!(
        code(content_xml("").as_bytes(), Direction::DocxToUdf),
        "INVALID_DOCX"
    );
}
