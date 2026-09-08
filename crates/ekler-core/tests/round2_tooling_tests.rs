use ekler_core::*;
use std::path::{Path, PathBuf};
// Fikstürler crate'in içinde: bu crate birleşik workspace'e taşınırken
// kendi kendine yeter hâle getirildi. Eskiden workspace kökündeki `qa/`
// altındaydı ve bağımsız DüzenEk deposunun varlığına bağlıydı.
fn source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name)
}
fn brand_count(path: &Path) -> usize {
    let doc = lopdf::Document::load(path).unwrap();
    doc.get_pages()
        .values()
        .filter(|id| {
            let p = pdf::resolved_page_dictionary(&doc, **id).unwrap();
            let (_, r) = doc.dereference(p.get(b"Resources").unwrap()).unwrap();
            let (_, x) = doc
                .dereference(r.as_dict().unwrap().get(b"XObject").unwrap())
                .unwrap();
            x.as_dict()
                .unwrap()
                .iter()
                .any(|(n, _)| n.starts_with(b"DuzenEkBrand"))
        })
        .count()
}
#[test]
fn five_tools_publish_branded_copies_without_mutating_sources() {
    let dir = tempfile::tempdir().unwrap();
    let src = source("vector.pdf");
    let hash = calculate_sha256(&src).unwrap();
    let cases = [
        toolbox::ToolOperation::Select { pages: vec![2] },
        toolbox::ToolOperation::Reorder { pages: vec![2, 1] },
        toolbox::ToolOperation::Delete { pages: vec![1] },
        toolbox::ToolOperation::RotateSelected {
            degrees: 90,
            pages: vec![2],
        },
        toolbox::ToolOperation::Merge,
    ];
    for (i, op) in cases.iter().enumerate() {
        let out = dir.path().join(format!("{i}.pdf"));
        let paths = if i == 4 {
            vec![src.clone(), src.clone()]
        } else {
            vec![src.clone()]
        };
        toolbox::run_tool(&paths, op, &out, false).unwrap();
        let d = lopdf::Document::load(&out).unwrap();
        assert_eq!(brand_count(&out), d.get_pages().len());
        assert!(out.metadata().unwrap().len() > 0);
        assert_eq!(calculate_sha256(&src).unwrap(), hash);
        if i == 3 {
            let ids = d.get_pages();
            assert_eq!(
                pdf::resolved_page_dictionary(&d, ids[&2])
                    .unwrap()
                    .get(b"Rotate")
                    .unwrap()
                    .as_i64()
                    .unwrap(),
                90
            );
        }
    }
}
#[test]
fn logo_is_not_added_to_byte_identical_signed_original() {
    let src = source("signed-marker.pdf");
    let scanned = scan_source_files(&[&src]);
    let s = scanned.sources[0].clone();
    let mut p = Project::new("Original");
    p.add_source(s.clone());
    p.create_exhibit("Signed").sources.push(ExhibitSourceRef {
        source_id: s.id,
        page_range: None,
    });
    let dir = tempfile::tempdir().unwrap();
    let plan = prepare_export_plan(&p).unwrap();
    let result = execute_export_plan(
        &plan,
        &ExecutionContext {
            output_dir: dir.path().into(),
        },
    )
    .unwrap();
    assert_eq!(
        std::fs::read(&src).unwrap(),
        std::fs::read(result.package_dir.join(&result.outputs[0].file_name)).unwrap()
    );
}
#[test]
#[cfg(target_os = "macos")]
fn preview_and_brand_gutter_preserve_source_pixels() {
    let dir = tempfile::tempdir().unwrap();
    let src = source("vector.pdf");
    let hash = calculate_sha256(&src).unwrap();
    let raw = raster::preview_page(&src, 1, 72, 0).unwrap();
    let original = ::image::load_from_memory(&raw).unwrap().to_rgb8();
    let mut d = lopdf::Document::load(&src).unwrap();
    pdf::stamp::apply_branding(&mut d).unwrap();
    let out = dir.path().join("branded.pdf");
    d.save(&out).unwrap();
    let bytes = raster::preview_page(&out, 1, 72, 0).unwrap();
    let output = ::image::load_from_memory(&bytes).unwrap().to_rgb8();
    assert_eq!(output.width(), original.width());
    assert_eq!(output.height(), original.height() + 34);
    let mut delta = 0u64;
    for (y, row) in original.rows().enumerate() {
        for (x, p) in row.enumerate() {
            for c in 0..3 {
                delta += p[c].abs_diff(output.get_pixel(x as u32, y as u32)[c]) as u64;
            }
        }
    }
    assert!(delta as f64 / (original.width() * original.height() * 3) as f64 <= 0.5);
    let mark_pixels = (original.height()..output.height())
        .flat_map(|y| (0..output.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| output.get_pixel(x, y).0.iter().any(|v| *v < 245))
        .count();
    assert!(mark_pixels > 30);
    let min_brand = (original.height()..output.height())
        .flat_map(|y| (0..output.width()).map(move |x| (x, y)))
        .flat_map(|(x, y)| output.get_pixel(x, y).0)
        .min()
        .unwrap();
    assert!(
        min_brand >= 180,
        "Brand must remain low contrast: {min_brand}"
    );
    let rotated =
        ::image::load_from_memory(&raster::preview_page(&src, 1, 72, 90).unwrap()).unwrap();
    use ::image::GenericImageView;
    assert_eq!(rotated.dimensions(), (500, 400));
    assert_eq!(calculate_sha256(&src).unwrap(), hash);
    if let Some(path) = std::env::var_os("DUZENEK_ROUND2_ARTIFACTS") {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::copy(out, PathBuf::from(&path).join("branded.pdf")).unwrap();
        std::fs::write(PathBuf::from(path).join("branded.png"), bytes).unwrap();
    }
}

#[test]
fn missing_renderer_selection_does_not_report_success() {
    let dir = tempfile::tempdir().unwrap();
    assert!(office::select_renderer(&dir.path().join("absent.app")).is_err());
}

#[test]
fn unsigned_export_has_brand_even_with_page_stamp_disabled() {
    let src = source("vector.pdf");
    let mut p = Project::new("Brand");
    p.stamp_config.enabled = false;
    let s = scan_source_files(&[&src]).sources.remove(0);
    p.add_source(s.clone());
    p.create_exhibit("Ek").sources.push(ExhibitSourceRef {
        source_id: s.id,
        page_range: None,
    });
    let dir = tempfile::tempdir().unwrap();
    let plan = prepare_export_plan(&p).unwrap();
    let result = execute_export_plan(
        &plan,
        &ExecutionContext {
            output_dir: dir.path().into(),
        },
    )
    .unwrap();
    for output in result.outputs {
        assert_eq!(
            brand_count(&result.package_dir.join(output.file_name)),
            output.page_count
        );
    }
}

#[test]
#[ignore = "Requires an installed LibreOffice; checks standalone converters with mandatory branding"]
fn office_pdf_publication_is_branded_and_preserves_input() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["corpus.docx", "corpus.doc", "corpus.udf", "signed.udf"] {
        let src = source(name);
        let before = calculate_sha256(&src).unwrap();
        let out = dir.path().join(format!("{name}.pdf"));
        office::convert_to_pdf_file(&src, &out, true).unwrap();
        let d = lopdf::Document::load(&out).unwrap();
        assert_eq!(brand_count(&out), d.get_pages().len());
        assert_eq!(calculate_sha256(&src).unwrap(), before);
        if let Some(path) = std::env::var_os("DUZENEK_ROUND2_ARTIFACTS") {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::copy(out, PathBuf::from(path).join(format!("{name}.pdf"))).unwrap();
        }
    }
}

#[test]
fn unrelated_file_cannot_be_selected_as_a_renderer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.pdf");
    std::fs::write(&path, b"not a renderer").unwrap();
    assert!(office::select_renderer(&path).is_err());
}
