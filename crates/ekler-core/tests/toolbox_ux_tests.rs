use ekler_core::{
    pdf,
    toolbox::{self, PageRotation, ToolOperation, ToolOutcome},
    *,
};
use lopdf::{dictionary, Document, Stream};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
fn vector() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/vector.pdf")
}
fn fixture(n: usize) -> Document {
    let d = Document::load(vector()).unwrap();
    let page = pdf::extract_page_range(&d, 1, 1).unwrap();
    pdf::merge_documents(&vec![page; n]).unwrap()
}
fn bytes(d: &mut Document) -> Vec<u8> {
    let mut b = vec![];
    d.save_to(&mut b).unwrap();
    b
}
fn artifact(name: &str, b: &[u8]) {
    if let Some(p) = std::env::var_os("DUZENEK_ROUND3_ARTIFACTS") {
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(PathBuf::from(p).join(name), b).unwrap();
    }
}
#[test]
fn watermark_resource_reused_across_pages() {
    let mut d = fixture(100);
    pdf::stamp::apply_branding(&mut d).unwrap();
    let mut refs = HashSet::new();
    for id in d.get_pages().values() {
        let r = pdf::resolved_page_dictionary(&d, *id).unwrap();
        let x = r
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"XObject")
            .unwrap()
            .as_dict()
            .unwrap();
        let (name, value) = x
            .iter()
            .find(|(k, _)| k.starts_with(b"DuzenEkBrand"))
            .unwrap();
        assert!(!name.is_empty());
        refs.insert(value.as_reference().unwrap());
    }
    assert_eq!(refs.len(), 1);
    let form = d
        .get_object(*refs.iter().next().unwrap())
        .unwrap()
        .as_stream()
        .unwrap();
    assert_eq!(
        form.dict.get(b"Subtype").unwrap().as_name().unwrap(),
        b"Form"
    );
    let opacity = form
        .dict
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"ExtGState")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"BrandAlpha")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"ca")
        .unwrap()
        .as_float()
        .unwrap();
    assert!((opacity - 0.24).abs() < 0.001);
    assert_eq!(
        d.objects
            .values()
            .filter(|v| v
                .as_stream()
                .ok()
                .and_then(|s| s.dict.get(b"Subtype").ok())
                .and_then(|v| v.as_name().ok())
                == Some(b"Image"))
            .count(),
        0
    );
}
#[test]
fn watermark_size_growth_is_bounded() {
    for n in [1, 10, 100] {
        let mut d = fixture(n);
        d.compress();
        let plain = bytes(&mut d);
        artifact(&format!("brand-{n}-source.pdf"), &plain);
        pdf::stamp::apply_branding(&mut d).unwrap();
        let branded = bytes(&mut d);
        artifact(&format!("brand-{n}-output.pdf"), &branded);
        println!(
            "brand {n}: {} -> {} (+{})",
            plain.len(),
            branded.len(),
            branded.len() - plain.len()
        );
        assert!(
            branded.len() <= plain.len() + 4096 + n * 100,
            "Unexpected repeated asset growth"
        );
        let reopened = Document::load_mem(&branded).unwrap();
        pdf::validate_document(&reopened).unwrap();
        assert_eq!(reopened.get_pages().len(), n);
    }
}
fn check_hash(p: &Path, h: &str) {
    assert_eq!(calculate_sha256(p).unwrap(), h);
}
#[test]
fn compression_output_must_not_be_larger_than_source_when_reported_success() {
    let dir = tempfile::tempdir().unwrap();
    let src = vector();
    let hash = calculate_sha256(&src).unwrap();
    let out = dir.path().join("vector-result.pdf");
    for level in [
        OptimizationLevel::LowRiskCleanup,
        OptimizationLevel::GentleCompression,
        OptimizationLevel::AggressiveCompression,
    ] {
        let result = toolbox::run_tool_with_outcome(
            std::slice::from_ref(&src),
            &ToolOperation::Compress { level },
            &out,
            false,
        )
        .unwrap();
        assert!(matches!(result, ToolOutcome::NoBenefit { .. }));
        assert!(!out.exists());
        check_hash(&src, &hash);
    }
    // A pre-compressed ~810 KB content stream must not be published just because it is valid.
    let mut d = fixture(1);
    let mut state = 7u32;
    let mut content = d.get_page_content(d.get_pages()[&1]).unwrap();
    for _ in 0..21600 {
        content.extend_from_slice(b"\n%");
        for _ in 0..64 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            content.push(b"0123456789abcdef"[(state & 15) as usize]);
        }
    }
    let stream = d.add_object(Stream::new(dictionary! {}, content));
    let id = d.get_pages()[&1];
    d.get_dictionary_mut(id).unwrap().set("Contents", stream);
    d.compress();
    let optimized = bytes(&mut d);
    artifact("optimized-small.pdf", &optimized);
    let src = dir.path().join("optimized.pdf");
    std::fs::write(&src, &optimized).unwrap();
    let hash = calculate_sha256(&src).unwrap();
    let result = toolbox::run_tool_with_outcome(
        std::slice::from_ref(&src),
        &ToolOperation::Compress {
            level: OptimizationLevel::GentleCompression,
        },
        &out,
        false,
    )
    .unwrap();
    assert!(matches!(result, ToolOutcome::NoBenefit { .. }));
    assert!(!out.exists());
    check_hash(&src, &hash);
    println!("optimized source bytes: {}", optimized.len());
    // A high-quality, image-heavy input must actually produce a smaller readable copy.
    let image = ::image::RgbImage::from_fn(1400, 1600, |x, y| {
        let grain = ((x * 37 + y * 71) % 19) as u8;
        ::image::Rgb([
            (x / 8) as u8 + grain / 4,
            (y / 8) as u8 + grain / 4,
            120 + grain,
        ])
    });
    let jpg = dir.path().join("image.jpg");
    let mut jpeg = vec![];
    ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
        .encode_image(&image)
        .unwrap();
    std::fs::write(&jpg, jpeg).unwrap();
    let mut d = ekler_core::image::image_file_to_pdf(&jpg).unwrap();
    for obj in d.objects.values_mut() {
        if let Ok(s) = obj.as_stream_mut() {
            if s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image") {
                s.set_content(std::fs::read(&jpg).unwrap());
            }
        }
    }
    let original = bytes(&mut d);
    artifact("image-heavy.pdf", &original);
    let src = dir.path().join("image.pdf");
    std::fs::write(&src, &original).unwrap();
    let hash = calculate_sha256(&src).unwrap();
    let result = toolbox::run_tool_with_outcome(
        std::slice::from_ref(&src),
        &ToolOperation::Compress {
            level: OptimizationLevel::GentleCompression,
        },
        &out,
        false,
    )
    .unwrap();
    assert!(
        matches!(result, ToolOutcome::Compressed { .. }),
        "{result:?}; input {}",
        original.len()
    );
    let output = std::fs::read(&out).unwrap();
    assert!(output.len() * 100 <= original.len() * 97);
    Document::load_mem(&output).unwrap();
    check_hash(&src, &hash);
    artifact("image-heavy-compressed.pdf", &output);
    println!("image-heavy: {} -> {}", original.len(), output.len());
}
#[test]
fn per_page_rotation_matches_accumulated_preview_and_preserves_source() {
    let src = vector();
    let hash = calculate_sha256(&src).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("rotated.pdf");
    toolbox::run_tool(
        std::slice::from_ref(&src),
        &ToolOperation::RotatePages {
            rotations: vec![
                PageRotation {
                    page: 1,
                    degrees: 270,
                },
                PageRotation {
                    page: 2,
                    degrees: 90,
                },
            ],
        },
        &out,
        false,
    )
    .unwrap();
    let d = Document::load(&out).unwrap();
    for (n, angle) in [(1, 270), (2, 90)] {
        assert_eq!(
            pdf::resolved_page_dictionary(&d, d.get_pages()[&n])
                .unwrap()
                .get(b"Rotate")
                .unwrap()
                .as_i64()
                .unwrap(),
            angle
        );
    }
    check_hash(&src, &hash);
    assert!(
        toolbox::run_tool(&[src], &ToolOperation::Rotate { degrees: 90 }, &out, false).is_err()
    );
}
