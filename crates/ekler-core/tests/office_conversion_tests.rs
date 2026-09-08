use ekler_core::*;
use std::io::Write;
use tavzih_core::{model::*, warnings::WarningSink};
fn fixture() -> Document {
    let mut heading = Paragraph::plain("Türkçe başlık: İı Şş Ğğ Üü Öö Çç");
    heading.props.style_name = Some("Heading 1".into());
    let mut styled = Paragraph::plain("BOLD ITALIC test");
    if let Run::Text { props, .. } = &mut styled.runs[0] {
        props.bold = true;
        props.italic = true;
    }
    let mut png = std::io::Cursor::new(Vec::new());
    ::image::DynamicImage::ImageRgb8(::image::RgbImage::from_pixel(
        20,
        20,
        ::image::Rgb([190, 20, 40]),
    ))
    .write_to(&mut png, ::image::ImageFormat::Png)
    .unwrap();
    let pic = Paragraph {
        runs: vec![Run::Image(Image {
            data: std::sync::Arc::new(png.into_inner()),
            format: ImageFormat::Png,
            width_pt: 60.,
            height_pt: 60.,
        })],
        ..Default::default()
    };
    let table = Table {
        column_widths: vec![100., 100.],
        rows: vec![
            TableRow {
                cells: vec![TableCell {
                    blocks: vec![Block::Paragraph(Paragraph::plain("MERGED CELL"))],
                    grid_span: 2,
                    ..Default::default()
                }],
                ..Default::default()
            },
            TableRow {
                cells: vec![
                    TableCell {
                        blocks: vec![Block::Paragraph(Paragraph::plain("LEFT"))],
                        grid_span: 1,
                        ..Default::default()
                    },
                    TableCell {
                        blocks: vec![Block::Paragraph(Paragraph::plain("RIGHT"))],
                        grid_span: 1,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    Document {
        sections: vec![Section {
            header: Some(HeaderFooter {
                blocks: vec![Block::Paragraph(Paragraph::plain("HEADER"))],
                ..Default::default()
            }),
            footer: Some(HeaderFooter {
                blocks: vec![Block::Paragraph(Paragraph::plain("FOOTER"))],
                ..Default::default()
            }),
            blocks: vec![
                Block::Paragraph(heading),
                Block::Paragraph(styled),
                Block::Table(table),
                Block::Paragraph(pic),
                Block::PageBreak,
                Block::Paragraph(Paragraph::plain("SECOND PAGE — Türkçe içerik")),
            ],
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn udf(signed: bool) -> Vec<u8> {
    let mut warn = WarningSink::new();
    let mut model = fixture();
    model.sections[0]
        .blocks
        .retain(|b| !matches!(b, Block::PageBreak));
    for i in 0..65 {
        model.sections[0]
            .blocks
            .push(Block::Paragraph(Paragraph::plain(&format!(
                "Paragraf {i}: Türkçe çok sayfalı düzen doğrulaması."
            ))));
    }
    let xml = tavzih_core::udf::writer::write_content_xml(&model, &mut warn).unwrap();
    assert!(!warn.into_vec().iter().any(|w| w.data_loss));
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("content.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&xml).unwrap();
    if signed {
        zip.start_file("sign.sgn", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"SYNTHETIC SIGNATURE MARKER - NOT CRYPTOGRAPHIC")
            .unwrap();
    }
    zip.finish().unwrap().into_inner()
}
#[test]
fn markdown_package_is_atomic_preserves_table_assets_and_requires_approval() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("İmzalı örnek.udf");
    std::fs::write(&src, udf(true)).unwrap();
    let hash = calculate_sha256(&src).unwrap();
    assert!(convert_udf_to_markdown(&src, d.path(), false).is_err());
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    let result = convert_udf_to_markdown(&src, d.path(), true).unwrap();
    assert!(result.markdown.contains("MERGED CELL"));
    assert!(result.markdown.contains("colspan=\"2\""));
    assert!(result.markdown.contains("HEADER"));
    assert!(result.markdown.contains("SECOND PAGE"));
    assert!(result.package_dir.join("image-001.png").exists());
    assert!(result.package_dir.join("belge.md").exists());
    assert_eq!(hash, calculate_sha256(&src).unwrap());
    let again = convert_udf_to_markdown(&src, d.path(), true).unwrap();
    assert_ne!(result.package_dir, again.package_dir);
}
#[test]
fn signed_udf_pdf_requires_backend_approval() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("signed.udf");
    std::fs::write(&src, udf(true)).unwrap();
    assert!(matches!(
        office::convert_to_pdf(&src, false),
        Err(EklerError::UnapprovedSignedUdf(_))
    ));
}
#[test]
#[ignore = "Requires installed local LibreOffice renderer; explicit integration gate"]
fn docx_and_udf_real_renderer_corpus() {
    assert!(office::find_renderer().is_some(), "LibreOffice required");
    let dir = std::env::var_os("DUZENEK_CORPUS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("duzenek-office-corpus"));
    std::fs::create_dir_all(&dir).unwrap();
    let mut warn = WarningSink::new();
    let docx = tavzih_core::docx::writer::write_docx(&fixture(), &mut warn).unwrap();
    for (name, bytes) in [
        ("corpus.docx", docx),
        ("corpus.udf", udf(false)),
        ("signed.udf", udf(true)),
    ] {
        let src = dir.join(name);
        std::fs::write(&src, bytes).unwrap();
        let hash = calculate_sha256(&src).unwrap();
        let mut pdf = office::convert_to_pdf(&src, true).unwrap();
        if name.ends_with("docx") {
            assert_eq!(pdf.get_pages().len(), 2, "{name}");
        } else {
            assert!(pdf.get_pages().len() >= 2, "{name}");
        }
        let pdf_path = dir.join(format!("{name}.pdf"));
        pdf.save(&pdf_path).unwrap();
        let output = std::process::Command::new(
            std::env::var("DUZENEK_PDFTOTEXT").unwrap_or("pdftotext".into()),
        )
        .arg(&pdf_path)
        .arg("-")
        .output()
        .expect("Independent pdftotext required for Unicode font extraction");
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        for marker in ["MERGED CELL", "HEADER", "FOOTER", "SECOND PAGE"] {
            assert!(text.contains(marker), "{name} missing {marker}: {text}");
        }
        pdf.save(dir.join(format!("{name}.pdf"))).unwrap();
        assert_eq!(hash, calculate_sha256(&src).unwrap());
    }
}

#[test]
#[ignore = "Requires local LibreOffice; binary DOC round-trip integration gate"]
fn binary_doc_is_rendered_with_text_and_pages() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("legacy.docx");
    let mut warn = WarningSink::new();
    std::fs::write(
        &src,
        tavzih_core::docx::writer::write_docx(&fixture(), &mut warn).unwrap(),
    )
    .unwrap();
    let renderer = office::find_renderer().expect("LibreOffice required");
    let profile = url::Url::from_directory_path(dir.path().join("profile")).unwrap();
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("/usr/bin/sandbox-exec");
        c.args(["-p", "(version 1)(allow default)(deny network*)"])
            .arg(renderer);
        c
    };
    #[cfg(not(target_os = "macos"))]
    let mut cmd = std::process::Command::new(renderer);
    let status = cmd
        .arg(format!("-env:UserInstallation={profile}"))
        .args(["--headless", "--convert-to", "doc:MS Word 97", "--outdir"])
        .arg(dir.path())
        .arg(&src)
        .output()
        .unwrap();
    assert!(status.status.success());
    let doc = dir.path().join("legacy.doc");
    let before = calculate_sha256(&doc).unwrap();
    let pdf = office::convert_to_pdf(&doc, false).unwrap();
    assert_eq!(pdf.get_pages().len(), 2);
    assert_eq!(before, calculate_sha256(&doc).unwrap());
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "Requires native macOS codec services outside a restricted process sandbox"]
fn native_heic_import_enters_the_main_scanner() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("source.png");
    ::image::RgbImage::from_pixel(64, 48, ::image::Rgb([200, 30, 60]))
        .save(&png)
        .unwrap();
    let heic = dir.path().join("Türkçe.heic");
    let status = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "heic"])
        .arg(&png)
        .arg("--out")
        .arg(&heic)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "Native HEIC encoder required on macOS test host"
    );
    let before = calculate_sha256(&heic).unwrap();
    let scan = scan_source_files(&[&heic]);
    assert!(scan.errors.is_empty(), "{:?}", scan.errors);
    assert_eq!(scan.sources[0].page_count, 1);
    assert_eq!(scan.sources[0].format, SourceFormat::Heic);
    assert_eq!(before, calculate_sha256(&heic).unwrap());
}
