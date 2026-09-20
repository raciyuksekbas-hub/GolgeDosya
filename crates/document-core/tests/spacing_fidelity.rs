//! DOCX -> UDF paragraf boşluğu sadakati (saha turu §27–29).
//!
//! SAHA BULGUSU: gerçek 52 sayfalık sözleşme UDF'ye çevrildiğinde metin dikey
//! olarak açıldı; bentler arası boşluk ve sayfa akışı değişti.
//!
//! Kök neden: `<w:contextualSpacing/>` yok sayılıyordu. Word bu işareti taşıyan
//! bir paragraf ile AYNI STİLDEKİ komşusu arasındaki boşluğu ÇİZMEZ; UDF'te
//! böyle bir kural yoktur, dolayısıyla karar dönüştürücüde verilip sonuç
//! yazılmalıdır. Yazılmayınca boşluk her paragrafta gerçekten oluştu. Gerçek
//! sözleşmede bu işareti taşıyan 165 paragraf vardı.
//!
//! Fixture SENTETİKTİR (§3): gerçek belge repoya girmez.
use std::io::Write;
use tavzih_core::model::Block;
use tavzih_core::warnings::WarningSink;
use tavzih_core::{docx, udf};

/// Verilen gövde XML'inden geçerli, asgari bir DOCX paketi kurar.
fn docx(body: &str) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
        let opts: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut put = |name: &str, data: &str| {
            z.start_file(name, opts).unwrap();
            z.write_all(data.as_bytes()).unwrap();
        };
        put(
            "[Content_Types].xml",
            r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        );
        put(
            "_rels/.rels",
            r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        );
        put(
            "word/document.xml",
            &format!(
                r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
            ),
        );
        z.finish().unwrap();
    }
    out
}

/// `w:pStyle` ve `w:spacing` taşıyan, istenirse contextualSpacing'li paragraf.
fn para(style: &str, after: u32, contextual: bool, text: &str) -> String {
    let ctx = if contextual {
        "<w:contextualSpacing/>"
    } else {
        ""
    };
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{style}"/><w:spacing w:after="{after}"/>{ctx}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
    )
}

fn paragraphs(bytes: &[u8]) -> Vec<tavzih_core::model::Paragraph> {
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(bytes, &mut w).expect("docx okunmalı");
    d.sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn contextual_spacing_is_suppressed_between_same_style_paragraphs() {
    // Word bu üç paragraf arasına boşluk ÇİZMEZ.
    let body = format!(
        "{}{}{}",
        para("ListParagraph", 160, true, "a"),
        para("ListParagraph", 160, true, "b"),
        para("ListParagraph", 160, true, "c"),
    );
    let ps = paragraphs(&docx(&body));
    assert_eq!(ps.len(), 3);
    assert_eq!(
        ps[0].props.space_after_pt, 0.0,
        "1-2 arası boşluk çizilmemeli"
    );
    assert_eq!(ps[1].props.space_before_pt, 0.0);
    assert_eq!(
        ps[1].props.space_after_pt, 0.0,
        "2-3 arası boşluk çizilmemeli"
    );
    // SON paragrafın after'ı korunur: ardından aynı stilde komşu yok.
    assert_eq!(ps[2].props.space_after_pt, 8.0);
}

#[test]
fn spacing_survives_between_different_styles() {
    // Kural YALNIZ aynı stil içindir; stil değişince boşluk çizilir.
    let body = format!(
        "{}{}",
        para("ListParagraph", 160, true, "liste"),
        para("Normal", 160, true, "gövde"),
    );
    let ps = paragraphs(&docx(&body));
    assert_eq!(
        ps[0].props.space_after_pt, 8.0,
        "farklı stile geçişte boşluk kalmalı"
    );
}

#[test]
fn paragraphs_without_the_mark_keep_their_spacing() {
    let body = format!(
        "{}{}",
        para("Normal", 160, false, "bir"),
        para("Normal", 160, false, "iki"),
    );
    let ps = paragraphs(&docx(&body));
    assert_eq!(ps[0].props.space_after_pt, 8.0);
    assert_eq!(ps[1].props.space_after_pt, 8.0);
}

#[test]
fn the_suppression_reaches_the_udf_output() {
    // Modelde doğru olması yetmez: UDF'ye yazılan şey ölçülür.
    let body = format!(
        "{}{}",
        para("ListParagraph", 160, true, "a"),
        para("ListParagraph", 160, true, "b"),
    );
    let mut w = WarningSink::new();
    let d = docx::reader::read_docx(&docx(&body), &mut w).expect("docx");
    let xml =
        String::from_utf8(udf::writer::write_content_xml(&d, &mut w).expect("udf")).expect("utf8");
    // İlk paragrafta SpaceBelow yazılmamalı; ikincide (son) yazılmalı.
    let count = xml.matches("SpaceBelow=").count();
    assert_eq!(count, 1, "bastırılan boşluk UDF'ye yazılmış: {xml}");
}

#[test]
fn twips_conversion_is_unchanged() {
    // 160 twip = 8 pt. Dönüşüm bozulmadı.
    let ps = paragraphs(&docx(&para("Normal", 240, false, "x")));
    assert_eq!(ps[0].props.space_after_pt, 12.0);
}

/// Revizyon KAYDI taşıyan paragraf: güncel aralık 12 pt, reddedilmiş olan 0 pt.
fn para_with_revision() -> String {
    concat!(
        r#"<w:p><w:pPr><w:pStyle w:val="Normal"/><w:spacing w:after="240"/>"#,
        // Revizyondan ÖNCEKİ hâl — belgenin güncel görünümü DEĞİL.
        r#"<w:pPrChange w:id="1" w:author="x" w:date="2026-01-01T00:00:00Z">"#,
        r#"<w:pPr><w:pStyle w:val="ListParagraph"/><w:spacing w:after="0"/></w:pPr>"#,
        r#"</w:pPrChange>"#,
        r#"</w:pPr><w:r><w:t>metin</w:t></w:r></w:p>"#,
    )
    .to_string()
}

#[test]
fn a_revision_record_does_not_overwrite_the_current_formatting() {
    // ÇEKİŞMELİ İNCELEME: `w:pPrChange` içindeki eski `w:pPr` tam bir
    // paragraf özelliği bloğudur ve okuyucu onu `w:pPr` içinde sayıyordu.
    // "Son yazan kazanır" olduğu için REDDEDİLMİŞ değerler güncelin üstüne
    // yazılıyordu: paragraf eski aralığıyla ve eski stiliyle dönüşüyordu.
    let ps = paragraphs(&docx(&para_with_revision()));
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].props.space_after_pt, 12.0, "güncel aralık korunmalı");
    assert_eq!(
        ps[0].props.style_name.as_deref(),
        Some("Normal"),
        "güncel stil korunmalı; revizyon kaydındaki stil değil"
    );
}

#[test]
fn revision_records_do_not_leak_into_the_contextual_spacing_decision() {
    // Stil adı `apply_contextual_spacing` kuralının ANAHTARIDIR: revizyon
    // kaydından sızan yanlış bir stil, boşluk kararını da bozardı.
    let body = format!("{}{}", para_with_revision(), para_with_revision());
    let ps = paragraphs(&docx(&body));
    // İkisi de "Normal" ve contextualSpacing YOK -> boşluk korunur.
    assert_eq!(ps[0].props.space_after_pt, 12.0);
    assert_eq!(ps[1].props.space_after_pt, 12.0);
}

#[test]
fn text_inside_a_revision_record_is_not_emitted_as_content() {
    // Kayıt YOK SAYILIR ama paragrafın kendi metni kalır.
    let ps = paragraphs(&docx(&para_with_revision()));
    assert_eq!(ps[0].text(), "metin");
}
