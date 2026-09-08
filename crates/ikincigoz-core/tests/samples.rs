//! End-to-end run over the sample documents in `samples/`.
//!
//! These are the files a user opens first, so they are also the ones that must
//! never regress. The test walks the whole pipeline — read, parse, analyse,
//! correct, verify — against real files on disk rather than fixtures built in
//! memory, which is the only way to catch a packaging mistake in the sample
//! generator itself.

use ikincigoz_core::dict::UserDictionary;
use ikincigoz_core::finding::Severity;
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};
use ikincigoz_core::writeback;
use std::collections::BTreeSet;
use std::path::PathBuf;

// Fikstürler crate'in içinde: bu crate birleşik workspace'e taşınırken
// kendi kendine yeter hâle getirildi. Eskiden workspace kökündeki `qa/`
// altındaydı ve bağımsız DüzenEk deposunun varlığına bağlıydı.
fn samples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples")
}

fn run(
    name: &str,
) -> (
    ikincigoz_core::cdm::Document,
    Vec<u8>,
    Vec<ikincigoz_core::finding::Finding>,
) {
    let path = samples_dir().join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    let document = parser::parse(name, &bytes).unwrap_or_else(|e| panic!("{name}: {}", e.code()));
    let dictionary = UserDictionary::default();
    let options = LintOptions::default();
    let ctx = Context::new(&document, &dictionary, None, &options);
    let findings = analyze(&ctx).findings;
    (document, bytes, findings)
}

#[test]
fn the_flawed_sample_reproduces_the_defects_it_was_written_to_contain() {
    let (_, _, findings) = run("ornek-dilekce-hatali.docx");
    let fired: BTreeSet<&str> = findings.iter().map(|f| f.rule_id.as_str()).collect();

    // Each of these is a defect deliberately planted in the sample. If one
    // stops firing, either the rule or the sample has drifted.
    for expected in [
        "TEXT_WORD_DUPLICATION",
        "TYPO_MULTIPLE_SPACES",
        "TYPO_SPACE_BEFORE_PUNCTUATION",
        "TYPO_MISSING_SPACE_AFTER_PUNCTUATION",
        "TYPO_DOUBLE_PUNCTUATION",
        "ORTHO_SEPARATE_WRITING",
        "SEC_SEQ_GAP",
        "REF_ATTACHMENT_SEQ_GAP",
        "REF_ATTACHMENT_MISSING",
        "DRAFT_PLACEHOLDER",
        "DRAFT_DANGLING_CONNECTOR",
        "CONSISTENCY_AMOUNT_WORDS",
        "CONSISTENCY_CITATION_FORMAT",
        "CONSISTENCY_DATE_FORMAT",
    ] {
        assert!(
            fired.contains(expected),
            "{expected} did not fire; fired: {fired:?}"
        );
    }

    assert!(findings.iter().any(|f| f.severity == Severity::Error));
}

#[test]
fn the_clean_sample_produces_no_errors_or_warnings() {
    let (_, _, findings) = run("ornek-dilekce-temiz.docx");
    let noisy: Vec<&ikincigoz_core::finding::Finding> = findings
        .iter()
        .filter(|f| f.severity != Severity::Review)
        .collect();
    assert!(
        noisy.is_empty(),
        "the clean sample produced {:?}",
        noisy
            .iter()
            .map(|f| (&f.rule_id, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn the_udf_sample_parses_with_structure_and_finds_its_planted_defects() {
    let (document, _, findings) = run("ornek-dilekce.udf");
    assert_eq!(document.format, ikincigoz_core::cdm::SourceFormat::Udf);
    assert!(document.blocks.len() >= 10);
    // Formatting must survive the UDF spans, not just the text.
    assert!(document
        .blocks
        .iter()
        .any(|b| b.runs.iter().any(|r| r.font_family.is_some())));
    assert!(document.blocks[0].text.contains("İZMİR"));

    let fired: BTreeSet<&str> = findings.iter().map(|f| f.rule_id.as_str()).collect();
    for expected in [
        "TYPO_MULTIPLE_SPACES",
        "SEC_SEQ_GAP",
        "CONSISTENCY_AMOUNT_WORDS",
    ] {
        assert!(
            fired.contains(expected),
            "{expected} did not fire; fired: {fired:?}"
        );
    }
}

/// The layout-heavy sample: a tabbed label column, an outline, a quoted
/// judgment with anonymised initials, and an academic initialism — with exactly
/// three defects planted in the prose.
///
/// This is the shape of the real petition that exposed the precision defects,
/// rebuilt with invented parties so it can live in the repository.
#[test]
fn the_layout_sample_reports_its_three_planted_defects_and_nothing_else() {
    let (_, _, findings) = run("ornek-dilekce-duzenli.udf");
    let fired: BTreeSet<&str> = findings.iter().map(|f| f.rule_id.as_str()).collect();
    assert_eq!(
        fired,
        [
            "TYPO_DOUBLE_PUNCTUATION",
            "TYPO_MISSING_SPACE_AFTER_PUNCTUATION",
            "TYPO_MULTIPLE_SPACES"
        ]
        .into_iter()
        .collect::<BTreeSet<&str>>(),
        "unexpected findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule_id, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(findings.len(), 3);
}

#[test]
fn the_layout_sample_keeps_its_structure_through_a_write_back() {
    let name = "ornek-dilekce-duzenli.udf";
    let (_, bytes, findings) = run(name);
    let chosen: Vec<ikincigoz_core::finding::Fix> =
        findings.iter().filter_map(|f| f.fix.clone()).collect();
    assert_eq!(chosen.len(), 3);

    let before = writeback::container_text_image(ikincigoz_core::cdm::SourceFormat::Udf, &bytes)
        .expect("image");
    let out = writeback::apply(name, &bytes, &chosen).unwrap_or_else(|e| panic!("{}", e.code()));
    let after = writeback::container_text_image(ikincigoz_core::cdm::SourceFormat::Udf, &out.bytes)
        .expect("image");

    // Every tab survives: the label column, the continuation rows, the outline.
    assert_eq!(
        before.matches('\t').count(),
        after.matches('\t').count(),
        "the alignment tabs did not survive the write-back"
    );
    for protected in [
        "DAVACI\t\t\t:\t",
        "VEKİLİ\t\t\t: \t",
        "HUKUKİ NEDENLER\t\t\t\t:",
        "SONUÇ VE İSTEM\t\t\t\t\t:",
        "Y.. Ö..'ın",
        "LL.M.",
        "TCKN.:",
        "1.\tDavamızın KABULÜ ile,",
    ] {
        assert!(after.contains(protected), "{protected:?} did not survive");
    }

    // And exactly the three approved changes, nothing else.
    let expected = before
        .replace("devraldığını  ortaya", "devraldığını ortaya")
        .replace("yaştadır.İki", "yaştadır. İki")
        .replace("açıktır..", "açıktır.");
    assert_eq!(
        expected, after,
        "a character moved outside the approved changes"
    );
}

#[test]
fn every_sample_survives_having_all_of_its_fixes_applied() {
    for name in [
        "ornek-dilekce-hatali.docx",
        "ornek-dilekce.udf",
        "ornek-dilekce-duzenli.udf",
    ] {
        let (document, bytes, findings) = run(name);

        // Take a non-overlapping set, the way the correction panel does.
        let mut fixes: Vec<ikincigoz_core::finding::Fix> =
            findings.iter().filter_map(|f| f.fix.clone()).collect();
        fixes.sort_by(|a, b| {
            (a.block_id.as_str(), a.char_start, a.char_end).cmp(&(
                b.block_id.as_str(),
                b.char_start,
                b.char_end,
            ))
        });
        let mut chosen: Vec<ikincigoz_core::finding::Fix> = Vec::new();
        for fix in fixes {
            let clash = chosen.last().is_some_and(|prev| {
                prev.block_id == fix.block_id
                    && (prev.char_end > fix.char_start
                        || (prev.char_start == fix.char_start && prev.char_end == fix.char_end))
            });
            if !clash {
                chosen.push(fix);
            }
        }
        assert!(!chosen.is_empty(), "{name}: no fixes to apply");

        let out = writeback::apply(name, &bytes, &chosen)
            .unwrap_or_else(|e| panic!("{name}: {}", e.code()));
        assert_eq!(out.applied, chosen.len());
        assert!(out.file_name.contains("İkinciGöz"));

        // The corrected copy must open, keep its structure, and hold fewer
        // findings than it started with.
        let corrected = parser::parse(name, &out.bytes)
            .unwrap_or_else(|e| panic!("{name}: corrected copy did not open ({})", e.code()));
        assert_eq!(corrected.blocks.len(), document.blocks.len());

        let dictionary = UserDictionary::default();
        let options = LintOptions::default();
        let ctx = Context::new(&corrected, &dictionary, None, &options);
        let after = analyze(&ctx).findings;
        assert!(
            after.len() < findings.len(),
            "{name}: corrections did not reduce the finding count ({} -> {})",
            findings.len(),
            after.len()
        );
    }
}

#[test]
fn the_original_sample_files_are_never_written_to() {
    // The write-back API takes bytes and returns bytes; it has no path to write
    // to by construction. This test pins that property against a real file.
    for name in ["ornek-dilekce-hatali.docx", "ornek-dilekce.udf"] {
        let path = samples_dir().join(name);
        let before = std::fs::read(&path).unwrap();
        let (_, bytes, findings) = run(name);
        let fix = findings.iter().find_map(|f| f.fix.clone()).expect("a fix");
        let _ = writeback::apply(name, &bytes, std::slice::from_ref(&fix)).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "{name} was modified on disk"
        );
    }
}

#[test]
fn a_document_profile_derived_from_the_clean_sample_is_usable() {
    let path = samples_dir().join("ornek-dilekce-temiz.docx");
    let bytes = std::fs::read(&path).unwrap();
    let document = parser::parse("ornek-dilekce-temiz.docx", &bytes).unwrap();
    let profile = ikincigoz_core::profile::DocumentProfile::derive("Dava Dilekçesi", &document);
    assert!(profile.is_usable(), "profile: {profile:?}");
    assert_eq!(profile.body.font_family.as_deref(), Some("Times New Roman"));
}
