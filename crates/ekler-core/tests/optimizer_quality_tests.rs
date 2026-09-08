use ekler_core::*;
#[test]
fn quality_presets_change_dimensions_and_solve_a_real_size_limit() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("tarama.png");
    let mut state = 7u32;
    let image = ::image::RgbImage::from_fn(2000, 1800, |_, _| {
        let mut channels = [0; 3];
        for v in &mut channels {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *v = state as u8;
        }
        ::image::Rgb(channels)
    });
    image.save(&src).unwrap();
    let original = ekler_core::image::image_file_to_pdf(&src).unwrap();
    let mut gentle = original.clone();
    let mut aggressive = original.clone();
    optimize_pdf(&mut gentle, OptimizationLevel::GentleCompression).unwrap();
    optimize_pdf(&mut aggressive, OptimizationLevel::AggressiveCompression).unwrap();
    let width = |d: &lopdf::Document| {
        d.objects
            .values()
            .find_map(|o| {
                o.as_stream()
                    .ok()
                    .filter(|s| {
                        s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image")
                    })
                    .map(|s| s.dict.get(b"Width").unwrap().as_i64().unwrap())
            })
            .unwrap()
    };
    assert_eq!(width(&gentle), 2000);
    assert_eq!(width(&aggressive), 1600);
    let mut bytes = Vec::new();
    aggressive.save_to(&mut bytes).unwrap();
    let target = bytes.len() as u64 + 20_000;
    let source = scan_source_files(&[&src]).sources.remove(0);
    let mut p = Project::new("quality");
    p.target_size_bytes = target;
    p.add_source(source.clone());
    p.create_exhibit("scan").sources.push(ExhibitSourceRef {
        source_id: source.id,
        page_range: None,
    });
    assert!(
        prepare_export_plan(&p).is_err(),
        "Unoptimized one-page image should exceed measured target"
    );
    p.optimization = OptimizationLevel::AggressiveCompression;
    let plan = prepare_export_plan(&p).unwrap();
    assert_eq!(plan.outputs.len(), 1);
    assert!(plan.outputs[0].size_bytes <= target);
}
