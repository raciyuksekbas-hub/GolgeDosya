//! Görsel süzgeç ZİNCİRİ: `/Filter [/FlateDecode /DCTDecode]`.
//!
//! Sahada 42 sayfalık bir taramada 42 görselin 14'ü (dosyanın %42'si) bu
//! biçimdeydi ve sıkıştırma motoru onları hiç çözemiyordu: `decode_image`
//! yalnız tek başına `DCTDecode`'u ya da tamamı `FlateDecode` olan zinciri
//! tanıyordu. ISO 32000-1 §7.4: `/Filter` dizisi ÇÖZME sırasıdır, ilk süzgeç
//! en dıştaki kodlamadır; `/DecodeParms` dizisi ona paraleldir.

use flate2::{write::ZlibEncoder, Compression};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_core::optimizer::decode_image;
use pdf_core::{optimize_pdf, OptimizationLevel};
use std::io::Write;

/// Gürültülü, belirlenimci bir tarama benzeri. Düz renk JPEG'i anlamsızca
/// küçük olur; yeniden kodlama kazancını sınamak için doku gerekir.
fn sample(w: u32, h: u32) -> image::RgbImage {
    let mut state = 0x9E37_79B9u32;
    image::RgbImage::from_fn(w, h, |x, y| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let n = (state % 60) as u8;
        let base = if (y / 6) % 5 == 0 { 40 } else { 180 };
        image::Rgb([base + n / 2, base + n / 3, base + n - (x % 7) as u8])
    })
}

fn jpeg(img: &image::RgbImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(img)
        .unwrap();
    out
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

fn names(list: &[&str]) -> Object {
    Object::Array(
        list.iter()
            .map(|n| Object::Name(n.as_bytes().to_vec()))
            .collect(),
    )
}

fn image_stream(w: u32, h: u32, filter: Object, parms: Option<Object>, content: Vec<u8>) -> Stream {
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => w as i64,
        "Height" => h as i64,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
        "Filter" => filter,
    };
    if let Some(p) = parms {
        dict.set("DecodeParms", p);
    }
    Stream::new(dict, content)
}

/// Görüntüleyicinin göreceği pikseller: JPEG'in kendisini çözen aynı çözücü.
fn reference(jpeg: &[u8]) -> Vec<u8> {
    image::load_from_memory(jpeg).unwrap().to_rgb8().into_raw()
}

#[test]
fn flate_wrapped_jpeg_is_decoded_through_the_whole_chain() {
    let img = sample(96, 64);
    let j = jpeg(&img, 90);
    let s = image_stream(
        96,
        64,
        names(&["FlateDecode", "DCTDecode"]),
        None,
        deflate(&j),
    );
    let decoded = decode_image(&s)
        .expect("zincir hatasız çözülmeli")
        .expect("[/FlateDecode /DCTDecode] görseli ATLANMAMALI");
    assert_eq!((decoded.width(), decoded.height()), (96, 64));
    assert_eq!(
        decoded.to_rgb8().into_raw(),
        reference(&j),
        "Flate katmanı soyulduktan sonra JPEG, tek başına DCTDecode gibi çözülmeli"
    );
}

#[test]
fn plain_dct_still_decodes() {
    let img = sample(64, 48);
    let j = jpeg(&img, 85);
    let s = image_stream(64, 48, Object::Name(b"DCTDecode".to_vec()), None, j.clone());
    let decoded = decode_image(&s).unwrap().expect("tek DCTDecode çözülmeli");
    assert_eq!(decoded.to_rgb8().into_raw(), reference(&j));
    // Tek elemanlı dizi biçimi de aynı şeydir.
    let s = image_stream(64, 48, names(&["DCTDecode"]), None, j.clone());
    assert_eq!(
        decode_image(&s).unwrap().unwrap().to_rgb8().into_raw(),
        reference(&j)
    );
}

#[test]
fn plain_flate_raw_samples_still_decode() {
    let img = sample(40, 30);
    let s = image_stream(
        40,
        30,
        Object::Name(b"FlateDecode".to_vec()),
        None,
        deflate(img.as_raw()),
    );
    let decoded = decode_image(&s)
        .unwrap()
        .expect("ham Flate örnekleri çözülmeli");
    assert_eq!(decoded.to_rgb8().into_raw(), img.into_raw());
}

#[test]
fn chained_flate_layers_are_each_undone_in_order() {
    // İki kat Flate: her katman sırayla geri alınmalı. Eskiden tek kat
    // açılıyor, örnek uzunluğu tutmadığı için görsel atlanıyordu.
    let img = sample(40, 30);
    let twice = deflate(&deflate(img.as_raw()));
    let s = image_stream(40, 30, names(&["FlateDecode", "FlateDecode"]), None, twice);
    let decoded = decode_image(&s).unwrap().expect("iki kat Flate çözülmeli");
    assert_eq!(decoded.to_rgb8().into_raw(), img.into_raw());
}

/// PNG "None" öngörücüsü, `/Columns 1` ile: her bayttan önce bir 0 süzgeç
/// baytı. Sonuç ancak öngörücü DOĞRU katmanda geri alınırsa JPEG'e döner.
fn png_none_predict(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().flat_map(|b| [0u8, *b]).collect()
}

fn predictor_parms() -> Object {
    Object::Dictionary(dictionary! {
        "Predictor" => 10,
        "Colors" => 1,
        "BitsPerComponent" => 8,
        "Columns" => 1,
    })
}

#[test]
fn decode_parms_array_belongs_to_its_own_filter() {
    let img = sample(48, 32);
    let j = jpeg(&img, 90);
    let content = deflate(&png_none_predict(&j));
    let s = image_stream(
        48,
        32,
        names(&["FlateDecode", "DCTDecode"]),
        Some(Object::Array(vec![predictor_parms(), Object::Null])),
        content.clone(),
    );
    let decoded = decode_image(&s)
        .unwrap()
        .expect("öngörücü Flate katmanına ait; zincir çözülmeli");
    assert_eq!(decoded.to_rgb8().into_raw(), reference(&j));

    // Parametre YANLIŞ katmana yazılmışsa (Flate'e değil DCT'ye) Flate
    // öngörücüsüz açılır, JPEG bozuk kalır: sessizce yanlış piksel üretmek
    // yerine görsel ya atlanır ya da hata verir — asla "başarılı" dönmez.
    let s = image_stream(
        48,
        32,
        names(&["FlateDecode", "DCTDecode"]),
        Some(Object::Array(vec![Object::Null, predictor_parms()])),
        content,
    );
    assert!(
        !matches!(decode_image(&s), Ok(Some(_))),
        "parametre kendi süzgecine uygulanmalı"
    );
}

#[test]
fn unsupported_chain_is_skipped_not_failed() {
    let j = jpeg(&sample(32, 32), 90);
    for filter in [
        // Desteklemediğimiz taşıma süzgeci.
        names(&["LZWDecode", "DCTDecode"]),
        names(&["ASCIIHexDecode", "DCTDecode"]),
        // Görsel kodeki zincirin sonunda değil.
        names(&["DCTDecode", "FlateDecode"]),
        // Desteklemediğimiz görsel kodekleri.
        names(&["FlateDecode", "JPXDecode"]),
        Object::Name(b"JBIG2Decode".to_vec()),
        Object::Name(b"CCITTFaxDecode".to_vec()),
    ] {
        let s = image_stream(32, 32, filter.clone(), None, j.clone());
        assert!(
            matches!(decode_image(&s), Ok(None)),
            "{filter:?} atlanmalı, hata değil"
        );
    }
}

#[test]
fn malformed_chain_is_skipped_without_panicking() {
    let j = jpeg(&sample(32, 32), 90);
    let wrapped = deflate(&j);
    let cases: Vec<(Object, Option<Object>)> = vec![
        // Süzgeç dizisinde ad olmayan öğe.
        (
            Object::Array(vec![
                Object::Name(b"FlateDecode".to_vec()),
                Object::Integer(7),
            ]),
            None,
        ),
        // Süzgeç ne ad ne dizi.
        (Object::Integer(3), None),
        // Parametre dizisi süzgeç dizisiyle aynı uzunlukta değil.
        (
            names(&["FlateDecode", "DCTDecode"]),
            Some(Object::Array(vec![Object::Null])),
        ),
        // Parametre öğesi sözlük/null değil.
        (
            names(&["FlateDecode", "DCTDecode"]),
            Some(Object::Array(vec![Object::Integer(1), Object::Null])),
        ),
        // Çok süzgeçli zincire tek sözlük: hangi katmana ait olduğu belirsiz.
        (
            names(&["FlateDecode", "DCTDecode"]),
            Some(predictor_parms()),
        ),
    ];
    for (filter, parms) in cases {
        let s = image_stream(32, 32, filter.clone(), parms.clone(), wrapped.clone());
        assert!(
            matches!(decode_image(&s), Ok(None)),
            "bozuk zincir {filter:?} / {parms:?} atlanmalı"
        );
    }
}

#[test]
fn corrupt_wrapper_never_yields_an_image() {
    // Yapısı geçerli ama içeriği bozuk zincir: panik yok, uydurma piksel yok.
    for content in [b"bu zlib degil".to_vec(), deflate(&[0xAB; 64]), Vec::new()] {
        let s = image_stream(32, 32, names(&["FlateDecode", "DCTDecode"]), None, content);
        assert!(!matches!(decode_image(&s), Ok(Some(_))));
    }
}

/// Tek sayfalık, tek görselli belge.
fn one_image_pdf(image: Stream) -> Document {
    let mut d = Document::with_version("1.5");
    let pages = d.new_object_id();
    let img = d.add_object(image);
    let contents = d.add_object(Stream::new(
        dictionary! {},
        b"q 595 0 0 842 0 0 cm /Scan Do Q".to_vec(),
    ));
    let page = d.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Resources" => dictionary! {"XObject" => dictionary! {"Scan" => img}},
        "Contents" => contents,
    });
    d.objects.insert(
        pages,
        dictionary! {"Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1}.into(),
    );
    let root = d.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
    d.trailer.set("Root", root);
    d
}

#[test]
fn optimizer_treats_a_chained_jpeg_as_a_compression_candidate() {
    // q100 kaynak: q80 yeniden kodlama belirgin küçülmeli.
    let img = sample(900, 700);
    let j = jpeg(&img, 100);
    let wrapped = deflate(&j);
    let original_len = wrapped.len();
    let mut doc = one_image_pdf(image_stream(
        900,
        700,
        names(&["FlateDecode", "DCTDecode"]),
        None,
        wrapped,
    ));
    let stats = optimize_pdf(&mut doc, OptimizationLevel::GentleCompression).unwrap();
    assert_eq!(stats.images_found, 1);
    assert_eq!(stats.images_supported, 1, "zincirli JPEG aday sayılmalı");
    assert_eq!(stats.images_recompressed_count, 1);

    let (_, out) = doc
        .objects
        .iter()
        .find(|(_, o)| {
            o.as_stream()
                .ok()
                .and_then(|s| s.dict.get(b"Subtype").ok())
                .and_then(|v| v.as_name().ok())
                == Some(b"Image")
        })
        .unwrap();
    let out = out.as_stream().unwrap();
    assert!(out.content.len() < original_len);
    // Yeniden yazılan akış artık yalnız DCTDecode'dur; eski zincirin
    // parametreleri geride kalmaz.
    assert_eq!(
        out.dict.get(b"Filter").unwrap().as_name().unwrap(),
        b"DCTDecode"
    );
    assert!(!out.dict.has(b"DecodeParms"));
    let back = decode_image(out).unwrap().expect("yeni akış çözülebilmeli");
    assert_eq!((back.width(), back.height()), (900, 700));
}
