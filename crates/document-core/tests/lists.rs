//! Word liste numaraları: UDF, Word'de GÖRÜNEN numarayı göstermeli.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (199 gerçek Word belgesi üzerinde
//! ölçüldü, burada sentetik olarak kurulur): okuyucu bir listenin yalnız İLK
//! kullanılan düzeyinin `numFmt`'ini okuyordu; `lvlText`, `start`,
//! `startOverride`, `lvlRestart`, `isLgl` ve aynı `abstractNum`'u paylaşan
//! listelerin numarayı SÜRDÜRMESİ yok sayılıyordu. "1)" → "1.", "3.2" → "2.",
//! 5'ten başlayan liste 1'den başlıyordu; yüzlerce gerçek paragrafta, uyarısız.
//!
//! Kural: UDF'te birebir aynı görünen, gerçek UYAP dosyalarında görülmüş bir
//! liste türü varsa liste UDF listesi olarak kalır; yoksa Word'ün gösterdiği
//! etiket paragrafın başına METİN olarak yazılır ve bu uyarıyla bildirilir.
//! UDF → Word yönü `udf_lists.rs`'te.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::collections::BTreeMap;
use std::io::{Read, Write};

use quick_xml::events::Event;
use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::warnings::{Warning, WarningCode, WarningSink};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

// ------------------------------------------------------------ Word tarafı

fn docx(body: &str, numbering: &str, styles: Option<&str>) -> Vec<u8> {
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        z.start_file("word/document.xml", o).unwrap();
        z.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#
            )
            .as_bytes(),
        )
        .unwrap();
        z.start_file("word/numbering.xml", o).unwrap();
        z.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="{W}">{numbering}</w:numbering>"#
            )
            .as_bytes(),
        )
        .unwrap();
        if let Some(styles) = styles {
            z.start_file("word/styles.xml", o).unwrap();
            z.write_all(
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{W}">{styles}</w:styles>"#
                )
                .as_bytes(),
            )
            .unwrap();
        }
        z.finish().unwrap();
    }
    cur.into_inner()
}

/// Bir `w:lvl`. `extra` `w:lvlText`'ten önce gelir (`w:lvlRestart`,
/// `w:pStyle`, `w:isLgl`, `w:suff`).
fn lvl(ilvl: u32, start: i64, fmt: &str, text: &str, extra: &str) -> String {
    format!(
        r#"<w:lvl w:ilvl="{ilvl}"><w:start w:val="{start}"/><w:numFmt w:val="{fmt}"/>{extra}<w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{left}" w:hanging="360"/></w:pPr></w:lvl>"#,
        left = 720 * (ilvl + 1)
    )
}

/// Word'ün kendi madde imi düzeyi: Symbol yazı tipinde U+F0B7.
fn symbol_bullet(ilvl: u32) -> String {
    lvl(ilvl, 1, "bullet", "\u{F0B7}", "").replace(
        "</w:pPr></w:lvl>",
        r#"</w:pPr><w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol" w:hint="default"/></w:rPr></w:lvl>"#,
    )
}

fn abstract_num(id: u32, levels: &[String]) -> String {
    format!(
        r#"<w:abstractNum w:abstractNumId="{id}">{}</w:abstractNum>"#,
        levels.concat()
    )
}

fn num(id: u32, abstract_id: u32, overrides: &str) -> String {
    format!(r#"<w:num w:numId="{id}"><w:abstractNumId w:val="{abstract_id}"/>{overrides}</w:num>"#)
}

/// Numaralı paragraf (`w:numPr` doğrudan).
fn item(num_id: u32, ilvl: u32, text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num_id}"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
    )
}

fn plain(text: &str) -> String {
    format!(r#"<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"#)
}

// ------------------------------------------------------------ UDF tarafı

/// UDF paragrafı: öznitelikleri ve metni (yaprakların arabellekteki karşılığı).
#[derive(Debug)]
struct UdfPara {
    attrs: BTreeMap<String, String>,
    text: String,
}

fn content_xml(udf: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf.to_vec())).unwrap();
    let mut s = String::new();
    z.by_name("content.xml")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

fn udf_paragraphs(udf: &[u8]) -> Vec<UdfPara> {
    let xml = content_xml(udf);
    let start = xml.find("<![CDATA[").expect("content") + 9;
    let end = xml[start..].find("]]>").unwrap() + start;
    let buf: Vec<u16> = xml[start..end].encode_utf16().collect();
    let from = xml.find("<elements").unwrap();
    let to = xml.find("</elements>").unwrap() + "</elements>".len();
    let mut rd = quick_xml::Reader::from_str(&xml[from..to]);
    let mut out = Vec::new();
    let mut cur: Option<UdfPara> = None;
    loop {
        match rd.read_event().unwrap() {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                let attrs: BTreeMap<String, String> = e
                    .attributes()
                    .flatten()
                    .map(|a| {
                        (
                            String::from_utf8_lossy(a.key.as_ref()).to_string(),
                            a.unescape_value().unwrap().to_string(),
                        )
                    })
                    .collect();
                match e.name().as_ref() {
                    b"paragraph" => {
                        if let Some(p) = cur.take() {
                            out.push(p);
                        }
                        cur = Some(UdfPara {
                            attrs,
                            text: String::new(),
                        });
                    }
                    b"content" | b"tab" | b"space" => {
                        let s: usize = attrs["startOffset"].parse().unwrap();
                        let l: usize = attrs["length"].parse().unwrap();
                        if let Some(p) = cur.as_mut() {
                            p.text.push_str(&String::from_utf16_lossy(&buf[s..s + l]));
                        }
                    }
                    _ => {}
                }
            }
            Event::End(e) if e.name().as_ref() == b"paragraph" => {
                if let Some(p) = cur.take() {
                    out.push(p);
                }
            }
            _ => {}
        }
    }
    for p in &mut out {
        if p.text.ends_with('\n') {
            p.text.pop();
        }
    }
    out
}

fn to_udf(docx: &[u8]) -> (Vec<u8>, Vec<Warning>) {
    let mut w = WarningSink::new();
    let udf = convert_bytes(docx, Direction::DocxToUdf, &mut w).expect("docx->udf");
    (udf, w.into_vec())
}

fn list_warnings(w: &[Warning]) -> Vec<String> {
    w.iter()
        .filter(|w| w.code == WarningCode::ListNumberingApproximated)
        .map(|w| w.detail.clone().unwrap_or_default())
        .collect()
}

fn token(p: &UdfPara) -> Option<&str> {
    p.attrs
        .get("NumberType")
        .or_else(|| p.attrs.get("BulletType"))
        .map(|s| s.as_str())
}

// ===================================================================== Word → UDF

#[test]
fn a_paren_list_keeps_its_parenthesis() {
    // Gerçek 10 belgede 443 paragraf: Word "1)" gösteriyor, UDF "1." diyordu.
    // UYAP'ın kendi "1)" türü (`NUMBER_TYPE_NUMBER_PARANTHESE`) gerçek imzalı
    // dosyalarda var: liste UDF listesi olarak kalır.
    let numbering = format!(
        "{}{}",
        abstract_num(0, &[lvl(0, 1, "decimal", "%1)", "")]),
        num(1, 0, "")
    );
    let body = format!("{}{}", item(1, 0, "Birinci"), item(1, 0, "İkinci"));
    let (udf, w) = to_udf(&docx(&body, &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(ps.len(), 2);
    for p in &ps {
        assert_eq!(token(p), Some("NUMBER_TYPE_NUMBER_PARANTHESE"), "{p:?}");
    }
    assert_eq!(ps[0].text, "Birinci", "UDF listesi numarayı kendisi çizer");
    assert!(list_warnings(&w).is_empty(), "{w:?}");
}

#[test]
fn a_list_that_starts_at_five_keeps_its_numbers() {
    // Gerçek 6 belgede 177 paragraf 1'den başlamayan listedeydi; UDF'te 1'den
    // başlıyordu. UDF'te başlangıç numarası yok: numara metin olarak yazılır.
    let numbering = format!(
        "{}{}",
        abstract_num(0, &[lvl(0, 5, "decimal", "%1.", "")]),
        num(1, 0, "")
    );
    let body = format!("{}{}", item(1, 0, "Beşinci"), item(1, 0, "Altıncı"));
    let (udf, w) = to_udf(&docx(&body, &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(ps[0].text, "5.\tBeşinci");
    assert_eq!(ps[1].text, "6.\tAltıncı");
    assert!(
        ps.iter().all(|p| !p.attrs.contains_key("ListId")),
        "numara hem metinde hem listede olamaz: {ps:?}"
    );
    let lw = list_warnings(&w);
    assert!(lw.iter().any(|d| d.contains("başlangıç 5")), "{lw:?}");
}

#[test]
fn lists_sharing_an_abstract_continue_and_a_start_override_restarts() {
    // Word'de aynı `abstractNum`'a bağlı iki `w:num` numarayı SÜRDÜRÜR;
    // "Numaralandırmayı yeniden başlat" yeni bir `w:num` + `startOverride`
    // yazar. Okuyucu her `numId`'yi ayrı liste sayıyordu: 3 olması gereken
    // madde 1 oluyordu.
    let numbering = format!(
        "{}{}{}{}",
        abstract_num(0, &[lvl(0, 1, "decimal", "%1.", "")]),
        num(1, 0, ""),
        num(2, 0, ""),
        num(
            3,
            0,
            r#"<w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride>"#
        )
    );
    let body = [
        item(1, 0, "Bir"),
        item(1, 0, "İki"),
        plain("Ara metin"),
        item(2, 0, "Üç"),
        item(3, 0, "Yeniden bir"),
        item(3, 0, "Yeniden iki"),
    ]
    .concat();
    let (udf, w) = to_udf(&docx(&body, &numbering, None));
    let ps = udf_paragraphs(&udf);
    let id = |i: usize| ps[i].attrs.get("ListId").cloned();
    assert!(id(0).is_some());
    assert_eq!(id(0), id(1));
    assert_eq!(id(0), id(3), "aynı abstractNum: numara sürer (3.)");
    assert!(id(4).is_some());
    assert_ne!(id(0), id(4), "startOverride: yeni liste 1'den başlar");
    assert_eq!(id(4), id(5));
    assert!(list_warnings(&w).is_empty(), "{w:?}");
}

#[test]
fn a_start_override_other_than_one_is_written_as_text() {
    let numbering = format!(
        "{}{}",
        abstract_num(0, &[lvl(0, 1, "decimal", "%1.", "")]),
        num(
            7,
            0,
            r#"<w:lvlOverride w:ilvl="0"><w:startOverride w:val="3"/></w:lvlOverride>"#
        )
    );
    let body = format!("{}{}", item(7, 0, "Üç"), item(7, 0, "Dört"));
    let (udf, w) = to_udf(&docx(&body, &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(ps[0].text, "3.\tÜç");
    assert_eq!(ps[1].text, "4.\tDört");
    assert!(list_warnings(&w).iter().any(|d| d.contains("başlangıç 3")));
}

#[test]
fn hierarchical_numbers_are_written_as_word_shows_them() {
    // "%1.%2." gerçek belgelerde 533 paragraf: madde "3.2" UDF'te "2." oluyordu.
    let numbering = format!(
        "{}{}",
        abstract_num(
            0,
            &[
                lvl(0, 1, "decimal", "%1.", ""),
                lvl(1, 1, "decimal", "%1.%2.", "")
            ]
        ),
        num(1, 0, "")
    );
    let body = [
        item(1, 0, "Olaylar"),
        item(1, 1, "Birinci olay"),
        item(1, 1, "İkinci olay"),
        item(1, 0, "Hukuki nedenler"),
        item(1, 1, "Birinci neden"),
    ]
    .concat();
    let (udf, w) = to_udf(&docx(&body, &numbering, None));
    let ps = udf_paragraphs(&udf);
    let texts: Vec<&str> = ps.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "Olaylar",
            "1.1.\tBirinci olay",
            "1.2.\tİkinci olay",
            "Hukuki nedenler",
            "2.1.\tBirinci neden"
        ]
    );
    // Birinci düzey UDF'in kendi "1." listesi olarak kalır.
    assert_eq!(token(&ps[0]), Some("NUMBER_TYPE_NUMBER_DOT"));
    assert_eq!(ps[0].attrs.get("ListId"), ps[3].attrs.get("ListId"));
    assert!(!ps[1].attrs.contains_key("ListId"));
    assert!(
        list_warnings(&w).iter().any(|d| d.contains("1.1.")),
        "{w:?}"
    );
}

#[test]
fn each_level_keeps_its_own_marker() {
    // Listenin türü ilk görülen düzeyden bir kez seçiliyordu: "a)" ve madde
    // imi alt düzeyleri "1." oluyordu (gerçek 6 belgede 24 paragraf).
    let numbering = format!(
        "{}{}",
        abstract_num(
            0,
            &[
                lvl(0, 1, "decimal", "%1.", ""),
                lvl(1, 1, "lowerLetter", "%2)", ""),
                symbol_bullet(2)
            ]
        ),
        num(1, 0, "")
    );
    let body = [item(1, 0, "Bir"), item(1, 1, "a şıkkı"), item(1, 2, "imli")].concat();
    let (udf, _) = to_udf(&docx(&body, &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(token(&ps[0]), Some("NUMBER_TYPE_NUMBER_DOT"));
    assert_eq!(token(&ps[1]), Some("NUMBER_TYPE_CHAR_SMALL_PARANTHESE"));
    assert_eq!(token(&ps[2]), Some("BULLET_TYPE_ELLIPSE"));
    assert_eq!(ps[1].attrs.get("ListLevel").map(|s| s.as_str()), Some("2"));
}

#[test]
fn a_number_format_of_none_invents_no_number() {
    let numbering = format!(
        "{}{}",
        abstract_num(0, &[lvl(0, 1, "none", "", "")]),
        num(1, 0, "")
    );
    let (udf, w) = to_udf(&docx(&item(1, 0, "Numarasız"), &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(ps[0].text, "Numarasız");
    assert!(
        !ps[0].attrs.contains_key("Numbered") && !ps[0].attrs.contains_key("ListId"),
        "Word bu paragrafa numara koymuyor: {ps:?}"
    );
    assert!(list_warnings(&w).is_empty(), "{w:?}");
}

#[test]
fn a_dash_label_and_a_literal_prefix_are_written_as_text() {
    // "%1-" (gerçek 1 belgede 21 paragraf) için UYAP'ta görüntüsü kanıtlanmış
    // bir tür yok; "Madde %1:" gibi sözcüklü etiket de eskiden kayboluyordu.
    let numbering = format!(
        "{}{}{}{}",
        abstract_num(0, &[lvl(0, 1, "decimal", "%1-", "")]),
        abstract_num(1, &[lvl(0, 1, "decimal", "Madde %1:", "")]),
        num(1, 0, ""),
        num(2, 1, "")
    );
    let body = [
        item(1, 0, "Tireli"),
        item(1, 0, "Tireli iki"),
        item(2, 0, "Sözcüklü"),
    ]
    .concat();
    let (udf, w) = to_udf(&docx(&body, &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(ps[0].text, "1-\tTireli");
    assert_eq!(ps[1].text, "2-\tTireli iki");
    assert_eq!(ps[2].text, "Madde 1:\tSözcüklü");
    assert_eq!(list_warnings(&w).len(), 2, "{w:?}");
}

#[test]
fn legal_numbering_and_label_suffixes_follow_word() {
    // `w:isLgl`: alt düzey, üst düzey Roma rakamıyla yazılsa da onu ondalık
    // gösterir ("I." altında "1.1"). `w:suff` etiketten sonra sekme yerine
    // boşluk ya da hiçbir şey koyar.
    let numbering = format!(
        "{}{}",
        abstract_num(
            0,
            &[
                lvl(0, 1, "upperRoman", "%1.", ""),
                lvl(
                    1,
                    1,
                    "decimal",
                    "%1.%2",
                    r#"<w:isLgl/><w:suff w:val="space"/>"#
                ),
                lvl(2, 1, "lowerLetter", "(%3)", r#"<w:suff w:val="nothing"/>"#)
            ]
        ),
        num(1, 0, "")
    );
    let body = [
        item(1, 0, "Giriş"),
        item(1, 1, "Kapsam"),
        item(1, 2, "bent"),
    ]
    .concat();
    let (udf, _) = to_udf(&docx(&body, &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(token(&ps[0]), Some("NUMBER_TYPE_ROMAN_BIG_DOT"));
    assert_eq!(ps[1].text, "1.1 Kapsam");
    assert_eq!(ps[2].text, "(a)bent");
}

#[test]
fn a_lower_level_does_not_restart_when_lvl_restart_says_never() {
    // `w:lvlRestart w:val="0"`: alt düzey üst düzey ilerlese de sürer.
    let numbering = format!(
        "{}{}",
        abstract_num(
            0,
            &[
                lvl(0, 1, "decimal", "%1.", ""),
                lvl(1, 1, "lowerLetter", "%2)", r#"<w:lvlRestart w:val="0"/>"#)
            ]
        ),
        num(1, 0, "")
    );
    let body = [
        item(1, 0, "Bir"),
        item(1, 1, "a"),
        item(1, 1, "b"),
        item(1, 0, "İki"),
        item(1, 1, "c olmalı"),
    ]
    .concat();
    let (udf, _) = to_udf(&docx(&body, &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(token(&ps[1]), Some("NUMBER_TYPE_CHAR_SMALL_PARANTHESE"));
    // UDF'in kendi sırası burada "a)" derdi; Word "c)" gösteriyor.
    assert_eq!(ps[4].text, "c)\tc olmalı");
}

#[test]
fn numbering_from_a_paragraph_style_is_numbered_like_word() {
    // Başlık stilleri numarayı stilden alır; düzey `w:lvl/w:pStyle` ile stile
    // bağlıdır. Gerçek 96 belgenin stillerinde `w:numPr` var.
    let styles = r#"<w:style w:type="paragraph" w:styleId="Baslik1"><w:name w:val="heading 1"/><w:pPr><w:numPr><w:numId w:val="4"/></w:numPr></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Baslik2"><w:name w:val="heading 2"/><w:pPr><w:numPr><w:numId w:val="4"/></w:numPr></w:pPr></w:style>"#;
    let numbering = format!(
        "{}{}",
        abstract_num(
            0,
            &[
                lvl(0, 1, "decimal", "%1.", r#"<w:pStyle w:val="Baslik1"/>"#),
                lvl(1, 1, "decimal", "%1.%2.", r#"<w:pStyle w:val="Baslik2"/>"#)
            ]
        ),
        num(4, 0, "")
    );
    let h = |style: &str, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    let body = [
        h("Baslik1", "Olaylar"),
        h("Baslik2", "Sözleşme"),
        h("Baslik2", "İhtar"),
    ]
    .concat();
    let (udf, _) = to_udf(&docx(&body, &numbering, Some(styles)));
    let ps = udf_paragraphs(&udf);
    assert_eq!(token(&ps[0]), Some("NUMBER_TYPE_NUMBER_DOT"));
    assert_eq!(ps[1].text, "1.1.\tSözleşme");
    assert_eq!(ps[2].text, "1.2.\tİhtar");
}

#[test]
fn a_label_written_as_text_keeps_the_indent_of_its_level() {
    // Word, numaralı paragrafı düzeyin `w:ind`'iyle girintili gösterir.
    // Etiket metne yazılınca bu girinti paragrafa taşınmazsa sekme etiketi
    // metinden ayırmaz, asılı girinti kaybolur.
    let numbering = format!(
        "{}{}",
        abstract_num(0, &[lvl(0, 4, "decimal", "%1.", "")]),
        num(1, 0, "")
    );
    let (udf, _) = to_udf(&docx(&item(1, 0, "Dördüncü"), &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(ps[0].text, "4.\tDördüncü");
    // left=720 (36 pt), hanging=360 (18 pt): ilk satır 18 pt'de, gövde 36 pt'de.
    assert_eq!(
        ps[0].attrs.get("LeftIndent").map(|s| s.as_str()),
        Some("18.0")
    );
    assert_eq!(ps[0].attrs.get("Hanging").map(|s| s.as_str()), Some("18.0"));
}

#[test]
fn a_bullet_unlike_uyaps_round_bullet_is_written_as_text() {
    // "–" madde imi (gerçek 3 belge) UDF'te yuvarlak imle çıkıyordu.
    let numbering = format!(
        "{}{}",
        abstract_num(0, &[lvl(0, 1, "bullet", "\u{2013}", "")]),
        num(1, 0, "")
    );
    let (udf, w) = to_udf(&docx(&item(1, 0, "Tireli madde"), &numbering, None));
    let ps = udf_paragraphs(&udf);
    assert_eq!(ps[0].text, "\u{2013}\tTireli madde");
    assert!(!ps[0].attrs.contains_key("Bulleted"));
    assert_eq!(list_warnings(&w).len(), 1);
}

// ===================================================================== UDF → Word → UDF

fn udf(paragraphs: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = String::new();
    let mut elems = String::new();
    for (attrs, text) in paragraphs {
        let start = buf.encode_utf16().count();
        buf.push_str(text);
        buf.push('\n');
        let len = text.encode_utf16().count() + 1;
        elems.push_str(&format!(
            r#"<paragraph Alignment="0" {attrs}><content startOffset="{start}" length="{len}" /></paragraph>"#
        ));
    }
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?><template format_id="1.8"><content><![CDATA[{buf}]]></content><properties><pageFormat mediaSizeName="1" leftMargin="70.875" rightMargin="70.875" topMargin="70.875" bottomMargin="70.875" paperOrientation="1" headerFOffset="20.0" footerFOffset="20.0" /></properties><elements resolver="hvl-default">{elems}</elements><styles><style name="hvl-default" family="Times New Roman" size="12" description="Gövde" /></styles></template>"#
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
fn udf_list_types_come_back_unchanged_through_word() {
    // UDF → Word → UDF: Word'e yazılan her tür geri aynı UDF türüne döner;
    // alt düzey kendi sayacıyla UDF listesi olarak kalır. Eskiden hepsi "1."
    // ya da yuvarlak madde imi olarak dönüyordu.
    let rows = [
        r#"ListId="1" ListLevel="1" Numbered="true" NumberType="NUMBER_TYPE_ROMAN_BIG_DOT""#,
        r#"ListId="1" ListLevel="2" Numbered="true" NumberType="NUMBER_TYPE_CHAR_SMALL_DOT""#,
        r#"ListId="2" ListLevel="1" Numbered="true" NumberType="NUMBER_TYPE_NUMBER_PARANTHESE""#,
        r#"ListId="3" ListLevel="1" Bulleted="true" BulletType="BULLET_TYPE_ARROW""#,
        r#"ListId="4" ListLevel="1" Bulleted="true" BulletType="BULLET_TYPE_TRIANGLE""#,
    ];
    let paras: Vec<(&str, &str)> = rows.iter().map(|a| (*a, "Madde")).collect();
    let mut w = WarningSink::new();
    let word = convert_bytes(&udf(&paras), Direction::UdfToDocx, &mut w).expect("udf->docx");
    let (back, _) = to_udf(&word);
    let ps = udf_paragraphs(&back);
    let tokens: Vec<Option<&str>> = ps.iter().map(token).collect();
    assert_eq!(
        tokens,
        vec![
            Some("NUMBER_TYPE_ROMAN_BIG_DOT"),
            Some("NUMBER_TYPE_CHAR_SMALL_DOT"),
            Some("NUMBER_TYPE_NUMBER_PARANTHESE"),
            Some("BULLET_TYPE_ARROW"),
            Some("BULLET_TYPE_TRIANGLE")
        ]
    );
    assert_eq!(ps[1].attrs.get("ListLevel").map(|s| s.as_str()), Some("2"));
    assert!(ps.iter().all(|p| p.text == "Madde"), "{ps:?}");
}
