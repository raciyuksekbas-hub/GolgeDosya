//! Regression tests for the precision and correction-safety hardening.
//!
//! Every case here comes from a real petition that İkinciGöz got wrong. The
//! document itself is not in the repository — it names living people — so each
//! defect is reproduced as the smallest synthetic document that exhibits it,
//! with the party names replaced.
//!
//! The tests are grouped the way the failures were: what the engine says
//! (findings), and what the engine does (corrections). The second group is the
//! important one. A wrong finding wastes a minute; a correction that changes a
//! character nobody approved damages a document that is about to be filed.

use ikincigoz_core::dict::UserDictionary;
use ikincigoz_core::finding::{Finding, Fix, Severity};
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};
use ikincigoz_core::writeback::{self, container_text_image, WriteBackError};
use std::io::{Cursor, Write};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Build a UDF whose paragraphs are the given lines, with contiguous spans.
fn udf(lines: &[&str]) -> Vec<u8> {
    let blob: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let mut elements = String::new();
    let mut offset = 0usize;
    for line in lines {
        let length: usize = line.chars().map(|c| c.len_utf16()).sum::<usize>() + 1;
        elements.push_str(&format!(
            "<paragraph Alignment=\"3\"><content startOffset=\"{offset}\" length=\"{length}\" \
             family=\"Times New Roman\" size=\"12\"/></paragraph>"
        ));
        offset += length;
    }
    package(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<template format_id=\"1.8\">\
         <content><![CDATA[{blob}]]></content>\
         <styles><style name=\"default\" family=\"Times New Roman\" size=\"12\"/></styles>\
         <elements resolver=\"hvl-default\">{elements}</elements></template>"
    ))
}

/// Build a UDF whose paragraph carries several spans with gaps between them,
/// the way UYAP does when only part of a paragraph has explicit formatting.
fn udf_with_gaps(blob: &str, spans: &[(usize, usize)]) -> Vec<u8> {
    let contents: String = spans
        .iter()
        .map(|(s, l)| format!("<content startOffset=\"{s}\" length=\"{l}\"/>"))
        .collect();
    let elements = format!("<paragraph>{contents}</paragraph>");
    package(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><template format_id=\"1.8\">\
         <content><![CDATA[{blob}]]></content>\
         <elements resolver=\"hvl-default\">{elements}</elements></template>"
    ))
}

fn package(content_xml: &str) -> Vec<u8> {
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

fn findings_of(bytes: &[u8]) -> Vec<Finding> {
    let doc = parser::parse("dava.udf", bytes).expect("parse");
    let dict = UserDictionary::default();
    let opts = LintOptions {
        max_per_rule: 500,
        ..LintOptions::default()
    };
    analyze(&Context::new(&doc, &dict, None, &opts)).findings
}

fn lint(lines: &[&str]) -> Vec<Finding> {
    findings_of(&udf(lines))
}

fn of<'a>(findings: &'a [Finding], rule: &str) -> Vec<&'a Finding> {
    findings.iter().filter(|f| f.rule_id == rule).collect()
}

fn assert_silent(lines: &[&str], rule: &str) {
    let f = lint(lines);
    let got = of(&f, rule);
    assert!(
        got.is_empty(),
        "{rule} should not fire on {lines:?}, got {:?}",
        got.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
}

/// The non-overlapping subset of corrections, exactly as the panel builds it.
fn choose(findings: &[Finding]) -> Vec<Fix> {
    let mut fixes: Vec<Fix> = findings.iter().filter_map(|f| f.fix.clone()).collect();
    fixes.sort_by(|a, b| {
        (a.block_id.as_str(), a.char_start, a.char_end).cmp(&(
            b.block_id.as_str(),
            b.char_start,
            b.char_end,
        ))
    });
    let mut chosen: Vec<Fix> = Vec::new();
    for fix in fixes {
        let clash = chosen.last().is_some_and(|p| {
            p.block_id == fix.block_id
                && (p.char_end > fix.char_start
                    || (p.char_start == fix.char_start && p.char_end == fix.char_end))
        });
        if !clash {
            chosen.push(fix);
        }
    }
    chosen
}

// ===========================================================================
// A — layout tabs
// ===========================================================================

#[test]
fn a_the_label_column_of_a_petition_produces_no_tab_finding_and_no_correction() {
    let lines = [
        "DAVACI\t\t\t:\tProf. Dr. Aylin YILMAZ",
        "\t\t\t\t\tÖrnek Mah. Deneme Sk. No: 1 Kadıköy / İSTANBUL",
        "VEKİLİ\t\t\t: \tAv. Deniz KAYA, LL.M.",
        "DAVALI\t\t\t:\tMert DEMİR",
        "KONU\t\t\t:\tKira sözleşmesinin feshi istemidir.",
        "DAVA DEĞERİ\t:\t31.656,00-TL",
        "HUKUKİ NEDENLER\t\t\t\t: TBK, TMK, HMK ve sair mevzuat.",
        "SONUÇ VE İSTEM\t\t\t\t\t: Yukarıda açıklanan nedenlerle,",
    ];
    assert_silent(&lines, "TYPO_TAB_IN_TEXT");
    assert_silent(&lines, "TYPO_SPACE_BEFORE_PUNCTUATION");
    assert_silent(&lines, "TYPO_MULTIPLE_SPACES");

    // And nothing in the header is offered as a correction.
    let bytes = udf(&lines);
    assert!(
        choose(&findings_of(&bytes)).is_empty(),
        "the header column must never be offered for correction"
    );
}

#[test]
fn a_the_tab_after_an_item_marker_is_left_alone() {
    for line in [
        "1.\tMüvekkil beyanda bulunmuş ve talebini yinelemiştir.",
        "A.\tUSULE İLİŞKİN AÇIKLAMALAR",
        "7/b.\tTemizliği ve Düzeni Kolaydır: daire tek başına kullanılabilir.",
    ] {
        assert_silent(&[line], "TYPO_TAB_IN_TEXT");
    }
}

// ===========================================================================
// B — a tab in running prose
// ===========================================================================

#[test]
fn b_a_tab_in_the_middle_of_a_sentence_is_reported() {
    let f = lint(&["Davacı bu nedenle\tmahkemeye başvurmuştur."]);
    let tabs = of(&f, "TYPO_TAB_IN_TEXT");
    assert_eq!(tabs.len(), 1, "{tabs:?}");
    assert_eq!(tabs[0].severity, Severity::Warning);
    assert!(tabs[0].fix.is_some(), "a prose tab is safe to correct");
}

// ===========================================================================
// C, D — multiple spaces, with the exact range
// ===========================================================================

#[test]
fn c_a_doubled_space_is_reported_at_exactly_the_two_characters() {
    let line = "sağlık riski oluşturmaktadır.  somut olayda müvekkilin ayağı kırılmıştır.";
    let f = lint(&[line]);
    let spaces = of(&f, "TYPO_MULTIPLE_SPACES");
    assert_eq!(spaces.len(), 1);

    let start = spaces[0].location.char_start.unwrap();
    let end = spaces[0].location.char_end.unwrap();
    assert_eq!(
        end - start,
        2,
        "the range must cover the two spaces and nothing else"
    );
    let covered: String = line.chars().skip(start).take(end - start).collect();
    assert_eq!(covered, "  ");
}

#[test]
fn d_a_doubled_space_after_a_comma_is_reported_at_exactly_the_two_characters() {
    let line = "art arda 3 kapıyı (müstakil binanın iç çelik kapısı,  cam kış bahçesi kapısı)";
    let f = lint(&[line]);
    let spaces = of(&f, "TYPO_MULTIPLE_SPACES");
    assert_eq!(spaces.len(), 1);

    let start = spaces[0].location.char_start.unwrap();
    let end = spaces[0].location.char_end.unwrap();
    let covered: String = line.chars().skip(start).take(end - start).collect();
    assert_eq!(covered, "  ");
}

#[test]
fn cd_the_context_shows_the_invisible_characters_it_is_reporting() {
    // A snippet that collapses whitespace hides the very defect it is printed
    // to show, which is what made these findings look like false alarms.
    let f = lint(&["sağlık riski oluşturmaktadır.  somut olayda müvekkil düşmüştür."]);
    let context = of(&f, "TYPO_MULTIPLE_SPACES")[0].context.clone().unwrap();
    assert!(
        context.contains("\u{00B7}\u{00B7}"),
        "the two spaces must be visible in the context: {context:?}"
    );
}

// ===========================================================================
// E, F — anonymised initials and quoted judgments
// ===========================================================================

#[test]
fn e_anonymised_initials_are_not_doubled_punctuation() {
    assert_silent(
        &["davacı, oğlu Y.. Ö..'ın konut ihtiyacı nedeniyle dava açmıştır."],
        "TYPO_DOUBLE_PUNCTUATION",
    );
}

#[test]
fn e_anonymised_initials_are_never_offered_as_a_correction() {
    let bytes = udf(&["davacı, oğlu Y.. Ö..'ın konut ihtiyacı nedeniyle dava açmıştır."]);
    assert!(choose(&findings_of(&bytes)).is_empty());
}

#[test]
fn f_a_quoted_judgment_is_protected_from_punctuation_corrections() {
    let line = "Yargıtay, \"davacı, oğlu Y.. Ö..'ın konut ihtiyacı nedeniyle dava açmıştır.\" \
                şeklinde içtihat etmiştir.";
    let f = lint(&[line]);
    assert!(of(&f, "TYPO_DOUBLE_PUNCTUATION").is_empty());
    // Whatever else is noticed inside the quotation, none of it is correctable.
    for finding in &f {
        let start = finding.location.char_start.unwrap_or(0);
        let quote_open = line.chars().position(|c| c == '"').unwrap();
        let quote_close = line
            .chars()
            .enumerate()
            .filter(|(_, c)| *c == '"')
            .nth(1)
            .unwrap()
            .0;
        if start > quote_open && start < quote_close {
            assert!(
                finding.fix.is_none(),
                "{} offered a correction inside a quotation",
                finding.rule_id
            );
            assert_eq!(finding.severity, Severity::Review);
        }
    }
}

#[test]
fn f_a_real_defect_outside_the_quotation_is_still_reported() {
    // The protection is scoped to the quotation, not to the paragraph.
    let f = lint(&["Yargıtay, \"kiraya veren tahliye isteyebilir.\" demiştir.."]);
    assert_eq!(of(&f, "TYPO_DOUBLE_PUNCTUATION").len(), 1);
}

// ===========================================================================
// G, H — abbreviations and initialisms
// ===========================================================================

#[test]
fn g_an_academic_initialism_is_not_split_by_a_spacing_correction() {
    let lines = ["Av. Deniz KAYA, LL.M.", "Davacı Vekili olarak sunulmuştur."];
    assert_silent(&lines, "TYPO_MISSING_SPACE_AFTER_PUNCTUATION");
    assert_silent(&lines, "TYPO_DOUBLE_PUNCTUATION");

    // And the token survives the write-back untouched.
    let bytes = udf(&lines);
    let chosen = choose(&findings_of(&bytes));
    if !chosen.is_empty() {
        let out = writeback::apply("dava.udf", &bytes, &chosen).expect("apply");
        let image =
            container_text_image(ikincigoz_core::cdm::SourceFormat::Udf, &out.bytes).unwrap();
        assert!(image.contains("LL.M."), "LL.M. was altered");
    }
}

#[test]
fn g_every_initialism_shape_is_protected() {
    for token in ["LL.M.", "Ph.D.", "T.C.", "A.Ş."] {
        assert_silent(
            &[&format!(
                "Bu belge {token} tarafından düzenlenmiş bulunmaktadır."
            )],
            "TYPO_MISSING_SPACE_AFTER_PUNCTUATION",
        );
    }
}

#[test]
fn h_a_title_before_a_name_produces_no_spacing_finding() {
    assert_silent(
        &["Prof. Dr. Aylin YILMAZ, 1958 doğumlu olup ileri yaştadır."],
        "TYPO_MISSING_SPACE_AFTER_PUNCTUATION",
    );
    assert_silent(
        &["Prof. Dr. Aylin YILMAZ, 1958 doğumlu olup ileri yaştadır."],
        "TYPO_DOUBLE_PUNCTUATION",
    );
}

#[test]
fn h_an_acronym_followed_by_a_colon_is_not_doubled_punctuation() {
    assert_silent(
        &["Aylin YILMAZ (TCKN.: 11111111111) adına dilekçe sunulmuştur."],
        "TYPO_DOUBLE_PUNCTUATION",
    );
}

// ===========================================================================
// I — a real doubled full stop
// ===========================================================================

#[test]
fn i_a_genuinely_doubled_full_stop_is_still_reported_and_corrected() {
    let f = lint(&["Davanın kabulünü saygıyla talep ederiz.."]);
    let doubled = of(&f, "TYPO_DOUBLE_PUNCTUATION");
    assert_eq!(doubled.len(), 1);
    assert_eq!(doubled[0].severity, Severity::Error);
    let fix = doubled[0].fix.clone().expect("a fix");
    assert_eq!(fix.original, "..");
    assert_eq!(fix.replacement, ".");
}

#[test]
fn i_a_missing_space_after_a_full_stop_is_still_reported() {
    let f = lint(&["oturmaya zorlanması da düşünülemez.Davacı ihtiyaçlının beyanı alınmalıdır."]);
    assert_eq!(of(&f, "TYPO_MISSING_SPACE_AFTER_PUNCTUATION").len(), 1);
}

// ===========================================================================
// J, K — correction safety
// ===========================================================================

#[test]
fn j_several_corrections_at_different_positions_all_land_and_nothing_else_moves() {
    let lines = [
        "Birinci paragrafta  çift boşluk bulunmaktadır burada.",
        "İkinci paragrafta da  çift boşluk bulunmaktadır burada.",
        "Üçüncü paragrafta  yine çift boşluk bulunmaktadır burada.",
        "Dördüncü paragraf hiç değişmemelidir ve aynen kalmalıdır.",
    ];
    let bytes = udf(&lines);
    let before = container_text_image(ikincigoz_core::cdm::SourceFormat::Udf, &bytes).unwrap();
    let chosen = choose(&findings_of(&bytes));
    assert_eq!(
        chosen.len(),
        3,
        "expected one correction per defective paragraph"
    );

    let out = writeback::apply("dava.udf", &bytes, &chosen).expect("apply");
    let after = container_text_image(ikincigoz_core::cdm::SourceFormat::Udf, &out.bytes).unwrap();

    assert_eq!(
        before.replace("  ", " "),
        after,
        "something moved outside the approved changes"
    );
    assert_eq!(before.chars().count() - 3, after.chars().count());
    assert!(after.contains("Dördüncü paragraf hiç değişmemelidir ve aynen kalmalıdır."));
}

#[test]
fn j_a_correction_never_creates_new_whitespace_elsewhere() {
    // The failure this test exists for: a zero-width insertion mapping through
    // a lossy model landed beside a space that was already there.
    let blob = "gösterir fotoğraf ve kayıtlar, müvekkilin mevcut konutun durumu\n";
    // The span deliberately stops one short, leaving the space uncovered.
    let bytes = udf_with_gaps(blob, &[(0, 29), (30, 34)]);

    let doc = parser::parse("dava.udf", &bytes).expect("parse");
    assert_eq!(
        doc.blocks[0].text, "gösterir fotoğraf ve kayıtlar, müvekkilin mevcut konutun durumu",
        "the character in the gap must survive into the model"
    );

    let chosen = choose(&findings_of(&bytes));
    assert!(chosen.is_empty(), "there is no defect here: {chosen:?}");
}

#[test]
fn k_two_corrections_covering_the_same_characters_are_refused() {
    let bytes = udf(&["ABCDEF ve devamında bir metin bulunmaktadır."]);
    let overlapping = vec![
        Fix {
            block_id: "p0".into(),
            char_start: 0,
            char_end: 3,
            original: "ABC".into(),
            replacement: "X".into(),
            description: "test".into(),
        },
        Fix {
            block_id: "p0".into(),
            char_start: 2,
            char_end: 5,
            original: "CDE".into(),
            replacement: "Y".into(),
            description: "test".into(),
        },
    ];
    assert_eq!(
        writeback::apply("dava.udf", &bytes, &overlapping).unwrap_err(),
        WriteBackError::OverlappingChanges
    );
}

#[test]
fn k_a_correction_computed_against_stale_text_is_refused() {
    let bytes = udf(&["Davacı beyanda bulunmuş ve talebini yinelemiştir."]);
    let stale = vec![Fix {
        block_id: "p0".into(),
        char_start: 0,
        char_end: 6,
        original: "Davalı".into(), // the document says Davacı
        replacement: "X".into(),
        description: "test".into(),
    }];
    assert_eq!(
        writeback::apply("dava.udf", &bytes, &stale).unwrap_err(),
        WriteBackError::TextMismatch { index: 0 }
    );
}

// ===========================================================================
// The whole petition shape, end to end
// ===========================================================================

/// The structure of the petition that exposed all of this, anonymised.
fn petition() -> Vec<&'static str> {
    vec![
        "İSTANBUL ANADOLU (...) SULH HUKUK MAHKEMESİNE",
        "DAVACI\t\t\t:\tProf. Dr. Aylin YILMAZ (TCKN.: 11111111111)",
        "\t\t\t\t\tÖrnek Mah. Deneme Sk. No: 1 Kadıköy / İSTANBUL",
        "VEKİLİ\t\t\t: \tAv. Deniz KAYA, LL.M.",
        "DAVALI\t\t\t:\tMert DEMİR (TCKN.: 22222222222)",
        "KONU\t\t\t:\tKira sözleşmesinin feshi ile tahliye istemidir.",
        "DAVA DEĞERİ\t:\t31.656,00-TL x 12 = 379.872,00-TL",
        "AÇIKLAMALAR \t:",
        "A.\tMÜVEKKİL, TBK M. 310 UYARINCA KİRAYA VEREN SIFATINI KAZANMIŞTIR",
        "1.\tMüvekkil, uyuşmazlığa konu taşınmazı 08.05.2026 tarihinde satın almıştır.",
        "2.\tTBK m. 310 hükmü uyarınca kiraya veren sıfatı müvekkile geçmiştir.",
        "B.\tMÜVEKKİLİN SAĞLIK DURUMU YERLEŞME ZORUNLULUĞUNU ORTAYA KOYMAKTADIR",
        "1.\tYerleşik Yargıtay içtihatlarında kabul edildiği üzere ihtiyaç samimi olmalıdır.",
        "2.\tMüvekkil 1958 doğumlu olup iki katlı bir konutta tek başına yaşamaktadır.",
        "3.\tBir başka içtihadında Yargıtay, \"Taraflar arasında 22/09/2012 başlangıç \
         tarihli kira sözleşmesi konusunda uyuşmazlık bulunmamaktadır. TBK, kiraya verene \
         tahliye isteme hakkı tanımış olup davacı, oğlu Y.. Ö..'ın konut ihtiyacı nedeniyle \
         dava açmıştır.\" şeklinde içtihat etmiştir.",
        "4.\tBu çerçevede keşif ve bilirkişi incelemesiyle de sübuta ereceği açıktır.",
        "HUKUKİ NEDENLER\t\t\t\t: TBK, TMK, HMK ve sair tüm yasal mevzuat.",
        "SONUÇ VE İSTEM\t\t\t\t\t: Yukarıda açıklanan nedenlerle,",
        "1.\tDavamızın KABULÜ ile,",
        "2.\tKira sözleşmesinin FESHİNE,",
        "3.\tYargılama gideri ile vekalet ücretinin davalıya yükletilmesine,",
        "\t\t\t\tkarar verilmesini saygılarımızla talep ederiz. 31.08.2026",
        "Davacı Vekili",
        "Av. Deniz KAYA, LL.M.",
    ]
}

#[test]
fn a_correctly_drafted_petition_produces_no_corrections_at_all() {
    let bytes = udf(&petition());
    let findings = findings_of(&bytes);
    let chosen = choose(&findings);
    assert!(
        chosen.is_empty(),
        "a clean petition must offer nothing to correct, got {:?}",
        chosen
            .iter()
            .map(|f| (f.original.clone(), f.replacement.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        findings.iter().all(|f| f.severity != Severity::Error),
        "unexpected errors: {:?}",
        findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .map(|f| (&f.rule_id, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_petition_keeps_every_tab_and_every_protected_token_through_a_write_back() {
    // Plant three genuine defects so there is something to apply.
    let mut lines = petition();
    lines.insert(
        9,
        "Bu paragrafta  çift boşluk bulunmaktadır ve düzeltilmelidir.",
    );
    lines.insert(
        11,
        "Bu cümle noktadan sonra boşluksuzdur.Devamı böyle gelmektedir.",
    );
    lines.insert(13, "Bu cümle çift nokta ile bitmektedir..");

    let bytes = udf(&lines);
    let before = container_text_image(ikincigoz_core::cdm::SourceFormat::Udf, &bytes).unwrap();
    let chosen = choose(&findings_of(&bytes));
    assert_eq!(
        chosen.len(),
        3,
        "expected exactly the three planted defects: {chosen:?}"
    );

    let out = writeback::apply("dava.udf", &bytes, &chosen).expect("apply");
    let after = container_text_image(ikincigoz_core::cdm::SourceFormat::Udf, &out.bytes).unwrap();

    // Nothing structural moved.
    assert_eq!(
        before.matches('\t').count(),
        after.matches('\t').count(),
        "the layout tabs did not survive"
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
        assert!(
            after.contains(protected),
            "{protected:?} did not survive the write-back"
        );
    }

    // And exactly the three approved changes, nothing else.
    let expected = before
        .replace("paragrafta  çift", "paragrafta çift")
        .replace("boşluksuzdur.Devamı", "boşluksuzdur. Devamı")
        .replace("bitmektedir..", "bitmektedir.");
    assert_eq!(
        expected, after,
        "a character moved outside the approved changes"
    );
}
