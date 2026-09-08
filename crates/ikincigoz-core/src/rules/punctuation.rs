//! Punctuation, spacing and bracket-pairing rules.
//!
//! This is where precision matters most: these rules fire on almost every
//! document, so a single sloppy pattern would flood the list. Every guard
//! below exists because of a construction that is ordinary Turkish legal
//! writing — thousands separators, `Ek-3'te`, inline `a)` enumerators, an
//! opening quote that closes three paragraphs later.

use super::{context_snippet, Context};
use crate::abbrev::{self, DotRole};
use crate::cdm::{char_slice, SourceLocation};
use crate::finding::{Finding, FindingBuilder, Fix};
use crate::text::tr_lower;

fn builder(id: &str) -> FindingBuilder {
    FindingBuilder::new(crate::finding::rule_info(id).expect("registered rule"))
}

const SENTENCE_PUNCT: [char; 6] = ['.', ',', ';', ':', '!', '?'];

pub fn run(ctx: &Context<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    for block in &ctx.document.blocks {
        let chars: Vec<char> = block.text.chars().collect();
        let bi = block.source_location.block_index;
        let marker_end = block.numbering.as_ref().map(|n| n.text_start).unwrap_or(0);
        let layout = ctx.zones().is_layout_block(bi);
        out.extend(double_punctuation(block, &chars, bi));
        out.extend(space_before_punctuation(block, &chars, bi, layout));
        out.extend(missing_space_after_punctuation(block, &chars, bi));
        out.extend(multiple_spaces(block, &chars, bi, layout));
        out.extend(apostrophe_spacing(block, &chars, bi));
        out.extend(bracket_padding(block, &chars, bi));
        out.extend(unclosed_pairs(block, &chars, bi, marker_end));
    }
    out.extend(unbalanced_quotes(ctx));
    out
}

fn fix_span(
    block: &crate::cdm::Block,
    start: usize,
    end: usize,
    replacement: &str,
    description: &str,
) -> Fix {
    Fix {
        block_id: block.id.clone(),
        char_start: start,
        char_end: end,
        original: char_slice(&block.text, start, end),
        replacement: replacement.to_string(),
        description: description.to_string(),
    }
}

fn double_punctuation(block: &crate::cdm::Block, chars: &[char], bi: usize) -> Vec<Finding> {
    let b = builder("TYPO_DOUBLE_PUNCTUATION");
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        if !SENTENCE_PUNCT.contains(&chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && SENTENCE_PUNCT.contains(&chars[i]) {
            i += 1;
        }
        let run: String = chars[start..i].iter().collect();
        if run.chars().count() < 2 {
            continue;
        }
        // `...` is an ellipsis and `?!` / `!?` are deliberate.
        if run == "..." || run == "?!" || run == "!?" {
            continue;
        }
        // `Y..` and `Ö..` are how a judgment anonymises a party. The doubled
        // dot is the convention, not a slip, and collapsing it alters the text
        // of a quotation.
        if abbrev::anonymised_initial_span(chars, start).is_some() {
            continue;
        }
        // A dot belonging to an initialism, an abbreviation or a number is not
        // sentence punctuation, so a run containing one is not doubled.
        if chars[start..i].iter().enumerate().any(|(offset, c)| {
            *c == '.'
                && matches!(
                    abbrev::classify_dot(chars, start + offset),
                    DotRole::Initialism | DotRole::Abbreviation | DotRole::NumberSeparator
                )
        }) {
            continue;
        }
        let uniform = run.chars().all(|c| c == chars[start]);
        let (message, fix) = if uniform {
            (
                format!("\u{201C}{run}\u{201D} yazılmış; tek bir işaret yeterli."),
                Some(fix_span(
                    block,
                    start,
                    i,
                    &chars[start].to_string(),
                    "tek işarete indirilir",
                )),
            )
        } else {
            (
                format!("\u{201C}{run}\u{201D} birbiriyle bağdaşmayan noktalama işaretleri."),
                None,
            )
        };
        let mut f = b.at(
            &block.id,
            SourceLocation::span(bi, start, i),
            if uniform { 0.97 } else { 0.9 },
            message,
            "Arka arkaya gelen noktalama işaretleri, yazım sırasında oluşan mekanik bir hatadır.",
        );
        f.context = Some(context_snippet(&block.text, start, i));
        f.fix = fix;
        out.push(f);
    }
    out
}

fn space_before_punctuation(
    block: &crate::cdm::Block,
    chars: &[char],
    bi: usize,
    layout: bool,
) -> Vec<Finding> {
    let b = builder("TYPO_SPACE_BEFORE_PUNCTUATION");
    let mut out = Vec::new();

    // In the label column the spacing before a colon is alignment, not a typo.
    if layout {
        return out;
    }

    for i in 1..chars.len() {
        if !SENTENCE_PUNCT.contains(&chars[i]) {
            continue;
        }
        if chars[i - 1] != ' ' {
            continue;
        }
        // Walk back over the run, but only over plain spaces. A run containing
        // a tab is a layout column and deleting it collapses the alignment;
        // whether that tab belongs there is the tab rule's question.
        let mut start = i;
        while start > 0 && chars[start - 1] == ' ' {
            start -= 1;
        }
        if start == 0 || chars[start - 1] == '\t' {
            continue;
        }
        // A punctuation mark opening the block is a stray, not a spacing error.
        if !chars[start - 1].is_alphanumeric()
            && !matches!(
                chars[start - 1],
                ')' | ']' | '"' | '\'' | '\u{201D}' | '\u{2019}'
            )
        {
            continue;
        }
        let mut f = b.at(
            &block.id,
            SourceLocation::span(bi, start, i + 1),
            0.95,
            format!("\u{201C}{}\u{201D} işaretinden önce boşluk var.", chars[i]),
            "Türkçede noktalama işareti kendinden önceki kelimeye bitişik yazılır.",
        );
        f.context = Some(context_snippet(&block.text, start, i));
        f.fix = Some(fix_span(block, start, i, "", "boşluk kaldırılır"));
        out.push(f);
    }
    out
}

fn missing_space_after_punctuation(
    block: &crate::cdm::Block,
    chars: &[char],
    bi: usize,
) -> Vec<Finding> {
    let b = builder("TYPO_MISSING_SPACE_AFTER_PUNCTUATION");
    let mut out = Vec::new();

    for i in 0..chars.len().saturating_sub(1) {
        let p = chars[i];
        if !SENTENCE_PUNCT.contains(&p) {
            continue;
        }
        if !chars[i + 1].is_alphabetic() || i == 0 {
            continue;
        }
        let prev = chars[i - 1];
        if !prev.is_alphanumeric() {
            continue;
        }
        if p == '.' {
            // Only a dot that ends a sentence needs a space after it. Inside
            // `LL.M.`, `T.C.` or `Ph.D.` the dot belongs to the word, and
            // inserting a space there breaks the name — which is exactly what
            // happened to a real signature block.
            if abbrev::classify_dot(chars, i) != DotRole::SentenceEnd {
                continue;
            }
            let mut ws = i;
            while ws > 0
                && (chars[ws - 1].is_alphanumeric()
                    || chars[ws - 1] == '\''
                    || chars[ws - 1] == '\u{2019}')
            {
                ws -= 1;
            }
            let word: String = chars[ws..i].iter().collect();
            if word.chars().any(|c| c.is_ascii_digit()) || word.chars().count() < 2 {
                continue;
            }
            // A dot inside an address or a file name has its own conventions.
            if looks_like_address(chars, ws) {
                continue;
            }
        } else if prev.is_ascii_digit() && p == ',' {
            continue; // the decimal comma of `50.000,00`
        }
        let mut f = b.at(
            &block.id,
            SourceLocation::span(bi, i, i + 2),
            0.9,
            format!("\u{201C}{p}\u{201D} işaretinden sonra boşluk yok."),
            "Noktalama işaretinden sonra bir boşluk gelir; işaret kendinden \
             sonraki kelimeye bitişik yazılmaz.",
        );
        f.context = Some(context_snippet(&block.text, i, i + 1));
        f.fix = Some(fix_span(block, i + 1, i + 1, " ", "boşluk eklenir"));
        out.push(f);
    }
    out
}

/// Detect a web or mail address around a position, so its dots are left alone.
fn looks_like_address(chars: &[char], word_start: usize) -> bool {
    let from = word_start.saturating_sub(8);
    let to = (word_start + 40).min(chars.len());
    let window: String = chars[from..to].iter().collect();
    let lower = tr_lower(&window);
    lower.contains("://")
        || lower.contains('@')
        || lower.contains("www.")
        || lower.contains(".com")
        || lower.contains(".org")
        || lower.contains(".gov")
        || lower.contains(".tr/")
}

fn multiple_spaces(
    block: &crate::cdm::Block,
    chars: &[char],
    bi: usize,
    layout: bool,
) -> Vec<Finding> {
    let b = builder("TYPO_MULTIPLE_SPACES");
    let mut out = Vec::new();

    // In the label column, runs of spaces are alignment.
    if layout {
        return out;
    }

    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != ' ' {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        // Leading spaces are indentation, and trailing spaces are invisible
        // slack at the end of a line. Neither is a word-spacing mistake.
        if start == 0 || i >= chars.len() || i - start < 2 {
            continue;
        }
        // A space run touching a tab is part of a layout gesture.
        if chars[start - 1] == '\t' || chars.get(i) == Some(&'\t') {
            continue;
        }
        let mut f = b.at(
            &block.id,
            SourceLocation::span(bi, start, i),
            0.92,
            format!("Kelimeler arasında {} boşluk var.", i - start),
            "Kelimeler arasında tek boşluk bulunur. Fazladan boşluk, hizalama \
             amacıyla eklenmiş olsa bile metinde bozulmaya yol açar.",
        );
        f.context = Some(context_snippet(&block.text, start, i));
        f.fix = Some(fix_span(block, start, i, " ", "tek boşluğa indirilir"));
        out.push(f);
    }
    out
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}' || c == '\u{02BC}'
}

fn apostrophe_spacing(block: &crate::cdm::Block, chars: &[char], bi: usize) -> Vec<Finding> {
    let b = builder("TYPO_APOSTROPHE_SPACING");
    let mut out = Vec::new();

    for i in 1..chars.len().saturating_sub(1) {
        if !is_apostrophe(chars[i]) {
            continue;
        }
        // `Ankara' da` — apostrophe attached to the name, space before the suffix.
        if chars[i - 1].is_alphanumeric() && chars[i + 1] == ' ' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            if is_suffix_run(chars, j) {
                let mut f = b.at(
                    &block.id,
                    SourceLocation::span(bi, i, j),
                    0.9,
                    "Kesme işareti ile ek arasına boşluk girmiş.",
                    "Kesme işaretinden sonra gelen ek, işarete bitişik yazılır: \
                     \u{201C}Ankara\u{2019}da\u{201D}.",
                );
                f.context = Some(context_snippet(&block.text, i, j));
                f.fix = Some(fix_span(block, i + 1, j, "", "boşluk kaldırılır"));
                out.push(f);
                continue;
            }
        }
        // `Ankara 'da` — space before the apostrophe instead.
        if chars[i - 1] == ' ' && is_suffix_run(chars, i + 1) {
            let mut start = i;
            while start > 0 && chars[start - 1] == ' ' {
                start -= 1;
            }
            if start > 0 && chars[start - 1].is_alphanumeric() {
                let mut f = b.at(
                    &block.id,
                    SourceLocation::span(bi, start, i + 1),
                    0.85,
                    "Kesme işaretinden önce boşluk var.",
                    "Kesme işareti, kendinden önceki kelimeye bitişik yazılır.",
                );
                f.context = Some(context_snippet(&block.text, start, i));
                f.fix = Some(fix_span(block, start, i, "", "boşluk kaldırılır"));
                out.push(f);
            }
        }
    }
    out
}

/// A short lowercase run ending at a word boundary: the shape of a Turkish
/// case suffix. Requiring this is what stops `dedi ' evet '` style quotes from
/// being read as a misplaced suffix.
fn is_suffix_run(chars: &[char], from: usize) -> bool {
    let mut n = 0usize;
    let mut i = from;
    while i < chars.len() && chars[i].is_alphabetic() && n < 5 {
        if tr_lower(&chars[i].to_string()) != chars[i].to_string() {
            return false;
        }
        n += 1;
        i += 1;
    }
    if n == 0 || n > 4 {
        return false;
    }
    match chars.get(i) {
        None => true,
        Some(c) => !c.is_alphanumeric(),
    }
}

fn bracket_padding(block: &crate::cdm::Block, chars: &[char], bi: usize) -> Vec<Finding> {
    let b = builder("TYPO_BRACKET_PADDING");
    let mut out = Vec::new();

    for i in 0..chars.len() {
        let opening = matches!(chars[i], '(' | '[');
        let closing = matches!(chars[i], ')' | ']');
        if opening && chars.get(i + 1).map(|c| *c == ' ').unwrap_or(false) {
            let mut j = i + 1;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            let mut f = b.at(
                &block.id,
                SourceLocation::span(bi, i, j),
                0.85,
                "Parantez açıldıktan hemen sonra boşluk var.",
                "Parantezin içeriği, parantez işaretine bitişik yazılır.",
            );
            f.context = Some(context_snippet(&block.text, i, j));
            f.fix = Some(fix_span(block, i + 1, j, "", "boşluk kaldırılır"));
            out.push(f);
        }
        if closing && i > 0 && chars[i - 1] == ' ' {
            let mut start = i;
            while start > 0 && chars[start - 1] == ' ' {
                start -= 1;
            }
            if start == 0 {
                continue;
            }
            let mut f = b.at(
                &block.id,
                SourceLocation::span(bi, start, i + 1),
                0.85,
                "Parantez kapatılmadan önce boşluk var.",
                "Parantezin içeriği, parantez işaretine bitişik yazılır.",
            );
            f.context = Some(context_snippet(&block.text, start, i));
            f.fix = Some(fix_span(block, start, i, "", "boşluk kaldırılır"));
            out.push(f);
        }
    }
    out
}

fn unclosed_pairs(
    block: &crate::cdm::Block,
    chars: &[char],
    bi: usize,
    marker_end: usize,
) -> Vec<Finding> {
    let b = builder("TYPO_UNCLOSED_PAIR");
    let mut out = Vec::new();
    let mut stack: Vec<(char, usize)> = Vec::new();

    for i in marker_end..chars.len() {
        match chars[i] {
            c @ ('(' | '[' | '{') => stack.push((c, i)),
            c @ (')' | ']' | '}') => {
                let expected = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                match stack.last() {
                    Some((open, _)) if *open == expected => {
                        stack.pop();
                    }
                    _ => {
                        // `... şu hâllerde: a) birinci, b) ikinci` — an inline
                        // enumerator, not an unmatched bracket.
                        if is_inline_enumerator(chars, i) {
                            continue;
                        }
                        let mut f = b.at(
                            &block.id,
                            SourceLocation::span(bi, i, i + 1),
                            0.9,
                            format!("\u{201C}{c}\u{201D} kapatıyor; ancak eşleşen bir açılış yok."),
                            "Açılıp kapanan işaretler eşleşmiyor.",
                        );
                        f.context = Some(context_snippet(&block.text, i, i + 1));
                        out.push(f);
                    }
                }
            }
            _ => {}
        }
    }

    for (open, pos) in stack {
        let mut f = b.at(
            &block.id,
            SourceLocation::span(bi, pos, pos + 1),
            0.9,
            format!("\u{201C}{open}\u{201D} açılmış; ancak kapatılmamış."),
            "Açılan parantez, paragrafın sonuna kadar kapatılmamış.",
        );
        f.context = Some(context_snippet(&block.text, pos, pos + 1));
        out.push(f);
    }
    out
}

/// `a)` or `1)` sitting inside a sentence as a list label.
fn is_inline_enumerator(chars: &[char], close: usize) -> bool {
    if chars[close] != ')' || close == 0 {
        return false;
    }
    let label = chars[close - 1];
    if !label.is_alphanumeric() {
        return false;
    }
    // Exactly one label character, preceded by a boundary.
    match close.checked_sub(2).and_then(|i| chars.get(i)) {
        None => true,
        Some(c) => c.is_whitespace() || *c == '(' || *c == ',' || *c == ';',
    }
}

/// Quotes are counted across the whole document.
///
/// A quotation in a petition routinely opens in one paragraph and closes three
/// paragraphs later, so a per-paragraph check would be wrong far more often
/// than it would be right.
fn unbalanced_quotes(ctx: &Context<'_>) -> Vec<Finding> {
    let b = builder("TYPO_UNCLOSED_PAIR");
    let mut out = Vec::new();
    let mut opens = 0usize;
    let mut closes = 0usize;
    let mut straight = 0usize;
    let mut last_open: Option<(String, usize, usize)> = None;

    for block in &ctx.document.blocks {
        for (i, c) in block.text.chars().enumerate() {
            match c {
                '\u{201C}' | '\u{00AB}' => {
                    opens += 1;
                    last_open = Some((block.id.clone(), block.source_location.block_index, i));
                }
                '\u{201D}' | '\u{00BB}' => closes += 1,
                '"' => straight += 1,
                _ => {}
            }
        }
    }

    if opens != closes {
        let (block_id, bi, at) = last_open.unwrap_or_else(|| ("p0".into(), 0, 0));
        let mut f = b.at(
            &block_id,
            SourceLocation::span(bi, at, at + 1),
            0.85,
            format!("Belgede {opens} açılış tırnağı ve {closes} kapanış tırnağı var."),
            "Tırnak işaretleri eşleşmiyor. Alıntı paragraf sınırını aşabileceği için \
             sayım belgenin tamamı üzerinden yapılır.",
        );
        f.context = None;
        out.push(f);
    }
    if straight % 2 == 1 {
        let mut f = b.at(
            "p0",
            SourceLocation::block(0),
            0.7,
            format!("Belgede tek sayıda ({straight}) düz tırnak işareti var."),
            "Düz tırnak işaretleri çift sayıda olmalıdır.",
        );
        f.context = None;
        out.push(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::rules::testing::*;

    const DOUBLE: &str = "TYPO_DOUBLE_PUNCTUATION";
    const BEFORE: &str = "TYPO_SPACE_BEFORE_PUNCTUATION";
    const AFTER: &str = "TYPO_MISSING_SPACE_AFTER_PUNCTUATION";
    const MULTI: &str = "TYPO_MULTIPLE_SPACES";
    const APO: &str = "TYPO_APOSTROPHE_SPACING";
    const PAD: &str = "TYPO_BRACKET_PADDING";
    const PAIR: &str = "TYPO_UNCLOSED_PAIR";

    #[test]
    fn a_doubled_full_stop_is_reported_and_fixable() {
        let f = lint(&["Dava dilekçesi sunulmuştur.. Ekleri de eklenmiştir."]);
        let d = of(&f, DOUBLE);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].fix.as_ref().unwrap().replacement, ".");
    }

    #[test]
    fn an_ellipsis_is_not_a_doubled_full_stop() {
        assert_silent(
            &["Beyanında \u{201C}bilmiyorum...\u{201D} demiştir."],
            DOUBLE,
        );
    }

    #[test]
    fn incompatible_marks_are_reported_without_a_fix() {
        let f = lint(&["Talebimiz şudur:, davanın kabulü."]);
        let d = of(&f, DOUBLE);
        assert_eq!(d.len(), 1);
        assert!(d[0].fix.is_none());
    }

    #[test]
    fn a_space_before_a_comma_is_reported() {
        assert_count(&["Davacı , davalıya karşı dava açmıştır."], BEFORE, 1);
    }

    #[test]
    fn a_missing_space_after_a_full_stop_is_reported() {
        assert_count(&["Dava açılmıştır.Davalı cevap vermiştir."], AFTER, 1);
    }

    #[test]
    fn a_thousands_separator_is_not_a_missing_space() {
        assert_silent(
            &["Alacak tutarı 50.000,00 TL olarak belirlenmiştir."],
            AFTER,
        );
    }

    #[test]
    fn an_abbreviation_dot_is_not_a_missing_space() {
        assert_silent(&["HMK m.119 uyarınca dava açılmıştır."], AFTER);
    }

    #[test]
    fn a_web_address_is_not_a_missing_space() {
        assert_silent(&["Karar www.uyap.gov.tr adresinde yayımlanmıştır."], AFTER);
    }

    #[test]
    fn multiple_spaces_between_words_are_reported() {
        let f = lint(&["Davacı  davalıya karşı dava açmıştır."]);
        let m = of(&f, MULTI);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].fix.as_ref().unwrap().replacement, " ");
    }

    #[test]
    fn leading_indentation_is_not_a_multiple_space_error() {
        assert_silent(&["    Girintili paragraf metni burada yer alır."], MULTI);
    }

    #[test]
    fn a_suffix_detached_from_its_apostrophe_is_reported() {
        assert_count(&["Ankara' da ikamet eden davacı beyan etmiştir."], APO, 1);
    }

    #[test]
    fn a_correctly_written_apostrophe_is_silent() {
        assert_silent(&["Ankara'da ikamet eden davacı beyan etmiştir."], APO);
        assert_silent(
            &["Ankara\u{2019}da ikamet eden davacı beyan etmiştir."],
            APO,
        );
    }

    #[test]
    fn padded_brackets_are_reported() {
        let f = lint(&["Taraf ( davacı ) beyanda bulunmuştur."]);
        assert_eq!(of(&f, PAD).len(), 2);
    }

    #[test]
    fn an_unclosed_bracket_is_reported() {
        assert_count(&["Taraf (davacı beyanda bulunmuştur."], PAIR, 1);
    }

    #[test]
    fn a_stray_closing_bracket_is_reported() {
        assert_count(&["Taraf davacı) beyanda bulunmuştur."], PAIR, 1);
    }

    #[test]
    fn balanced_brackets_are_silent() {
        assert_silent(
            &["Taraf (davacı) beyanda bulunmuştur ve [ek-1] sunulmuştur."],
            PAIR,
        );
    }

    #[test]
    fn an_inline_enumerator_is_not_a_stray_bracket() {
        assert_silent(
            &["Şu hâllerde uygulanır: a) sözleşmenin feshi, b) temerrüt, c) ayıp ihbarı."],
            PAIR,
        );
    }

    #[test]
    fn a_list_marker_is_not_a_stray_bracket() {
        assert_silent(
            &["1) Davanın kabulüne karar verilmesini talep ederiz."],
            PAIR,
        );
    }

    #[test]
    fn a_quotation_spanning_paragraphs_is_not_unbalanced() {
        assert_silent(
            &[
                "Bilirkişi raporunda \u{201C}taşınmazın değeri",
                "güncel piyasa koşullarına göre belirlenmiştir\u{201D} denilmektedir.",
            ],
            PAIR,
        );
    }

    #[test]
    fn an_unclosed_quotation_is_reported_once_for_the_document() {
        let f = lint(&[
            "Bilirkişi raporunda \u{201C}taşınmazın değeri belirlenmiştir denilmektedir.",
            "Bu tespite itiraz edilmiştir.",
        ]);
        assert_eq!(of(&f, PAIR).len(), 1);
    }

    #[test]
    fn ordinary_prose_produces_no_punctuation_findings() {
        let f = lint(&[
            "Sayın İstanbul 5. Asliye Hukuk Mahkemesine",
            "Davacı, davalıya karşı 50.000,00 TL tutarında alacak davası açmıştır.",
            "HMK m. 119 uyarınca dava dilekçesinde bulunması gereken hususlar sunulmuştur.",
        ]);
        assert!(
            f.is_empty(),
            "unexpected findings: {:?}",
            f.iter()
                .map(|x| (&x.rule_id, &x.message))
                .collect::<Vec<_>>()
        );
    }
}
