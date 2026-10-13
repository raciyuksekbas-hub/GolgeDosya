//! UDF → Word: boyutu yazılmamış görsel kendi piksel boyutunda çizilir.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (293 gerçek UDF üzerinde ölçüldü,
//! burada sentetik olarak kurulur): 26 belgede 54 `<image>` öğesinin
//! `width`/`height`'ı yok (46'sı 16x17 piksellik simge, ötekiler 100x50, 70x70,
//! 95x60). Word yazıcısı bunları sabit 72x72 pt kare çiziyordu: simgeler ~4,5 kat
//! büyüyor, 2:1 görseller kareye eziliyordu. UYAP (Swing) görseli 72 dpi'da
//! çizer: 1 piksel = 1 pt. Eksik boyut piksellerden, en-boy oranı korunarak
//! alınır ve metin genişliğine sığdırılır. Açıkça yazılmış boyut olduğu gibi kalır.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use base64::Engine as _;
use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::warnings::WarningSink;

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(w, h));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

/// Tek paragrafta tek görsel; `size` görselin boyut öznitelikleri (boş olabilir).
fn udf_with_image(data: &[u8], size: &str) -> Vec<u8> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<template format_id=\"1.8\" >\n\
         <content><![CDATA[\u{00B8}\n]]></content>\n\
         <properties><pageFormat mediaSizeName=\"1\" leftMargin=\"70.875\" rightMargin=\"70.875\" \
         topMargin=\"70.875\" bottomMargin=\"70.875\" paperOrientation=\"1\" headerFOffset=\"20.0\" \
         footerFOffset=\"20.0\" /></properties>\n\
         <elements resolver=\"hvl-default\" ><paragraph><image imageData=\"{b64}\"{size} startOffset=\"0\" length=\"1\" />\
         <content startOffset=\"1\" length=\"1\" /></paragraph></elements>\n\
         <styles><style name=\"hvl-default\" family=\"Times New Roman\" size=\"12\" /></styles>\n</template>"
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

/// Word çıktısındaki ilk görselin `wp:extent`'i, EMU.
fn extent(udf: &[u8]) -> (i64, i64) {
    let mut w = WarningSink::new();
    let docx = convert_bytes(udf, Direction::UdfToDocx, &mut w).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
    let mut x = String::new();
    z.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut x)
        .unwrap();
    let a = &x[x.find("<wp:extent cx=\"").expect("extent") + 15..];
    let cx = a[..a.find('"').unwrap()].parse().unwrap();
    let b = &a[a.find("cy=\"").unwrap() + 4..];
    let cy = b[..b.find('"').unwrap()].parse().unwrap();
    (cx, cy)
}

const EMU_PER_PT: i64 = 12_700;

#[test]
fn an_image_without_a_size_is_drawn_at_its_pixel_size() {
    // Gerçek derlemin en sık biçimi: 16x17 piksellik simge.
    assert_eq!(
        extent(&udf_with_image(&png(16, 17), "")),
        (16 * EMU_PER_PT, 17 * EMU_PER_PT)
    );
    // 2:1 görsel kareye ezilmez.
    assert_eq!(
        extent(&udf_with_image(&png(100, 50), "")),
        (100 * EMU_PER_PT, 50 * EMU_PER_PT)
    );
}

#[test]
fn a_single_given_dimension_keeps_the_aspect_ratio() {
    assert_eq!(
        extent(&udf_with_image(&png(200, 50), " width=\"100.0\"")),
        (100 * EMU_PER_PT, 25 * EMU_PER_PT)
    );
}

#[test]
fn a_large_sizeless_image_is_fitted_to_the_text_width() {
    // A4, 70,875 pt kenar payı: metin genişliği 453,526 pt.
    let (cx, cy) = extent(&udf_with_image(&png(1000, 500), ""));
    let text_width_emu = (453.526_f64 * EMU_PER_PT as f64).round() as i64;
    assert!((cx - text_width_emu).abs() <= EMU_PER_PT, "cx={cx}");
    assert!((cx as f64 / cy as f64 - 2.0).abs() < 0.01, "oran bozuldu");
}

#[test]
fn an_explicit_size_is_kept_as_written() {
    assert_eq!(
        extent(&udf_with_image(
            &png(16, 17),
            " width=\"30.0\" height=\"30.0\""
        )),
        (30 * EMU_PER_PT, 30 * EMU_PER_PT)
    );
}
