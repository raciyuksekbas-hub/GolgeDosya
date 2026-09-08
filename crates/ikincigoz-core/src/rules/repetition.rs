//! Repetition rules: the same word, sentence or paragraph written twice.
//!
//! These are the classic copy-paste injuries, and the first three are
//! mechanical enough to be reported as certain. Near-duplicates are not: a
//! petition legitimately repeats a formula, so they stay in the review tier.

use super::Context;
use crate::cdm::{char_slice, Block, BlockKind, SourceLocation};
use crate::finding::{Finding, FindingBuilder, Fix};
use crate::text::{normalize, normalized_levenshtein, split_sentences, tokenize, tr_lower};
use once_cell::sync::Lazy;
use std::collections::HashMap;

fn builder(id: &str) -> FindingBuilder {
    FindingBuilder::new(crate::finding::rule_info(id).expect("registered rule"))
}

/// Turkish doubles a word to make an adverb or to intensify. These are correct
/// writing, not a repeated word, and each one here would otherwise be a false
/// positive on ordinary prose.
static REDUPLICATIONS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "yavaş",
        "ağır",
        "hızlı",
        "sık",
        "seyrek",
        "az",
        "çok",
        "bol",
        "derin",
        "uzun",
        "kısa",
        "ara",
        "yer",
        "zaman",
        "kimi",
        "bazı",
        "teker",
        "birer",
        "ikişer",
        "üçer",
        "tek",
        "çift",
        "adım",
        "damla",
        "parça",
        "tane",
        "sıra",
        "kat",
        "yudum",
        "günden",
        "yıllar",
        "defa",
        "iyi",
        "kötü",
        "güzel",
        "doğru",
        "gide",
        "baka",
        "koşa",
        "güle",
        "ağlaya",
        "düşe",
        "kalka",
        "yavaşça",
        "usul",
        "hafif",
        "tatlı",
        "acı",
        "serin",
        "sıcak",
        "soğuk",
        "dolu",
        "boş",
        "başka",
        "türlü",
        "renk",
        "biçim",
        "yan",
        "üst",
        "alt",
        "art",
        "peş",
        "yakın",
        "uzak",
    ]
});

/// Words whose immediate repetition is grammatical for other reasons.
fn is_licensed_repeat(word: &str) -> bool {
    let w = tr_lower(word);
    if REDUPLICATIONS.contains(&w.as_str()) {
        return true;
    }
    // `bir bir`, `da da`, `mı mı` and friends: too short to judge, and a pure
    // digit repeat is usually a table or an amount range.
    w.chars().count() <= 2 || w.chars().all(|c| c.is_ascii_digit())
}

pub fn run(ctx: &Context<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(word_duplication(ctx));
    out.extend(sentence_duplication(ctx));
    out.extend(paragraph_duplication(ctx));
    out.extend(near_duplicates(ctx));
    out
}

fn word_duplication(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("TEXT_WORD_DUPLICATION");
    let mut out = Vec::new();
    for block in &ctx.document.blocks {
        let tokens = tokenize(&block.text);
        for pair in tokens.windows(2) {
            let (first, second) = (&pair[0], &pair[1]);
            if tr_lower(&first.text) != tr_lower(&second.text) {
                continue;
            }
            if is_licensed_repeat(&first.text) {
                continue;
            }
            // Only a run of plain spaces may separate them. A comma, a line
            // break or a bullet between the two makes it a list, not a slip.
            let between = char_slice(&block.text, first.end, second.start);
            if between.is_empty() || !between.chars().all(|c| c == ' ') {
                continue;
            }
            let mut f = b.at(
                &block.id,
                SourceLocation::span(block.source_location.block_index, first.start, second.end),
                0.97,
                format!(
                    "\u{201C}{}\u{201D} kelimesi arka arkaya iki kez yazılmış.",
                    first.text
                ),
                "Aynı kelimenin ardışık tekrarı, yazım sırasında oluşan mekanik bir hatadır. \
                 Türkçede ikileme olarak kullanılan kelimeler bu kuralın dışında tutulur.",
            );
            f.context = Some(crate::text::excerpt(&block.text, first.start, 40));
            f.fix = Some(Fix {
                block_id: block.id.clone(),
                char_start: first.end,
                char_end: second.end,
                original: char_slice(&block.text, first.end, second.end),
                replacement: String::new(),
                description: format!("ikinci \u{201C}{}\u{201D} kaldırılır", second.text),
            });
            out.push(f);
        }
    }
    out
}

fn sentence_duplication(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("TEXT_SENTENCE_DUPLICATION");
    let mut out = Vec::new();
    for block in &ctx.document.blocks {
        let sentences = split_sentences(&block.text);
        for pair in sentences.windows(2) {
            let (first, second) = (&pair[0], &pair[1]);
            let (na, nb) = (normalize(&first.text), normalize(&second.text));
            if na != nb || na.split_whitespace().count() < 3 {
                continue;
            }
            let mut f = b.at(
                &block.id,
                SourceLocation::span(block.source_location.block_index, second.start, second.end),
                0.96,
                "Aynı cümle arka arkaya iki kez yazılmış.",
                "Birbirini izleyen iki cümle, büyük/küçük harf ve boşluk farkları \
                 dışında birebir aynı.",
            );
            f.context = Some(crate::text::excerpt(&block.text, second.start, 50));
            f.fix = Some(Fix {
                block_id: block.id.clone(),
                char_start: first.end,
                char_end: second.end,
                original: char_slice(&block.text, first.end, second.end),
                replacement: String::new(),
                description: "ikinci cümle kaldırılır".into(),
            });
            out.push(f);
        }
    }
    out
}

/// A paragraph short enough to be a recurring label rather than real content.
///
/// `Ekler:` or a party name appearing twice is normal in a petition, so the
/// rule only looks at paragraphs long enough that repeating one is a mistake.
const MIN_PARAGRAPH_WORDS: usize = 6;

fn paragraph_duplication(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("TEXT_PARAGRAPH_DUPLICATION");
    let mut out = Vec::new();
    let mut first_seen: HashMap<&str, usize> = HashMap::new();

    for block in &ctx.document.blocks {
        if !eligible(block) {
            continue;
        }
        match first_seen.get(block.normalized_text.as_str()) {
            None => {
                first_seen.insert(
                    block.normalized_text.as_str(),
                    block.source_location.block_index,
                );
            }
            Some(&first_index) => {
                let mut f = b.at(
                    &block.id,
                    SourceLocation::block(block.source_location.block_index),
                    0.95,
                    format!(
                        "Bu paragraf belgede daha önce de yer alıyor ({}. paragraf).",
                        first_index + 1
                    ),
                    "İki paragraf, büyük/küçük harf ve boşluk farkları dışında birebir aynı.",
                );
                f.context = Some(crate::text::excerpt(&block.text, 0, 60));
                out.push(f);
            }
        }
    }
    out
}

fn eligible(block: &Block) -> bool {
    block.kind != BlockKind::TableCell
        && block.text.split_whitespace().count() >= MIN_PARAGRAPH_WORDS
}

/// Similarity at which two paragraphs stop being "similar" and start being
/// "the same". Above this, `TEXT_PARAGRAPH_DUPLICATION` already fired or the
/// difference is a single character, which is worth a look either way.
const NEAR_LOW: f32 = 0.86;
/// Blocks considered by the near-duplicate scan.
const MAX_NEAR_BLOCKS: usize = 400;
/// Hard ceiling on edit-distance comparisons.
///
/// The scan is pairwise, so a document of near-identical paragraphs is the
/// worst case. Clustering below keeps that case cheap; this is the backstop for
/// anything the clustering does not catch.
const MAX_COMPARISONS: usize = 20_000;

/// A candidate with its comparison inputs computed once.
///
/// Recomputing the token set inside the inner loop is what turns this rule
/// from linear-ish into a document-freezing quadratic, so it is done up front.
struct Candidate<'a> {
    block: &'a Block,
    tokens: std::collections::HashSet<String>,
    char_len: usize,
}

fn near_duplicates(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("TEXT_NEAR_DUPLICATE");
    let mut out = Vec::new();

    let candidates: Vec<Candidate<'_>> = ctx
        .document
        .blocks
        .iter()
        .filter(|bl| eligible(bl) && bl.text.split_whitespace().count() >= 12)
        .take(MAX_NEAR_BLOCKS)
        .map(|block| Candidate {
            tokens: crate::text::tokenize(&block.normalized_text)
                .into_iter()
                .map(|t| t.text)
                .collect(),
            char_len: block.normalized_text.chars().count(),
            block,
        })
        .collect();

    // Once a paragraph has been reported as a near-duplicate of an earlier one,
    // it is part of that cluster. Comparing it against everything else again
    // would report the same repetition n² times and do n² work to say it.
    let mut clustered = vec![false; candidates.len()];
    let mut comparisons = 0usize;

    for i in 0..candidates.len() {
        if clustered[i] {
            continue;
        }
        for j in (i + 1)..candidates.len() {
            if clustered[j] {
                continue;
            }
            if comparisons >= MAX_COMPARISONS || out.len() >= ctx.options.max_per_rule {
                return out;
            }
            let (a, c) = (&candidates[i], &candidates[j]);
            if a.block.normalized_text == c.block.normalized_text {
                continue; // an exact duplicate is already reported as one
            }
            let ratio =
                a.char_len.min(c.char_len) as f32 / a.char_len.max(c.char_len).max(1) as f32;
            if ratio < 0.8 {
                continue;
            }
            // Cheap set overlap first; the edit distance is the expensive part.
            let inter = a.tokens.intersection(&c.tokens).count() as f32;
            let union = (a.tokens.len() + c.tokens.len()) as f32 - inter;
            if union <= 0.0 || inter / union < 0.7 {
                continue;
            }
            comparisons += 1;
            let sim = normalized_levenshtein(&a.block.normalized_text, &c.block.normalized_text);
            if sim < NEAR_LOW {
                continue;
            }
            clustered[j] = true;
            let mut f = b.at(
                &c.block.id,
                SourceLocation::block(c.block.source_location.block_index),
                0.6,
                format!(
                    "Bu paragraf, {}. paragrafa çok benziyor (benzerlik %{}).",
                    a.block.source_location.block_index + 1,
                    (sim * 100.0).round() as u32
                ),
                "İki paragraf neredeyse aynı. Bilinçli bir tekrar olabileceği için \
                 kesin hata sayılmadı; farkın kasıtlı olup olmadığını siz değerlendirin.",
            );
            f.context = Some(crate::text::excerpt(&c.block.text, 0, 60));
            out.push(f);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::rules::testing::*;

    const RULE_WORD: &str = "TEXT_WORD_DUPLICATION";
    const RULE_SENT: &str = "TEXT_SENTENCE_DUPLICATION";
    const RULE_PARA: &str = "TEXT_PARAGRAPH_DUPLICATION";
    const RULE_NEAR: &str = "TEXT_NEAR_DUPLICATE";

    #[test]
    fn a_repeated_word_is_reported() {
        assert_count(
            &["Davacı davacı tarafından açılan davada beyan edilmiştir."],
            RULE_WORD,
            1,
        );
    }

    #[test]
    fn a_repeated_word_is_reported_regardless_of_case() {
        assert_count(&["Mahkeme mahkeme kararını açıklamıştır."], RULE_WORD, 1);
    }

    #[test]
    fn a_turkish_reduplication_is_not_a_repeated_word() {
        assert_silent(
            &["Taraflar yavaş yavaş anlaşmaya vardı ve dosya kapandı."],
            RULE_WORD,
        );
        assert_silent(
            &["Belgeler teker teker incelenmiş ve tutanağa geçirilmiştir."],
            RULE_WORD,
        );
    }

    #[test]
    fn a_repeat_across_punctuation_is_not_a_slip() {
        assert_silent(
            &["Tanık beyanı, beyanı destekleyen delillerle birlikte sunulmuştur."],
            RULE_WORD,
        );
    }

    #[test]
    fn a_repeated_word_carries_a_fix_that_deletes_the_second_copy() {
        let f = lint(&["Davacı davacı tarafından açılan davada beyan edilmiştir."]);
        let fix = of(&f, RULE_WORD)[0].fix.clone().unwrap();
        assert_eq!(fix.replacement, "");
        assert_eq!(fix.original, " davacı");
    }

    #[test]
    fn a_repeated_sentence_is_reported() {
        assert_count(
            &["Davanın kabulüne karar verilmesini talep ederim. Davanın kabulüne karar verilmesini talep ederim."],
            RULE_SENT,
            1,
        );
    }

    #[test]
    fn two_different_sentences_are_not_a_repeat() {
        assert_silent(
            &["Davanın kabulüne karar verilmesini talep ederim. Yargılama giderleri davalıya yükletilsin."],
            RULE_SENT,
        );
    }

    #[test]
    fn a_repeated_paragraph_is_reported_once_on_the_second_copy() {
        let p = "Yukarıda açıklanan nedenlerle davanın kabulüne karar verilmesini saygıyla talep ederim";
        assert_count(
            &[
                p,
                "Araya giren başka bir paragraf metni burada yer alır.",
                p,
            ],
            RULE_PARA,
            1,
        );
    }

    #[test]
    fn a_short_repeated_label_is_not_a_paragraph_duplicate() {
        assert_silent(&["EKLER", "Bir şeyler", "EKLER"], RULE_PARA);
    }

    #[test]
    fn a_paragraph_repeated_with_different_spacing_is_still_a_duplicate() {
        let a = "Yukarıda açıklanan nedenlerle davanın kabulüne karar verilmesini talep ederim";
        let b = "Yukarıda  açıklanan nedenlerle  davanın kabulüne KARAR verilmesini talep ederim";
        assert_count(
            &[
                a,
                "Araya giren bir başka paragraf burada yer almaktadır.",
                b,
            ],
            RULE_PARA,
            1,
        );
    }

    #[test]
    fn near_duplicates_are_review_only_and_not_errors() {
        let a = "Davalı taraf sözleşmeden doğan edimini süresi içinde yerine getirmemiş olduğundan temerrüde düşmüştür";
        let b = "Davalı taraf sözleşmeden doğan edimini süresi içinde yerine getirmemiş olduğundan temerrüde düşmemiştir";
        let f = lint(&[a, b]);
        let near = of(&f, RULE_NEAR);
        assert_eq!(near.len(), 1);
        assert_eq!(near[0].severity, crate::finding::Severity::Review);
    }

    #[test]
    fn plainly_different_paragraphs_are_not_near_duplicates() {
        assert_silent(
            &[
                "Davacı, davalıdan olan alacağının tahsili amacıyla icra takibi başlatmıştır ve itiraz gelmiştir",
                "Bilirkişi raporunda taşınmazın değeri güncel piyasa koşullarına göre ayrıntılı biçimde belirlenmiştir",
            ],
            RULE_NEAR,
        );
    }
}
