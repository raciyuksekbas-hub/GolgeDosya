//! The lint engine.
//!
//! Rules receive a [`Context`] and return findings. Nothing here knows about
//! DOCX or UDF: a rule that behaved differently for the two containers would
//! be a bug in the parser, not in the rule.
//!
//! The default configuration favours precision over recall. A rule that fires
//! often but is right only sometimes is worse than no rule, because it teaches
//! the user to skim past the list.

pub mod consistency;
pub mod draft;
pub mod invisible;
pub mod ortho;
pub mod punctuation;
pub mod references;
pub mod repetition;
pub mod sequence;
pub mod style;

use crate::cdm::Document;
use crate::dict::UserDictionary;
use crate::finding::{Finding, Severity};
use crate::profile::DocumentProfile;
use crate::zones::ZoneMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LintOptions {
    /// Rule ids the user has switched off.
    pub disabled_rules: HashSet<String>,
    /// Findings below this confidence are dropped entirely.
    pub min_confidence: f32,
    /// When false, `Review` findings are not produced at all. On by default:
    /// they are the "have a look" tier, and hiding them by default would mean
    /// shipping rules that never run.
    pub include_review: bool,
    /// Upper bound on findings from a single rule, so one systematic quirk
    /// cannot bury everything else.
    pub max_per_rule: usize,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            disabled_rules: HashSet::new(),
            min_confidence: 0.55,
            include_review: true,
            max_per_rule: 50,
        }
    }
}

/// Everything a rule is allowed to see.
pub struct Context<'a> {
    pub document: &'a Document,
    pub dictionary: &'a UserDictionary,
    pub profile: Option<&'a DocumentProfile>,
    pub options: &'a LintOptions,
    /// Which parts of the document are layout, quotation or ordinary prose.
    ///
    /// Built here rather than supplied, so no caller can assemble a context in
    /// which the rules have no idea where they are.
    zones: ZoneMap,
}

impl<'a> Context<'a> {
    pub fn new(
        document: &'a Document,
        dictionary: &'a UserDictionary,
        profile: Option<&'a DocumentProfile>,
        options: &'a LintOptions,
    ) -> Self {
        Self {
            zones: ZoneMap::build(document),
            document,
            dictionary,
            profile,
            options,
        }
    }

    pub fn enabled(&self, rule_id: &str) -> bool {
        !self.options.disabled_rules.contains(rule_id)
    }

    pub fn zones(&self) -> &ZoneMap {
        &self.zones
    }
}

/// How much text to show on each side of a finding.
const SNIPPET_RADIUS: usize = 34;

/// Build a finding's context from its own source range.
///
/// Two properties matter and the previous helper had neither.
///
/// First, the snippet is cut from the range the finding actually points at, so
/// the card and the document preview can never disagree about where the problem
/// is. Second, the offending characters are rendered visibly — a doubled space
/// is invisible by definition, and a snippet that collapses whitespace hides
/// the very thing it was printed to show, which made correct findings look like
/// false alarms.
///
/// Only the offending range is made visible. The surrounding text stays
/// readable, so the anomaly stands out instead of the whole line turning into
/// dots.
pub fn context_snippet(text: &str, start: usize, end: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = start.min(chars.len());
    let end = end.clamp(start, chars.len());

    let from = start.saturating_sub(SNIPPET_RADIUS);
    let to = (end + SNIPPET_RADIUS).min(chars.len());

    let mut out = String::new();
    if from > 0 {
        out.push('\u{2026}');
    }
    out.push_str(&tidy(&chars[from..start]));
    for c in &chars[start..end] {
        out.push_str(&visible(*c));
    }
    out.push_str(&tidy(&chars[end..to]));
    if to < chars.len() {
        out.push('\u{2026}');
    }
    out
}

/// Context either side of the range: readable, with line breaks flattened.
fn tidy(chars: &[char]) -> String {
    chars
        .iter()
        .map(|c| match c {
            '\n' | '\r' | '\t' => ' ',
            other => *other,
        })
        .collect()
}

/// A printable stand-in for a character the reader cannot see.
fn visible(c: char) -> String {
    match c {
        ' ' => "\u{00B7}".to_string(),
        '\t' => "\u{2192}".to_string(),
        '\u{00A0}' => "\u{237D}".to_string(),
        '\n' | '\r' => "\u{21B5}".to_string(),
        c if c.is_control()
            || matches!(
                c,
                '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}' | '\u{00AD}'
            ) =>
        {
            format!("\u{27E8}U+{:04X}\u{27E9}", c as u32)
        }
        other => other.to_string(),
    }
}

/// Result of a full analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LintResult {
    pub findings: Vec<Finding>,
    pub error_count: usize,
    pub warning_count: usize,
    pub review_count: usize,
    /// Rules that ran, for the report.
    pub rules_evaluated: usize,
    /// Findings dropped by `max_per_rule`, so truncation is never silent.
    pub truncated_rules: Vec<TruncatedRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TruncatedRule {
    pub rule_id: String,
    pub shown: usize,
    pub total: usize,
}

/// Run every enabled rule over the document.
pub fn analyze(ctx: &Context<'_>) -> LintResult {
    let mut raw: Vec<Finding> = Vec::new();

    raw.extend(repetition::run(ctx));
    raw.extend(sequence::run(ctx));
    raw.extend(punctuation::run(ctx));
    raw.extend(invisible::run(ctx));
    raw.extend(draft::run(ctx));
    raw.extend(references::run(ctx));
    raw.extend(style::run(ctx));
    raw.extend(consistency::run(ctx));
    raw.extend(ortho::run(ctx));

    finalize(raw, ctx)
}

/// Apply the global gates: disabled rules, confidence floor, review tier and
/// the per-rule cap. Kept in one place so no rule can bypass them.
fn finalize(raw: Vec<Finding>, ctx: &Context<'_>) -> LintResult {
    let opts = ctx.options;
    let mut kept: Vec<Finding> = raw
        .into_iter()
        .filter(|f| ctx.enabled(&f.rule_id))
        .filter(|f| f.confidence >= opts.min_confidence)
        .collect();

    kept.sort_by(|a, b| {
        a.sort_key()
            .cmp(&b.sort_key())
            .then(a.rule_id.cmp(&b.rule_id))
    });
    // Applied after demotion, so a finding that dropped into the review tier
    // because it sits inside a quotation is hidden along with the rest of it.
    kept.retain(|f| opts.include_review || f.severity != Severity::Review);
    kept.dedup_by(|a, b| {
        a.rule_id == b.rule_id
            && a.block_id == b.block_id
            && a.location.char_start == b.location.char_start
            && a.message == b.message
    });

    // Cap per rule while preserving the global ordering.
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut totals: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in &kept {
        *totals.entry(f.rule_id.clone()).or_default() += 1;
    }
    let mut findings = Vec::with_capacity(kept.len());
    for f in kept {
        let c = counts.entry(f.rule_id.clone()).or_default();
        if *c < opts.max_per_rule {
            *c += 1;
            findings.push(f);
        }
    }
    let mut truncated: Vec<TruncatedRule> = totals
        .into_iter()
        .filter(|(_, total)| *total > opts.max_per_rule)
        .map(|(rule_id, total)| TruncatedRule {
            shown: opts.max_per_rule,
            total,
            rule_id,
        })
        .collect();
    truncated.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));

    LintResult {
        error_count: findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count(),
        warning_count: findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count(),
        review_count: findings
            .iter()
            .filter(|f| f.severity == Severity::Review)
            .count(),
        rules_evaluated: crate::finding::RULES.len(),
        truncated_rules: truncated,
        findings,
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::cdm::{Block, BlockKind, ParagraphStyle, Run, SourceFormat, SourceLocation};
    use crate::parser::finalize_blocks;

    /// Build a document from plain paragraph strings.
    pub fn doc(paragraphs: &[&str]) -> Document {
        let blocks: Vec<Block> = paragraphs
            .iter()
            .map(|t| Block {
                id: String::new(),
                kind: BlockKind::Paragraph,
                text: (*t).to_string(),
                normalized_text: String::new(),
                runs: vec![Run {
                    text: (*t).to_string(),
                    ..Default::default()
                }],
                style: ParagraphStyle::default(),
                numbering: None,
                hierarchy_level: None,
                source_location: SourceLocation::default(),
            })
            .collect();
        Document::new(SourceFormat::Docx, finalize_blocks(blocks))
    }

    /// Run the whole engine and return the findings.
    pub fn lint(paragraphs: &[&str]) -> Vec<Finding> {
        let d = doc(paragraphs);
        lint_doc(&d)
    }

    pub fn lint_doc(d: &Document) -> Vec<Finding> {
        let dict = UserDictionary::default();
        let opts = LintOptions::default();
        let ctx = Context::new(d, &dict, None, &opts);
        analyze(&ctx).findings
    }

    /// Findings for one rule id.
    pub fn of(findings: &[Finding], rule_id: &str) -> Vec<Finding> {
        findings
            .iter()
            .filter(|f| f.rule_id == rule_id)
            .cloned()
            .collect()
    }

    /// Assert a rule fires exactly `n` times.
    pub fn assert_count(paragraphs: &[&str], rule_id: &str, n: usize) {
        let f = lint(paragraphs);
        let got = of(&f, rule_id);
        assert_eq!(
            got.len(),
            n,
            "{rule_id}: expected {n}, got {} -> {:?}",
            got.len(),
            got.iter().map(|f| f.message.as_str()).collect::<Vec<_>>()
        );
    }

    /// Assert a rule does not fire. The negative half of every rule's tests:
    /// a rule that cannot be silenced is a rule that will be ignored.
    pub fn assert_silent(paragraphs: &[&str], rule_id: &str) {
        assert_count(paragraphs, rule_id, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::cdm::SourceLocation;
    use crate::finding::{Finding, RULES};

    fn f(rule_id: &str, sev: Severity, conf: f32) -> Finding {
        Finding {
            rule_id: rule_id.into(),
            title: "t".into(),
            severity: sev,
            confidence: conf,
            message: format!("m{conf}"),
            explanation: "e".into(),
            block_id: "p0".into(),
            location: Default::default(),
            context: None,
            fix: None,
        }
    }

    fn run_finalize(raw: Vec<Finding>, opts: LintOptions) -> LintResult {
        let d = doc(&["x"]);
        let dict = UserDictionary::default();
        let ctx = Context::new(&d, &dict, None, &opts);
        super::finalize(raw, &ctx)
    }

    #[test]
    fn the_confidence_floor_drops_weak_findings() {
        let r = run_finalize(
            vec![
                f("SEC_SEQ_GAP", Severity::Error, 0.9),
                f("SEC_SEQ_GAP", Severity::Error, 0.2),
            ],
            LintOptions::default(),
        );
        assert_eq!(r.findings.len(), 1);
    }

    #[test]
    fn disabled_rules_produce_nothing() {
        let mut opts = LintOptions::default();
        opts.disabled_rules.insert("SEC_SEQ_GAP".into());
        let r = run_finalize(vec![f("SEC_SEQ_GAP", Severity::Error, 0.99)], opts);
        assert!(r.findings.is_empty());
    }

    #[test]
    fn review_findings_can_be_switched_off_as_a_tier() {
        let opts = LintOptions {
            include_review: false,
            ..LintOptions::default()
        };
        let r = run_finalize(
            vec![
                f("TEXT_NEAR_DUPLICATE", Severity::Review, 0.9),
                f("SEC_SEQ_GAP", Severity::Error, 0.9),
            ],
            opts,
        );
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].severity, Severity::Error);
    }

    #[test]
    fn truncation_is_reported_rather_than_silent() {
        let opts = LintOptions {
            max_per_rule: 2,
            ..LintOptions::default()
        };
        let raw: Vec<Finding> = (0..7)
            .map(|i| f("SEC_SEQ_GAP", Severity::Error, 0.9 - i as f32 * 0.01))
            .collect();
        let r = run_finalize(raw, opts);
        assert_eq!(r.findings.len(), 2);
        assert_eq!(r.truncated_rules[0].total, 7);
        assert_eq!(r.truncated_rules[0].shown, 2);
    }

    #[test]
    fn findings_are_ordered_errors_first_then_by_position() {
        let a = Finding {
            location: SourceLocation::block(0),
            ..f("A", Severity::Review, 0.9)
        };
        let b = Finding {
            location: SourceLocation::block(5),
            ..f("B", Severity::Error, 0.9)
        };
        let r = run_finalize(vec![a, b], LintOptions::default());
        assert_eq!(r.findings[0].severity, Severity::Error);
        assert_eq!(r.error_count, 1);
        assert_eq!(r.review_count, 1);
    }

    #[test]
    fn a_clean_document_produces_no_findings_at_all() {
        let findings = lint(&[
            "Sayın İstanbul 5. Asliye Hukuk Mahkemesine",
            "Davacı, davalıya karşı alacak davası açmıştır.",
            "Yukarıda açıklanan nedenlerle davanın kabulüne karar verilmesini talep ederim.",
        ]);
        assert!(
            findings.is_empty(),
            "clean document produced {:?}",
            findings
        );
    }

    #[test]
    fn every_registered_rule_has_a_non_empty_description() {
        for r in RULES {
            assert!(!r.description.is_empty(), "{} has no description", r.id);
            assert!(!r.title.is_empty(), "{} has no title", r.id);
        }
    }
}
