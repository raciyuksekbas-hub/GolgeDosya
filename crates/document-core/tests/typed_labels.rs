//! Elle yazılmış madde numarasından sonraki boşluklar UDF'te sekme olur.
//!
//! Saha: "4. ve 5. yazdıktan sonra birkaç boşluk oluyor. Ya tamamen bitişik
//! olmalı ya da cetvelde düzelttiğim statüye uymalı." Word'de `4.␠␠Metin` asılı
//! girintili, iki yana yaslı bir paragraftı; UYAP boşlukları geriyordu ve metin
//! her maddede başka yerden başlıyordu.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes, convert_bytes_with, ConvertOptions, Direction};
use tavzih_core::docx;
use tavzih_core::model::*;
use tavzih_core::warnings::WarningSink;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// 567 twip = 1 cm = 28,35 pt: sahadaki dilekçenin asılı girintisi.
const HANGING_1CM: &str = r#"<w:ind w:left="567" w:hanging="567"/><w:jc w:val="both"/>"#;

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

fn p(ppr: &str, runs: &str) -> String {
    format!("<w:p><w:pPr>{ppr}</w:pPr>{runs}</w:p>")
}
fn r(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}
fn b(text: &str) -> String {
    format!(r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

fn to_udf_xml(docx: &[u8]) -> String {
    let mut w = WarningSink::new();
    let udf = convert_bytes_with(
        docx,
        Direction::DocxToUdf,
        &ConvertOptions::default(),
        &mut w,
    )
    .expect("convert");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf)).unwrap();
    let mut xml = String::new();
    z.by_name("content.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
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

/// Tek paragraflık belgenin UDF satırı.
fn only_line(paragraph: String, numbering: Option<&str>) -> (String, String) {
    let l = lines(&to_udf_xml(&package(&paragraph, numbering)));
    assert_eq!(l.len(), 1, "{l:?}");
    l.into_iter().next().unwrap()
}

#[test]
fn the_field_case_starts_the_text_at_the_hanging_indent() {
    let (tag, text) = only_line(
        p(HANGING_1CM, &r("4.  Davacı vekili süreci açıklamıştır.")),
        None,
    );
    assert_eq!(text, "4.⇥Davacı vekili süreci açıklamıştır.");
    assert_eq!(attr(&tag, "TabSet").as_deref(), Some("28.35:0:0"), "{tag}");
    // Girinti, kullanıcının cetvelde gördüğü gibi kalır.
    assert_eq!(attr(&tag, "Hanging").as_deref(), Some("28.35"), "{tag}");
    assert_eq!(attr(&tag, "LeftIndent"), None, "{tag}");
    assert_eq!(attr(&tag, "Alignment").as_deref(), Some("3"), "{tag}");
}

#[test]
fn every_usual_label_form_is_aligned() {
    for (label, expected) in [
        ("1.", "1.⇥Metin burada."),
        ("11.", "11.⇥Metin burada."),
        ("A.", "A.⇥Metin burada."),
        ("Ç.", "Ç.⇥Metin burada."),
        ("a)", "a)⇥Metin burada."),
        ("1)", "1)⇥Metin burada."),
        ("(1)", "(1)⇥Metin burada."),
        ("1-", "1-⇥Metin burada."),
        ("IV.", "IV.⇥Metin burada."),
        ("1.2.", "1.2.⇥Metin burada."),
    ] {
        let (tag, text) = only_line(p(HANGING_1CM, &r(&format!("{label}  Metin burada."))), None);
        assert_eq!(text, expected);
        assert_eq!(
            attr(&tag, "TabSet").as_deref(),
            Some("28.35:0:0"),
            "{label}"
        );
    }
}

#[test]
fn any_number_of_spaces_becomes_one_tab() {
    let (_, text) = only_line(p(HANGING_1CM, &r("4.      Metin burada.")), None);
    assert_eq!(text, "4.⇥Metin burada.");
}

#[test]
fn a_label_split_across_runs_is_still_aligned() {
    for (runs, expected) in [
        // Kalın numara, boşluklar gövdede.
        (
            format!("{}{}", b("4."), r("  Metin burada.")),
            "4.⇥Metin burada.",
        ),
        // Boşluklar numarayla aynı parçada, gövde ayrı.
        (
            format!("{}{}", b("4.  "), r("Metin burada.")),
            "4.⇥Metin burada.",
        ),
        // Boşluklar iki parçaya bölünmüş.
        (
            format!("{}{}", r("4. "), r(" Metin burada.")),
            "4.⇥Metin burada.",
        ),
        // Numara iki parçaya bölünmüş.
        (
            format!("{}{}", r("1"), r("1.  Metin burada.")),
            "11.⇥Metin burada.",
        ),
    ] {
        let (tag, text) = only_line(p(HANGING_1CM, &runs), None);
        assert_eq!(text, expected);
        assert_eq!(attr(&tag, "TabSet").as_deref(), Some("28.35:0:0"), "{tag}");
    }
}

#[test]
fn the_label_keeps_its_own_formatting() {
    let xml = to_udf_xml(&package(
        &p(HANGING_1CM, &format!("{}{}", b("4."), r("  Metin burada."))),
        None,
    ));
    let elems = &xml[xml.find("<elements").unwrap()..];
    let first = &elems[elems.find("<content").unwrap()..];
    assert!(
        first[..first.find("/>").unwrap()].contains(r#"bold="true""#),
        "{first}"
    );
    // Yalnız numara kalın; gövde kalınlaşmaz (`<tab>` biçim taşımaz).
    assert_eq!(xml.matches(r#"bold="true""#).count(), 1, "{xml}");
}

/// Saha: istinaf dilekçesinde maddeler `1.⇥Metin` olarak yazılmıştı (0,75 cm
/// asılı girinti). Word sekmeyi girinti noktasına götürür; UDF'te durak yoktu ve
/// UYAP metni 72 pt'ye atıyordu.
#[test]
fn a_tab_after_the_number_goes_to_the_hanging_indent() {
    let ppr = r#"<w:ind w:left="425" w:hanging="425"/><w:jc w:val="both"/>"#;
    for runs in [
        format!(
            "{}<w:r><w:tab/></w:r>{}",
            r("1."),
            r("Müvekkil ile davalı arasında")
        ),
        r("1.\tMüvekkil ile davalı arasında"),
    ] {
        let (tag, text) = only_line(p(ppr, &runs), None);
        assert_eq!(text, "1.⇥Müvekkil ile davalı arasında");
        assert_eq!(attr(&tag, "TabSet").as_deref(), Some("21.25:0:0"), "{tag}");
        assert_eq!(attr(&tag, "Hanging").as_deref(), Some("21.25"), "{tag}");
    }
}

/// Bu paragraflarda boşluklar numara ayıracı değildir ya da sekme yanlış yere
/// gider: metin ve paragraf biçimi olduğu gibi kalmalı.
#[test]
fn what_is_not_a_typed_label_stays_as_written() {
    let wide = r#"<w:ind w:left="300" w:hanging="300"/><w:jc w:val="both"/>"#;
    let nested = r#"<w:ind w:left="1134" w:hanging="567"/><w:jc w:val="both"/>"#;
    let centered = r#"<w:ind w:left="567" w:hanging="567"/><w:jc w:val="center"/>"#;
    let plain = r#"<w:jc w:val="both"/>"#;
    let early_stop = r#"<w:tabs><w:tab w:val="left" w:pos="300"/></w:tabs><w:ind w:left="567" w:hanging="567"/><w:jc w:val="both"/>"#;
    for (ppr, written) in [
        // Tek boşluk: "2. Hukuk Dairesi" gibi cümle başı da olabilir.
        (HANGING_1CM, "2. Hukuk Dairesi kararı."),
        (HANGING_1CM, "A. Yılmaz tarafından verildi."),
        // Numara değil.
        (HANGING_1CM, "Ek-1:  Tutanak örneği."),
        (HANGING_1CM, "DAVACI    : Örnek Kişi"),
        (HANGING_1CM, "Av.  Örnek Avukat"),
        (HANGING_1CM, "2026.  yılı içinde."),
        (HANGING_1CM, "md.  119 uyarınca."),
        // Numaradan sonra metin yok.
        (HANGING_1CM, "4.  "),
        // Asılı girinti yok: hizalanacak nokta da yok.
        (plain, "4.  Metin burada."),
        // Numara asılı girintiye sığmıyor: sekme sonraki durağa atlardı.
        (wide, "11.  Metin burada."),
        // İlk satır kenarda değil: bu biçim UYAP'ta doğrulanmadı.
        (nested, "4.  Metin burada."),
        // Ortalanmış paragrafın sütunu yok.
        (centered, "4.  Metin burada."),
        // Yazarın kendi sekme düzeni var.
        (early_stop, "4.  Metin burada."),
    ] {
        let (tag, text) = only_line(p(ppr, &r(written)), None);
        assert_eq!(text, written, "{ppr}");
        let stops = (ppr == early_stop).then(|| "15.0:0:0".to_string());
        assert_eq!(attr(&tag, "TabSet"), stops, "{written}: {tag}");
    }
}

/// Word'ün kendi listesi numarasını UDF listesi ya da sekmeyle zaten hizalar.
#[test]
fn word_lists_are_left_to_the_list_path() {
    let numbering = r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="567" w:hanging="567"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#;
    let num = r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#;
    let (tag, text) = only_line(p(num, &r("Metin  burada.")), Some(numbering));
    assert!(attr(&tag, "ListId").is_some(), "{tag}");
    assert_eq!(text, "Metin  burada.");
}

/// Yalnız gövde: üst ve alt bilgi, tablo hücreleri bu turda doğrulanmadı.
#[test]
fn table_cells_are_not_touched() {
    let cell = p(HANGING_1CM, &r("4.  Hücre metni."));
    let tbl = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="9000" w:type="dxa"/></w:tcPr>{cell}</w:tc></w:tr></w:tbl>"#
    );
    let xml = to_udf_xml(&package(&tbl, None));
    assert!(xml.contains("4.  Hücre metni."), "{xml}");
    assert!(!xml.contains("TabSet"), "{xml}");
}

/// Word'e geri dönüşte madde numarası, sekmesi ve asılı girintisiyle gelir.
#[test]
fn the_aligned_label_survives_the_way_back_to_word() {
    let source = package(&p(HANGING_1CM, &r("4.  Metin burada.")), None);
    let mut w = WarningSink::new();
    let udf = convert_bytes(&source, Direction::DocxToUdf, &mut w).unwrap();
    let back = convert_bytes(&udf, Direction::UdfToDocx, &mut w).unwrap();
    let doc = docx::reader::read_docx(&back, &mut w).unwrap();
    let Block::Paragraph(para) = &doc.sections[0].blocks[0] else {
        panic!("paragraf bekleniyordu");
    };
    assert_eq!(para.text(), "4.\tMetin burada.");
    assert!((para.props.hanging_pt - 28.35).abs() < 0.01);
    assert!((para.props.left_indent_pt - 28.35).abs() < 0.01);
    assert!(para
        .props
        .tab_stops
        .iter()
        .any(|t| (t.pos_pt - 28.35).abs() < 0.01));
}
