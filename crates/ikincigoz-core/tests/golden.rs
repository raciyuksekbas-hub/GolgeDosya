//! Golden corpus.
//!
//! Each case is a synthetic document plus the findings it must produce. The
//! documents contain no real party names, no case numbers and no personal data
//! of any kind; they are written to exercise one rule each.
//!
//! A case declares `exact: true` when the listed findings are the *only* ones
//! allowed. Most cases do, because a rule that quietly starts firing on a
//! clean fixture is exactly the regression this corpus exists to catch.

use ikincigoz_core::dict::{CorrectionRule, UserDictionary};
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

// Fikstürler crate'in içinde: bu crate birleşik workspace'e taşınırken
// kendi kendine yeter hâle getirildi. Eskiden workspace kökündeki `qa/`
// altındaydı ve bağımsız DüzenEk deposunun varlığına bağlıydı.
fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Wrap `w:body` inner XML into a complete DOCX package.
fn build_docx(body: &str) -> Vec<u8> {
    let document =
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {NS}><w:body>{body}</w:body></w:document>");
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buffer);
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("[Content_Types].xml", o).unwrap();
        w.write_all(b"<Types/>").unwrap();
        w.start_file("word/styles.xml", o).unwrap();
        w.write_all(format!("<w:styles {NS}/>").as_bytes()).unwrap();
        w.start_file("word/document.xml", o).unwrap();
        w.write_all(document.as_bytes()).unwrap();
        w.finish().unwrap();
    }
    buffer.into_inner()
}

fn build_udf(content_xml: &str) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buffer);
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("content.xml", o).unwrap();
        w.write_all(content_xml.as_bytes()).unwrap();
        w.finish().unwrap();
    }
    buffer.into_inner()
}

/// One paragraph per line, as a plain DOCX.
fn build_from_text(text: &str) -> Vec<u8> {
    let body: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .map(|line| {
            format!(
                "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                escape(line)
            )
        })
        .collect();
    build_docx(&body)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Debug)]
struct Expectation {
    counts: BTreeMap<String, usize>,
    exact: bool,
}

/// The expectation file is deliberately a flat, hand-editable shape:
/// `{"exact": true, "findings": {"SEC_SEQ_GAP": 1}}`.
fn read_expectation(path: &Path) -> Expectation {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let value: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));
    let exact = value.get("exact").and_then(|v| v.as_bool()).unwrap_or(true);
    let mut counts = BTreeMap::new();
    if let Some(map) = value.get("findings").and_then(|v| v.as_object()) {
        for (k, v) in map {
            counts.insert(
                k.clone(),
                v.as_u64().unwrap_or_else(|| panic!("{k} must be a number")) as usize,
            );
        }
    }
    Expectation { counts, exact }
}

fn read_dictionary(path: &Path) -> UserDictionary {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return UserDictionary::default();
    };
    let value: serde_json::Value = serde_json::from_str(&raw).expect("dictionary JSON");
    let mut dict = UserDictionary::default();
    if let Some(list) = value.get("accepted").and_then(|v| v.as_array()) {
        for w in list.iter().filter_map(|v| v.as_str()) {
            dict.accept(w);
        }
    }
    if let Some(list) = value.get("corrections").and_then(|v| v.as_array()) {
        for c in list {
            let from = c.get("from").and_then(|v| v.as_str()).expect("from");
            let to = c.get("to").and_then(|v| v.as_str()).expect("to");
            let inflect = c.get("inflect").and_then(|v| v.as_bool()).unwrap_or(true);
            dict.add_correction(CorrectionRule {
                from: from.into(),
                to: to.into(),
                inflect,
                note: None,
            })
            .expect("valid correction");
        }
    }
    dict
}

struct Case {
    name: String,
    file_name: String,
    bytes: Vec<u8>,
    expectation: Expectation,
    dictionary: UserDictionary,
}

fn collect_cases() -> Vec<Case> {
    let root = corpus_root();
    let mut cases = Vec::new();
    let mut categories: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("cannot read corpus at {}: {e}", root.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    categories.sort();

    for category in categories {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&category)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        entries.sort();
        for path in entries {
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            let (stem, bytes, file_name) = if let Some(s) = file.strip_suffix(".docx.xml") {
                (
                    s.to_string(),
                    build_docx(&std::fs::read_to_string(&path).unwrap()),
                    format!("{s}.docx"),
                )
            } else if let Some(s) = file.strip_suffix(".udf.xml") {
                (
                    s.to_string(),
                    build_udf(&std::fs::read_to_string(&path).unwrap()),
                    format!("{s}.udf"),
                )
            } else if let Some(s) = file.strip_suffix(".txt") {
                (
                    s.to_string(),
                    build_from_text(&std::fs::read_to_string(&path).unwrap()),
                    format!("{s}.docx"),
                )
            } else {
                continue;
            };
            let expected_path = category.join(format!("{stem}.expected.json"));
            assert!(
                expected_path.exists(),
                "fixture {} has no expectation file at {}",
                path.display(),
                expected_path.display()
            );
            cases.push(Case {
                name: format!("{}/{stem}", category.file_name().unwrap().to_string_lossy()),
                file_name,
                bytes,
                expectation: read_expectation(&expected_path),
                dictionary: read_dictionary(&category.join(format!("{stem}.dict.json"))),
            });
        }
    }
    cases
}

#[test]
fn the_corpus_is_not_empty() {
    let cases = collect_cases();
    assert!(
        cases.len() >= 15,
        "expected a real corpus, found {} cases",
        cases.len()
    );
}

#[test]
fn every_case_produces_exactly_the_expected_findings() {
    let mut failures: Vec<String> = Vec::new();

    for case in collect_cases() {
        let document = match parser::parse(&case.file_name, &case.bytes) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!(
                    "{}: document did not parse ({})",
                    case.name,
                    e.code()
                ));
                continue;
            }
        };
        let options = LintOptions::default();
        let ctx = Context::new(&document, &case.dictionary, None, &options);
        let result = analyze(&ctx);

        let mut actual: BTreeMap<String, usize> = BTreeMap::new();
        for f in &result.findings {
            *actual.entry(f.rule_id.clone()).or_default() += 1;
        }

        for (rule, expected) in &case.expectation.counts {
            let got = actual.get(rule).copied().unwrap_or(0);
            if got != *expected {
                failures.push(format!(
                    "{}: {rule} expected {expected}, got {got}",
                    case.name
                ));
            }
        }
        if case.expectation.exact {
            for (rule, got) in &actual {
                if !case.expectation.counts.contains_key(rule) {
                    let sample = result
                        .findings
                        .iter()
                        .find(|f| &f.rule_id == rule)
                        .map(|f| f.message.clone())
                        .unwrap_or_default();
                    failures.push(format!(
                        "{}: unexpected {rule} x{got} -> {sample}",
                        case.name
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "golden corpus mismatches:\n  {}",
        failures.join("\n  ")
    );
}

/// A finding must point at the text it claims to.
///
/// Two things can drift apart here and both are invisible until someone acts on
/// a correction: the card can quote one place while the fix edits another, and
/// a fix can carry an `original` that no longer matches the document. The first
/// misleads; the second is what the write-back gate refuses, so it should never
/// reach the gate in the first place.
#[test]
fn every_finding_points_at_the_text_it_claims_to() {
    for case in collect_cases() {
        let Ok(document) = parser::parse(&case.file_name, &case.bytes) else {
            continue;
        };
        let options = LintOptions::default();
        let ctx = Context::new(&document, &case.dictionary, None, &options);

        for f in analyze(&ctx).findings {
            let block = document.block(&f.block_id).unwrap_or_else(|| {
                panic!(
                    "{}: {} names a block that does not exist",
                    case.name, f.rule_id
                )
            });

            // The reported location must lie inside the block it names.
            if let (Some(start), Some(end)) = (f.location.char_start, f.location.char_end) {
                assert!(
                    start <= end && end <= block.char_len(),
                    "{}: {} reports [{start}..{end}] in a block of {} characters",
                    case.name,
                    f.rule_id,
                    block.char_len()
                );
                assert_eq!(
                    f.location.block_index, block.source_location.block_index,
                    "{}: {} disagrees with itself about which block it is in",
                    case.name, f.rule_id
                );
            }

            // A fix must quote exactly the characters it is going to replace.
            if let Some(fix) = &f.fix {
                let actual =
                    ikincigoz_core::cdm::char_slice(&block.text, fix.char_start, fix.char_end);
                assert_eq!(
                    actual, fix.original,
                    "{}: {} would replace {:?} but claims to be replacing {:?}",
                    case.name, f.rule_id, actual, fix.original
                );
                assert_eq!(
                    fix.block_id, f.block_id,
                    "{}: {} points the fix at a different block from the finding",
                    case.name, f.rule_id
                );
            }
        }
    }
}

/// Every fix the corpus produces must apply cleanly and survive verification.
///
/// This is the write-back gate: it runs the real correction path over every
/// document in the corpus, so a change that would corrupt a container fails the
/// build rather than a user's petition.
#[test]
fn every_offered_fix_applies_and_verifies() {
    for case in collect_cases() {
        let Ok(document) = parser::parse(&case.file_name, &case.bytes) else {
            continue;
        };
        let options = LintOptions::default();
        let ctx = Context::new(&document, &case.dictionary, None, &options);
        let findings = analyze(&ctx).findings;

        for f in findings.iter().filter(|f| f.fix.is_some()) {
            let fix = f.fix.clone().unwrap();
            let result = ikincigoz_core::writeback::apply(
                &case.file_name,
                &case.bytes,
                std::slice::from_ref(&fix),
            );
            match result {
                Ok(out) => {
                    let back = parser::parse(&case.file_name, &out.bytes).unwrap_or_else(|e| {
                        panic!("{}: output did not reparse ({})", case.name, e.code())
                    });
                    assert_eq!(
                        back.blocks.len(),
                        document.blocks.len(),
                        "{}: {} changed the paragraph count",
                        case.name,
                        f.rule_id
                    );
                }
                Err(e) => panic!(
                    "{}: fix from {} failed to apply ({})",
                    case.name,
                    f.rule_id,
                    e.code()
                ),
            }
        }
    }
}

/// All fixes from one document, applied together.
#[test]
fn all_fixes_from_a_document_can_be_applied_in_one_pass() {
    for case in collect_cases() {
        let Ok(document) = parser::parse(&case.file_name, &case.bytes) else {
            continue;
        };
        let options = LintOptions::default();
        let ctx = Context::new(&document, &case.dictionary, None, &options);
        let findings = analyze(&ctx).findings;

        // Keep a non-overlapping subset, the way the correction panel does.
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
            let clashes = chosen.last().map(|prev| {
                prev.block_id == fix.block_id
                    && (prev.char_end > fix.char_start
                        || (prev.char_start == fix.char_start && prev.char_end == fix.char_end))
            });
            if clashes == Some(true) {
                continue;
            }
            chosen.push(fix);
        }
        if chosen.is_empty() {
            continue;
        }
        let out = ikincigoz_core::writeback::apply(&case.file_name, &case.bytes, &chosen)
            .unwrap_or_else(|e| panic!("{}: batch apply failed ({})", case.name, e.code()));
        assert_eq!(
            out.applied,
            chosen.len(),
            "{}: not every change was applied",
            case.name
        );
    }
}
