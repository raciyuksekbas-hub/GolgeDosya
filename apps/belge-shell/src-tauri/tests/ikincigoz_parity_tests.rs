//! Standalone İkinciGöz ile birleşik Denetle arasında davranış eşitliği.
//!
//! Bu fazın başarısı "parser'ı Tavzih'e bağladık" DEĞİLDİR; kanıtlanmış denetim
//! ve cerrahi writeback davranışını bozmadan taşımaktır. Bu dosya onu ölçer.
//!
//! Karşılaştırılan alanlar, migration regresyonunun görünebileceği yerler:
//! bulgu sayısı, kural kimliği, severity, kaynak konumu (blok + codepoint
//! offset), önerilen düzeltme ve writeback çıktısı.
//!
//! Referans fixture'lar artık `ikincigoz-core`'un kendi `tests/samples/`
//! klasöründedir; bağımsız depodan taşındıklarında içerikleri değişmedi. Aynı
//! çekirdek crate'i tükettiğimiz için farkın çıkabileceği tek yer sarmalayan
//! katmandır — ayarlar, sözlük, profil ve seçeneklerin bağlanması.

#![cfg(feature = "feature_ikincigoz")]

use ikincigoz_core::dict::UserDictionary;
use ikincigoz_core::finding::Finding;
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};
use ikincigoz_core::writeback;
use std::path::PathBuf;

/// Örnek belgeler — birleşik depo içinde, `ikincigoz-core` ile birlikte.
///
/// Eskiden bağımsız İkinciGöz deposuna işaret ediyordu. Taze klon kanıtı bunu
/// yakaladı: manifest'te yol bağımlılığı yoktu, ama KAYNAK KODU depo dışına
/// uzanıyordu ve klonda beş test düşüyordu.
///
/// Bulunamazsa test SESSİZCE GEÇMEZ — parity ölçülmediyse bunu bilmemiz gerekir.
fn samples() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/ikincigoz-core/tests/samples");
    assert!(
        p.is_dir(),
        "Referans fixture'lar bulunamadı: {}. Parity ölçülemez.",
        p.display()
    );
    p
}

/// Standalone uygulamanın varsayılan ayarlarla kurduğu bağlam.
///
/// `Settings::default()` -> `to_lint_options()` sonucu budur: kapalı kural yok,
/// inceleme katmanı açık.
fn baseline_findings(name: &str) -> Vec<Finding> {
    let path = samples().join(name);
    let bytes = std::fs::read(&path).expect("örnek okunmalı");
    let file_name = parser::base_name(&path.to_string_lossy());
    let document = parser::parse(&file_name, &bytes).expect("örnek parse edilmeli");
    let dictionary = UserDictionary::default();
    let options = LintOptions {
        include_review: true,
        ..LintOptions::default()
    };
    analyze(&Context::new(&document, &dictionary, None, &options)).findings
}

fn signature(f: &Finding) -> String {
    format!(
        "{}|{}|{:?}|{}|{:?}|{:?}|{}",
        f.rule_id,
        f.block_id,
        f.severity,
        f.location.block_index,
        f.location.char_start,
        f.location.char_end,
        f.fix
            .as_ref()
            .map(|x| format!(
                "{}→{}@{}..{}",
                x.original, x.replacement, x.char_start, x.char_end
            ))
            .unwrap_or_else(|| "-".into()),
    )
}

#[test]
fn the_faulty_docx_sample_reports_exactly_the_baseline_findings() {
    let findings = baseline_findings("ornek-dilekce-hatali.docx");

    // 2026-09-07'de standalone `lint` örneğinden alınan referans:
    // 11 kesin hata · 2 uyarı · 3 incele
    let errors = findings
        .iter()
        .filter(|f| format!("{:?}", f.severity) == "Error")
        .count();
    let warnings = findings
        .iter()
        .filter(|f| format!("{:?}", f.severity) == "Warning")
        .count();
    let reviews = findings
        .iter()
        .filter(|f| format!("{:?}", f.severity) == "Review")
        .count();
    assert_eq!(errors, 11, "kesin hata sayısı değişti");
    assert_eq!(warnings, 2, "uyarı sayısı değişti");
    assert_eq!(reviews, 3, "inceleme sayısı değişti");

    // Kural kimlikleri ve blokları, standalone çıktısıyla birebir.
    let ids: Vec<String> = findings
        .iter()
        .map(|f| format!("{}@{}", f.rule_id, f.block_id))
        .collect();
    // DİKKAT — bu değerler `lint` örneğinin ekran çıktısından KOPYALANAMAZ.
    // `examples/lint.rs` insan için `location.block_index + 1` yazdırır;
    // `block_id` ise `p{block_index}`tir. İkisi bir eksik/bir fazladır ve bu
    // fark, olmayan bir migration regresyonu uydurmanın en kolay yoludur.
    // Aşağıdakiler gerçek `block_id` değerleridir ve `examples/verify_writeback`
    // çıktısıyla (p4, p7) tutarlıdır.
    for expected in [
        "TYPO_MULTIPLE_SPACES@p6",
        "TEXT_WORD_DUPLICATION@p6",
        "TYPO_SPACE_BEFORE_PUNCTUATION@p7",
        "TYPO_MISSING_SPACE_AFTER_PUNCTUATION@p7",
        "CONSISTENCY_AMOUNT_WORDS@p11",
        "TYPO_DOUBLE_PUNCTUATION@p12",
        "ORTHO_SEPARATE_WRITING@p12",
        "SEC_SEQ_GAP@p15",
        "DRAFT_PLACEHOLDER@p19",
        "REF_ATTACHMENT_SEQ_GAP@p24",
        "REF_ATTACHMENT_MISSING@p25",
        "STYLE_HEADING_OUTLIER@p13",
        "DRAFT_DANGLING_CONNECTOR@p19",
        "CONSISTENCY_DATE_FORMAT@p9",
        "CONSISTENCY_CITATION_FORMAT@p17",
        "REF_ATTACHMENT_UNUSED@p24",
    ] {
        assert!(
            ids.contains(&expected.to_string()),
            "eksik bulgu: {expected}"
        );
    }
}

#[test]
fn the_block_id_is_zero_based_while_the_cli_prints_it_one_based() {
    // Bu sözleşmeyi teste gömüyoruz çünkü bu tur bizzat buna takıldı:
    // referans sayıları `lint` ekran çıktısından okumak sahte bir regresyon üretir.
    let findings = baseline_findings("ornek-dilekce.udf");
    for f in &findings {
        assert_eq!(
            f.block_id,
            format!("p{}", f.location.block_index),
            "block_id, block_index ile aynı tabanı kullanmalı"
        );
    }
}

#[test]
fn the_udf_sample_reports_exactly_the_baseline_findings() {
    let findings = baseline_findings("ornek-dilekce.udf");
    assert_eq!(findings.len(), 4, "bulgu sayısı değişti");
    let sigs: Vec<String> = findings.iter().map(signature).collect();
    // Türkçe codepoint offset'leri dâhil birebir eşleşme.
    // Standalone `verify_writeback` referansı: p4 [5..7] ve p7 [13..14].
    assert!(
        sigs.iter()
            .any(|s| s.starts_with("TYPO_MULTIPLE_SPACES|p4|Error|4|Some(5)|Some(7)|")),
        "{sigs:?}"
    );
    assert!(
        sigs.iter()
            .any(|s| s.starts_with("TYPO_SPACE_BEFORE_PUNCTUATION|p7|Error|7|Some(13)|")),
        "{sigs:?}"
    );
    assert!(
        sigs.iter()
            .any(|s| s.starts_with("CONSISTENCY_AMOUNT_WORDS|p8|")),
        "{sigs:?}"
    );
    assert!(
        sigs.iter().any(|s| s.starts_with("SEC_SEQ_GAP|p9|")),
        "{sigs:?}"
    );
}

#[test]
fn a_clean_document_produces_no_findings() {
    assert!(
        baseline_findings("ornek-dilekce-temiz.docx").is_empty(),
        "temiz belgede bulgu çıkmamalı"
    );
}

#[test]
fn writeback_matches_the_standalone_baseline_and_never_touches_the_source() {
    let path = samples().join("ornek-dilekce.udf");
    let bytes = std::fs::read(&path).unwrap();
    let before = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        h.finish()
    };
    let file_name = parser::base_name(&path.to_string_lossy());

    let fixes: Vec<_> = baseline_findings("ornek-dilekce.udf")
        .into_iter()
        .filter_map(|f| f.fix)
        .collect();

    // Standalone `verify_writeback` referansı: 2 düzeltme önerildi.
    assert_eq!(fixes.len(), 2, "önerilen düzeltme sayısı değişti");

    let result = writeback::apply(&file_name, &bytes, &fixes).expect("writeback uygulanmalı");
    assert_eq!(result.applied, 2, "uygulanan düzeltme sayısı değişti");

    // Kaynak baytları bellekte bile değişmedi; writeback kaynağı hiç açmaz.
    let after = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        h.finish()
    };
    assert_eq!(before, after);

    // Çıktı yeniden parse edilebilmeli ve net karakter değişimi -2 olmalı
    // (standalone referansı: "net character change: -2").
    let reparsed = parser::parse(&result.file_name, &result.bytes).expect("çıktı okunabilmeli");
    let original = parser::parse(&file_name, &bytes).unwrap();
    let delta = reparsed.metadata.char_count as i64 - original.metadata.char_count as i64;
    assert_eq!(
        delta, -2,
        "karakter değişimi standalone referansından farklı"
    );

    // Blok sayısı korunmalı: writeback yalnız metin düğümlerini yamalar.
    assert_eq!(
        reparsed.metadata.block_count, original.metadata.block_count,
        "writeback blok yapısını değiştirmemeli"
    );
}

#[test]
fn a_stale_fix_is_refused_rather_than_applied_at_the_wrong_offset() {
    let path = samples().join("ornek-dilekce.udf");
    let bytes = std::fs::read(&path).unwrap();
    let file_name = parser::base_name(&path.to_string_lossy());

    let mut fixes: Vec<_> = baseline_findings("ornek-dilekce.udf")
        .into_iter()
        .filter_map(|f| f.fix)
        .collect();
    // Belge o aralıkta bunu içermiyor: bayat bir değişiklik.
    fixes[0].original = "BU METİN ORADA YOK".into();

    assert!(
        writeback::apply(&file_name, &bytes, &fixes).is_err(),
        "bayat düzeltme reddedilmeliydi"
    );
}

#[test]
fn the_full_chain_writes_a_real_corrected_copy_that_reopens() {
    // §11 zinciri, diskte: girdi -> denetle -> beklenen bulgu -> düzeltme uygula
    // -> yeniden aç. Bellek içi writeback parity'sinden ayrıdır: burada gerçek
    // dosya yazılır ve gerçek dosyadan geri okunur.
    for name in ["ornek-dilekce.udf", "ornek-dilekce-hatali.docx"] {
        let source = samples().join(name);
        let original = std::fs::read(&source).expect("örnek okunmalı");
        let file_name = parser::base_name(&source.to_string_lossy());

        let fixes: Vec<_> = baseline_findings(name)
            .into_iter()
            .filter_map(|f| f.fix)
            .collect();
        assert!(
            !fixes.is_empty(),
            "{name}: düzeltilebilir bulgu bekleniyordu"
        );

        let result = writeback::apply(&file_name, &original, &fixes)
            .unwrap_or_else(|e| panic!("{name}: writeback reddedildi: {e:?}"));

        // Kopya geçici bir dizine yazılır; kaynak klasöre hiç dokunulmaz.
        let out = std::env::temp_dir().join(format!(
            "belge-ig-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&out).unwrap();
        let target = out.join(&result.file_name);
        assert_ne!(target, source, "kopya kaynağın üzerine yazamaz");
        std::fs::write(&target, &result.bytes).unwrap();

        // Gerçekten diskte, boş değil, yeniden açılabiliyor.
        let written = std::fs::read(&target).expect("kopya diskte olmalı");
        assert!(!written.is_empty());
        let reopened = parser::parse(&result.file_name, &written)
            .unwrap_or_else(|e| panic!("{name}: kopya yeniden açılamadı: {e:?}"));
        let before = parser::parse(&file_name, &original).unwrap();

        // Yalnız beklenen değişiklik: blok sayısı ve blok kimlikleri korunur.
        assert_eq!(
            reopened.metadata.block_count, before.metadata.block_count,
            "{name}: writeback blok yapısını değiştirdi"
        );
        let ids_before: Vec<_> = before.blocks.iter().map(|b| b.id.clone()).collect();
        let ids_after: Vec<_> = reopened.blocks.iter().map(|b| b.id.clone()).collect();
        assert_eq!(ids_before, ids_after, "{name}: blok kimlikleri değişti");

        // Dokunulmayan bloklar birebir aynı kalmalı: başka içerik kaybı yok.
        let touched: std::collections::HashSet<_> =
            fixes.iter().map(|f| f.block_id.as_str()).collect();
        for (a, b) in before.blocks.iter().zip(reopened.blocks.iter()) {
            if !touched.contains(a.id.as_str()) {
                assert_eq!(
                    a.text, b.text,
                    "{name}: {} bloğu beklenmedik şekilde değişti",
                    a.id
                );
            }
        }

        // Kaynak dosya diskte de değişmedi.
        assert_eq!(
            original,
            std::fs::read(&source).unwrap(),
            "{name}: kaynak dosya değişti"
        );

        std::fs::remove_dir_all(&out).ok();
    }
}
