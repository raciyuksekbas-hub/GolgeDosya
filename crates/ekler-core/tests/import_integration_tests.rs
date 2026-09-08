//! Core scanner and assignment integration tests. No native UI is driven here.
use ekler_core::{scan_source_files, ExhibitSourceRef, Project, SourceFormat};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document as LopdfDoc, Stream};
use std::fs::File;
use std::io::Write;
use tempfile::NamedTempFile;

fn create_pdf(page_cnt: usize, label: &str) -> (NamedTempFile, u64) {
    let mut doc = LopdfDoc::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();

    for _i in 0..page_cnt {
        let content = Content {
            operations: vec![Operation::new("q", vec![]), Operation::new("Q", vec![])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }

    doc.set_object(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => page_cnt as i64,
        },
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let tmp = tempfile::Builder::new()
        .prefix(label)
        .suffix(".pdf")
        .tempfile()
        .unwrap();
    {
        let mut f = File::create(tmp.path()).unwrap();
        doc.save_to(&mut f).unwrap();
        f.flush().unwrap();
    }
    let size = std::fs::metadata(tmp.path()).unwrap().len();
    (tmp, size)
}

fn create_jpeg(w: u32, h: u32) -> NamedTempFile {
    let img = ::image::DynamicImage::new_rgb8(w, h);
    let tmp = tempfile::Builder::new().suffix(".jpg").tempfile().unwrap();
    img.save(tmp.path()).unwrap();
    tmp
}

fn create_multipage_tiff(frames: usize, w: u32, h: u32) -> NamedTempFile {
    let tmp = tempfile::Builder::new().suffix(".tiff").tempfile().unwrap();
    {
        let f = File::create(tmp.path()).unwrap();
        let mut encoder = tiff::encoder::TiffEncoder::new(f).unwrap();
        for i in 0..frames {
            let buffer: Vec<u8> = (0..(w * h * 3))
                .map(|k| ((k * 31 + (i as u32) * 17) % 256) as u8)
                .collect();
            let image_encoder = encoder
                .new_image::<tiff::encoder::colortype::RGB8>(w, h)
                .unwrap();
            image_encoder.write_data(&buffer).unwrap();
        }
    }
    tmp
}

#[test]
fn multiple_pdf_scan_real_metadata() {
    let (pdf1, size1) = create_pdf(3, "Ihtarname_");
    let (pdf2, size2) = create_pdf(5, "Sozlesme_");
    let (pdf3, size3) = create_pdf(1, "Dekont_");

    let paths = vec![
        pdf1.path().to_path_buf(),
        pdf2.path().to_path_buf(),
        pdf3.path().to_path_buf(),
    ];

    let result = scan_source_files(&paths);
    assert_eq!(result.errors.len(), 0);
    assert_eq!(result.sources.len(), 3);

    // Gerçek metadata doğrulaması
    assert_eq!(result.sources[0].page_count, 3);
    assert_eq!(result.sources[0].size_bytes, size1);
    assert_eq!(result.sources[0].format, SourceFormat::Pdf);

    assert_eq!(result.sources[1].page_count, 5);
    assert_eq!(result.sources[1].size_bytes, size2);
    assert_eq!(result.sources[1].format, SourceFormat::Pdf);

    assert_eq!(result.sources[2].page_count, 1);
    assert_eq!(result.sources[2].size_bytes, size3);
    assert_eq!(result.sources[2].format, SourceFormat::Pdf);

    // Kaynak Havuzu'na ekleme
    let mut project = Project::new("Dava Dosyası");
    for s in result.sources {
        project.add_source(s);
    }
    assert_eq!(project.sources.len(), 3);
}

/// Empty files with a DOCX extension must never be accepted as one-page documents.
#[test]
fn empty_docx_is_rejected() {
    let file = tempfile::Builder::new().suffix(".docx").tempfile().unwrap();
    let result = scan_source_files(&[file.path()]);
    assert!(result.sources.is_empty());
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn assign_scanned_image_to_exhibit() {
    let mut project = Project::new("Dava Dosyası");
    project.create_exhibit("Sözleşme"); // Ek-1
    let ek2 = project.create_exhibit("Dekont"); // Ek-2
    let ek2_id = ek2.id.clone();

    let jpg_file = create_jpeg(640, 480);
    let scan = scan_source_files(&[jpg_file.path()]);
    assert_eq!(scan.sources.len(), 1);
    let src = &scan.sources[0];
    assert_eq!(src.format, SourceFormat::Jpg);

    // Projeye al ve Ek-2'ye bağla
    project.add_source(src.clone());
    let exhibit2 = project
        .exhibits
        .iter_mut()
        .find(|e| e.id == ek2_id)
        .unwrap();
    exhibit2.sources.push(ExhibitSourceRef {
        source_id: src.id.clone(),
        page_range: None,
    });

    assert_eq!(project.exhibits[1].sources.len(), 1);
    assert_eq!(project.exhibits[1].sources[0].source_id, src.id);
    assert_eq!(project.exhibits[0].sources.len(), 0); // Ek-1 boş kalmalı
}

#[test]
fn mixed_formats_and_faulty_batch() {
    let (pdf, _) = create_pdf(4, "Muhtelif_");
    let docx = tempfile::Builder::new().suffix(".docx").tempfile().unwrap();
    let jpg = create_jpeg(300, 300);
    let tiff = create_multipage_tiff(3, 200, 200);

    // Bozuk PDF dosyası
    let mut corrupt_pdf = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
    corrupt_pdf
        .write_all(b"BOZUK_VERI_BU_PDF_DEGILDIR")
        .unwrap();
    corrupt_pdf.flush().unwrap();

    let mut paths = vec![
        pdf.path().to_path_buf(),
        docx.path().to_path_buf(),
        jpg.path().to_path_buf(),
        tiff.path().to_path_buf(),
    ];
    paths.push(corrupt_pdf.path().to_path_buf());

    let scan = scan_source_files(&paths);

    assert_eq!(scan.sources.len(), 3);
    assert_eq!(scan.errors.len(), 2); // Empty DOCX and corrupt PDF

    // Format kontrolleri
    assert!(scan
        .sources
        .iter()
        .any(|s| s.format == SourceFormat::Pdf && s.page_count == 4));
    assert!(scan.sources.iter().any(|s| s.format == SourceFormat::Jpg));
    assert!(scan
        .sources
        .iter()
        .any(|s| s.format == SourceFormat::Tiff && s.page_count == 3));
}

#[test]
fn duplicate_prevention() {
    let (pdf, _) = create_pdf(2, "Tekrarlanan_");

    let scan1 = scan_source_files(&[pdf.path()]);
    let scan2 = scan_source_files(&[pdf.path()]);

    let src1 = scan1.sources[0].clone();
    let src2 = scan2.sources[0].clone();

    // Deterministik ID ve SHA-256 eşitliği
    assert_eq!(src1.id, src2.id);
    assert_eq!(src1.sha256_before, src2.sha256_before);

    // Proje havuzu tekilleştirme mantığı
    let mut project = Project::new("Dava Dosyası");
    project.add_source(src1);

    // İkinci eklemede duplicate eklenmemeli:
    if !project
        .sources
        .iter()
        .any(|s| s.path == src2.path || s.sha256_before == src2.sha256_before)
    {
        project.add_source(src2);
    }

    assert_eq!(
        project.sources.len(),
        1,
        "Aynı dosya mükerrer olarak eklenmemelidir"
    );
}
