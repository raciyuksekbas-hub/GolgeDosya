//! Üst/alt bilgi, sayfa numarası ve sayfa sonu: sayfaya bağlı biçim.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (199 gerçek Word belgesi ve 293 gerçek
//! UDF üzerinde ölçüldü, burada sentetik olarak kurulur):
//! - UDF üst bilgisinin sayfa aralığı (`stopPage="1"`: yalnız ilk sayfa, 73 gerçek
//!   antet; `startPage="2"`: ikinci sayfadan sonra, 35 gerçek belge) Word'e
//!   geçmiyordu: antet her sayfada basılıyor, devam başlığı ilk sayfada da
//!   çıkıyordu; hiçbir uyarı yoktu;
//! - Word'ün GÖSTERMEDİĞİ üst bilgi çeşitleri (`w:titlePg` kapalıyken "first",
//!   `w:evenAndOddHeaders` yokken "even") "aktarılamadı" diye KAYIP uyarısı
//!   veriyordu (9 gerçek belge);
//! - UYAP'ın canlı sayfa numarası (`pageNumber-spec`, gerçek UDF'lerin 140'ında)
//!   Word'den gelirken üretilmiyordu: Word'ün PAGE/NUMPAGES alanı son görünen
//!   değerine donuyor, UDF'nin her sayfasında aynı numara yazıyordu (118 gerçek
//!   Word belgesi); UDF → Word → UDF turu da 137 gerçek canlı numarayı sabit
//!   "1/1" metnine çeviriyordu;
//! - paragraf içindeki sayfa sonu iki yanındaki sözcükleri birleştiriyordu
//!   ("önce" + sayfa sonu + "sonra" → "öncesonra"); `w:pageBreakBefore` (22 gerçek
//!   belge) hiçbir uyarı vermeden düşüyordu;
//! - `w:orient` yazılmamış yatay A4 sayfa "842x595 pt → A4" diye yanıltıcı bir
//!   boyut kaybı olarak bildiriliyor, cilt payı (`w:gutter`) sessizce düşüyordu.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::docx;
use tavzih_core::model::*;
use tavzih_core::udf;
use tavzih_core::warnings::{Severity, WarningCode, WarningSink};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

// ------------------------------------------------------------ yardımcılar

/// Sentetik Word paketi: gövde, `w:sectPr` içeriği, üst/alt bilgi parçaları,
/// isteğe bağlı `word/settings.xml` ve `word/styles.xml`.
#[derive(Default)]
struct Docx {
    body: String,
    /// `w:sectPr` içinde başvurulardan SONRA gelen öğeler (pgSz, pgMar, titlePg).
    sect: String,
    /// (`header`|`footer`, `default`|`first`|`even`, parçanın iç XML'i)
    parts: Vec<(&'static str, &'static str, String)>,
    settings: Option<String>,
    styles: Option<String>,
}

fn build(d: &Docx) -> Vec<u8> {
    let mut refs = String::new();
    let mut rels = String::new();
    let mut files: Vec<(String, String)> = Vec::new();
    for (i, (kind, ty, inner)) in d.parts.iter().enumerate() {
        let n = i + 1;
        let (el, rel_ty) = if *kind == "header" {
            ("hdr", "header")
        } else {
            ("ftr", "footer")
        };
        refs.push_str(&format!(
            r#"<w:{kind}Reference w:type="{ty}" r:id="rIdHf{n}"/>"#
        ));
        rels.push_str(&format!(
            r#"<Relationship Id="rIdHf{n}" Type="{REL}/{rel_ty}" Target="{kind}{n}.xml"/>"#
        ));
        files.push((
            format!("word/{kind}{n}.xml"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{el} {NS}>{inner}</w:{el}>"#
            ),
        ));
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{}<w:sectPr>{refs}{}</w:sectPr></w:body></w:document>"#,
        d.body, d.sect
    );
    files.push(("word/document.xml".into(), document));
    files.push((
        "word/_rels/document.xml.rels".into(),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#
        ),
    ));
    if let Some(s) = &d.settings {
        files.push((
            "word/settings.xml".into(),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings {NS}>{s}</w:settings>"#
            ),
        ));
    }
    if let Some(s) = &d.styles {
        files.push((
            "word/styles.xml".into(),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {NS}>{s}</w:styles>"#
            ),
        ));
    }
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for (name, xml) in &files {
            z.start_file(name.as_str(), o).unwrap();
            z.write_all(xml.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    cur.into_inner()
}

fn p(inner: &str) -> String {
    format!("<w:p>{inner}</w:p>")
}

fn r(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// Word'ün "complex field"ı: başlangıç, komut, ayırıcı, son görünen değer, son.
fn field(instr: &str, cached: &str) -> String {
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {instr} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{cached}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )
}

/// `w:fldSimple`: komut özniteliktedir, son görünen değer içindeki run'dadır.
fn simple(instr: &str, cached: &str) -> String {
    format!(r#"<w:fldSimple w:instr=" {instr} "><w:r><w:t>{cached}</w:t></w:r></w:fldSimple>"#)
}

fn read_docx(bytes: &[u8]) -> (Document, Vec<WarningCode>) {
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(bytes, &mut w).expect("read docx");
    (d, w.into_vec().iter().map(|x| x.code).collect())
}

/// DOCX → UDF; `content.xml` metni ve uyarı kodları.
fn to_udf(docx: &[u8]) -> (String, Vec<WarningCode>) {
    let mut w = WarningSink::new();
    let udf = convert_bytes(docx, Direction::DocxToUdf, &mut w).expect("docx -> udf");
    (
        content_xml(&udf),
        w.into_vec().iter().map(|x| x.code).collect(),
    )
}

fn content_xml(udf: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf)).unwrap();
    let mut s = String::new();
    z.by_name("content.xml")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

fn buffer(content: &str) -> String {
    let start = content.find("<![CDATA[").expect("content") + 9;
    let end = content[start..].find("]]>").unwrap() + start;
    content[start..end].to_string()
}

/// `<header …>` ya da `<footer …>` açılış etiketi (öznitelikleriyle).
fn hf_tag(content: &str, tag: &str) -> Option<String> {
    let i = content.find(&format!("<{tag} "))?;
    let end = content[i..].find('>')? + i;
    Some(content[i..=end].to_string())
}

fn attr(tag: &str, key: &str) -> Option<String> {
    tag.split(&format!(" {key}=\""))
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(|s| s.to_string())
}

fn zip_part(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    match z.by_name(name) {
        Ok(mut f) => {
            f.read_to_string(&mut s).unwrap();
        }
        Err(_) => return String::new(),
    }
    s
}

/// Bir parçanın görünen `w:t` metni.
fn visible_text(xml: &str) -> String {
    let mut out = String::new();
    for seg in xml.split("<w:t").skip(1) {
        let Some(gt) = seg.find('>') else { continue };
        if seg[..gt].ends_with('/') {
            continue;
        }
        let rest = &seg[gt + 1..];
        out.push_str(&rest[..rest.find("</w:t>").unwrap_or(0)]);
    }
    out
}

/// Üretilen Word paketinin bölüm ayarı: `w:titlePg` açık mı, ve her başvuru için
/// (tür, çeşit, parçanın görünen metni).
fn word_hf(docx: &[u8]) -> (bool, Vec<(String, String, String)>) {
    let doc = zip_part(docx, "word/document.xml");
    let rels = zip_part(docx, "word/_rels/document.xml.rels");
    let sect = &doc[doc.rfind("<w:sectPr").expect("sectPr")..];
    let mut out = Vec::new();
    for kind in ["header", "footer"] {
        for seg in sect.split(&format!("<w:{kind}Reference ")).skip(1) {
            let tag = &seg[..seg.find("/>").unwrap()];
            let ty = attr(&format!(" {tag}"), "w:type").unwrap_or_default();
            let rid = attr(&format!(" {tag}"), "r:id").unwrap();
            let rel = rels
                .split("<Relationship ")
                .find(|s| s.contains(&format!("Id=\"{rid}\"")))
                .expect("relationship");
            let target = attr(&format!(" {rel}"), "Target").unwrap();
            out.push((
                kind.to_string(),
                ty,
                visible_text(&zip_part(docx, &format!("word/{target}"))),
            ));
        }
    }
    (sect.contains("<w:titlePg"), out)
}

/// Sentetik UDF: isteğe bağlı üst bilgi, tek gövde paragrafı, isteğe bağlı alt bilgi.
/// Her kısım tek paragraftır; (öznitelikler, metin).
fn udf(header: Option<(&str, &str)>, body: &str, footer: Option<(&str, &str)>) -> Vec<u8> {
    let mut buf = String::new();
    let mut elements = String::new();
    let mut cursor = 0usize;
    let para = |text: &str, buf: &mut String, cursor: &mut usize| {
        let t = format!("{text}\n");
        let len = t.encode_utf16().count();
        let x = format!(
            r#"<paragraph Alignment="0"><content startOffset="{cursor}" length="{len}" /></paragraph>"#
        );
        buf.push_str(&t);
        *cursor += len;
        x
    };
    if let Some((attrs, text)) = header {
        let x = para(text, &mut buf, &mut cursor);
        elements.push_str(&format!(
            r#"<header background="-1" foreground="-1"{attrs}>{x}</header>"#
        ));
    }
    elements.push_str(&para(body, &mut buf, &mut cursor));
    if let Some((attrs, text)) = footer {
        let x = para(text, &mut buf, &mut cursor);
        elements.push_str(&format!(
            r#"<footer background="-1" foreground="-1"{attrs}>{x}</footer>"#
        ));
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<template format_id=\"1.8\" >\
         <content><![CDATA[{buf}]]></content>\
         <properties><pageFormat mediaSizeName=\"1\" leftMargin=\"70.875\" rightMargin=\"70.875\" \
         topMargin=\"70.875\" bottomMargin=\"70.875\" paperOrientation=\"1\" headerFOffset=\"20.0\" \
         footerFOffset=\"20.0\" /></properties>\
         <elements resolver=\"hvl-default\" >{elements}</elements>\
         <styles><style name=\"hvl-default\" family=\"Times New Roman\" size=\"12\" description=\"Gövde\" /></styles>\
         </template>"
    );
    udf::writer::package(xml.as_bytes()).unwrap()
}

fn to_docx(udf: &[u8]) -> (Vec<u8>, Vec<WarningCode>) {
    let mut w = WarningSink::new();
    let out = convert_bytes(udf, Direction::UdfToDocx, &mut w).expect("udf -> docx");
    docx::writer::validate(&out).expect("output must validate");
    (out, w.into_vec().iter().map(|x| x.code).collect())
}

fn read_udf(bytes: &[u8]) -> Document {
    let mut w = WarningSink::new();
    udf::reader::read_udf(bytes, &mut w).expect("read udf")
}

fn hf_texts(hf: &Option<HeaderFooter>) -> Vec<String> {
    hf.as_ref()
        .map(|h| {
            h.blocks
                .iter()
                .filter_map(|b| match b {
                    Block::Paragraph(p) => Some(p.text()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------- (a) UDF sayfa aralığı → Word

/// 73 gerçek UDF'nin anteti `stopPage="1"` taşır: yalnız ilk sayfada basılır.
/// Word'de bu, `w:titlePg` + "first" üst bilgisi + BOŞ "default" üst bilgisidir.
#[test]
fn udf_first_page_only_header_stays_on_the_first_page_in_word() {
    let (docx, warns) = to_docx(&udf(Some((r#" stopPage="1""#, "ANTET")), "Gövde", None));
    let (title_pg, refs) = word_hf(&docx);
    assert!(
        title_pg,
        "w:titlePg yok: antet her sayfada basılır ({refs:?})"
    );
    let first = refs
        .iter()
        .find(|(k, t, _)| k == "header" && t == "first")
        .unwrap_or_else(|| panic!("ilk sayfa üst bilgisi yok: {refs:?}"));
    assert_eq!(first.2, "ANTET");
    let default = refs
        .iter()
        .find(|(k, t, _)| k == "header" && t == "default")
        .unwrap_or_else(|| panic!("diğer sayfalar için boş üst bilgi yok: {refs:?}"));
    assert_eq!(default.2, "", "antet sonraki sayfalarda da basılıyor");
    assert!(
        !warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
}

/// 35 gerçek UDF'nin üst bilgisi `startPage="2"` taşır: ilk sayfada yoktur.
#[test]
fn udf_header_from_page_two_is_absent_on_the_first_page_in_word() {
    let (docx, warns) = to_docx(&udf(
        Some((r#" startPage="2""#, "DEVAM BAŞLIĞI")),
        "Gövde",
        None,
    ));
    let (title_pg, refs) = word_hf(&docx);
    assert!(
        title_pg,
        "w:titlePg yok: devam başlığı ilk sayfada da basılır"
    );
    let first = refs
        .iter()
        .find(|(k, t, _)| k == "header" && t == "first")
        .unwrap_or_else(|| panic!("boş ilk sayfa üst bilgisi yok: {refs:?}"));
    assert_eq!(first.2, "");
    let default = refs
        .iter()
        .find(|(k, t, _)| k == "header" && t == "default")
        .unwrap();
    assert_eq!(default.2, "DEVAM BAŞLIĞI");
    assert!(
        !warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
}

/// Aynı kural alt bilgide de geçerlidir. `w:titlePg` bölümün TAMAMINA uygulanır:
/// aralığı olmayan üst bilgi ilk sayfada boş kalmasın diye "first" çeşidi de
/// aynı içerikle yazılır.
#[test]
fn udf_footer_page_range_is_mirrored_and_the_every_page_header_keeps_page_one() {
    let (docx, warns) = to_docx(&udf(
        Some(("", "HER SAYFA")),
        "Gövde",
        Some((r#" stopPage="1""#, "İLK SAYFA ALT")),
    ));
    let (title_pg, refs) = word_hf(&docx);
    assert!(title_pg);
    let get = |k: &str, t: &str| {
        refs.iter()
            .find(|(kk, tt, _)| kk == k && tt == t)
            .map(|x| x.2.clone())
            .unwrap_or_else(|| panic!("{k}/{t} yok: {refs:?}"))
    };
    assert_eq!(get("footer", "first"), "İLK SAYFA ALT");
    assert_eq!(get("footer", "default"), "");
    assert_eq!(
        get("header", "first"),
        "HER SAYFA",
        "ilk sayfa üst bilgisiz kaldı"
    );
    assert_eq!(get("header", "default"), "HER SAYFA");
    assert!(
        !warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
}

/// Word'ün ifade edemediği bir aralık (ör. yalnız ilk üç sayfa): içerik her sayfada
/// korunur ve kayıp dürüstçe bildirilir.
#[test]
fn an_unsupported_udf_page_range_keeps_the_content_and_warns() {
    let (docx, warns) = to_docx(&udf(Some((r#" stopPage="3""#, "ANTET")), "Gövde", None));
    let (_, refs) = word_hf(&docx);
    assert!(
        refs.iter()
            .any(|(k, t, x)| k == "header" && t == "default" && x == "ANTET"),
        "{refs:?}"
    );
    assert!(
        warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
}

/// UDF → Word → UDF: aralık geri gelir (eskiden `stopPage` kayboluyordu).
#[test]
fn udf_page_ranges_survive_a_word_round_trip() {
    for (attrs, stop, start) in [
        (r#" stopPage="1""#, Some(1), None),
        (r#" startPage="2""#, None, Some(2)),
    ] {
        let (docx, _) = to_docx(&udf(Some((attrs, "ANTET")), "Gövde", Some(("", "ALT"))));
        let mut w = WarningSink::new();
        let back = convert_bytes(&docx, Direction::DocxToUdf, &mut w).unwrap();
        let codes: Vec<WarningCode> = w.into_vec().iter().map(|x| x.code).collect();
        let d = read_udf(&back);
        let h = d.sections[0].header.as_ref().expect("üst bilgi kayboldu");
        assert_eq!((h.stop_page, h.start_page), (stop, start), "{attrs}");
        assert_eq!(hf_texts(&d.sections[0].header), vec!["ANTET"], "{attrs}");
        let f = d.sections[0].footer.as_ref().expect("alt bilgi kayboldu");
        assert_eq!(
            (f.stop_page, f.start_page),
            (None, None),
            "{attrs}: alt bilgi her sayfada"
        );
        assert_eq!(hf_texts(&d.sections[0].footer), vec!["ALT"], "{attrs}");
        assert!(
            !codes.contains(&WarningCode::HeaderFooterVariantDropped),
            "{attrs}: {codes:?}"
        );
    }
}

// ------------------------------------------- (a) Word ilk sayfa → UDF aralığı

/// Word'de "İlk sayfa farklı" + BOŞ ilk sayfa üst bilgisi: başlık ikinci sayfadan
/// başlar. UDF bunu `startPage="2"` ile tutar; eskiden "aktarılamadı" diyordu ve
/// başlık UDF'de ilk sayfada da basılıyordu.
#[test]
fn word_title_page_with_an_empty_first_header_becomes_start_page_two() {
    let d = Docx {
        body: p(&r("Gövde")),
        sect: "<w:titlePg/>".into(),
        parts: vec![
            ("header", "default", p(&r("DEVAM BAŞLIĞI"))),
            ("header", "first", p("")),
        ],
        ..Default::default()
    };
    let (content, warns) = to_udf(&build(&d));
    let tag = hf_tag(&content, "header").expect("üst bilgi yok");
    assert_eq!(attr(&tag, "startPage").as_deref(), Some("2"), "{tag}");
    assert_eq!(attr(&tag, "stopPage"), None, "{tag}");
    assert!(buffer(&content).contains("DEVAM BAŞLIĞI"));
    assert!(
        !warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
}

/// "İlk sayfa farklı" + dolu ilk sayfa + BOŞ diğer sayfalar: antet yalnız ilk
/// sayfada (`stopPage="1"`). Boş "default" parça eskiden anteti eziyordu.
#[test]
fn word_first_page_letterhead_with_an_empty_default_becomes_stop_page_one() {
    let d = Docx {
        body: p(&r("Gövde")),
        sect: "<w:titlePg/>".into(),
        parts: vec![
            ("header", "default", p("")),
            ("header", "first", p(&r("ANTET"))),
        ],
        ..Default::default()
    };
    let (content, warns) = to_udf(&build(&d));
    let tag = hf_tag(&content, "header").expect("üst bilgi yok");
    assert_eq!(attr(&tag, "stopPage").as_deref(), Some("1"), "{tag}");
    assert!(buffer(&content).contains("ANTET"), "antet kayboldu");
    assert!(
        !warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
}

/// "İlk sayfa farklı" ama iki parça aynı (Word bunu sık yazar; gerçek belgelerin
/// 48'inde böyle): her sayfada aynı üst bilgi, uyarı yok.
#[test]
fn identical_first_and_default_headers_are_one_every_page_header() {
    let d = Docx {
        body: p(&r("Gövde")),
        sect: "<w:titlePg/>".into(),
        parts: vec![
            ("header", "default", p(&r("ANTET"))),
            ("header", "first", p(&r("ANTET"))),
        ],
        ..Default::default()
    };
    let (content, warns) = to_udf(&build(&d));
    let tag = hf_tag(&content, "header").expect("üst bilgi yok");
    assert_eq!(attr(&tag, "stopPage"), None, "{tag}");
    assert_eq!(attr(&tag, "startPage"), None, "{tag}");
    assert_eq!(buffer(&content).matches("ANTET").count(), 1);
    assert!(
        !warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
}

/// İki farklı, dolu parça: UDF'de tek üst bilgi olabilir. Diğer sayfaların
/// başlığı korunur, ilk sayfanınki dürüstçe "aktarılamadı" diye bildirilir.
#[test]
fn different_first_and_default_headers_still_report_the_lost_variant() {
    let d = Docx {
        body: p(&r("Gövde")),
        sect: "<w:titlePg/>".into(),
        parts: vec![
            ("header", "default", p(&r("DEVAM"))),
            ("header", "first", p(&r("ANTET"))),
        ],
        ..Default::default()
    };
    let (content, warns) = to_udf(&build(&d));
    assert!(buffer(&content).contains("DEVAM"));
    assert!(
        warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
}

/// Word'ün GÖSTERMEDİĞİ çeşitler kayıp değildir: `w:titlePg` yokken "first",
/// `w:evenAndOddHeaders` yokken "even" parçası hiçbir sayfada basılmaz. Gerçek
/// 9 belgede bu yüzden yersiz bir KAYIP uyarısı çıkıyordu.
#[test]
fn variants_word_never_shows_are_not_reported_as_lost() {
    let d = Docx {
        body: p(&r("Gövde")),
        parts: vec![
            ("header", "default", p(&r("ANTET"))),
            ("header", "first", p(&r("KULLANILMAYAN İLK"))),
            ("header", "even", p(&r("KULLANILMAYAN ÇİFT"))),
            ("footer", "default", p(&r("ALT"))),
            ("footer", "even", p(&r("KULLANILMAYAN ÇİFT ALT"))),
        ],
        ..Default::default()
    };
    let bytes = build(&d);
    let (doc, warns) = read_docx(&bytes);
    assert!(
        !warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
    assert_eq!(hf_texts(&doc.sections[0].header), vec!["ANTET"]);
    assert_eq!(hf_texts(&doc.sections[0].footer), vec!["ALT"]);
    let h = doc.sections[0].header.as_ref().unwrap();
    assert_eq!((h.stop_page, h.start_page), (None, None));
}

/// Tek/çift sayfa üst bilgisi Word'de GERÇEKTEN açıksa (settings.xml), UDF bunu
/// tutamaz ve bu bildirilir.
#[test]
fn an_even_page_header_word_shows_is_reported() {
    let d = Docx {
        body: p(&r("Gövde")),
        parts: vec![
            ("header", "default", p(&r("TEK"))),
            ("header", "even", p(&r("ÇİFT"))),
        ],
        settings: Some("<w:evenAndOddHeaders/>".into()),
        ..Default::default()
    };
    let (doc, warns) = read_docx(&build(&d));
    assert!(
        warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
    assert_eq!(hf_texts(&doc.sections[0].header), vec!["TEK"]);
}

// ------------------------------------------------ (c) sayfa numarası → UDF

/// "Sayfa X / Y" alt bilgisi: PAGE ve NUMPAGES alanları UYAP'ın canlı sayfa
/// numarasına çevrilir. Eskiden son görünen "1 / 3" metne donuyor, UDF'nin her
/// sayfasında aynı numara yazıyordu (gerçek belgelerin 118'inde).
#[test]
fn a_footer_page_x_of_y_becomes_a_live_udf_page_number() {
    let footer = p(&format!(
        "{}{}{}{}",
        r("Sayfa "),
        field("PAGE", "1"),
        r(" / "),
        field("NUMPAGES", "3")
    ));
    let d = Docx {
        body: p(&r("Gövde")),
        parts: vec![("footer", "default", footer)],
        ..Default::default()
    };
    let (content, warns) = to_udf(&build(&d));
    let tag = hf_tag(&content, "footer").expect("alt bilgi yok");
    assert_eq!(
        attr(&tag, "pageNumber-spec").as_deref(),
        Some("BSP32_2120"),
        "{tag}"
    );
    assert_eq!(
        attr(&tag, "pageNumber-seperator").as_deref(),
        Some(" / "),
        "{tag}"
    );
    assert_eq!(
        attr(&tag, "pageNumber-foreStr").as_deref(),
        Some("Sayfa "),
        "{tag}"
    );
    let buf = buffer(&content);
    assert!(!buf.contains("Sayfa"), "donmuş numara metni kaldı: {buf:?}");
    assert!(!buf.contains("1 / 3"), "donmuş numara metni kaldı: {buf:?}");
    assert!(
        warns.contains(&WarningCode::UdfPageNumberApproximated),
        "{warns:?}"
    );
    assert!(!warns.contains(&WarningCode::FieldFlattened), "{warns:?}");
    // Numara canlı kaldı; yeri ve biçimi yaklaşık. Bu bir kayıp değildir.
    assert_eq!(
        WarningCode::UdfPageNumberApproximated.severity(),
        Severity::Info
    );
}

/// `w:fldSimple` (gerçek belgelerin 21'i yalnız bunu kullanır; eskiden hiçbir uyarı
/// vermeden donuyordu). Toplam sayfa yoksa UYAP'ın tek numara biçimi.
#[test]
fn a_simple_page_field_without_a_total_becomes_a_live_udf_page_number() {
    let d = Docx {
        body: p(&r("Gövde")),
        parts: vec![(
            "footer",
            "default",
            format!(
                r#"<w:p><w:pPr><w:jc w:val="right"/></w:pPr>{}{}</w:p>"#,
                r("Sayfa "),
                simple("PAGE", "1")
            ),
        )],
        ..Default::default()
    };
    let (content, warns) = to_udf(&build(&d));
    let tag = hf_tag(&content, "footer").expect("alt bilgi yok");
    assert_eq!(
        attr(&tag, "pageNumber-spec").as_deref(),
        Some("BSP32_40"),
        "{tag}"
    );
    assert_eq!(attr(&tag, "pageNumber-seperator"), None, "{tag}");
    assert_eq!(
        attr(&tag, "pageNumber-foreStr").as_deref(),
        Some("Sayfa "),
        "{tag}"
    );
    assert!(!buffer(&content).contains("Sayfa"));
    assert!(
        warns.contains(&WarningCode::UdfPageNumberApproximated),
        "{warns:?}"
    );
}

/// Üst bilgideki "X / Y": UYAP'ın üst bilgi için yazdığı biçim. Alt bilgideki
/// diğer metin olduğu gibi kalır; numara paragrafı ayrıca çıkarılır.
#[test]
fn a_header_page_x_of_y_keeps_the_other_header_text() {
    let header = format!(
        "{}{}",
        p(&r("Av. Ayşe FATMA")),
        p(&format!(
            "{}{}{}",
            simple("PAGE", "2"),
            r("/"),
            simple("NUMPAGES", "5")
        ))
    );
    let d = Docx {
        body: p(&r("Gövde")),
        parts: vec![("header", "default", header)],
        ..Default::default()
    };
    let (content, warns) = to_udf(&build(&d));
    let tag = hf_tag(&content, "header").expect("üst bilgi yok");
    assert_eq!(
        attr(&tag, "pageNumber-spec").as_deref(),
        Some("BSP32_2116"),
        "{tag}"
    );
    assert_eq!(
        attr(&tag, "pageNumber-seperator").as_deref(),
        Some("/"),
        "{tag}"
    );
    let buf = buffer(&content);
    assert!(buf.contains("Av. Ayşe FATMA"), "diğer metin kayboldu");
    assert!(!buf.contains("2/5"), "donmuş numara kaldı: {buf:?}");
    assert!(
        warns.contains(&WarningCode::UdfPageNumberApproximated),
        "{warns:?}"
    );
}

/// Korpusta karşılığı GÖRÜLMEMİŞ biçimler tahmin edilmez: üst bilgide toplam
/// sayfasız numara, sekmeyle hizalanmış numara, başka alanlar. Bunlar bugünkü
/// gibi son görünen değerine sabitlenir ve bildirilir.
#[test]
fn page_number_shapes_the_corpus_never_shows_stay_frozen_and_reported() {
    for (kind, inner) in [
        (
            "header",
            p(&format!("{}{}", r("Sayfa "), simple("PAGE", "1"))),
        ),
        (
            "footer",
            p(&format!(
                "{}<w:r><w:tab/></w:r>{}",
                r("Av. Ayşe FATMA"),
                field("PAGE", "1")
            )),
        ),
        (
            "footer",
            p(&format!("{}{}", r("Tarih: "), field("DATE", "12.10.2026"))),
        ),
        (
            "footer",
            p(&format!("{}{}", field("PAGE", "1"), r(" sayfa"))),
        ),
        // Uzun ön metin (ad, adres): UYAP'ın `foreStr`'ı tam gösterdiği doğrulanmadı;
        // metin görünmez kalmasın diye paragraf olduğu gibi kalır.
        (
            "footer",
            p(&format!(
                "{}{}{}{}",
                r("Örnek Hukuk Bürosu — Dilekçe sayfası "),
                field("PAGE", "1"),
                r(" / "),
                field("NUMPAGES", "3")
            )),
        ),
        (
            "footer",
            p(&format!("{}{}", field("PAGE \\* ROMAN", "I"), r(""))),
        ),
    ] {
        let d = Docx {
            body: p(&r("Gövde")),
            parts: vec![(kind, "default", inner.clone())],
            ..Default::default()
        };
        let (content, warns) = to_udf(&build(&d));
        let tag = hf_tag(&content, kind).expect("parça yok");
        assert_eq!(attr(&tag, "pageNumber-spec"), None, "{inner}: {tag}");
        assert!(
            warns.contains(&WarningCode::FieldFlattened),
            "{inner}: {warns:?}"
        );
        assert!(
            !warns.contains(&WarningCode::UdfPageNumberApproximated),
            "{inner}: {warns:?}"
        );
    }
}

/// GölgeDosya'nın kendi UDF → Word sayfa numarası paragrafı geri döndüğünde AYNI
/// UYAP özniteliklerine çevrilir (eskiden "1/1" metni olarak dönüyordu; gerçek
/// UDF'lerin 137'sinde).
#[test]
fn the_engines_own_page_number_paragraph_round_trips_to_the_same_attributes() {
    let with_total = r#" pageNumber-spec="BSP32_2120" pageNumber-seperator="/" pageNumber-fontBold="false" pageNumber-fontItalic="false" pageNumber-fontFace="Arial" pageNumber-fontSize="11" pageNumber-color="-16777216" pageNumber-foreStr="" pageNumber-pageStartNumStr="""#;
    let without_total = r#" pageNumber-spec="BSP32_40" pageNumber-color="-16777216" pageNumber-fontFace="Arial" pageNumber-fontSize="11" pageNumber-foreStr="" pageNumber-pageStartNumStr="""#;
    let header_total = r#" pageNumber-spec="BSP32_2116" pageNumber-seperator="/" pageNumber-fontBold="false" pageNumber-fontItalic="false" pageNumber-fontFace="Arial Black" pageNumber-fontSize="9" pageNumber-color="-16777216" pageNumber-foreStr="Dosya" pageNumber-pageStartNumStr="1""#;
    for (in_header, attrs) in [
        (false, with_total),
        (false, without_total),
        (true, header_total),
    ] {
        let src = if in_header {
            udf(Some((attrs, "")), "Gövde", None)
        } else {
            udf(None, "Gövde", Some((attrs, "")))
        };
        let tag_name = if in_header { "header" } else { "footer" };
        let (docx, _) = to_docx(&src);
        let mut w = WarningSink::new();
        let back = convert_bytes(&docx, Direction::DocxToUdf, &mut w).unwrap();
        let codes: Vec<WarningCode> = w.into_vec().iter().map(|x| x.code).collect();
        let content = content_xml(&back);
        let tag = hf_tag(&content, tag_name).expect("parça kayboldu");
        let orig = format!("<x{attrs}>");
        for key in [
            "pageNumber-spec",
            "pageNumber-seperator",
            "pageNumber-fontBold",
            "pageNumber-fontItalic",
            "pageNumber-fontFace",
            "pageNumber-fontSize",
            "pageNumber-color",
            "pageNumber-foreStr",
            "pageNumber-pageStartNumStr",
        ] {
            assert_eq!(attr(&tag, key), attr(&orig, key), "{key}: {tag}");
        }
        // UYAP'ın kendi biçimi: numara metin değildir, parça tek boş paragraftır.
        let d = read_udf(&back);
        let part = if in_header {
            &d.sections[0].header
        } else {
            &d.sections[0].footer
        };
        assert_eq!(hf_texts(part), vec![String::new()], "{attrs}");
        assert!(!codes.contains(&WarningCode::FieldFlattened), "{codes:?}");
    }
}

/// "İlk sayfa farklı" + iki AYNI sayfa numarası alt bilgisi: Word iki parçaya ayrı
/// son görünen değer yazar ("1" ve "2"). Numara canlı kalınca iki parça aynıdır;
/// eskiden donmuş rakamlar farklı olduğu için "aktarılamadı" (KAYIP) çıkıyordu
/// (gerçek 3 belge).
#[test]
fn first_and_default_footers_differing_only_in_cached_page_digits_are_one_footer() {
    let d = Docx {
        body: p(&r("Gövde")),
        sect: "<w:titlePg/>".into(),
        parts: vec![
            ("footer", "default", p(&field("PAGE", "2"))),
            ("footer", "first", p(&field("PAGE", "1"))),
        ],
        ..Default::default()
    };
    let (content, warns) = to_udf(&build(&d));
    let tag = hf_tag(&content, "footer").expect("alt bilgi yok");
    assert_eq!(
        attr(&tag, "pageNumber-spec").as_deref(),
        Some("BSP32_40"),
        "{tag}"
    );
    assert_eq!(attr(&tag, "stopPage"), None, "{tag}");
    assert_eq!(attr(&tag, "startPage"), None, "{tag}");
    assert!(
        !warns.contains(&WarningCode::HeaderFooterVariantDropped),
        "{warns:?}"
    );
}

// ------------------------------------------------------------ (d) sayfa sonu

const PAGE_BREAK: &str = r#"<w:r><w:br w:type="page"/></w:r>"#;

/// Gövdenin blok dizisi: paragraf metni, sayfa sonu, tablo.
fn block_shape(d: &Document) -> Vec<String> {
    d.sections[0]
        .blocks
        .iter()
        .map(|b| match b {
            Block::Paragraph(p) => p.text(),
            Block::PageBreak => "<SAYFA SONU>".into(),
            Block::Table(_) => "<TABLO>".into(),
        })
        .collect()
}

/// "önce" + sayfa sonu + "sonra" aynı paragrafta: eskiden "öncesonra" tek sözcük
/// oluyordu. Paragraf sayfa sonunda bölünür; UDF'de sayfa sonu yine bildirilir.
#[test]
fn a_page_break_inside_a_paragraph_does_not_glue_the_words() {
    let body = p(&format!("{}{PAGE_BREAK}{}", r("önce"), r("sonra")));
    let bytes = build(&Docx {
        body,
        ..Default::default()
    });
    let (doc, _) = read_docx(&bytes);
    assert_eq!(block_shape(&doc), vec!["önce", "<SAYFA SONU>", "sonra"]);
    let (content, warns) = to_udf(&bytes);
    let buf = buffer(&content);
    assert!(!buf.contains("öncesonra"), "{buf:?}");
    assert!(buf.contains("önce\nsonra\n"), "{buf:?}");
    assert!(warns.contains(&WarningCode::PageBreakDropped), "{warns:?}");
}

/// Sayfa sonu paragrafın sonundaysa sayfa sonu paragraftan SONRA gelir (eskiden
/// öncesine konuyordu) ve boş bir satır eklenmez; başındaysa eskisi gibi önce
/// gelir. Ctrl+Enter'ın yazdığı yalnız sayfa sonlu paragraf değişmez.
#[test]
fn page_breaks_at_the_edges_of_a_paragraph_keep_their_place_without_new_lines() {
    let body = format!(
        "{}{}{}",
        p(&format!("{}{PAGE_BREAK}", r("önce"))),
        p(&format!("{PAGE_BREAK}{}", r("sonra"))),
        p(PAGE_BREAK)
    );
    let (doc, _) = read_docx(&build(&Docx {
        body,
        ..Default::default()
    }));
    assert_eq!(
        block_shape(&doc),
        vec![
            "önce",
            "<SAYFA SONU>",
            "<SAYFA SONU>",
            "sonra",
            "<SAYFA SONU>",
            ""
        ]
    );
}

/// `w:pageBreakBefore` (paragrafın kendisinde ya da stilinde): paragraf yeni
/// sayfada başlar. UDF'de karşılığı yok; eskiden hiçbir uyarı vermeden düşüyordu.
#[test]
fn page_break_before_is_reported_whether_direct_or_from_the_style() {
    let body = |ppr: &str| {
        format!(
            "{}<w:p><w:pPr>{ppr}</w:pPr>{}</w:p>",
            p(&r("Kapak")),
            r("Dilekçe")
        )
    };
    let styles = r#"<w:style w:type="paragraph" w:styleId="Bolum"><w:name w:val="Bölüm"/><w:pPr><w:pageBreakBefore/></w:pPr></w:style>"#;
    for (body, st) in [
        (body("<w:pageBreakBefore/>"), None),
        (
            body(r#"<w:pStyle w:val="Bolum"/>"#),
            Some(styles.to_string()),
        ),
    ] {
        let bytes = build(&Docx {
            body,
            styles: st,
            ..Default::default()
        });
        let (content, warns) = to_udf(&bytes);
        assert!(warns.contains(&WarningCode::PageBreakDropped), "{warns:?}");
        assert!(buffer(&content).contains("Kapak\nDilekçe\n"));
    }
    // Kapalı (`w:val="0"`) işaret sayfa sonu değildir.
    let (_, warns) = to_udf(&build(&Docx {
        body: body(r#"<w:pageBreakBefore w:val="0"/>"#),
        ..Default::default()
    }));
    assert!(!warns.contains(&WarningCode::PageBreakDropped), "{warns:?}");
}

// ------------------------------------------------ (e) sayfa yönü ve cilt payı

/// `w:orient` yazılmamış ama genişliği yüksekliğinden büyük sayfa yataydır. Eskiden
/// dikey A4'e çevriliyor ve "842x595 pt → A4" diye yanıltıcı bir boyut kaybı
/// bildiriliyordu.
#[test]
fn a_wide_page_without_orient_is_landscape_a4_not_a_size_change() {
    let bytes = build(&Docx {
        body: p(&r("Gövde")),
        sect: r#"<w:pgSz w:w="16838" w:h="11906"/>"#.into(),
        ..Default::default()
    });
    let (doc, _) = read_docx(&bytes);
    assert_eq!(doc.sections[0].page.orientation, Orientation::Landscape);
    let (content, warns) = to_udf(&bytes);
    assert!(!warns.contains(&WarningCode::PageSizeChanged), "{warns:?}");
    assert!(
        warns.contains(&WarningCode::PageOrientationUnverified),
        "{warns:?}"
    );
    assert!(
        content.contains("paperOrientation=\"0\""),
        "yatay yazılmadı"
    );
}

/// Cilt payı (`w:gutter`) Word'de sol kenar boşluğuna, `w:gutterAtTop` açıksa üst
/// kenar boşluğuna eklenir; düşerse metin sütunu kayar.
#[test]
fn the_gutter_is_added_to_the_left_or_top_margin() {
    let sect = r#"<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1418" w:right="1418" w:bottom="1418" w:left="1418" w:header="709" w:footer="709" w:gutter="567"/>"#;
    let (doc, _) = read_docx(&build(&Docx {
        body: p(&r("Gövde")),
        sect: sect.into(),
        ..Default::default()
    }));
    let page = &doc.sections[0].page;
    assert!(
        (page.margin_left_pt - (1418.0 + 567.0) / 20.0).abs() < 0.01,
        "{page:?}"
    );
    assert!(
        (page.margin_right_pt - 1418.0 / 20.0).abs() < 0.01,
        "{page:?}"
    );
    assert!(
        (page.margin_top_pt - 1418.0 / 20.0).abs() < 0.01,
        "{page:?}"
    );

    let (doc, _) = read_docx(&build(&Docx {
        body: p(&r("Gövde")),
        sect: sect.into(),
        settings: Some("<w:gutterAtTop/>".into()),
        ..Default::default()
    }));
    let page = &doc.sections[0].page;
    assert!(
        (page.margin_top_pt - (1418.0 + 567.0) / 20.0).abs() < 0.01,
        "{page:?}"
    );
    assert!(
        (page.margin_left_pt - 1418.0 / 20.0).abs() < 0.01,
        "{page:?}"
    );
}
