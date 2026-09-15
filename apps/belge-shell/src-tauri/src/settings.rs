//! Birleşik ayar deposu.
//!
//! Tek JSON dosyası, uygulamanın kendi yapılandırma dizininde. Şema, dört
//! bağımsız uygulamanın kullanıcıya ait kalıcı verisinin **birleşimidir**;
//! yeni bir tercih icat edilmez.
//!
//! Her alan `#[serde(default)]` taşır: eksik alan hata değil, varsayılandır.
//! Bu, ileri/geri uyumluluğu sürüm numarası olmadan sağlar.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SETTINGS_FILE: &str = "settings.json";

/// Kullanım koşullarının sürümü. Metin esaslı biçimde değişirse artırılır ve
/// kullanıcıya yeniden sorulur. Bağımsız Tavzih'teki değerle aynıdır: taşınan
/// kabul geçerliliğini korur, kullanıcıya ikinci kez sorulmaz.
pub const TERMS_VERSION: u32 = 1;

fn yes() -> bool {
    true
}
fn no() -> bool {
    false
}
fn system() -> String {
    "system".into()
}
fn default_text_scale() -> u16 {
    100
}

/// Kullanıcının açtığı bir belge.
///
/// Yalnız yol ve zaman tutulur; belge içeriğinden hiçbir şey saklanmaz.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentDocument {
    pub path: String,
    /// Unix saniye.
    pub opened_at: u64,
}

/// Son kullanılanlar listesinin üst sınırı. macOS'un kendi "Recent Items"
/// varsayılanıyla aynı; daha uzun bir liste kullanıcıya yardımcı olmaz.
pub const MAX_RECENTS: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    // --- kabuk ---
    /// `system` | `light` | `dark`
    #[serde(default = "system")]
    pub theme: String,

    // --- erişilebilirlik (kaynak: İkinciGöz) ---
    /// Arayüz metin ölçeği yüzdesi: 100, 125, 150, 175, 200.
    #[serde(default = "default_text_scale")]
    pub text_scale: u16,
    /// `system` | `on` | `off`
    #[serde(default = "system")]
    pub high_contrast: String,
    /// `system` | `on` | `off`
    ///
    /// `system` iken karar `respect_reduced_motion`a düşer; `on`/`off` onu ezer.
    #[serde(default = "system")]
    pub reduce_motion: String,
    /// Eski anahtar: işletim sisteminin hareket azaltma tercihine uyulsun mu?
    ///
    /// `reduce_motion` "system" ise anlamlıdır. `false` + "system" bileşimi
    /// "sistem azaltma dese bile animasyonları göster" demektir; bu bileşim
    /// başka hiçbir alanla ifade edilemez, o yüzden ayrı tutuluyor.
    #[serde(default = "yes")]
    pub respect_reduced_motion: bool,

    // --- Tavzih ---
    /// Kullanıcının kabul ettiği kullanım koşulları sürümü.
    #[serde(default)]
    pub accepted_terms: Option<u32>,
    /// Dönüştürülen belgelerin yazıldığı klasör. `None` = uygulama varsayılanı.
    #[serde(default)]
    pub output_dir: Option<String>,

    // --- DüzenEk ---
    /// Kullanıcının seçtiği LibreOffice çalıştırıcısı. `None` = otomatik keşif.
    #[serde(default)]
    pub renderer_path: Option<String>,

    // --- İkinciGöz ---
    #[serde(default)]
    pub disabled_rules: Vec<String>,
    #[serde(default = "yes")]
    pub include_review: bool,
    #[serde(default = "yes")]
    pub source_read_only: bool,
    #[serde(default = "no")]
    pub linear_results: bool,

    /// Son açılan belgeler, en yeni önce.
    #[serde(default)]
    pub recent_documents: Vec<RecentDocument>,

    /// Ayarların hangi eski uygulamalardan okunduğu. Yalnız kayıt amaçlı;
    /// migration'ın bir kez çalıştığını buradan anlarız.
    #[serde(default)]
    pub migrated_from: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: system(),
            text_scale: default_text_scale(),
            high_contrast: system(),
            reduce_motion: system(),
            respect_reduced_motion: true,
            accepted_terms: None,
            output_dir: None,
            renderer_path: None,
            disabled_rules: Vec::new(),
            include_review: true,
            source_read_only: true,
            linear_results: false,
            recent_documents: Vec::new(),
            migrated_from: Vec::new(),
        }
    }
}

impl Settings {
    /// Bir belgeyi listenin başına al. Aynı yol iki kez görünmez ve liste
    /// `MAX_RECENTS` ile sınırlıdır.
    pub fn remember(&mut self, path: &str, now: u64) {
        self.recent_documents.retain(|r| r.path != path);
        self.recent_documents.insert(
            0,
            RecentDocument {
                path: path.to_string(),
                opened_at: now,
            },
        );
        self.recent_documents.truncate(MAX_RECENTS);
    }
}

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE)
}

/// Ayarları oku. Dosya yoksa **veya bozuksa** varsayılanları döndürür.
///
/// Bozuk dosyada hata döndürmek uygulamayı açılmaz hâle getirirdi; ayar dosyası
/// bunu hak edecek kadar kritik değil. Bozuk dosya üzerine de yazılmaz — bir
/// sonraki `save` yazana kadar diskte durur.
pub fn load_from(dir: &Path) -> Settings {
    let path = settings_path(dir);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    // Mutlu yol: yapı bütünüyle çözülür.
    if let Ok(settings) = serde_json::from_str::<Settings>(&text) {
        return settings;
    }
    // Aksi hâlde alan alan kurtar: tek yanlış tipli alan bütün ayarları
    // düşürmemeli. Yalnız geçerli değerler devralınır, gerisi varsayılan kalır.
    lenient(&text)
}

/// Geçerli her alanı ayrı ayrı çözer; çözülemeyen alan varsayılanında kalır.
fn lenient(text: &str) -> Settings {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text) else {
        return Settings::default();
    };
    let mut s = Settings::default();
    macro_rules! field {
        ($key:literal, $field:ident) => {
            if let Some(v) = map.get($key) {
                if let Ok(val) = serde_json::from_value(v.clone()) {
                    s.$field = val;
                }
            }
        };
    }
    field!("theme", theme);
    field!("textScale", text_scale);
    field!("highContrast", high_contrast);
    field!("reduceMotion", reduce_motion);
    field!("respectReducedMotion", respect_reduced_motion);
    field!("acceptedTerms", accepted_terms);
    field!("outputDir", output_dir);
    field!("rendererPath", renderer_path);
    field!("disabledRules", disabled_rules);
    field!("includeReview", include_review);
    field!("sourceReadOnly", source_read_only);
    field!("linearResults", linear_results);
    field!("recentDocuments", recent_documents);
    field!("migratedFrom", migrated_from);
    s
}

pub fn save_to(dir: &Path, settings: &Settings) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // Atomik yazım: yarım kalan dosya sonraki okumayı varsayılana düşürüp ilk
    // kayıtta kalıcı veri kaybına yol açıyordu (bkz. atomic::write).
    crate::atomic::write(&settings_path(dir), text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "belge-settings-{}-{}",
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
    fn missing_file_yields_defaults() {
        let d = tmp();
        let s = load_from(&d);
        assert_eq!(s, Settings::default());
        assert_eq!(s.theme, "system");
        assert!(s.source_read_only);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn round_trips() {
        let d = tmp();
        let s = Settings {
            theme: "dark".into(),
            text_scale: 150,
            output_dir: Some("/tmp/çıktı".into()),
            disabled_rules: vec!["ORTHO_01".into()],
            ..Default::default()
        };
        save_to(&d, &s).unwrap();
        assert_eq!(load_from(&d), s);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_what_it_omits() {
        // İleri/geri uyumluluk: yeni bir alan eklendiğinde eski dosya hâlâ okunur.
        let d = tmp();
        std::fs::write(settings_path(&d), r#"{"theme":"light"}"#).unwrap();
        let s = load_from(&d);
        assert_eq!(s.theme, "light");
        assert_eq!(s.text_scale, 100);
        assert!(s.include_review);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_corrupt_file_is_not_fatal_and_is_not_overwritten() {
        let d = tmp();
        std::fs::write(settings_path(&d), "bu JSON değil {{{").unwrap();
        assert_eq!(load_from(&d), Settings::default());
        // Okuma yazmaz: bozuk içerik hâlâ diskte, kullanıcı kurtarabilir.
        assert_eq!(
            std::fs::read_to_string(settings_path(&d)).unwrap(),
            "bu JSON değil {{{"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_older_motion_switch_survives_because_no_other_field_can_express_it() {
        let d = tmp();
        let s = Settings {
            reduce_motion: "system".into(),
            respect_reduced_motion: false,
            ..Default::default()
        };
        save_to(&d, &s).unwrap();
        let back = load_from(&d);
        assert_eq!(back.reduce_motion, "system");
        assert!(!back.respect_reduced_motion);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn one_wrong_typed_field_does_not_discard_every_other_value() {
        // Bir alanın tipi yanlışsa (indirme sonrası eski sürüm, elle düzenleme
        // ya da kısmi bozulma) serde bütün yapıyı reddediyordu: kabul edilen
        // kullanım koşulları, çıktı klasörü, kurallar ve son belgeler tümden
        // varsayılana düşüyordu; bir sonraki kayıt kaybı kalıcı yapıyordu.
        let d = tmp();
        std::fs::write(
            settings_path(&d),
            r#"{"theme":123,"textScale":150,"acceptedTerms":3,"outputDir":"/tmp/x",
                "disabledRules":["ORTHO_01"],"includeReview":false}"#,
        )
        .unwrap();
        let s = load_from(&d);
        // Yanlış tipli alan varsayılanda kalır…
        assert_eq!(s.theme, "system");
        // …ama geçerli olan her alan KORUNUR.
        assert_eq!(s.text_scale, 150);
        assert_eq!(s.accepted_terms, Some(3));
        assert_eq!(s.output_dir.as_deref(), Some("/tmp/x"));
        assert_eq!(s.disabled_rules, vec!["ORTHO_01".to_string()]);
        assert!(!s.include_review);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temporary_file() {
        // Yerinde yazımda yarım kalan dosya sessizce varsayılana düşürüyordu.
        // Atomik yazım ya eski ya yeni tam dosyayı bırakır; geçici dosya kalmaz.
        let d = tmp();
        let s = Settings {
            theme: "dark".into(),
            accepted_terms: Some(2),
            ..Default::default()
        };
        save_to(&d, &s).unwrap();
        // İkinci yazım da atomik; birinci sürümün üzerine güvenle yazar.
        let s2 = Settings {
            theme: "light".into(),
            ..s.clone()
        };
        save_to(&d, &s2).unwrap();
        assert_eq!(load_from(&d), s2);
        let leftovers: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != SETTINGS_FILE)
            .collect();
        assert!(leftovers.is_empty(), "geçici dosya kaldı: {leftovers:?}");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn remembering_a_document_puts_it_first_without_duplicating_it() {
        let mut s = Settings::default();
        s.remember("/belge/a.docx", 100);
        s.remember("/belge/b.udf", 200);
        s.remember("/belge/a.docx", 300);
        assert_eq!(s.recent_documents.len(), 2);
        assert_eq!(s.recent_documents[0].path, "/belge/a.docx");
        assert_eq!(s.recent_documents[0].opened_at, 300);
        assert_eq!(s.recent_documents[1].path, "/belge/b.udf");
    }

    #[test]
    fn the_recent_list_is_capped() {
        let mut s = Settings::default();
        for i in 0..(MAX_RECENTS + 5) {
            s.remember(&format!("/belge/{i}.docx"), i as u64);
        }
        assert_eq!(s.recent_documents.len(), MAX_RECENTS);
        // En yeni başta, en eskiler düşmüş olmalı.
        assert_eq!(
            s.recent_documents[0].path,
            format!("/belge/{}.docx", MAX_RECENTS + 4)
        );
    }

    #[test]
    fn recents_survive_a_round_trip() {
        let d = tmp();
        let mut s = Settings::default();
        s.remember("/Belgeler/Dönüştürülen/İŞ SÖZLEŞMESİ.docx", 42);
        save_to(&d, &s).unwrap();
        assert_eq!(load_from(&d).recent_documents, s.recent_documents);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn turkish_text_survives_a_round_trip() {
        let d = tmp();
        let s = Settings {
            output_dir: Some("/Users/x/Belgeler/Dönüştürülen Belgeler/İŞ".into()),
            ..Default::default()
        };
        save_to(&d, &s).unwrap();
        assert_eq!(load_from(&d).output_dir, s.output_dir);
        std::fs::remove_dir_all(&d).ok();
    }
}
