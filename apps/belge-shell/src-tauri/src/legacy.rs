//! Eski uygulamaların kullanıcı verisinin okunması (talep §11).
//!
//! Üç değişmez:
//!
//! 1. **Eski dizinler asla silinmez, taşınmaz, değiştirilmez.** Bu modül eski
//!    konumları yalnız `read_to_string` ile açar.
//! 2. **Zaten ayarlanmış bir değer ezilmez.** Migration boş alanları doldurur;
//!    kullanıcının birleşik uygulamada yaptığı bir seçimi geri almaz.
//! 3. **Bir kaynağın okunamaması hata değildir.** Uygulama kurulmamış olabilir;
//!    rapor "bulunamadı" der ve devam eder.
//!
//! Kapsam dışı: DüzenEk'in seçtiği LibreOffice yolu ile Değişikİş'in kenar
//! çubuğu tercihi WebView localStorage'ında (SQLite) tutuluyor. Bunları okumak
//! yeni bir SQLite bağımlılığı gerektirir ve ilgili modüller henüz taşınmadı.
//! Bu modül localStorage'ı yalnız **tespit eder** ve raporlar; okuma işi
//! Phase 6'ya (DüzenEk migration'ı) aittir. Ayrıntı için
//! `duzenek_localstorage_status`.

use crate::paths;
use crate::settings::Settings;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Bir eski kaynağın migration sonucundaki durumu.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceStatus {
    /// Okundu ve en az bir değer aktarıldı.
    Migrated { fields: Vec<String> },
    /// Kaynak bulundu ama aktarılacak yeni bir şey yoktu.
    NothingToDo,
    /// Kaynak bu makinede yok.
    NotFound,
    /// Kaynak var ama okunamadı veya çözümlenemedi. Uygulama yine de açılır.
    Unreadable { detail: String },
    /// Kaynak var, elle ele alınması gerekiyor (bkz. modül başlığı).
    NeedsManualStep { detail: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReport {
    pub app: String,
    pub identifier: String,
    pub path: String,
    pub status: SourceStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    /// Migration bu çalıştırmada gerçekten bir şey yazdı mı?
    pub changed: bool,
    pub sources: Vec<SourceReport>,
}

// --------------------------------------------------------------------- Tavzih

#[derive(serde::Deserialize)]
struct TavzihPrefs {
    #[serde(default)]
    output_dir: Option<String>,
    #[serde(default)]
    accepted_terms: Option<u32>,
}

fn migrate_tavzih(dir: &Path, s: &mut Settings) -> SourceStatus {
    let file = dir.join("preferences.json");
    if !file.is_file() {
        return SourceStatus::NotFound;
    }
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) => {
            return SourceStatus::Unreadable {
                detail: e.to_string(),
            }
        }
    };
    let prefs: TavzihPrefs = match serde_json::from_str(&text) {
        Ok(p) => p,
        Err(e) => {
            return SourceStatus::Unreadable {
                detail: e.to_string(),
            }
        }
    };

    let mut fields = Vec::new();
    if s.output_dir.is_none() {
        if let Some(v) = prefs.output_dir {
            s.output_dir = Some(v);
            fields.push("outputDir".to_string());
        }
    }
    if s.accepted_terms.is_none() {
        if let Some(v) = prefs.accepted_terms {
            s.accepted_terms = Some(v);
            fields.push("acceptedTerms".to_string());
        }
    }
    if fields.is_empty() {
        SourceStatus::NothingToDo
    } else {
        SourceStatus::Migrated { fields }
    }
}

// ------------------------------------------------------------------ İkinciGöz

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct IkinciGozSettings {
    #[serde(default)]
    disabled_rules: Option<Vec<String>>,
    #[serde(default)]
    include_review: Option<bool>,
    #[serde(default)]
    source_read_only: Option<bool>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    text_scale: Option<u16>,
    #[serde(default)]
    high_contrast: Option<String>,
    #[serde(default)]
    reduce_motion: Option<String>,
    #[serde(default)]
    linear_results: Option<bool>,
}

/// İkinciGöz'ün sözlük ve profil dosyaları. Ayrıştırılmaz: birleşik uygulamaya
/// **birebir kopyalanır**. Şemayı burada yeniden yorumlamak, kullanıcının
/// öğrettiği kelimeleri sessizce bozmanın en kolay yoludur.
const IKINCIGOZ_VERBATIM: [&str; 2] = ["dictionary.json", "profiles.json"];

fn migrate_ikincigoz(dir: &Path, target_dir: &Path, s: &mut Settings) -> SourceStatus {
    let file = dir.join("settings.json");
    let mut fields = Vec::new();

    if file.is_file() {
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) => {
                return SourceStatus::Unreadable {
                    detail: e.to_string(),
                }
            }
        };
        let old: IkinciGozSettings = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                return SourceStatus::Unreadable {
                    detail: e.to_string(),
                }
            }
        };
        let defaults = Settings::default();
        // Yalnız kullanıcının varsayılandan saptığı değerler taşınır; böylece
        // "hiç dokunmadım" ile "bilerek varsayılana getirdim" karışmaz.
        if s.theme == defaults.theme {
            if let Some(v) = old.theme {
                if v != defaults.theme {
                    s.theme = v;
                    fields.push("theme".to_string());
                }
            }
        }
        if s.text_scale == defaults.text_scale {
            if let Some(v) = old.text_scale {
                if v != defaults.text_scale {
                    s.text_scale = v;
                    fields.push("textScale".to_string());
                }
            }
        }
        if s.high_contrast == defaults.high_contrast {
            if let Some(v) = old.high_contrast {
                if v != defaults.high_contrast {
                    s.high_contrast = v;
                    fields.push("highContrast".to_string());
                }
            }
        }
        if s.reduce_motion == defaults.reduce_motion {
            if let Some(v) = old.reduce_motion {
                if v != defaults.reduce_motion {
                    s.reduce_motion = v;
                    fields.push("reduceMotion".to_string());
                }
            }
        }
        if s.disabled_rules.is_empty() {
            if let Some(v) = old.disabled_rules {
                if !v.is_empty() {
                    s.disabled_rules = v;
                    fields.push("disabledRules".to_string());
                }
            }
        }
        if s.include_review == defaults.include_review {
            if let Some(v) = old.include_review {
                if v != defaults.include_review {
                    s.include_review = v;
                    fields.push("includeReview".to_string());
                }
            }
        }
        if s.source_read_only == defaults.source_read_only {
            if let Some(v) = old.source_read_only {
                if v != defaults.source_read_only {
                    s.source_read_only = v;
                    fields.push("sourceReadOnly".to_string());
                }
            }
        }
        if s.linear_results == defaults.linear_results {
            if let Some(v) = old.linear_results {
                if v != defaults.linear_results {
                    s.linear_results = v;
                    fields.push("linearResults".to_string());
                }
            }
        }
    }

    // Sözlük ve profiller: hedefte yoksa birebir kopyala. Hedefte varsa dokunma.
    for name in IKINCIGOZ_VERBATIM {
        let src = dir.join(name);
        let dst = target_dir.join(name);
        if src.is_file() && !dst.exists() {
            if std::fs::create_dir_all(target_dir).is_err() {
                continue;
            }
            if std::fs::copy(&src, &dst).is_ok() {
                fields.push(name.to_string());
            }
        }
    }

    if !file.is_file() && fields.is_empty() {
        return SourceStatus::NotFound;
    }
    if fields.is_empty() {
        SourceStatus::NothingToDo
    } else {
        SourceStatus::Migrated { fields }
    }
}

// --------------------------------------------------------------------- DüzenEk

/// DüzenEk'in seçtiği LibreOffice yolunun bugünkü durumu.
///
/// Değer `~/Library/WebKit/tr.yuksekbas.duzenek/.../LocalStorage/localstorage.sqlite3`
/// içinde, `duzenek-renderer` anahtarında, UTF-16LE olarak duruyor. Okumak için
/// SQLite gerekir; birleşik uygulamada DüzenEk henüz yok, dolayısıyla bu bağımlılık
/// bugün eklenmiyor.
///
/// Kaybın gerçek etkisi sınırlıdır: DüzenEk'in kendi keşif zinciri
/// (env → /Applications → Homebrew → PATH) çalışmaya devam eder. Elle seçim
/// yalnız keşfin yanlış kurulumu bulduğu durumda önemlidir. Phase 6'da ya değer
/// okunur ya da kullanıcıdan bir kez yeniden seçmesi istenir.
pub fn duzenek_localstorage_status() -> SourceStatus {
    let dir = paths::legacy_webkit_dir(paths::LEGACY_DUZENEK);
    if !dir.exists() {
        return SourceStatus::NotFound;
    }
    match find_localstorage(&dir) {
        Some(p) => SourceStatus::NeedsManualStep {
            detail: format!(
                "Seçili LibreOffice yolu eski webview localStorage'ında ({}). \
                 Bundle kimliği değiştiği için otomatik taşınmaz. Düzenle modülü \
                 taşındığında (Phase 6) ya okunur ya da bir kez yeniden sorulur. \
                 Bu dosya silinmez.",
                p.display()
            ),
        },
        None => SourceStatus::NothingToDo,
    }
}

fn find_localstorage(root: &Path) -> Option<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Option<PathBuf>) {
        if out.is_some() || depth > 6 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, depth + 1, out);
            } else if p.file_name().and_then(|n| n.to_str()) == Some("localstorage.sqlite3") {
                *out = Some(p);
                return;
            }
        }
    }
    let mut out = None;
    walk(root, 0, &mut out);
    out
}

// ----------------------------------------------------------------- orchestrator

/// Eski uygulamaların ayarlarını **bir kez** birleşik depoya taşır.
///
/// Yeniden çalıştırmak güvenlidir: hiçbir değer ezilmediği için sonuç değişmez.
pub fn migrate_into(target_dir: &Path, settings: &mut Settings) -> MigrationReport {
    let before = settings.clone();
    let mut sources = Vec::new();

    let tavzih_dir = paths::legacy_config_dir(paths::LEGACY_TAVZIH);
    sources.push(SourceReport {
        app: "Tavzih".into(),
        identifier: paths::LEGACY_TAVZIH.into(),
        path: tavzih_dir.display().to_string(),
        status: migrate_tavzih(&tavzih_dir, settings),
    });

    let ig_dir = paths::legacy_config_dir(paths::LEGACY_IKINCIGOZ);
    sources.push(SourceReport {
        app: "İkinciGöz".into(),
        identifier: paths::LEGACY_IKINCIGOZ.into(),
        path: ig_dir.display().to_string(),
        status: migrate_ikincigoz(&ig_dir, target_dir, settings),
    });

    sources.push(SourceReport {
        app: "DüzenEk".into(),
        identifier: paths::LEGACY_DUZENEK.into(),
        path: paths::legacy_webkit_dir(paths::LEGACY_DUZENEK)
            .display()
            .to_string(),
        status: duzenek_localstorage_status(),
    });

    // Değişikİş'in tek kalıcı tercihi kenar çubuğu durumu. İşlevsel değeri yok;
    // talep §11 uyarınca taşınmıyor. Kaynak yine de raporlanır ki karar görünür olsun.
    sources.push(SourceReport {
        app: "Değişikİş".into(),
        identifier: paths::LEGACY_DEGISIKIS.into(),
        path: paths::legacy_webkit_dir(paths::LEGACY_DEGISIKIS)
            .display()
            .to_string(),
        status: SourceStatus::NothingToDo,
    });

    for r in &sources {
        if matches!(r.status, SourceStatus::Migrated { .. })
            && !settings.migrated_from.contains(&r.identifier)
        {
            settings.migrated_from.push(r.identifier.clone());
        }
    }

    MigrationReport {
        changed: *settings != before,
        sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "belge-legacy-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn tavzih_preferences_are_read() {
        let d = tmp("tavzih");
        std::fs::write(
            d.join("preferences.json"),
            r#"{"output_dir":"/Users/x/Belgeler/Dönüştürülen Belgeler","accepted_terms":1}"#,
        )
        .unwrap();
        let mut s = Settings::default();
        let status = migrate_tavzih(&d, &mut s);
        assert_eq!(
            s.output_dir.as_deref(),
            Some("/Users/x/Belgeler/Dönüştürülen Belgeler")
        );
        assert_eq!(s.accepted_terms, Some(1));
        assert!(matches!(status, SourceStatus::Migrated { .. }));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_absent_legacy_app_is_not_an_error() {
        let d = tmp("yok");
        std::fs::remove_dir_all(&d).ok();
        let mut s = Settings::default();
        assert_eq!(migrate_tavzih(&d, &mut s), SourceStatus::NotFound);
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn a_corrupt_legacy_file_is_reported_not_fatal() {
        let d = tmp("bozuk");
        std::fs::write(d.join("preferences.json"), "{{{").unwrap();
        let mut s = Settings::default();
        assert!(matches!(
            migrate_tavzih(&d, &mut s),
            SourceStatus::Unreadable { .. }
        ));
        assert_eq!(s, Settings::default());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn migration_never_overwrites_a_value_the_user_already_set() {
        let d = tmp("ezme");
        std::fs::write(
            d.join("preferences.json"),
            r#"{"output_dir":"/eski","accepted_terms":1}"#,
        )
        .unwrap();
        let mut s = Settings {
            output_dir: Some("/yeni".into()),
            ..Default::default()
        };
        migrate_tavzih(&d, &mut s);
        assert_eq!(s.output_dir.as_deref(), Some("/yeni"));
        // Ayarlanmamış olan yine de dolar.
        assert_eq!(s.accepted_terms, Some(1));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn ikincigoz_accessibility_preferences_are_the_ones_that_matter() {
        let d = tmp("ig");
        let t = tmp("ig-hedef");
        std::fs::write(
            d.join("settings.json"),
            r#"{"disabledRules":["ORTHO_01"],"includeReview":true,"sourceReadOnly":true,
                "theme":"system","respectReducedMotion":true,"textScale":150,
                "highContrast":"on","reduceMotion":"system","linearResults":false}"#,
        )
        .unwrap();
        let mut s = Settings::default();
        let status = migrate_ikincigoz(&d, &t, &mut s);
        assert_eq!(s.text_scale, 150);
        assert_eq!(s.high_contrast, "on");
        assert_eq!(s.disabled_rules, vec!["ORTHO_01".to_string()]);
        // Varsayılandan sapmayanlar "taşındı" diye raporlanmaz.
        match status {
            SourceStatus::Migrated { fields } => {
                assert!(fields.contains(&"textScale".to_string()));
                assert!(fields.contains(&"highContrast".to_string()));
                assert!(!fields.contains(&"theme".to_string()));
                assert!(!fields.contains(&"linearResults".to_string()));
            }
            other => panic!("beklenmeyen: {other:?}"),
        }
        std::fs::remove_dir_all(&d).ok();
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn dictionary_and_profiles_are_copied_verbatim_and_never_reinterpreted() {
        let d = tmp("sozluk");
        let t = tmp("sozluk-hedef");
        let payload = r#"{"accepted":["tazminat","müvekkil"],"corrections":[]}"#;
        std::fs::write(d.join("dictionary.json"), payload).unwrap();
        std::fs::write(d.join("profiles.json"), r#"{"profiles":[]}"#).unwrap();
        let mut s = Settings::default();
        migrate_ikincigoz(&d, &t, &mut s);
        assert_eq!(
            std::fs::read_to_string(t.join("dictionary.json")).unwrap(),
            payload
        );
        assert!(t.join("profiles.json").is_file());
        std::fs::remove_dir_all(&d).ok();
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn an_existing_dictionary_in_the_new_app_is_not_replaced() {
        let d = tmp("sozluk2");
        let t = tmp("sozluk2-hedef");
        std::fs::write(d.join("dictionary.json"), "ESKİ").unwrap();
        std::fs::write(t.join("dictionary.json"), "YENİ").unwrap();
        let mut s = Settings::default();
        migrate_ikincigoz(&d, &t, &mut s);
        assert_eq!(
            std::fs::read_to_string(t.join("dictionary.json")).unwrap(),
            "YENİ"
        );
        std::fs::remove_dir_all(&d).ok();
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn the_legacy_directory_is_never_modified() {
        let d = tmp("dokunma");
        std::fs::write(d.join("preferences.json"), r#"{"accepted_terms":1}"#).unwrap();
        std::fs::write(d.join("dictionary.json"), "veri").unwrap();
        let before: Vec<_> = {
            let mut v: Vec<_> = std::fs::read_dir(&d)
                .unwrap()
                .flatten()
                .map(|e| (e.file_name(), e.metadata().unwrap().len()))
                .collect();
            v.sort();
            v
        };
        let t = tmp("dokunma-hedef");
        let mut s = Settings::default();
        migrate_tavzih(&d, &mut s);
        migrate_ikincigoz(&d, &t, &mut s);
        let after: Vec<_> = {
            let mut v: Vec<_> = std::fs::read_dir(&d)
                .unwrap()
                .flatten()
                .map(|e| (e.file_name(), e.metadata().unwrap().len()))
                .collect();
            v.sort();
            v
        };
        assert_eq!(before, after, "eski dizin değişmemeli");
        std::fs::remove_dir_all(&d).ok();
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn running_migration_twice_changes_nothing_the_second_time() {
        let d = tmp("iki-kez");
        let mut s = Settings::default();
        let first = migrate_into(&d, &mut s);
        let snapshot = s.clone();
        let second = migrate_into(&d, &mut s);
        assert_eq!(s, snapshot);
        assert!(!second.changed || first.changed);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn every_legacy_app_appears_in_the_report() {
        let d = tmp("rapor");
        let mut s = Settings::default();
        let report = migrate_into(&d, &mut s);
        let apps: Vec<_> = report.sources.iter().map(|r| r.app.as_str()).collect();
        assert_eq!(apps, vec!["Tavzih", "İkinciGöz", "DüzenEk", "Değişikİş"]);
        std::fs::remove_dir_all(&d).ok();
    }
}
