//! Sıkıştırma "kazanç yok" politikası, kullanıcı cümlesi ve çift sıkıştırma.
//!
//! Saha: 9,4 MB'lık bir taramada motor 204 KB (%2,1) gerçek kazanç buldu,
//! tek ölçüt %3 olduğu için çıktı atıldı ve kullanıcıya "Bu belge zaten
//! yeterince optimize" dendi. Bağımsız ölçüm dosyanın gerçekten sınıra yakın
//! olduğunu doğruladı; ama motorun bulamadığı küçülmeden "zaten optimize"
//! sonucu çıkarılamaz ve bulunan 204 KB sessizce atılmamalıydı.

use ekler_core::toolbox::{
    is_meaningful_saving, no_benefit_message, run_tool_with_outcome, ToolOperation, ToolOutcome,
};
use ekler_core::*;
use lopdf::{dictionary, Document, Object, Stream};
use std::path::{Path, PathBuf};

const KB: u64 = 1024;
const MB: u64 = 1024 * 1024;

/// Eski kural: aday kaynağın en fazla %97'si.
fn old_rule(source: u64, candidate: u64) -> bool {
    (candidate as u128) * 100 <= (source as u128) * 97
}

#[test]
fn field_case_204_kb_on_a_9_mb_scan_is_now_kept() {
    let (source, candidate) = (9_834_246, 9_834_246 - 208_794);
    assert!(!old_rule(source, candidate), "eski kural bunu atıyordu");
    assert!(is_meaningful_saving(source, candidate));
}

#[test]
fn old_three_percent_rule_is_never_made_stricter() {
    // "%2 VE 100 KB" kolu tek başına bunu reddederdi: 1 MB'ın %5'i 51 KB.
    let (source, candidate) = (MB, MB - MB * 5 / 100);
    assert!(old_rule(source, candidate));
    assert!(is_meaningful_saving(source, candidate));

    // Genel özellik: eskiden kaydedilen her sonuç şimdi de kaydedilir.
    for source in [40 * KB, 300 * KB, MB, 3 * MB, 9 * MB, 60 * MB] {
        for permille in 0..=1000u64 {
            let candidate = source - source * permille / 1000;
            if old_rule(source, candidate) {
                assert!(
                    is_meaningful_saving(source, candidate),
                    "{source} → {candidate} eskiden kaydediliyordu"
                );
            }
        }
    }
}

#[test]
fn trivial_savings_are_still_rejected() {
    // Sahadaki gerçek sonuçlar: 33 KB (%2,6) ve 1 KB (%1,1).
    assert!(!is_meaningful_saving(1_317_864, 1_317_864 - 34_202));
    assert!(!is_meaningful_saving(95_133, 95_133 - 1_126));
    // %2'nin altı, 500 KB'ın altı.
    assert!(!is_meaningful_saving(10 * MB, 10 * MB - 195 * KB));
    // Büyümüş ya da aynı kalmış aday asla "kazanç" değildir.
    assert!(!is_meaningful_saving(MB, MB));
    assert!(!is_meaningful_saving(MB, MB + 1));
    assert!(!is_meaningful_saving(0, 0));
}

#[test]
fn policy_boundaries_are_exact() {
    // %3 tam sınırı: geçer.
    assert!(is_meaningful_saving(100_000, 97_000));
    assert!(!is_meaningful_saving(100_000, 97_001));
    // %2 VE 100 KB. 5 MiB'ın %2'si 104.858 bayt: 100 KB'ı geçmek yetmez,
    // oran da tutmalı.
    let source = 5 * MB;
    assert!(is_meaningful_saving(source, source - 104_858));
    // 101 KB ama yalnız %1,97.
    assert!(!is_meaningful_saving(source, source - 103_448));
    // 5.120.000 baytın %2'si tam 102.400 bayt = tam 100 KB.
    assert!(is_meaningful_saving(
        5_120_000,
        5_120_000 - 102_400 // tam %2, tam 100 KB
    ));
    assert!(!is_meaningful_saving(5_120_000, 5_120_000 - 102_399));
    // 500 KB kolu: çok büyük belgede %1'in altı da geçer.
    assert!(is_meaningful_saving(60 * MB, 60 * MB - 500 * KB));
    assert!(!is_meaningful_saving(60 * MB, 60 * MB - 500 * KB + 1));
}

#[test]
fn real_corpus_simulation_matches_the_measured_table() {
    // (kaynak, en iyi markalı aday, eski sonuç, yeni sonuç) — sahadaki yedi
    // gerçek PDF'in yalnız BOYUTLARI; içerik yok.
    let table: [(u64, u64, bool, bool); 7] = [
        (9_032_806, 1_751_450, true, true),   // tarama, %80,6
        (3_521_881, 2_182_656, true, true),   // Ekler çıktısı, %38,0
        (8_436_429, 6_078_259, true, true),   // fotoğraflar, %28,0
        (5_616_009, 5_415_117, true, true),   // %3,6
        (1_317_864, 1_283_662, false, false), // Ekler çıktısı, 33 KB
        (9_834_246, 9_625_452, false, true),  // Ekler çıktısı, 204 KB
        (95_133, 94_106, false, false),       // Ekler çıktısı, 1 KB
    ];
    for (source, candidate, old, new) in table {
        assert_eq!(old_rule(source, candidate), old, "{source} eski");
        assert_eq!(
            is_meaningful_saving(source, candidate),
            new,
            "{source} yeni"
        );
    }
}

/// Arayüzle PAYLAŞILAN örnekler: `apps/belge-shell/src/modules/duzenek/
/// noBenefitMessage.test.ts` aynı tabloyu sınar. Biri değişirse ikisi de.
const TAIL: &str =
    "küçültülebildi. Bu değer anlamlı küçülme eşiğinin altında kaldığı için çıktı oluşturulmadı.";
const NONE: &str = "Bu ayarlarla kayda değer bir küçülme sağlanamadı. Çıktı oluşturulmadı.";

#[test]
fn no_benefit_message_states_the_real_bytes() {
    let cases: [(u64, u64, String); 9] = [
        // Saha: 9,4 MB taramada 203,9 KB.
        (9_834_246, 9_625_452, format!("204 KB (%2,1) {TAIL}")),
        // Çift sıkıştırmada ölçülen: 405 bayt. "0 KB" yazılmaz.
        (447_945, 447_540, format!("405 bayt (%0,1) {TAIL}")),
        // 10 KB'ın altı ondalıklı.
        (1_048_576, 1_043_456, format!("5,0 KB (%0,5) {TAIL}")),
        // Tam yarım: %2,25 → yarım yukarı "2,3" (`{:.1}` "2,2" derdi,
        // `toFixed` "2,3" — iki uç ayrışırdı).
        (4_000, 3_910, format!("90 bayt (%2,3) {TAIL}")),
        // Tam yarım KB: 10,5 KB → "11 KB".
        (10_000_000, 9_989_248, format!("11 KB (%0,1) {TAIL}")),
        // Yuvarlanınca %0,0: sayı uydurulmaz.
        (1_048_576, 1_048_556, NONE.to_string()),
        // Kazanç yok ya da aday büyümüş.
        (1_048_576, 1_048_576, NONE.to_string()),
        (1_048_576, 1_053_576, NONE.to_string()),
        (0, 0, NONE.to_string()),
    ];
    for (source, candidate, expected) in cases {
        let text = no_benefit_message(source, candidate);
        assert_eq!(text, expected, "{source} → {candidate}");
        assert!(!text.contains("optimize"), "{text}");
        assert!(!text.contains('-'), "negatif kazanç yazılmamalı: {text}");
    }
}

// --- Çift sıkıştırma: Ekler hattının hedefe sıkıştırdığı çıktı tekrar Sıkıştır'a gelir.

/// Gürültülü, q100 kodlanmış tarama sayfası: Ekler hattının küçültebileceği
/// türden bir kaynak.
fn scan_pdf(path: &Path) {
    let mut state = 0x2545_F491u32;
    let rgb = ::image::RgbImage::from_fn(1100, 1500, |_, y| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let n = (state % 70) as u8;
        let v = if y % 90 < 6 { 30 + n / 4 } else { 175 + n };
        ::image::Rgb([v, v, v.saturating_sub(4)])
    });
    let mut jpeg = Vec::new();
    ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
        .encode_image(&rgb)
        .unwrap();
    let mut d = Document::with_version("1.7");
    let pages = d.new_object_id();
    let img = d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>1100,"Height"=>1500,
        "ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,"Filter"=>"DCTDecode"},
        jpeg,
    ));
    let contents = d.add_object(Stream::new(
        dictionary! {},
        b"q 595 0 0 842 0 0 cm /Scan Do Q".to_vec(),
    ));
    let page = d.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,
    "MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()],
    "Resources"=>dictionary!{"XObject"=>dictionary!{"Scan"=>img}},"Contents"=>contents});
    d.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer.set("Root", root);
    d.save(path).unwrap();
}

/// Kaynağı GERÇEK Ekler hattından geçirir; hedef kaynaktan küçük olduğu için
/// hat sıkıştırır. Tek çıktının yolunu döner.
fn ekler_output(dir: &Path, source: &Path) -> PathBuf {
    let size = std::fs::metadata(source).unwrap().len();
    let mut project = Project::new("Çift sıkıştırma");
    project.add_source(SourceFile {
        id: "src-1".into(),
        path: source.to_path_buf(),
        file_name: "tarama.pdf".into(),
        format: SourceFormat::Pdf,
        size_bytes: size,
        sha256_before: calculate_sha256(source).unwrap(),
        mtime: 1000,
        page_count: 1,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    });
    project
        .create_exhibit("Tarama")
        .sources
        .push(ExhibitSourceRef {
            source_id: "src-1".into(),
            page_range: None,
        });
    project.optimization = OptimizationLevel::AggressiveCompression;
    project.target_size_bytes = size - 1;
    let out_dir = dir.join("paket");
    std::fs::create_dir_all(&out_dir).unwrap();
    let result = execute_uyap_preparation(
        &project,
        &ExecutionContext {
            output_dir: out_dir,
        },
    )
    .unwrap();
    assert_eq!(result.outputs.len(), 1, "tek sayfa, tek çıktı");
    let output = result.package_dir.join(&result.outputs[0].file_name);
    assert!(
        std::fs::metadata(&output).unwrap().len() < size,
        "Ekler hattı kaynağı gerçekten küçültmüş olmalı"
    );
    output
}

/// Sayfa başına (dönüş, MediaBox, CropBox) — sayılar `f32` olarak.
fn page_geometry(path: &Path) -> Vec<(i64, [f32; 4], Option<[f32; 4]>)> {
    let d = Document::load(path).unwrap();
    let boxed = |v: &Object| -> [f32; 4] {
        let a = v.as_array().unwrap();
        [0, 1, 2, 3].map(|i| a[i].as_float().unwrap())
    };
    d.get_pages()
        .values()
        .map(|id| {
            let page = d.get_dictionary(*id).unwrap();
            (
                page.get(b"Rotate").and_then(|v| v.as_i64()).unwrap_or(0),
                boxed(page.get(b"MediaBox").unwrap()),
                page.get(b"CropBox").ok().map(boxed),
            )
        })
        .collect()
}

/// Sıkıştırma çıktısının sayfa geometrisi korunmuş mu?
///
/// Sayfa sayısı ve dönüş BİREBİR. Kutu: GölgeDosya ilk kez markaladığı
/// sayfanın ALTINA bir marka bandı ekler ve görünür kutuyu ona göre
/// genişletir — mevcut, bilinçli marka davranışı; kaynak zaten markalıysa
/// hiç eklenmez. Bu yüzden sol, sağ ve üst kenar birebir kalır; alt kenar
/// ancak aşağı doğru uzayabilir, içerik alanı asla kırpılmaz.
fn assert_page_area_preserved(out: &Path, src: &Path) {
    let (after, before) = (page_geometry(out), page_geometry(src));
    assert_eq!(after.len(), before.len(), "sayfa sayısı");
    for ((rot_a, media_a, crop_a), (rot_b, media_b, _)) in after.iter().zip(&before) {
        assert_eq!(rot_a, rot_b, "dönüş");
        assert_eq!(
            [media_a[0], media_a[2], media_a[3]],
            [media_b[0], media_b[2], media_b[3]],
            "sol/sağ/üst kenar"
        );
        assert!(
            media_a[1] <= media_b[1],
            "alt kenar yalnız aşağı uzayabilir"
        );
        if let Some(crop) = crop_a {
            assert_eq!(crop, media_a, "görünür kutu sayfanın tamamı");
        }
    }
}

#[test]
fn compressing_an_ekler_output_again_reports_real_numbers_and_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let scan = dir.path().join("tarama.pdf");
    scan_pdf(&scan);
    let input = ekler_output(dir.path(), &scan);
    let before = std::fs::read(&input).unwrap();
    let out = dir.path().join("tekrar.pdf");

    let outcome = run_tool_with_outcome(
        std::slice::from_ref(&input),
        &ToolOperation::Compress {
            level: OptimizationLevel::AggressiveCompression,
        },
        &out,
        false,
    )
    .unwrap();

    assert_eq!(std::fs::read(&input).unwrap(), before, "kaynak değişmemeli");
    match outcome {
        ToolOutcome::NoBenefit {
            source_bytes,
            candidate_bytes,
            previously_processed,
            ..
        } => {
            assert_eq!(
                source_bytes,
                before.len() as u64,
                "kazanç GERÇEK kaynağa göre"
            );
            assert!(!is_meaningful_saving(source_bytes, candidate_bytes));
            assert!(
                previously_processed,
                "Ekler çıktısı GölgeDosya çıktısı olarak tanınmalı"
            );
            let text = no_benefit_message(source_bytes, candidate_bytes);
            println!("cift-sikistirma: NoBenefit {source_bytes} -> {candidate_bytes} | {text}");
            assert!(!text.contains("optimize"));
            // Küçük de olsa gerçek kazanç rakamla söylenir; biçimi
            // `no_benefit_message_states_the_real_bytes` sabitler (bu ölçüm
            // o tabloda: 447.945 → 447.540 = "405 bayt (%0,1)").
            if candidate_bytes < source_bytes && text != NONE {
                assert!(text.ends_with(TAIL), "{text}");
            }
            assert!(!out.exists(), "kazanç yokken dosya yazılmaz");
        }
        ToolOutcome::Compressed {
            source_bytes,
            output_bytes,
            ..
        } => {
            println!("cift-sikistirma: Compressed {source_bytes} -> {output_bytes}");
            assert!(is_meaningful_saving(source_bytes, output_bytes));
            assert_page_area_preserved(&out, &input);
        }
        other => panic!("çift sıkıştırma çökmemeli / başarısız olmamalı: {other:?}"),
    }
}

fn evidence(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/evidence")
        .join(name)
}

#[test]
fn previously_processed_is_reported_only_for_golgedosya_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let compress = |src: &Path, name: &str| {
        run_tool_with_outcome(
            &[src.to_path_buf()],
            &ToolOperation::Compress {
                level: OptimizationLevel::BalancedCompression,
            },
            &dir.path().join(name),
            false,
        )
        .unwrap()
    };
    // Yabancı vektör belge: kazanç yok, "daha önce işlenmiş" DEĞİL.
    let plain = dir.path().join("yabanci.pdf");
    std::fs::copy(evidence("vector.pdf"), &plain).unwrap();
    assert!(matches!(
        compress(&plain, "a.pdf"),
        ToolOutcome::NoBenefit {
            previously_processed: false,
            ..
        }
    ));
    // Aynı belge GölgeDosya'dan geçmiş (yalnız döndürülmüş, SIKIŞTIRILMAMIŞ):
    // "işlenmiş" doğrudur; alan sıkıştırma geçmişi iddia etmez.
    let rotated = dir.path().join("dondurulmus.pdf");
    ekler_core::toolbox::run_tool(
        std::slice::from_ref(&plain),
        &ToolOperation::Rotate { degrees: 90 },
        &rotated,
        false,
    )
    .unwrap();
    assert!(matches!(
        compress(&rotated, "b.pdf"),
        ToolOutcome::NoBenefit {
            previously_processed: true,
            ..
        }
    ));
}

/// Kazancı tam denetlenen belge: sıkıştırılamaz balast (zaten Flate'li
/// rastgele bayt; motor dokunmaz) + temizlikte silinen `/Metadata`.
fn ballast_pdf(path: &Path, ballast: usize, metadata: usize) {
    let mut state = 0x9E37_79B9u32;
    let noise: Vec<u8> = (0..ballast)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    let mut d = Document::with_version("1.7");
    let pages = d.new_object_id();
    let contents = d.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
    let page = d.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,
    "MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()],"Contents"=>contents});
    d.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let mut blob = Stream::new(dictionary! {}, noise);
    blob.dict.set("Filter", "FlateDecode");
    blob.set_content({
        use std::io::Write;
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(&blob.content).unwrap();
        e.finish().unwrap()
    });
    let blob = d.add_object(blob);
    let meta = d.add_object(Stream::new(
        dictionary! {"Type"=>"Metadata","Subtype"=>"XML"},
        vec![b'x'; metadata],
    ));
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages,
    "Ballast"=>blob,"Metadata"=>meta});
    d.trailer.set("Root", root);
    d.save(path).unwrap();
}

#[test]
fn a_real_two_to_three_percent_saving_is_published_end_to_end() {
    // Politika fonksiyonunu yalıtılmış sınamak yetmez: çağrı noktası eski
    // %3 kuralına dönerse saha regresyonu sessizce geri gelirdi.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("balast.pdf");
    ballast_pdf(&src, 8 * MB as usize, 215 * KB as usize);
    let before = std::fs::read(&src).unwrap();
    let out = dir.path().join("kucuk.pdf");
    let outcome = run_tool_with_outcome(
        std::slice::from_ref(&src),
        &ToolOperation::Compress {
            level: OptimizationLevel::BalancedCompression,
        },
        &out,
        false,
    )
    .unwrap();
    assert_eq!(std::fs::read(&src).unwrap(), before, "kaynak değişmemeli");
    match outcome {
        ToolOutcome::Compressed {
            source_bytes,
            output_bytes,
            ..
        } => {
            let saved = source_bytes - output_bytes;
            println!("iki-uc: {source_bytes} -> {output_bytes} ({saved} bayt)");
            assert!(saved >= 100 * KB, "fixture 100 KB üstü kazanç vermeli");
            assert!(
                !old_rule(source_bytes, output_bytes),
                "fixture %2–3 bandında olmalı; eski kural bunu ATARDI"
            );
            assert_eq!(std::fs::metadata(&out).unwrap().len(), output_bytes);
            assert_page_area_preserved(&out, &src);
        }
        other => panic!("%2–3 ve 100 KB üstü gerçek kazanç yayınlanmalı: {other:?}"),
    }
}

#[test]
fn no_benefit_reaches_the_shell_with_the_fields_it_reads() {
    // Kabuk (`PdfWorkspace.tsx` → `noBenefitStatus`) bu adları okur; Tauri
    // komutu `ToolOutcome`'u olduğu gibi serileştirir.
    let value = serde_json::to_value(ToolOutcome::NoBenefit {
        source_bytes: 9_834_246,
        body_bytes: 9_625_000,
        candidate_bytes: 9_625_452,
        images_found: 42,
        images_recompressed: 5,
        previously_processed: true,
    })
    .unwrap();
    assert_eq!(value["status"], "no_benefit");
    assert_eq!(value["source_bytes"], 9_834_246);
    assert_eq!(value["candidate_bytes"], 9_625_452);
    assert_eq!(value["previously_processed"], true);
}
