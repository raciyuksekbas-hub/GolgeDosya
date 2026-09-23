use ekler_core::pdf::{extract_page_range, resolved_page_dictionary};
use lopdf::{dictionary, Document, Object, Stream};
fn nested_fixture() -> Document {
    let mut d = Document::with_version("1.7");
    let root = d.new_object_id();
    let nested = d.new_object_id();
    let font =
        d.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let image=d.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Image","Width"=>2,"Height"=>2,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8},vec![255,0,0,0,255,0,0,0,255,255,255,0]));
    let resources = d.add_object(
        dictionary! {"Font"=>dictionary!{"F1"=>font},"XObject"=>dictionary!{"Im1"=>image}},
    );
    let contents = d.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 20 Tf 45 170 Td (INHERITED FONT) Tj ET q 40 0 0 40 50 60 cm /Im1 Do Q".to_vec(),
    ));
    let page = d.add_object(dictionary! {"Type"=>"Page","Parent"=>nested,"Contents"=>contents});
    d.set_object(nested,dictionary!{"Type"=>"Pages","Parent"=>root,"Kids"=>vec![page.into()],"Count"=>1,"CropBox"=>vec![10.into(),10.into(),290.into(),210.into()],"Rotate"=>90});
    d.set_object(root,dictionary!{"Type"=>"Pages","Kids"=>vec![nested.into()],"Count"=>1,"MediaBox"=>vec![0.into(),0.into(),300.into(),220.into()],"Resources"=>resources});
    let catalog = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>root});
    d.trailer.set("Root", catalog);
    d
}
#[test]
fn nested_page_inheritance_and_resource_graph_survive_copy() {
    let src = nested_fixture();
    let output = extract_page_range(&src, 1, 1).unwrap();
    let src_page =
        resolved_page_dictionary(&src, *src.get_pages().values().next().unwrap()).unwrap();
    let out_page =
        resolved_page_dictionary(&output, *output.get_pages().values().next().unwrap()).unwrap();
    for key in [b"MediaBox".as_slice(), b"CropBox", b"Rotate"] {
        assert_eq!(src_page.get(key).unwrap(), out_page.get(key).unwrap());
    }
    let resources = output
        .get_dictionary(out_page.get(b"Resources").unwrap().as_reference().unwrap())
        .unwrap();
    for key in [b"Font".as_slice(), b"XObject"] {
        assert!(resources.has(key));
    }
    assert!(output
        .extract_text(&[1])
        .unwrap()
        .contains("INHERITED FONT"));
}
#[test]
fn local_page_attribute_overrides_parent() {
    let mut src = nested_fixture();
    let id = *src.get_pages().values().next().unwrap();
    src.get_dictionary_mut(id).unwrap().set("Rotate", 180);
    assert_eq!(
        resolved_page_dictionary(&src, id)
            .unwrap()
            .get(b"Rotate")
            .unwrap(),
        &Object::Integer(180)
    );
}
#[test]
#[ignore = "Independent renderer gate: requires pdftoppm (QA only, never bundled in the product)"]
fn independent_renderer_pixel_comparison() {
    let dir = std::env::var_os("DUZENEK_RENDER_OUTPUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("duzenek-render-fidelity"));
    std::fs::create_dir_all(&dir).unwrap();
    let mut source = nested_fixture();
    let mut output = extract_page_range(&source, 1, 1).unwrap();
    source.save(dir.join("source.pdf")).unwrap();
    output.save(dir.join("output.pdf")).unwrap();
    for name in ["source", "output"] {
        let result = std::process::Command::new(
            std::env::var("DUZENEK_PDFTOPPM").unwrap_or("pdftoppm".into()),
        )
        .args(["-png", "-r", "96", "-singlefile"])
        .arg(dir.join(format!("{name}.pdf")))
        .arg(dir.join(name))
        .output()
        .expect("pdftoppm required; no silent pass");
        assert!(result.status.success(), "{:?}", result);
    }
    let source = ::image::open(dir.join("source.png")).unwrap().to_rgb8();
    let output = ::image::open(dir.join("output.png")).unwrap().to_rgb8();
    assert_eq!(source.dimensions(), output.dimensions());
    let mae = source
        .as_raw()
        .iter()
        .zip(output.as_raw())
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / source.as_raw().len() as f64;
    assert!(
        mae <= 0.5,
        "Mean absolute pixel difference {mae} exceeds 0.5/255; artifacts: {}",
        dir.display()
    );
}
#[test]
fn stamp_resolves_inheritance_isolates_graphics_and_uses_rotated_crop() {
    let mut d = nested_fixture();
    let page = *d.get_pages().values().next().unwrap();
    ekler_core::pdf::stamp::apply_stamp_to_document(
        &mut d,
        2,
        19,
        &ekler_core::StampConfig::default(),
    )
    .unwrap();
    ekler_core::pdf::validate_document(&d).unwrap();
    if let Some(dir) = std::env::var_os("DUZENEK_STAMP_ARTIFACTS") {
        std::fs::create_dir_all(&dir).unwrap();
        d.save(std::path::PathBuf::from(dir).join("rotated-stamp.pdf"))
            .unwrap();
    }
    let text = d.extract_text(&[1]).unwrap();
    assert!(text.contains("INHERITED FONT"));
    assert!(text.contains("Ek-2 / 19. Sayfa"));
    let bytes = d.get_page_content(page).unwrap();
    let content = lopdf::content::Content::decode(&bytes).unwrap();
    assert_eq!(content.operations[0].operator, "q");
    assert!(content.operations.iter().any(|op| op.operator == "cm"
        && op
            .operands
            .iter()
            .map(|v| v.as_float().unwrap())
            .collect::<Vec<_>>()
            == vec![0., 1., -1., 0., 290., 10.]));
}
#[test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn native_pdf_raster_has_expected_rotated_crop_and_visible_content() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("source.pdf");
    nested_fixture().save(&path).unwrap();
    let out = ekler_core::raster::pdf_to_images(&path, d.path(), "png", 72, false).unwrap();
    let img = ::image::open(out.join("sayfa-0001.png")).unwrap().to_rgb8();
    assert_eq!(img.dimensions(), (200, 314)); // 34 pt required derived-logo gutter
    let colored = img
        .pixels()
        .filter(|p| p[0].abs_diff(p[1]) > 100 || p[1].abs_diff(p[2]) > 100)
        .count();
    assert!(
        colored > 1000,
        "Inherited image should remain visible: {colored}"
    );
}

#[test]
#[ignore = "Independent renderer gate for all rotations and stamp positions; requires pdftoppm"]
fn stamp_gutter_never_covers_source_pixels() {
    use ekler_core::{StampConfig, StampPosition};
    let dir = std::env::var_os("DUZENEK_STAMP_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("duzenek-stamp-qa"));
    std::fs::create_dir_all(&dir).unwrap();
    for rotate in [0, 90, 180, 270] {
        for top in [true, false] {
            let mut source = nested_fixture();
            let page = *source.get_pages().values().next().unwrap();
            source
                .get_dictionary_mut(page)
                .unwrap()
                .set("Rotate", rotate);
            let mut output = source.clone();
            let config = StampConfig {
                position: if top {
                    StampPosition::TopRight
                } else {
                    StampPosition::BottomLeft
                },
                ..Default::default()
            };
            ekler_core::pdf::stamp::apply_stamp_to_document(&mut output, 1, 1, &config).unwrap();
            let prefix = format!("{rotate}-{top}");
            for (name, mut doc) in [("source", source), ("output", output)] {
                let file = dir.join(format!("{prefix}-{name}.pdf"));
                doc.save(&file).unwrap();
                let status = std::process::Command::new(
                    std::env::var("DUZENEK_PDFTOPPM").unwrap_or("pdftoppm".into()),
                )
                .args(["-cropbox", "-png", "-r", "72", "-singlefile"])
                .arg(&file)
                .arg(dir.join(format!("{prefix}-{name}")))
                .output()
                .unwrap();
                assert!(status.status.success(), "{:?}", status);
            }
            let source = ::image::open(dir.join(format!("{prefix}-source.png")))
                .unwrap()
                .to_rgb8();
            let output = ::image::open(dir.join(format!("{prefix}-output.png")))
                .unwrap()
                .to_rgb8();
            let gutter = (config.margin_pt * 2. + config.font_size + 10.) as u32;
            assert_eq!(
                output.dimensions(),
                (source.width(), source.height() + gutter)
            );
            let content = ::image::imageops::crop_imm(
                &output,
                0,
                if top { gutter } else { 0 },
                source.width(),
                source.height(),
            )
            .to_image();
            let mae = source
                .as_raw()
                .iter()
                .zip(content.as_raw())
                .map(|(a, b)| f64::from(a.abs_diff(*b)))
                .sum::<f64>()
                / source.as_raw().len() as f64;
            assert!(mae <= 0.5, "{prefix}: content changed MAE={mae}");
        }
    }
}
