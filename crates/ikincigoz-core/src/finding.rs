//! The output vocabulary of the lint engine.

use crate::cdm::SourceLocation;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Something the engine knows mechanically, with no room for taste.
    Error,
    /// Very probably wrong, but a deliberate choice is conceivable.
    Warning,
    /// No verdict is possible; worth a human glance.
    Review,
}

impl Severity {
    pub fn label_tr(self) -> &'static str {
        match self {
            Severity::Error => "Kesin hata",
            Severity::Warning => "Uyarı",
            Severity::Review => "İncele",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Review => 2,
        }
    }
}

/// A concrete edit the engine is confident enough to offer.
///
/// A fix is only ever attached when the replacement is mechanical. Anything
/// requiring judgement is reported without one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fix {
    pub block_id: String,
    /// Char offsets into the block's `text`.
    pub char_start: usize,
    pub char_end: usize,
    /// Exactly the text currently occupying the span, for verification.
    pub original: String,
    pub replacement: String,
    /// Shown in the correction preview, e.g. `çift boşluk kaldırıldı`.
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    /// Short human title, e.g. `Başlık sırası bozuk`.
    pub title: String,
    pub severity: Severity,
    /// `0.0..=1.0`. Used for ordering and for the confidence gate.
    pub confidence: f32,
    /// One sentence saying exactly what was seen and what was expected.
    pub message: String,
    /// Why the engine considers this a problem. Answers "why did it say that?".
    pub explanation: String,
    pub block_id: String,
    pub location: SourceLocation,
    /// A short excerpt of surrounding text, for the UI only.
    pub context: Option<String>,
    pub fix: Option<Fix>,
}

impl Finding {
    pub fn sort_key(&self) -> (u8, usize, usize) {
        (
            self.severity.rank(),
            self.location.block_index,
            self.location.char_start.unwrap_or(0),
        )
    }
}

/// Static description of a rule, used for the settings UI and the reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub default_severity: Severity,
    pub category: &'static str,
    pub description: &'static str,
}

/// Builder that stamps the shared fields so individual rules stay short.
pub struct FindingBuilder {
    pub info: &'static RuleInfo,
}

impl FindingBuilder {
    pub fn new(info: &'static RuleInfo) -> Self {
        Self { info }
    }

    pub fn at(
        &self,
        block_id: &str,
        location: SourceLocation,
        confidence: f32,
        message: impl Into<String>,
        explanation: impl Into<String>,
    ) -> Finding {
        Finding {
            rule_id: self.info.id.to_string(),
            title: self.info.title.to_string(),
            severity: self.info.default_severity,
            confidence,
            message: message.into(),
            explanation: explanation.into(),
            block_id: block_id.to_string(),
            location,
            context: None,
            fix: None,
        }
    }
}

/// Every rule the build ships, in report order.
pub const RULES: &[RuleInfo] = &[
    // --- Sequence and hierarchy -------------------------------------------
    RuleInfo {
        id: "SEC_SEQ_GAP",
        title: "Sıra numarasında atlama",
        default_severity: Severity::Error,
        category: "Sıra ve hiyerarşi",
        description: "Aynı düzeydeki başlık veya madde numaralarında atlanan bir sıra var.",
    },
    RuleInfo {
        id: "SEC_SEQ_DUPLICATE",
        title: "Sıra numarası tekrarı",
        default_severity: Severity::Error,
        category: "Sıra ve hiyerarşi",
        description: "Aynı düzeyde aynı sıra numarası birden çok kez kullanılmış.",
    },
    RuleInfo {
        id: "SEC_SEQ_BACKTRACK",
        title: "Sıra numarası geriye dönüyor",
        default_severity: Severity::Error,
        category: "Sıra ve hiyerarşi",
        description: "Sıra numaraları artan düzende ilerlemiyor.",
    },
    RuleInfo {
        id: "SEC_HIERARCHY_ORPHAN",
        title: "Üst başlığı olmayan alt başlık",
        default_severity: Severity::Warning,
        category: "Sıra ve hiyerarşi",
        description: "Alt düzey bir başlık, kendisini kapsayan üst başlık olmadan açılmış.",
    },
    RuleInfo {
        id: "SEC_EMPTY_SECTION",
        title: "İçeriği olmayan başlık",
        default_severity: Severity::Warning,
        category: "Sıra ve hiyerarşi",
        description: "Başlık var; ancak altında metin yok.",
    },
    // --- Repetition --------------------------------------------------------
    RuleInfo {
        id: "TEXT_WORD_DUPLICATION",
        title: "Kelime tekrarı",
        default_severity: Severity::Error,
        category: "Tekrar",
        description: "Aynı kelime arka arkaya iki kez yazılmış.",
    },
    RuleInfo {
        id: "TEXT_SENTENCE_DUPLICATION",
        title: "Cümle tekrarı",
        default_severity: Severity::Error,
        category: "Tekrar",
        description: "Birbirini izleyen iki cümle birebir aynı.",
    },
    RuleInfo {
        id: "TEXT_PARAGRAPH_DUPLICATION",
        title: "Paragraf tekrarı",
        default_severity: Severity::Error,
        category: "Tekrar",
        description: "Belgede birebir aynı paragraf birden çok kez yer alıyor.",
    },
    RuleInfo {
        id: "TEXT_NEAR_DUPLICATE",
        title: "Birbirine çok benzeyen paragraflar",
        default_severity: Severity::Review,
        category: "Tekrar",
        description: "İki paragraf neredeyse aynı. Bilinçli bir tekrar olabilir.",
    },
    // --- Punctuation and spacing -------------------------------------------
    RuleInfo {
        id: "TYPO_DOUBLE_PUNCTUATION",
        title: "Çift noktalama",
        default_severity: Severity::Error,
        category: "Noktalama",
        description: "Arka arkaya yinelenen veya birbiriyle bağdaşmayan noktalama işaretleri.",
    },
    RuleInfo {
        id: "TYPO_SPACE_BEFORE_PUNCTUATION",
        title: "Noktalamadan önce boşluk",
        default_severity: Severity::Error,
        category: "Noktalama",
        description: "Noktalama işaretinden önce gereksiz boşluk bırakılmış.",
    },
    RuleInfo {
        id: "TYPO_MISSING_SPACE_AFTER_PUNCTUATION",
        title: "Noktalamadan sonra boşluk yok",
        default_severity: Severity::Error,
        category: "Noktalama",
        description: "Noktalama işaretinden sonra gelmesi gereken boşluk yok.",
    },
    RuleInfo {
        id: "TYPO_MULTIPLE_SPACES",
        title: "Çoklu boşluk",
        default_severity: Severity::Error,
        category: "Noktalama",
        description: "Kelimeler arasında birden fazla boşluk var.",
    },
    RuleInfo {
        id: "TYPO_APOSTROPHE_SPACING",
        title: "Kesme işaretinden sonra boşluk",
        default_severity: Severity::Error,
        category: "Noktalama",
        description: "Kesme işareti ile ek arasına boşluk girmiş: `Ankara' da`.",
    },
    RuleInfo {
        id: "TYPO_BRACKET_PADDING",
        title: "Parantez içinde gereksiz boşluk",
        default_severity: Severity::Warning,
        category: "Noktalama",
        description: "Parantezin hemen içinde boşluk bırakılmış.",
    },
    RuleInfo {
        id: "TYPO_UNCLOSED_PAIR",
        title: "Kapanmamış işaret çifti",
        default_severity: Severity::Error,
        category: "Noktalama",
        description: "Açılan parantez veya tırnak kapatılmamış ya da ters eşleşmiş.",
    },
    // --- Invisible characters ----------------------------------------------
    RuleInfo {
        id: "TYPO_INVISIBLE_CHAR",
        title: "Görünmez karakter",
        default_severity: Severity::Warning,
        category: "Görünmez karakterler",
        description: "Ekranda görünmeyen; ancak metinde yer alan bir karakter bulundu.",
    },
    RuleInfo {
        id: "TYPO_REPLACEMENT_CHAR",
        title: "Bozuk karakter",
        default_severity: Severity::Error,
        category: "Görünmez karakterler",
        description: "Kodlama hatasından kaynaklanan bozuk karakter var.",
    },
    RuleInfo {
        id: "TYPO_MIXED_DASH",
        title: "Karışık tire kullanımı",
        default_severity: Severity::Review,
        category: "Görünmez karakterler",
        description: "Belgede birden çok tire karakteri bir arada kullanılmış.",
    },
    RuleInfo {
        id: "TYPO_TAB_IN_TEXT",
        title: "Metin içinde sekme",
        default_severity: Severity::Warning,
        category: "Görünmez karakterler",
        description: "Hizalama için sekme karakteri kullanılmış.",
    },
    // --- Draft residue ------------------------------------------------------
    RuleInfo {
        id: "DRAFT_PLACEHOLDER",
        title: "Taslak artığı",
        default_severity: Severity::Error,
        category: "Taslak artıkları",
        description: "Doldurulmamış yer tutucu veya taslak notu belgede kalmış.",
    },
    RuleInfo {
        id: "DRAFT_DANGLING_CONNECTOR",
        title: "Yarım kalmış bağlaç",
        default_severity: Severity::Warning,
        category: "Taslak artıkları",
        description: "Paragraf bir bağlaçla bitiyor; cümle tamamlanmamış görünüyor.",
    },
    RuleInfo {
        id: "DRAFT_EMPTY_NUMBERED",
        title: "Boş numaralı madde",
        default_severity: Severity::Warning,
        category: "Taslak artıkları",
        description: "Numara verilmiş; ancak maddenin içeriği yazılmamış.",
    },
    // --- References ---------------------------------------------------------
    RuleInfo {
        id: "REF_ATTACHMENT_SEQ_GAP",
        title: "Ek numarasında atlama",
        default_severity: Severity::Error,
        category: "Ekler ve atıflar",
        description: "Ek numaralandırmasında atlanan bir sıra var.",
    },
    RuleInfo {
        id: "REF_ATTACHMENT_MISSING",
        title: "Bulunmayan eke atıf",
        default_severity: Severity::Error,
        category: "Ekler ve atıflar",
        description: "Metin bir eke atıf yapıyor; ancak o ek belgede tanımlı değil.",
    },
    RuleInfo {
        id: "REF_ATTACHMENT_UNUSED",
        title: "Metinde anılmayan ek",
        default_severity: Severity::Review,
        category: "Ekler ve atıflar",
        description: "Ek listesinde yer alan bir ek, metin içinde hiç anılmamış.",
    },
    RuleInfo {
        id: "REF_SECTION_BROKEN",
        title: "Bulunmayan bölüme atıf",
        default_severity: Severity::Warning,
        category: "Ekler ve atıflar",
        description: "Metin, belgede bulunmayan bir başlığa veya maddeye atıf yapıyor.",
    },
    // --- Style --------------------------------------------------------------
    RuleInfo {
        id: "STYLE_HEADING_OUTLIER",
        title: "Başlık biçimi diğerlerinden farklı",
        default_severity: Severity::Warning,
        category: "Biçim",
        description: "Aynı düzeydeki başlıkların biçimi bir başlıkta farklılaşıyor.",
    },
    RuleInfo {
        id: "STYLE_BODY_OUTLIER",
        title: "Gövde biçimi diğerlerinden farklı",
        default_severity: Severity::Warning,
        category: "Biçim",
        description: "Bir paragrafın biçimi, belgenin geri kalanından ayrılıyor.",
    },
    RuleInfo {
        id: "STYLE_PROFILE_DEVIATION",
        title: "Belge standardından sapma",
        default_severity: Severity::Warning,
        category: "Biçim",
        description: "Paragraf biçimi, seçilen belge standardıyla uyuşmuyor.",
    },
    // --- Internal consistency -----------------------------------------------
    RuleInfo {
        id: "CONSISTENCY_CITATION_FORMAT",
        title: "Mevzuat atfı biçimi tutarsız",
        default_severity: Severity::Review,
        category: "Tutarlılık",
        description: "Aynı mevzuat atfı belgede farklı biçimlerde yazılmış.",
    },
    RuleInfo {
        id: "CONSISTENCY_DATE_FORMAT",
        title: "Tarih biçimi tutarsız",
        default_severity: Severity::Review,
        category: "Tutarlılık",
        description: "Belgede birden çok tarih biçimi kullanılmış.",
    },
    RuleInfo {
        id: "CONSISTENCY_AMOUNT_FORMAT",
        title: "Tutar biçimi tutarsız",
        default_severity: Severity::Review,
        category: "Tutarlılık",
        description: "Para tutarları belgede farklı biçimlerde yazılmış.",
    },
    RuleInfo {
        id: "CONSISTENCY_TERM_SPELLING",
        title: "Terim yazımı tutarsız",
        default_severity: Severity::Review,
        category: "Tutarlılık",
        description: "Aynı terim belgede farklı biçimlerde yazılmış.",
    },
    RuleInfo {
        id: "CONSISTENCY_AMOUNT_WORDS",
        title: "Rakam ile yazı uyuşmuyor",
        default_severity: Severity::Error,
        category: "Tutarlılık",
        description: "Parantez içinde yazıyla belirtilen tutar, rakamla yazılan tutardan farklı.",
    },
    // --- Orthography ---------------------------------------------------------
    RuleInfo {
        id: "ORTHO_SEPARATE_WRITING",
        title: "Ayrı yazılması gereken kelime",
        default_severity: Severity::Error,
        category: "Yazım",
        description: "Türkçede ayrı yazılan bir kelime bitişik yazılmış.",
    },
    RuleInfo {
        id: "ORTHO_USER_DICTIONARY",
        title: "Kullanıcı düzeltme kuralı",
        default_severity: Severity::Warning,
        category: "Yazım",
        description: "Kullanıcının tanımladığı bir düzeltme kuralı eşleşti.",
    },
    // --- Morphology -----------------------------------------------------------
    RuleInfo {
        id: "MORPH_SAFE_REPLACEMENT",
        title: "Çekimli biçim için düzeltme",
        default_severity: Severity::Warning,
        category: "Yazım",
        description: "Kullanıcı sözlüğündeki bir terimin çekimli biçimi bulundu ve güvenle dönüştürülebiliyor.",
    },
    RuleInfo {
        id: "MORPH_AMBIGUOUS",
        title: "Çekimli biçim güvenle dönüştürülemedi",
        default_severity: Severity::Review,
        category: "Yazım",
        description: "Terimin çekimli biçimi bulundu; ancak otomatik dönüşüm güvenli değil.",
    },
];

pub fn rule_info(id: &str) -> Option<&'static RuleInfo> {
    RULES.iter().find(|r| r.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn rule_ids_are_unique() {
        let mut seen = HashSet::new();
        for r in RULES {
            assert!(seen.insert(r.id), "duplicate rule id {}", r.id);
        }
    }

    #[test]
    fn rule_ids_use_the_agreed_prefixes() {
        const PREFIXES: &[&str] = &[
            "SEC_",
            "TEXT_",
            "TYPO_",
            "STYLE_",
            "ORTHO_",
            "REF_",
            "DRAFT_",
            "MORPH_",
            "CONSISTENCY_",
        ];
        for r in RULES {
            assert!(
                PREFIXES.iter().any(|p| r.id.starts_with(p)),
                "rule id {} has no known prefix",
                r.id
            );
        }
    }

    #[test]
    fn severity_orders_error_before_review() {
        let mut v = vec![Severity::Review, Severity::Error, Severity::Warning];
        v.sort();
        assert_eq!(
            v,
            vec![Severity::Error, Severity::Warning, Severity::Review]
        );
    }
}
