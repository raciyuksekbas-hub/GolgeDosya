//! Word paragrafının İÇİNDEKİ özel öğeler: yalnız görünen metin, tam bir kez.
//!
//! v0.3.2 kabul turunun bağımsız denetimi (199 gerçek Word belgesi üzerinde
//! ölçüldü, burada sentetik olarak kurulur):
//! - paragraf içindeki HER metin düğümü yazılıyordu: çizim konumları
//!   (`wp:posOffset`) gerçek belgelerde hayalet rakam olarak çıktı;
//! - yalnız bölünmez boşluk taşıyan `w:t` siliniyor, sözcükler birleşiyordu;
//! - metin kutusu dıştaki paragrafı siliyor, `mc:Fallback` kopyası ikinci kez
//!   yazılıyordu;
//! - bölünmez kısa çizgi düşüyor ("2024-129" → "2024129"), gizli metin
//!   yayımlanıyor, karakter stili ve büyük harf biçimi yok sayılıyordu.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::{Read, Write};

use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::docx;
use tavzih_core::model::*;
use tavzih_core::warnings::{WarningCode, WarningSink};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing" mc:Ignorable="wp14""#;
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// 1x1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn package(body: &str, styles: Option<&str>) -> Vec<u8> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{body}</w:body></w:document>"#
    );
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        z.start_file("word/document.xml", o).unwrap();
        z.write_all(document.as_bytes()).unwrap();
        z.start_file("word/_rels/document.xml.rels", o).unwrap();
        z.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdImg" Type="{REL}/image" Target="media/image1.png"/></Relationships>"#
            )
            .as_bytes(),
        )
        .unwrap();
        z.start_file("word/media/image1.png", o).unwrap();
        z.write_all(PNG).unwrap();
        if let Some(styles) = styles {
            z.start_file("word/styles.xml", o).unwrap();
            z.write_all(
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {NS}>{styles}</w:styles>"#
                )
                .as_bytes(),
            )
            .unwrap();
        }
        z.finish().unwrap();
    }
    cur.into_inner()
}

fn read(bytes: &[u8]) -> (Document, Vec<WarningCode>) {
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(bytes, &mut w).expect("read docx");
    (d, w.into_vec().iter().map(|x| x.code).collect())
}

/// Gövde paragraflarının metni: sekme `\t`, satır sonu `\n`, resim `[R]`.
fn paragraphs(d: &Document) -> Vec<String> {
    d.sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(
                p.runs
                    .iter()
                    .map(|r| match r {
                        Run::Text { text, .. } => text.clone(),
                        Run::Tab { .. } => "\t".into(),
                        Run::LineBreak { .. } => "\n".into(),
                        Run::Image { .. } => "[R]".into(),
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

fn udf_text(docx: &[u8]) -> String {
    let mut w = WarningSink::new();
    let udf = convert_bytes(docx, Direction::DocxToUdf, &mut w).expect("convert");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf)).unwrap();
    let mut s = String::new();
    z.by_name("content.xml")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    let start = s.find("<![CDATA[").expect("content") + 9;
    let end = s[start..].find("]]>").unwrap() + start;
    s[start..end].to_string()
}

fn p(inner: &str) -> String {
    format!("<w:p>{inner}</w:p>")
}

fn r(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

// ------------------------------------------------------------ yalnız w:t

#[test]
fn drawing_positions_and_field_data_are_not_document_text() {
    // Gerçek belgede bir çizgi şekli "-444583820" gibi rakamlar olarak çıktı.
    let anchor = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="1" behindDoc="1" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>3810000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:align>top</wp:align></wp:positionV><wp:extent cx="952500" cy="952500"/><wp:docPr id="2" name="Resim 2"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="x.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImg"/></pic:blipFill><pic:spPr/></pic:pic></a:graphicData></a:graphic><wp14:sizeRelV relativeFrom="margin"><wp14:pctHeight>0</wp14:pctHeight></wp14:sizeRelV></wp:anchor></w:drawing></w:r>"#;
    let field_data = r#"<w:r><w:fldChar w:fldCharType="begin"><w:fldData xml:space="preserve">CNDJ6nn5us4RjIIAqgBLqQsCAAAACAAAAA4AAABfAFIAZQBmADEAMgAzAAAA</w:fldData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> REF _Ref123 </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Madde 4</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
    let body = format!(
        "{}{}",
        p(&format!("{}{anchor}", r("İmza"))),
        p(&format!("{}{field_data}", r("Bkz. ")))
    );
    let docx = package(&body, None);
    assert_eq!(
        paragraphs(&read(&docx).0),
        vec!["İmza[R]".to_string(), "Bkz. Madde 4".to_string()]
    );
    let text = udf_text(&docx);
    assert!(!text.contains("3810000") && !text.contains("top"), "{text}");
}

#[test]
fn equation_text_is_not_published_as_plain_text() {
    let body = p(&format!(
        r#"{}<m:oMath><m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath>"#,
        r("Formül: ")
    ));
    let (d, w) = read(&package(&body, None));
    assert_eq!(paragraphs(&d), vec!["Formül: ".to_string()]);
    assert!(w.contains(&WarningCode::EquationDropped));
}

#[test]
fn a_run_of_only_a_no_break_space_is_kept() {
    // Ctrl+Shift+Boşluk ile yazılan U+00A0 XML boşluğu değildir; Word onu
    // xml:space olmadan yazar.
    let body = p("<w:r><w:t>Madde</w:t></w:r><w:r><w:t>\u{a0}</w:t></w:r><w:r><w:t>5</w:t></w:r>");
    let docx = package(&body, None);
    assert_eq!(paragraphs(&read(&docx).0), vec!["Madde\u{a0}5".to_string()]);
    assert!(udf_text(&docx).contains("Madde\u{a0}5"));
}

// ------------------------------------------------------------ metin kutusu

const BOX_CHOICE: &str = r#"<mc:Choice Requires="wps"><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="2" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>1905000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>95250</wp:posOffset></wp:positionV><wp:extent cx="952500" cy="476250"/><wp:docPr id="1" name="Metin Kutusu 1"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:txbx><w:txbxContent><w:p><w:r><w:t>KUTU</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice>"#;
const BOX_FALLBACK: &str = r#"<mc:Fallback><w:pict><v:shape id="Kutu 1" type="_x0000_t202" style="position:absolute"><v:textbox><w:txbxContent><w:p><w:r><w:t>KUTU</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></mc:Fallback>"#;

#[test]
fn a_text_box_appears_once_and_keeps_the_paragraph_around_it() {
    let body = format!(
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}<w:r><mc:AlternateContent>{BOX_CHOICE}{BOX_FALLBACK}</mc:AlternateContent></w:r>{}</w:p>{}"#,
        r("ÖNCE "),
        r(" SONRA"),
        p(&r("SONRAKİ"))
    );
    let (d, w) = read(&package(&body, None));
    assert_eq!(
        paragraphs(&d),
        vec![
            "ÖNCE  SONRA".to_string(),
            "KUTU".to_string(),
            "SONRAKİ".to_string()
        ]
    );
    // Dıştaki paragrafın biçimi korunur.
    let Block::Paragraph(host) = &d.sections[0].blocks[0] else {
        panic!("paragraf değil")
    };
    assert_eq!(host.props.alignment, Alignment::Center);
    assert!(w.contains(&WarningCode::TextboxFlattened));
}

#[test]
fn a_picture_with_a_fallback_copy_appears_once() {
    let body = p(
        r#"<w:r><mc:AlternateContent><mc:Choice Requires="wpg"><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="952500" cy="952500"/><wp:docPr id="3" name="Resim 3"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="x.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImg"/></pic:blipFill><pic:spPr/></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:shape style="width:75pt;height:75pt"><v:imagedata r:id="rIdImg" o:title=""/></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r>"#,
    );
    assert_eq!(
        paragraphs(&read(&package(&body, None)).0),
        vec!["[R]".to_string()]
    );
}

#[test]
fn a_drawn_line_is_reported_not_turned_into_digits() {
    let body = p(
        r#"<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="3" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>-44458</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>3820</wp:posOffset></wp:positionV><wp:extent cx="5760000" cy="0"/><wp:docPr id="4" name="Düz Bağlayıcı 4"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr/><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice><mc:Fallback><w:pict><v:line from="0,0" to="453pt,0"/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"#,
    );
    let (d, w) = read(&package(&body, None));
    assert_eq!(paragraphs(&d), vec![String::new()]);
    assert!(w.contains(&WarningCode::ShapeDropped), "{w:?}");
}

// ------------------------------------------------------------ özel karakterler

#[test]
fn a_no_break_hyphen_keeps_the_number_as_it_was() {
    let body = p(r#"<w:r><w:t>2024</w:t><w:noBreakHyphen/><w:t>129 E.</w:t></w:r>"#);
    let docx = package(&body, None);
    assert_eq!(paragraphs(&read(&docx).0), vec!["2024-129 E.".to_string()]);
    assert!(udf_text(&docx).contains("2024-129 E."));
}

#[test]
fn positional_tabs_and_carriage_returns_keep_words_apart() {
    let body = p(
        r#"<w:r><w:t>Sol</w:t><w:ptab w:relativeTo="margin" w:alignment="right" w:leader="none"/><w:t>Sağ</w:t><w:cr/><w:t>Alt</w:t></w:r>"#,
    );
    assert_eq!(
        paragraphs(&read(&package(&body, None)).0),
        vec!["Sol\tSağ\nAlt".to_string()]
    );
}

#[test]
fn a_symbol_font_character_is_kept_with_its_font() {
    let body = p(&format!(
        r#"<w:r><w:sym w:font="Wingdings" w:char="F0FE"/></w:r>{}"#,
        r(" Kabul")
    ));
    let (d, _) = read(&package(&body, None));
    let Block::Paragraph(para) = &d.sections[0].blocks[0] else {
        panic!()
    };
    let Run::Text { text, props } = &para.runs[0] else {
        panic!()
    };
    assert_eq!(text, "\u{F0FE}");
    assert_eq!(props.font_family.as_deref(), Some("Wingdings"));
}

// ------------------------------------------------------------ alanlar

#[test]
fn a_nested_field_shows_only_what_word_shows() {
    // { IF { MERGEFIELD Cinsiyet } = "E" "Bay" "Bayan" } → Word "Bay" gösterir.
    let body = p(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> IF </w:instrText></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> MERGEFIELD Cinsiyet </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>E</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:instrText xml:space="preserve"> = "E" "Bay" "Bayan" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Bay</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
    );
    assert_eq!(
        paragraphs(&read(&package(&body, None)).0),
        vec!["Bay".to_string()]
    );
}

#[test]
fn an_unclosed_field_code_does_not_swallow_the_next_paragraph() {
    let body = format!(
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> XE "dizin" </w:instrText></w:r></w:p>{}"#,
        p(&r("Sonraki paragraf."))
    );
    assert_eq!(
        paragraphs(&read(&package(&body, None)).0),
        vec![String::new(), "Sonraki paragraf.".to_string()]
    );
}

#[test]
fn a_simple_field_keeps_its_result_and_says_so() {
    let body = p(&format!(
        r#"{}<w:fldSimple w:instr=" MERGEFIELD Ad "><w:r><w:t>Ahmet</w:t></w:r></w:fldSimple>"#,
        r("Ad: ")
    ));
    let (d, w) = read(&package(&body, None));
    assert_eq!(paragraphs(&d), vec!["Ad: Ahmet".to_string()]);
    assert!(w.contains(&WarningCode::FieldFlattened));
}

// ------------------------------------------------------------ biçim

const STYLES: &str = r#"<w:style w:type="character" w:styleId="Vurgu"><w:name w:val="Vurgu"/><w:rPr><w:b/></w:rPr></w:style><w:style w:type="character" w:styleId="Gizli"><w:name w:val="Gizli"/><w:rPr><w:vanish/></w:rPr></w:style>"#;

#[test]
fn a_character_style_formats_its_text() {
    let body = p(r#"<w:r><w:rPr><w:rStyle w:val="Vurgu"/></w:rPr><w:t>DAVACI</w:t></w:r>"#);
    let (d, _) = read(&package(&body, Some(STYLES)));
    let Block::Paragraph(para) = &d.sections[0].blocks[0] else {
        panic!()
    };
    let Run::Text { props, .. } = &para.runs[0] else {
        panic!()
    };
    assert!(props.bold, "karakter stilindeki kalın kayboldu");
}

#[test]
fn hidden_text_is_not_published_and_is_reported() {
    let body = p(&format!(
        r#"{}<w:r><w:rPr><w:vanish/></w:rPr><w:t>İÇ NOT</w:t></w:r><w:r><w:rPr><w:rStyle w:val="Gizli"/></w:rPr><w:t>STİL NOTU</w:t></w:r>{}"#,
        r("Görünür "),
        r("metin.")
    ));
    let docx = package(&body, Some(STYLES));
    let (d, w) = read(&docx);
    assert_eq!(paragraphs(&d), vec!["Görünür metin.".to_string()]);
    assert!(w.contains(&WarningCode::RunFeatureDropped));
    assert!(!udf_text(&docx).contains("NOT"));
}

#[test]
fn text_shown_in_capitals_is_written_in_turkish_capitals() {
    let body = p(r#"<w:r><w:rPr><w:caps/></w:rPr><w:t>itiraz dilekçesi ılık</w:t></w:r>"#);
    assert_eq!(
        paragraphs(&read(&package(&body, None)).0),
        vec!["İTİRAZ DİLEKÇESİ ILIK".to_string()]
    );
}

#[test]
fn content_control_placeholder_formatting_does_not_leak_into_its_text() {
    let body = r#"<w:p><w:sdt><w:sdtPr><w:rPr><w:b/><w:i/></w:rPr><w:id w:val="3"/></w:sdtPr><w:sdtContent><w:r><w:t>DÜZ</w:t></w:r></w:sdtContent></w:sdt></w:p>"#;
    let (d, _) = read(&package(body, None));
    let Block::Paragraph(para) = &d.sections[0].blocks[0] else {
        panic!()
    };
    let Run::Text { props, .. } = &para.runs[0] else {
        panic!()
    };
    assert!(!props.bold && !props.italic);
}
