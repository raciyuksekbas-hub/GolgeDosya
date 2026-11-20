//! PDF → görsel sayfa sayfa üretilir.
//!
//! Eskiden bütün sayfalar önce belleğe çiziliyor ve toplam 512 MiB'lık bir
//! bütçe aşılınca paket reddediliyordu: 150 dpi'da 61 A4 sayfadan uzun her PDF
//! "bellek sınırını aşıyor" hatası veriyordu. Artık yalnız sayfa başına sınır
//! (40 MP) vardır; bellekte aynı anda birkaç sayfa bulunur.

#![cfg(any(target_os = "macos", target_os = "windows"))]

use lopdf::{dictionary, Document, Object, Stream};

/// `pages` sayfalı, her sayfası `side`×`side` pt'lik bir PDF.
fn square_pages_pdf(pages: usize, side: i64) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"0 0 0 rg 72 72 144 144 re f".to_vec(),
    ));
    let kids: Vec<Object> = (0..pages)
        .map(|_| {
            doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), side.into(), side.into()],
                "Contents" => content,
            })
            .into()
        })
        .collect();
    doc.objects.insert(
        pages_id,
        dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => pages as i64}.into(),
    );
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
    doc.trailer.set("Root", catalog);
    doc
}

#[test]
fn page_images_beyond_the_old_total_memory_budget_are_produced() {
    // 72 dpi'da 3000 pt = 3000 px: sayfa başına 9 MP, RGBA 36 MB; 16 sayfa
    // 576 MB eder ve eski 512 MiB'lık toplam bütçeyi aşar.
    let pages = 16;
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("uzun.pdf");
    square_pages_pdf(pages, 3000).save(&src).unwrap();
    let out = dir.path().join("cikti");
    std::fs::create_dir(&out).unwrap();

    let folder = ekler_core::raster::pdf_to_images(&src, &out, "jpg", 72, false).unwrap();

    let mut names: Vec<String> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), pages);
    assert_eq!(names.first().unwrap(), "sayfa-0001.jpg");
    assert_eq!(names.last().unwrap(), "sayfa-0016.jpg");
    let (w, h) = image::image_dimensions(folder.join("sayfa-0016.jpg")).unwrap();
    assert_eq!((w, h), (3000, 3000));
    // Hazırlık klasörü yayımlandı; çıktı klasöründe yalnız paket kaldı.
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
}

#[test]
fn a_page_over_the_per_page_limit_still_fails_and_publishes_nothing() {
    // 300 dpi'da 2000 pt ≈ 8334 px: 69 MP > 40 MP sayfa sınırı.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("dev.pdf");
    square_pages_pdf(2, 2000).save(&src).unwrap();
    let out = dir.path().join("cikti");
    std::fs::create_dir(&out).unwrap();

    let e = ekler_core::raster::pdf_to_images(&src, &out, "png", 300, false).unwrap_err();
    assert!(e.to_string().contains("güvenli sınırı"), "{e}");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
}
