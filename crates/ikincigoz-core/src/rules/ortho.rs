//! Orthography and the user's own correction rules.
//!
//! There is no general Turkish spell checker here, on purpose. A spell checker
//! needs a lexicon, a lexicon needs a licence, and a legal document is full of
//! proper nouns and Ottoman-era legal terms that any general lexicon would
//! flag. What ships instead is a short, hand-written list of compounds whose
//! separate spelling is not a matter of taste, plus whatever the user has
//! taught the program.

use super::Context;
use crate::cdm::SourceLocation;
use crate::dict::CorrectionRule;
use crate::finding::{Finding, FindingBuilder, Fix};
use crate::morph::{transfer, Transfer};
use crate::text::{tokenize, tr_lower, tr_upper};

fn builder(id: &str) -> FindingBuilder {
    FindingBuilder::new(crate::finding::rule_info(id).expect("registered rule"))
}

/// Compounds written together that must be written apart.
///
/// Deliberately tiny. Every entry is a word whose separate spelling is settled,
/// and whose joined spelling is not a word at all — so a match cannot be a
/// proper noun or a term of art.
const SEPARATE_WRITING: &[(&str, &str)] = &[
    ("birşey", "bir şey"),
    ("herşey", "her şey"),
    ("hiçbirşey", "hiçbir şey"),
    ("herbiri", "her biri"),
    ("hiçkimse", "hiç kimse"),
    ("herhangibir", "herhangi bir"),
    ("birsürü", "bir sürü"),
    ("birçok", "birçok"), // correct as written; kept out of the way below
];

/// The suffixes a joined compound may carry, longest first so `birşeyler`
/// matches before `birşeyi`.
fn split_compound(word_folded: &str) -> Option<(&'static str, &'static str, String)> {
    for (joined, separate) in SEPARATE_WRITING {
        if joined == separate {
            continue;
        }
        if let Some(rest) = word_folded.strip_prefix(joined) {
            // The suffix must be a plausible Turkish ending, not a new word.
            if rest.chars().count() <= 6 && rest.chars().all(|c| c.is_alphabetic()) {
                return Some((joined, separate, rest.to_string()));
            }
        }
    }
    None
}

pub fn run(ctx: &Context<'_>) -> Vec<Finding> {
    let mut out = separate_writing(ctx);
    out.extend(user_corrections(ctx));
    out
}

fn separate_writing(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("ORTHO_SEPARATE_WRITING");
    let mut out = Vec::new();

    for block in &ctx.document.blocks {
        for token in tokenize(&block.text) {
            let folded = tr_lower(&token.text);
            if ctx.dictionary.is_accepted(&folded) {
                continue;
            }
            let Some((joined, separate, suffix)) = split_compound(&folded) else {
                continue;
            };
            let corrected = format!("{separate}{suffix}");
            let cased = carry_case(&token.text, &corrected);
            let mut f = b.at(
                &block.id,
                SourceLocation::span(block.source_location.block_index, token.start, token.end),
                0.95,
                format!(
                    "\u{201C}{}\u{201D} bitişik yazılmış; \u{201C}{cased}\u{201D} olmalı.",
                    token.text
                ),
                format!(
                    "\u{201C}{joined}\u{201D} Türkçede ayrı yazılır. Bu kural, GölgeDosya \
                     içinde elle tanımlanmış kısa listeye dayanır; genel bir yazım denetimi \
                     yapılmaz."
                ),
            );
            f.context = Some(crate::text::excerpt(&block.text, token.start, 35));
            f.fix = Some(Fix {
                block_id: block.id.clone(),
                char_start: token.start,
                char_end: token.end,
                original: token.text.clone(),
                replacement: cased,
                description: format!("\u{201C}{separate}\u{201D} ayrı yazılır"),
            });
            out.push(f);
        }
    }
    out
}

fn user_corrections(ctx: &Context<'_>) -> Vec<Finding> {
    let plain = builder("ORTHO_USER_DICTIONARY");
    let safe = builder("MORPH_SAFE_REPLACEMENT");
    let unsure = builder("MORPH_AMBIGUOUS");
    let mut out = Vec::new();
    let rules: Vec<&CorrectionRule> = ctx.dictionary.corrections().collect();
    if rules.is_empty() {
        return out;
    }

    for block in &ctx.document.blocks {
        for token in tokenize(&block.text) {
            let folded = tr_lower(&token.text);

            // An exact hit needs no morphology.
            if let Some(rule) = ctx.dictionary.correction_for(&folded) {
                let replacement = carry_case(&token.text, &rule.to);
                let mut f = plain.at(
                    &block.id,
                    SourceLocation::span(block.source_location.block_index, token.start, token.end),
                    0.9,
                    format!(
                        "\u{201C}{}\u{201D} yerine \u{201C}{}\u{201D} kullanılması tanımlanmış.",
                        token.text, rule.to
                    ),
                    rule.note.clone().unwrap_or_else(|| {
                        "Bu düzeltme, sizin kullanıcı sözlüğünüzde tanımlı.".to_string()
                    }),
                );
                f.context = Some(crate::text::excerpt(&block.text, token.start, 35));
                f.fix = Some(Fix {
                    block_id: block.id.clone(),
                    char_start: token.start,
                    char_end: token.end,
                    original: token.text.clone(),
                    replacement,
                    description: format!("\u{201C}{}\u{201D} olarak değiştirilir", rule.to),
                });
                out.push(f);
                continue;
            }

            // Otherwise try to read it as an inflected form.
            for rule in &rules {
                if !rule.inflect {
                    continue;
                }
                let from = tr_lower(&rule.from);
                if !folded.starts_with(&from) || folded == from {
                    continue;
                }
                match transfer(&token.text, &rule.from, &rule.to) {
                    Transfer::Safe { replacement, .. } => {
                        let mut f = safe.at(
                            &block.id,
                            SourceLocation::span(
                                block.source_location.block_index,
                                token.start,
                                token.end,
                            ),
                            0.85,
                            format!(
                                "\u{201C}{}\u{201D} yerine \u{201C}{replacement}\u{201D}.",
                                token.text
                            ),
                            format!(
                                "\u{201C}{}\u{201D} teriminin çekimli biçimi bulundu. Ek, \
                                 \u{201C}{}\u{201D} sözcüğüne Türkçe ses uyumuna göre yeniden \
                                 uygulandı.",
                                rule.from, rule.to
                            ),
                        );
                        f.context = Some(crate::text::excerpt(&block.text, token.start, 35));
                        f.fix = Some(Fix {
                            block_id: block.id.clone(),
                            char_start: token.start,
                            char_end: token.end,
                            original: token.text.clone(),
                            replacement,
                            description: "çekim korunarak değiştirilir".into(),
                        });
                        out.push(f);
                        break;
                    }
                    Transfer::Ambiguous { reason } => {
                        let mut f = unsure.at(
                            &block.id,
                            SourceLocation::span(
                                block.source_location.block_index,
                                token.start,
                                token.end,
                            ),
                            0.6,
                            format!(
                                "\u{201C}{}\u{201D} biçimi otomatik olarak güvenle dönüştürülemedi.",
                                token.text
                            ),
                            reason.message_tr(),
                        );
                        f.context = Some(crate::text::excerpt(&block.text, token.start, 35));
                        out.push(f);
                        break;
                    }
                    Transfer::NotAForm => {}
                }
            }
        }
    }
    out
}

/// Apply the source token's capitalisation to a replacement.
pub(crate) fn carry_case(source: &str, replacement: &str) -> String {
    if source.chars().any(|c| c.is_alphabetic()) && tr_upper(source) == source {
        return tr_upper(replacement);
    }
    let Some(first) = source.chars().next() else {
        return replacement.to_string();
    };
    if tr_upper(&first.to_string()) == first.to_string() {
        let mut out = tr_upper(&replacement.chars().take(1).collect::<String>());
        out.push_str(&replacement.chars().skip(1).collect::<String>());
        return out;
    }
    replacement.to_string()
}

#[cfg(test)]
mod tests {
    use crate::dict::{CorrectionRule, UserDictionary};
    use crate::rules::testing::{doc, of};
    use crate::rules::{analyze, Context, LintOptions};

    const SEP: &str = "ORTHO_SEPARATE_WRITING";
    const USER: &str = "ORTHO_USER_DICTIONARY";
    const SAFE: &str = "MORPH_SAFE_REPLACEMENT";
    const AMBIG: &str = "MORPH_AMBIGUOUS";

    fn with_dict(paragraphs: &[&str], dict: UserDictionary) -> Vec<crate::finding::Finding> {
        let d = doc(paragraphs);
        let opts = LintOptions::default();
        let ctx = Context::new(&d, &dict, None, &opts);
        analyze(&ctx).findings
    }

    fn dict_with(from: &str, to: &str) -> UserDictionary {
        let mut d = UserDictionary::default();
        d.add_correction(CorrectionRule {
            from: from.into(),
            to: to.into(),
            inflect: true,
            note: None,
        })
        .unwrap();
        d
    }

    #[test]
    fn a_compound_written_together_is_reported_with_a_fix() {
        let f = with_dict(
            &["Davalı birşey söylemedi ve beyanda bulunmadı."],
            UserDictionary::default(),
        );
        let s = of(&f, SEP);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].fix.as_ref().unwrap().replacement, "bir şey");
    }

    #[test]
    fn an_inflected_compound_keeps_its_suffix_in_the_fix() {
        let f = with_dict(
            &["Davalı herşeyi reddetmiş ve savunma yapmıştır."],
            UserDictionary::default(),
        );
        let s = of(&f, SEP);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].fix.as_ref().unwrap().replacement, "her şeyi");
    }

    #[test]
    fn capitalisation_is_carried_into_the_fix() {
        let f = with_dict(
            &["Herşey usulüne uygun biçimde tamamlanmıştır."],
            UserDictionary::default(),
        );
        assert_eq!(of(&f, SEP)[0].fix.as_ref().unwrap().replacement, "Her şey");
    }

    #[test]
    fn correctly_separated_writing_is_silent() {
        assert!(of(
            &with_dict(
                &["Davalı bir şey söylemedi ve her şeyi reddetti."],
                UserDictionary::default()
            ),
            SEP
        )
        .is_empty());
    }

    #[test]
    fn an_accepted_word_suppresses_the_rule() {
        let mut d = UserDictionary::default();
        d.accept("birşey");
        assert!(of(
            &with_dict(&["Davalı birşey söylemedi ve beyanda bulunmadı."], d),
            SEP
        )
        .is_empty());
    }

    #[test]
    fn an_exact_user_correction_is_offered() {
        let f = with_dict(
            &["Mecur tahliye edilmiştir ve teslim alınmıştır."],
            dict_with("mecur", "kiralanan"),
        );
        let u = of(&f, USER);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].fix.as_ref().unwrap().replacement, "Kiralanan");
    }

    #[test]
    fn inflected_forms_are_converted_with_the_right_suffix() {
        let f = with_dict(
            &["Mecurun tahliyesi ve mecurda yapılan tadilat ile mecurdan çıkarılması istenmiştir."],
            dict_with("mecur", "kiralanan"),
        );
        let s = of(&f, SAFE);
        let replacements: Vec<&str> = s
            .iter()
            .map(|x| x.fix.as_ref().unwrap().replacement.as_str())
            .collect();
        assert!(replacements.contains(&"Kiralananın"), "{replacements:?}");
        assert!(replacements.contains(&"kiralananda"), "{replacements:?}");
        assert!(replacements.contains(&"kiralanandan"), "{replacements:?}");
    }

    #[test]
    fn a_form_that_cannot_be_converted_safely_is_review_only_and_has_no_fix() {
        // `sokak` may or may not soften, so the accusative cannot be derived.
        let f = with_dict(
            &["Mecuru teslim etmiş ve tutanağı imzalamıştır."],
            dict_with("mecur", "sokak"),
        );
        let a = of(&f, AMBIG);
        assert_eq!(a.len(), 1, "{:?}", a);
        assert!(a[0].fix.is_none());
        assert_eq!(a[0].severity, crate::finding::Severity::Review);
    }

    #[test]
    fn an_unrelated_word_starting_with_the_same_letters_is_not_converted() {
        let f = with_dict(
            &["Mecburiyet nedeniyle sözleşme feshedilmiş ve bildirim yapılmıştır."],
            dict_with("mecur", "kiralanan"),
        );
        assert!(of(&f, SAFE).is_empty());
        assert!(of(&f, AMBIG).is_empty());
    }

    #[test]
    fn an_empty_dictionary_produces_no_user_findings() {
        let f = with_dict(
            &["Mecur tahliye edilmiştir ve teslim alınmıştır."],
            UserDictionary::default(),
        );
        assert!(of(&f, USER).is_empty());
        assert!(of(&f, SAFE).is_empty());
    }
}
