use ekler_core::{
    calculate_sha256,
    toolbox::{run_tool_with_outcome, ToolOperation, ToolOutcome},
    OptimizationLevel as Level,
};
use lopdf::{dictionary, Document, Object, Stream};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};
fn corpus() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
  let root=std::env::var_os("DUZENEK_COMPRESSION_CORPUS").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join(format!("duzenek-compression-{}",std::process::id())));
  std::fs::create_dir_all(&root).unwrap();
  let mut state=7u32;
  let rgb=image::RgbImage::from_fn(3000,4200,|x,y| {
   state^=state<<13;state^=state>>17;state^=state<<5;
   let noise=(state%80) as u8;
   let v=if y%140<8 && x>150 && x<2300 { 25+noise/8 } else { 170+noise };
   image::Rgb([v,v,v])
  });
  let mut raw=Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Image","Width"=>3000,"Height"=>4200,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8},rgb.as_raw().clone());
  raw.compress().unwrap();
  image_pdf(raw).save(root.join("A-scan-300dpi.pdf")).unwrap();
  let mut jpeg=vec![];
  image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg,100).encode_image(&rgb).unwrap();
  image_pdf(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Image","Width"=>3000,"Height"=>4200,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,"Filter"=>vec![Object::Name(b"DCTDecode".to_vec())]},jpeg)).save(root.join("B-large-jpeg.pdf")).unwrap();
  let base=Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/evidence");
  std::fs::copy(base.join("vector.pdf"),root.join("C-vector.pdf")).unwrap();
  std::fs::copy(base.join("optimized-small.pdf"),root.join("D-optimized.pdf")).unwrap();
  root
 })
}
fn image_pdf(image: Stream) -> Document {
    let mut d = Document::with_version("1.5");
    let pages = d.new_object_id();
    let img = d.add_object(image);
    let contents = d.add_object(Stream::new(
        dictionary! {},
        b"q 595 0 0 842 0 0 cm /Scan Do Q".to_vec(),
    ));
    let page=d.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()],"Resources"=>dictionary!{"XObject"=>dictionary!{"Scan"=>img}},"Contents"=>contents});
    d.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer.set("Root", root);
    d
}
fn run(name: &str, level: Level, suffix: &str) -> ToolOutcome {
    let src = corpus().join(name);
    let out = corpus().join(format!("{suffix}.pdf"));
    let hash = calculate_sha256(&src).unwrap();
    let result = run_tool_with_outcome(
        std::slice::from_ref(&src),
        &ToolOperation::Compress { level },
        &out,
        false,
    )
    .unwrap();
    assert_eq!(hash, calculate_sha256(&src).unwrap());
    if let ToolOutcome::Compressed { output_bytes, .. } = result {
        assert!(output_bytes > 0 && output_bytes < std::fs::metadata(&src).unwrap().len());
        assert_eq!(output_bytes, std::fs::metadata(&out).unwrap().len());
        let pdf = Document::load(&out).unwrap();
        assert_eq!(
            pdf.get_pages().len(),
            Document::load(&src).unwrap().get_pages().len()
        );
    } else {
        assert!(!out.exists());
    }
    println!("{name} {level:?}: {result:?}");
    result
}
#[test]
fn compression_large_scan_must_produce_real_smaller_candidate() {
    assert!(
        std::fs::metadata(corpus().join("A-scan-300dpi.pdf"))
            .unwrap()
            .len()
            > 10_000_000
    );
    assert!(matches!(
        run(
            "A-scan-300dpi.pdf",
            Level::BalancedCompression,
            "scan-balanced"
        ),
        ToolOutcome::Compressed {
            images_recompressed: 1,
            ..
        }
    ));
}
#[test]
fn compression_no_benefit_is_not_failure() {
    for name in ["C-vector.pdf", "D-optimized.pdf"] {
        assert!(matches!(
            run(
                name,
                Level::BalancedCompression,
                &format!("no-benefit-{name}")
            ),
            ToolOutcome::NoBenefit { .. }
        ));
    }
}
#[test]
fn compression_failure_is_not_reported_as_already_optimized() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.pdf");
    image_pdf(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Image","Width"=>100,"Height"=>100,"ColorSpace"=>"DeviceRGB","Filter"=>"DCTDecode","BitsPerComponent"=>8},b"broken JPEG".to_vec())).save(&src).unwrap();
    let out = dir.path().join("out.pdf");
    assert!(matches!(
        run_tool_with_outcome(
            &[src],
            &ToolOperation::Compress {
                level: Level::BalancedCompression
            },
            &out,
            false
        )
        .unwrap(),
        ToolOutcome::Failed { .. }
    ));
    assert!(!out.exists());
}
#[test]
fn compression_presets_must_have_real_effect() {
    let mut sizes = std::collections::HashSet::new();
    for (i, level) in [
        Level::GentleCompression,
        Level::BalancedCompression,
        Level::AggressiveCompression,
    ]
    .into_iter()
    .enumerate()
    {
        match run("B-large-jpeg.pdf", level, &format!("jpeg-preset-{i}")) {
            ToolOutcome::Compressed { output_bytes, .. } => {
                sizes.insert(output_bytes);
            }
            v => panic!("{v:?}"),
        }
    }
    assert_eq!(sizes.len(), 3);
}
#[test]
fn compression_success_output_must_be_smaller_than_source() {
    assert!(matches!(
        run(
            "B-large-jpeg.pdf",
            Level::BalancedCompression,
            "jpeg-success"
        ),
        ToolOutcome::Compressed { .. }
    ));
}
#[test]
fn watermark_cost_must_not_hide_compression_gain() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = Document::load(corpus().join("C-vector.pdf")).unwrap();
    let metadata = d.add_object(Stream::new(dictionary! {}, vec![b'x'; 1000]));
    d.catalog_mut().unwrap().set("Metadata", metadata);
    let src = dir.path().join("metadata.pdf");
    d.save(&src).unwrap();
    let out = dir.path().join("out.pdf");
    match run_tool_with_outcome(
        &[src],
        &ToolOperation::Compress {
            level: Level::BalancedCompression,
        },
        &out,
        false,
    )
    .unwrap()
    {
        ToolOutcome::NoBenefit {
            source_bytes,
            body_bytes,
            candidate_bytes,
            ..
        } => {
            assert!(body_bytes < source_bytes);
            assert!(candidate_bytes > source_bytes);
            println!("logo-gain: {source_bytes} / {body_bytes} / {candidate_bytes}");
        }
        v => panic!("{v:?}"),
    }
    assert!(!out.exists());
}
