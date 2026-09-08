//! The user's local dictionary and correction rules.
//!
//! Everything here lives on the machine as one JSON file. There is no shared
//! or remote dictionary, and no bundled corpus of uncertain provenance: the
//! only word list İkinciGöz ships is the small, hand-written one in
//! [`crate::rules::ortho`].

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A term the user has told the engine to replace, e.g. `mecur` -> `kiralanan`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrectionRule {
    pub from: String,
    pub to: String,
    /// When true, inflected forms of `from` are matched too and the suffix is
    /// carried across to `to`. See [`crate::morph`].
    #[serde(default = "default_true")]
    pub inflect: bool,
    /// Optional note shown in the correction preview.
    #[serde(default)]
    pub note: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UserDictionary {
    /// Words the user has confirmed are correct. Case-folded on insert.
    #[serde(default)]
    accepted: Vec<String>,
    /// Replacement rules, keyed by the folded source term so lookups are cheap
    /// and a term cannot be defined twice with different targets.
    #[serde(default)]
    corrections: BTreeMap<String, CorrectionRule>,
}

impl UserDictionary {
    pub fn accept(&mut self, word: &str) {
        let w = crate::text::tr_lower(word.trim());
        if w.is_empty() || self.accepted.iter().any(|a| a == &w) {
            return;
        }
        self.accepted.push(w);
    }

    pub fn unaccept(&mut self, word: &str) {
        let w = crate::text::tr_lower(word.trim());
        self.accepted.retain(|a| a != &w);
    }

    pub fn is_accepted(&self, word: &str) -> bool {
        let w = crate::text::tr_lower(word.trim());
        self.accepted.iter().any(|a| a == &w)
    }

    pub fn accepted_words(&self) -> &[String] {
        &self.accepted
    }

    pub fn add_correction(&mut self, rule: CorrectionRule) -> Result<(), &'static str> {
        let from = rule.from.trim();
        let to = rule.to.trim();
        if from.is_empty() || to.is_empty() {
            return Err("empty");
        }
        if crate::text::tr_lower(from) == crate::text::tr_lower(to) {
            return Err("identical");
        }
        // A multi-word source cannot be inflected safely, so refuse rather than
        // silently produce wrong suffixes.
        if from.split_whitespace().count() > 1 && rule.inflect {
            return Err("multiword_inflect");
        }
        let key = crate::text::tr_lower(from);
        self.corrections.insert(
            key,
            CorrectionRule {
                from: from.to_string(),
                to: to.to_string(),
                ..rule
            },
        );
        Ok(())
    }

    pub fn remove_correction(&mut self, from: &str) {
        self.corrections.remove(&crate::text::tr_lower(from.trim()));
    }

    pub fn corrections(&self) -> impl Iterator<Item = &CorrectionRule> {
        self.corrections.values()
    }

    pub fn correction_for(&self, folded: &str) -> Option<&CorrectionRule> {
        self.corrections.get(folded)
    }

    pub fn is_empty(&self) -> bool {
        self.accepted.is_empty() && self.corrections.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(from: &str, to: &str) -> CorrectionRule {
        CorrectionRule {
            from: from.into(),
            to: to.into(),
            inflect: true,
            note: None,
        }
    }

    #[test]
    fn accepted_words_are_matched_case_insensitively_in_turkish() {
        let mut d = UserDictionary::default();
        d.accept("Tevdiat");
        assert!(d.is_accepted("tevdiat"));
        assert!(d.is_accepted("TEVDİAT"));
        assert!(!d.is_accepted("tevdi"));
    }

    #[test]
    fn accepting_the_same_word_twice_does_not_duplicate_it() {
        let mut d = UserDictionary::default();
        d.accept("mecur");
        d.accept("MECUR");
        assert_eq!(d.accepted_words().len(), 1);
    }

    #[test]
    fn a_correction_to_itself_is_refused() {
        let mut d = UserDictionary::default();
        assert!(d.add_correction(rule("mecur", "Mecur")).is_err());
    }

    #[test]
    fn a_multi_word_source_cannot_be_declared_inflectable() {
        let mut d = UserDictionary::default();
        assert!(d
            .add_correction(rule("kira sözleşmesi", "kira akdi"))
            .is_err());
        let mut plain = rule("kira sözleşmesi", "kira akdi");
        plain.inflect = false;
        assert!(d.add_correction(plain).is_ok());
    }

    #[test]
    fn corrections_round_trip_through_json() {
        let mut d = UserDictionary::default();
        d.accept("tevdiat");
        d.add_correction(rule("mecur", "kiralanan")).unwrap();
        let json = serde_json::to_string(&d).unwrap();
        let back: UserDictionary = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
        assert!(back.correction_for("mecur").is_some());
    }

    #[test]
    fn redefining_a_term_replaces_rather_than_duplicates() {
        let mut d = UserDictionary::default();
        d.add_correction(rule("mecur", "kiralanan")).unwrap();
        d.add_correction(rule("mecur", "kiralanan taşınmaz"))
            .unwrap();
        assert_eq!(d.corrections().count(), 1);
        assert_eq!(d.correction_for("mecur").unwrap().to, "kiralanan taşınmaz");
    }
}
