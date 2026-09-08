//! Characters the author cannot see.
//!
//! These survive copy-paste from PDFs, from UYAP screens and from the web, and
//! they are invisible by definition, so nobody proofreads them out. Every
//! message names the Unicode code point, because "there is a character here you
//! cannot see" is only actionable if the user is told which one.

use super::Context;
use crate::cdm::SourceLocation;
use crate::finding::{Finding, FindingBuilder, Fix};
use std::collections::BTreeSet;

fn builder(id: &str) -> FindingBuilder {
    FindingBuilder::new(crate::finding::rule_info(id).expect("registered rule"))
}

/// An invisible character worth reporting, with what to do about it.
struct Invisible {
    name: &'static str,
    /// What to put in its place. Empty means simply delete it.
    replacement: &'static str,
}

fn classify(c: char) -> Option<Invisible> {
    Some(match c {
        '\u{200B}' => Invisible {
            name: "sıfır genişlikli boşluk",
            replacement: "",
        },
        '\u{200C}' => Invisible {
            name: "sıfır genişlikli bitiştirmeyen",
            replacement: "",
        },
        '\u{200D}' => Invisible {
            name: "sıfır genişlikli bitiştiren",
            replacement: "",
        },
        '\u{FEFF}' => Invisible {
            name: "bayt sırası işareti",
            replacement: "",
        },
        '\u{2060}' => Invisible {
            name: "sözcük bitiştirici",
            replacement: "",
        },
        '\u{00AD}' => Invisible {
            name: "isteğe bağlı tire",
            replacement: "",
        },
        '\u{180E}' => Invisible {
            name: "Moğol sesli harf ayırıcı",
            replacement: "",
        },
        '\u{00A0}' => Invisible {
            name: "bölünmez boşluk",
            replacement: " ",
        },
        '\u{2007}' => Invisible {
            name: "rakam genişliğinde boşluk",
            replacement: " ",
        },
        '\u{202F}' => Invisible {
            name: "dar bölünmez boşluk",
            replacement: " ",
        },
        '\u{2009}' => Invisible {
            name: "ince boşluk",
            replacement: " ",
        },
        '\u{2002}' => Invisible {
            name: "en boşluğu",
            replacement: " ",
        },
        '\u{2003}' => Invisible {
            name: "em boşluğu",
            replacement: " ",
        },
        c if (c.is_control() && c != '\t' && c != '\n' && c != '\r') => Invisible {
            name: "denetim karakteri",
            replacement: "",
        },
        _ => return None,
    })
}

pub fn run(ctx: &Context<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    let inv = builder("TYPO_INVISIBLE_CHAR");
    let rep = builder("TYPO_REPLACEMENT_CHAR");
    let tab = builder("TYPO_TAB_IN_TEXT");
    let mut dashes: BTreeSet<char> = BTreeSet::new();

    for block in &ctx.document.blocks {
        let bi = block.source_location.block_index;
        out.extend(tabs_in_prose(ctx, block, &tab, bi));
        for (i, c) in block.text.chars().enumerate() {
            if c == '\u{FFFD}' {
                let mut f = rep.at(
                    &block.id,
                    SourceLocation::span(bi, i, i + 1),
                    0.99,
                    "Metinde bozuk bir karakter var (U+FFFD).",
                    "Bu karakter, metnin bir bölümünün yanlış kodlamayla \
                     okunduğunu gösterir. Doğru karakter belgeden anlaşılamadığı için \
                     otomatik düzeltme önerilmez.",
                );
                f.context = Some(crate::text::excerpt(&block.text, i, 30));
                out.push(f);
                continue;
            }
            if c == '\t' {
                // Tabs are handled as runs, below. A petition's label column is
                // three or four tabs wide, and reporting each one separately
                // produced four findings for one piece of alignment.
                continue;
            }
            if matches!(c, '\u{2010}'..='\u{2015}' | '\u{2212}' | '-') {
                dashes.insert(c);
            }
            let Some(kind) = classify(c) else { continue };
            let mut f = inv.at(
                &block.id,
                SourceLocation::span(bi, i, i + 1),
                0.9,
                format!(
                    "Görünmeyen bir karakter var: {} (U+{:04X}).",
                    kind.name, c as u32
                ),
                "Bu karakter ekranda görünmez; ancak metinde yer alır. Genellikle \
                 başka bir uygulamadan yapılan kopyalamayla gelir ve arama, sıralama \
                 veya karşılaştırma işlemlerini bozar.",
            );
            f.context = Some(crate::text::excerpt(&block.text, i, 30));
            f.fix = Some(Fix {
                block_id: block.id.clone(),
                char_start: i,
                char_end: i + 1,
                original: c.to_string(),
                replacement: kind.replacement.to_string(),
                description: if kind.replacement.is_empty() {
                    "karakter kaldırılır".into()
                } else {
                    "normal boşluğa çevrilir".into()
                },
            });
            out.push(f);
        }
    }

    if dashes.len() > 1 {
        let names: Vec<String> = dashes
            .iter()
            .map(|c| format!("U+{:04X}", *c as u32))
            .collect();
        let b = builder("TYPO_MIXED_DASH");
        let mut f = b.at(
            "p0",
            SourceLocation::block(0),
            0.6,
            format!(
                "Belgede {} farklı tire karakteri kullanılmış: {}.",
                dashes.len(),
                names.join(", ")
            ),
            "Kısa çizgi, uzun çizgi ve eksi işareti görünüş olarak benzer; ancak \
             farklı karakterlerdir. Aynı belgede karışık kullanılmaları, arama ve \
             karşılaştırma sırasında beklenmedik sonuç verir.",
        );
        f.context = None;
        out.push(f);
    }
    out
}

/// Tabs that are doing alignment are not findings.
///
/// A petition's header is a label/value column held together by tabs, and its
/// numbered items are separated from their markers the same way. Replacing
/// those with spaces — which is what this rule used to offer — collapses the
/// column. Only a tab inside running prose is reported, and even then the whole
/// run is one finding rather than one per tab.
fn tabs_in_prose(
    ctx: &Context<'_>,
    block: &crate::cdm::Block,
    builder: &FindingBuilder,
    bi: usize,
) -> Vec<Finding> {
    let chars: Vec<char> = block.text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] != '\t' {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i] == '\t' {
            i += 1;
        }
        if ctx.zones().is_structural_tab(bi, start) {
            continue;
        }
        let count = i - start;
        let mut f = builder.at(
            &block.id,
            SourceLocation::span(bi, start, i),
            0.7,
            if count == 1 {
                "Cümlenin içinde sekme karakteri var.".to_string()
            } else {
                format!("Cümlenin içinde {count} sekme karakteri var.")
            },
            "Bu sekme bir hizalama sütununun parçası değil; cümlenin ortasında \
             duruyor. Yazı tipi veya kenar boşluğu değiştiğinde metin kayar.",
        );
        f.context = Some(crate::rules::context_snippet(&block.text, start, i));
        f.fix = Some(Fix {
            block_id: block.id.clone(),
            char_start: start,
            char_end: i,
            original: chars[start..i].iter().collect(),
            replacement: " ".into(),
            description: "sekme yerine tek boşluk konur".into(),
        });
        out.push(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::rules::testing::*;

    const INV: &str = "TYPO_INVISIBLE_CHAR";
    const REP: &str = "TYPO_REPLACEMENT_CHAR";
    const DASH: &str = "TYPO_MIXED_DASH";
    const TAB: &str = "TYPO_TAB_IN_TEXT";

    #[test]
    fn a_zero_width_space_is_reported_with_its_code_point() {
        let f = lint(&["Davacı\u{200B} beyanda bulunmuştur."]);
        let i = of(&f, INV);
        assert_eq!(i.len(), 1);
        assert!(
            i[0].message.contains("U+200B"),
            "message was {:?}",
            i[0].message
        );
        assert_eq!(i[0].fix.as_ref().unwrap().replacement, "");
    }

    #[test]
    fn a_non_breaking_space_is_converted_rather_than_deleted() {
        let f = lint(&["Davacı\u{00A0}beyanda bulunmuştur."]);
        let i = of(&f, INV);
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].fix.as_ref().unwrap().replacement, " ");
    }

    #[test]
    fn a_soft_hyphen_is_reported() {
        assert_count(&["Ta\u{00AD}şınmaz devredilmiştir."], INV, 1);
    }

    #[test]
    fn a_replacement_character_is_an_error_without_a_fix() {
        let f = lint(&["Ta\u{FFFD}ınmaz devredilmiştir."]);
        let r = of(&f, REP);
        assert_eq!(r.len(), 1);
        assert!(r[0].fix.is_none());
        assert_eq!(r[0].severity, crate::finding::Severity::Error);
    }

    #[test]
    fn ordinary_text_has_no_invisible_characters() {
        assert_silent(
            &["Davacı beyanda bulunmuştur ve delillerini sunmuştur."],
            INV,
        );
    }

    #[test]
    fn a_single_dash_style_is_not_a_mixed_dash_finding() {
        assert_silent(&["Ek-1 ve Ek-2 sunulmuştur.", "Ek-3 de eklenmiştir."], DASH);
    }

    #[test]
    fn two_dash_styles_in_one_document_are_reported_once() {
        let f = lint(&[
            "Ek-1 sunulmuştur.",
            "Taraflar \u{2013} davacı ve davalı \u{2013} anlaşmıştır.",
        ]);
        assert_eq!(of(&f, DASH).len(), 1);
    }

    #[test]
    fn a_tab_inside_running_prose_is_reported() {
        assert_count(&["Davacı bu nedenle\tmahkemeye başvurmuştur."], TAB, 1);
    }

    #[test]
    fn a_leading_tab_is_indentation_and_is_not_reported() {
        assert_silent(&["\tGirintili paragraf metni burada yer almaktadır."], TAB);
    }

    #[test]
    fn the_label_column_of_a_petition_is_left_alone() {
        assert_silent(&["DAVACI\t\t\t:\tProf. Dr. Nurşen MAZICI"], TAB);
        assert_silent(&["VEKİLİ\t\t\t: \tAv. Raci Çetin YÜKSEKBAŞ"], TAB);
        assert_silent(&["HUKUKİ NEDENLER\t\t\t\t: TBK, TMK, HMK"], TAB);
        assert_silent(&["\t\t\t\t\tZekeriyaköy Mah. Şair Nazım Sk."], TAB);
    }

    #[test]
    fn the_tab_after_an_item_marker_is_left_alone() {
        assert_silent(
            &["1.\tMüvekkil Prof. Nurşen Mazıcı beyanda bulunmuştur."],
            TAB,
        );
        assert_silent(&["A.\tMÜVEKKİL, TBK M. 310 UYARINCA HAK SAHİBİDİR"], TAB);
        assert_silent(
            &["7/b.\tTemizliği ve Düzeni Kolaydır: daire tek başına"],
            TAB,
        );
    }

    #[test]
    fn a_run_of_tabs_in_prose_is_one_finding_not_one_per_tab() {
        let f = lint(&["Davacı bu nedenle\t\t\tmahkemeye başvurmuştur."]);
        assert_eq!(of(&f, TAB).len(), 1);
    }
}
