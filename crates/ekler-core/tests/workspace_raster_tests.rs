#![cfg(any(target_os = "macos", target_os = "windows"))]
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
        // A full-bleed colored rectangle: the overlaid mark must be visible on
        // it and must not change any pixel outside its corner.
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
        // The export is exactly the page: the mark is overlaid in the visual
        // bottom-right corner (it used to add a 34 pt strip below the page).
        assert_eq!(image.dimensions(), original.dimensions());
        assert_eq!(image.height(), (h as f64 * dpi as f64 / 72.).ceil() as u32);
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
        // Raster mark size: 9.4 % of the page width in pixels, 32..240 px,
        // inset 12/56 of its width, height 72/290 of its width.
        let (iw, ih) = image.dimensions();
        let mw = (f64::from(iw) * 0.094).clamp(32., 240.);
        let (inset, mh) = (mw * 12. / 56., mw * 72. / 290.);
        let cx = (f64::from(iw) - inset - mw - 6.).max(0.) as u32;
        let cy = (f64::from(ih) - inset - mh - 6.).max(0.) as u32;
        let corner = |x: u32, y: u32| x >= cx && y >= cy;
        let (mut delta, mut n) = (0u64, 0u64);
        for y in 0..ih.saturating_sub(2) {
            for x in 0..iw {
                if corner(x, y) {
                    continue;
                }
                for c in 0..3 {
                    delta += original.get_pixel(x, y)[c].abs_diff(image.get_pixel(x, y)[c]) as u64;
                }
                n += 3;
            }
        }
        let mae = delta as f64 / n as f64;
        assert!(
            mae < if format == "png" { 0.6 } else { 3.0 },
            "Content altered outside the mark corner: {mae}"
        );
        let differs = |x: u32, y: u32| {
            let (a, b) = (original.get_pixel(x, y), image.get_pixel(x, y));
            (0..3).map(|c| u32::from(a[c].abs_diff(b[c]))).sum::<u32>() > 30
        };
        let marks: Vec<_> = (cy..ih)
            .flat_map(|y| (cx..iw).map(move |x| (x, y)))
            .filter(|&(x, y)| differs(x, y))
            .collect();
        assert!(marks.len() > 8, "No visible mark: {name}/{format}");
        assert!(
            marks.iter().all(|(x, _)| *x > iw / 2),
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
