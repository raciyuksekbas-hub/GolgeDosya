#![cfg(target_os = "macos")]
use ekler_core::{calculate_sha256, raster};
use lopdf::{dictionary, Document, Stream};
use std::path::PathBuf;
fn check(format: &str) {
    let dir = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("DUZENEK_WORKSPACE_ARTIFACTS").map(PathBuf::from);
    for (name, w, h, dpi) in [
        ("portrait", 400, 600, 150),
        ("landscape", 600, 400, 150),
        ("small", 120, 90, 150),
        ("high-resolution", 600, 840, 300),
        ("large-page-low-dpi", 1200, 1680, 72),
    ] {
        let mut d = Document::with_version("1.5");
        let pages = d.new_object_id();
        // A full-bleed colored rectangle makes content/gutter overlap measurable.
        let content = d.add_object(Stream::new(
            dictionary! {},
            format!("0.2 0.5 0.7 rg 0 0 {w} {h} re f").into_bytes(),
        ));
        let page=d.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),w.into(),h.into()],"Resources"=>dictionary!{},"Contents"=>content});
        d.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
        );
        let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        d.trailer.set("Root", root);
        let src = dir.path().join(format!("{name}.pdf"));
        d.save(&src).unwrap();
        let hash = calculate_sha256(&src).unwrap();
        let folder = raster::pdf_to_images(&src, dir.path(), format, dpi, false).unwrap();
        let output = folder.join(format!("sayfa-0001.{format}"));
        let image = image::open(&output).unwrap().to_rgb8();
        let original = image::load_from_memory(&raster::preview_page(&src, 1, dpi, 0).unwrap())
            .unwrap()
            .to_rgb8();
        assert_eq!(image.width(), original.width());
        assert_eq!(
            image.height(),
            ((h + 34) as f64 * dpi as f64 / 72.).ceil() as u32
        );
        if let Some(ref dest) = evidence {
            std::fs::create_dir_all(dest).unwrap();
            std::fs::copy(&src, dest.join(format!("{name}.pdf"))).unwrap();
            std::fs::copy(&output, dest.join(format!("{name}.{format}"))).unwrap();
            original
                .save(dest.join(format!("{name}-original.png")))
                .unwrap();
        }
        assert!(
            original.get_pixel(10, 10)[0] < 100,
            "DPI enlarged canvas without scaling page content"
        );
        let mut delta = 0u64;
        for y in 0..original.height().saturating_sub(2) {
            for x in 0..original.width() {
                for c in 0..3 {
                    delta += original.get_pixel(x, y)[c].abs_diff(image.get_pixel(x, y)[c]) as u64;
                }
            }
        }
        let mae = delta as f64 / (original.width() * original.height() * 3) as f64;
        assert!(
            mae < if format == "png" { 0.6 } else { 3.0 },
            "Content altered: {mae}"
        );
        let marks: Vec<_> = (original.height() + 8..image.height())
            .flat_map(|y| (0..image.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| image.get_pixel(x, y).0.iter().any(|v| *v < 245))
            .collect();
        assert!(marks.len() > 8, "No visible mark: {name}/{format}");
        assert!(
            marks.iter().all(|(x, _)| *x > image.width() / 2),
            "Mark outside right safe area"
        );
        let span = marks.iter().map(|(x, _)| x).max().unwrap()
            - marks.iter().map(|(x, _)| x).min().unwrap();
        assert!(span <= 245, "Mark too wide: {span}");
        assert_eq!(calculate_sha256(&src).unwrap(), hash);
        println!(
            "{name}/{format}: {} bytes, {}x{}, visible brand pixels={}, content MAE={mae:.4}",
            std::fs::metadata(&output).unwrap().len(),
            image.width(),
            image.height(),
            marks.len()
        );
        if let Some(ref dest) = evidence {
            std::fs::create_dir_all(dest).unwrap();
            std::fs::copy(&src, dest.join(format!("{name}.pdf"))).unwrap();
            std::fs::copy(output, dest.join(format!("{name}.{format}"))).unwrap();
        }
    }
}
#[test]
fn pdf_to_png_contains_brand_mark() {
    check("png");
}
#[test]
fn pdf_to_jpeg_contains_brand_mark() {
    check("jpg");
}
