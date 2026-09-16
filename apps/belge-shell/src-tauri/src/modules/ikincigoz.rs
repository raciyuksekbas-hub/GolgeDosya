//! Denetle — İkinciGöz'ün belge denetimi ve cerrahi düzeltme yüzeyi.
//!
//! Bu bir taşımadır, yeniden tasarım değildir. Motor `ikincigoz-core`'dur ve
//! **kaynağı standalone deposunda durmaktadır**: hem referans implementasyon
//! hem de parser/writeback karşılaştırma kaynağı odur. Kopyalanmadı.
//!
//! Bu fazda bilinçli olarak YAPILMAYANLAR:
//! * parser consolidation — `AnalysisDocument`, `document-core` projeksiyonuna
//!   bağlanmadı; İkinciGöz kendi DOCX/UDF parser'ını kullanmaya devam ediyor
//! * `SourceLocation` semantiği, codepoint offset hesapları
//! * `writeback.rs` davranışı — import yolu bile değişmedi
//! * lint kuralları, kural adları, severity sözleşmesi, Türkçe morfoloji
//!
//! Bu bounded duplication bir eksiklik değil, writeback güvenliğinin bedelidir:
//! `writeback` `container_path` provenance'ına dayanır ve `document-core`'un
//! sadakat modelinde o alan yoktur.
//!
//! Tek adaptasyon: sözlük, profil ve lint ayarları artık kabuğun ortak
//! yapılandırma dizininden okunuyor.

use crate::{paths, settings};
use ikincigoz_core::dict::{CorrectionRule, UserDictionary};
use ikincigoz_core::finding::{Finding, Fix, RuleInfo, RULES};
use ikincigoz_core::parser;
use ikincigoz_core::profile::{DocumentProfile, ProfileStore};
use ikincigoz_core::rules::{analyze, Context, LintOptions, LintResult};
use ikincigoz_core::writeback;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Açılabilecek en büyük dosya. Standalone uygulamadaki sınırla aynı.
const MAX_OPEN_BYTES: u64 = 128 * 1024 * 1024;

const DICTIONARY_FILE: &str = "dictionary.json";
const PROFILES_FILE: &str = "profiles.json";

// --------------------------------------------------------------- görünüm tipleri
// Sözleşme standalone uygulamadan birebir alındı; alan adları ve camelCase
// dönüşümü değiştirilmedi ki arayüz davranışı aynı kalsın.

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub file_name: String,
    pub format: String,
    pub block_count: usize,
    pub word_count: usize,
    pub char_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockView {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub is_heading: bool,
    pub level: Option<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruncatedRuleView {
    pub rule_id: String,
    pub shown: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub document: DocumentSummary,
    pub blocks: Vec<BlockView>,
    pub findings: Vec<Finding>,
    pub error_count: usize,
    pub warning_count: usize,
    pub review_count: usize,
    pub truncated_rules: Vec<TruncatedRuleView>,
    /// İncelemenin gerçekten sürdüğü milisaniye. İlerleme asla taklit edilmez.
    pub elapsed_ms: u64,
    /// İncelemenin ölçüldüğü profil, varsa.
    pub profile_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleView {
    pub id: String,
    pub title: String,
    pub category: String,
    pub description: String,
    pub default_severity: String,
}

impl From<&RuleInfo> for RuleView {
    fn from(r: &RuleInfo) -> Self {
        RuleView {
            id: r.id.to_string(),
            title: r.title.to_string(),
            category: r.category.to_string(),
            description: r.description.to_string(),
            default_severity: format!("{:?}", r.default_severity).to_lowercase(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub file_name: String,
    pub directory: String,
    pub applied: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryView {
    pub accepted: Vec<String>,
    pub corrections: Vec<CorrectionRule>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCorrection {
    pub from: String,
    pub to: String,
    /// Açıkken `from`'un çekimli hâlleri de eşleşir ve ek `to`'ya taşınır.
    #[serde(default = "yes")]
    pub inflect: bool,
    /// Düzeltme önizlemesinde gösterilen isteğe bağlı not.
    #[serde(default)]
    pub note: Option<String>,
}

fn yes() -> bool {
    true
}

// --------------------------------------------------------------------- depolama
//
// Sözlük ve profiller birleşik yapılandırma dizininde, kendi JSON dosyalarında.
// Eski konumdan buraya kopyalanmaları `legacy.rs` migration katmanının işidir;
// burada yalnız okunur ve yazılır.

fn read_json<T: Default + for<'de> Deserialize<'de>>(name: &str) -> T {
    // Bozuk veya elle düzenlenmiş bir dosya uygulamanın açılmasını engellememeli;
    // kullanıcının bundan kurtulmasının bir yolu olmazdı.
    std::fs::read_to_string(paths::app_config_dir().join(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(name: &str, value: &T) -> Result<(), String> {
    let dir = paths::app_config_dir();
    let json = serde_json::to_string_pretty(value).map_err(|_| "Kaydedilemedi.".to_string())?;
    // Atomik yazım: yarıda kesilen bir yazma öğrenilen kelime/düzeltme
    // sözlüğünü sessizce silebiliyordu (read_json bozuk dosyada boş döner).
    crate::atomic::write(&dir.join(name), json.as_bytes()).map_err(|_| "Kaydedilemedi.".to_string())
}

fn dictionary() -> UserDictionary {
    read_json(DICTIONARY_FILE)
}

fn profiles() -> ProfileStore {
    read_json(PROFILES_FILE)
}

/// Kabuğun ortak ayarlarından lint seçenekleri.
///
/// `disabled_rules` ve `include_review` standalone uygulamadaki alanların
/// aynısıdır ve migration katmanı bunları eski konumdan taşımıştır.
fn lint_options() -> LintOptions {
    let s = settings::load_from(&paths::app_config_dir());
    LintOptions {
        disabled_rules: s.disabled_rules.iter().cloned().collect(),
        include_review: s.include_review,
        ..LintOptions::default()
    }
}

fn dictionary_view(d: &UserDictionary) -> DictionaryView {
    DictionaryView {
        accepted: d.accepted_words().to_vec(),
        corrections: d.corrections().cloned().collect(),
    }
}

// --------------------------------------------------------------------- yardımcı

/// Kullanıcının seçtiği dosyayı parser'ın beklediği sınırlarla oku.
///
/// Yol sistem dosya diyaloğundan gelir, yani kullanıcının zaten erişebildiği bir
/// konumdur. Yine de burada denetlenir: bir dizin ya da devasa bir şeye giden
/// symlink, yarı yolda değil, net bir mesajla başarısız olmalıdır.
fn read_document(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "Dosya açılamadı.".to_string())?;
    if !metadata.is_file() {
        return Err("Seçilen öğe bir dosya değil.".into());
    }
    if metadata.len() > MAX_OPEN_BYTES {
        return Err("Dosya çok büyük.".into());
    }
    if parser::format_from_name(&parser::base_name(&path.to_string_lossy())).is_none() {
        return Err("Bu dosya biçimi desteklenmiyor. DOCX veya UDF seçin.".into());
    }
    std::fs::read(path).map_err(|_| "Dosya açılamadı.".into())
}

fn summarise(d: &ikincigoz_core::cdm::Document) -> DocumentSummary {
    DocumentSummary {
        file_name: d.metadata.file_name.clone(),
        format: d.format.label().to_string(),
        block_count: d.metadata.block_count,
        word_count: d.metadata.word_count,
        char_count: d.metadata.char_count,
    }
}

fn block_views(d: &ikincigoz_core::cdm::Document) -> Vec<BlockView> {
    use ikincigoz_core::cdm::BlockKind;
    d.blocks
        .iter()
        .map(|b| BlockView {
            id: b.id.clone(),
            kind: format!("{:?}", b.kind).to_lowercase(),
            text: b.text.clone(),
            is_heading: b.kind == BlockKind::Heading,
            level: b.hierarchy_level,
        })
        .collect()
}

fn to_result(
    document: &ikincigoz_core::cdm::Document,
    lint: LintResult,
    elapsed_ms: u64,
    profile_name: Option<String>,
) -> AnalysisResult {
    AnalysisResult {
        document: summarise(document),
        blocks: block_views(document),
        error_count: lint.error_count,
        warning_count: lint.warning_count,
        review_count: lint.review_count,
        truncated_rules: lint
            .truncated_rules
            .into_iter()
            .map(|t| TruncatedRuleView {
                rule_id: t.rule_id,
                shown: t.shown,
                total: t.total,
            })
            .collect(),
        findings: lint.findings,
        elapsed_ms,
        profile_name,
    }
}

// --------------------------------------------------------------------- komutlar

/// Belgeyi incele.
///
/// Parse ve lint, bütün bir belge üzerinde saf CPU işidir; ana iş parçacığının
/// dışında koşar: uzun bir dilekçede pencere yanıt vermeye devam etmelidir.
#[tauri::command]
pub async fn ikincigoz_analyze_document(path: String) -> Result<AnalysisResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let path = PathBuf::from(path);
        let bytes = read_document(&path)?;
        let file_name = parser::base_name(&path.to_string_lossy());

        let document = parser::parse(&file_name, &bytes).map_err(|e| e.message_tr().to_string())?;

        let dictionary = dictionary();
        let profiles = profiles();
        let profile: Option<&DocumentProfile> = profiles.active_profile();
        let options = lint_options();

        let lint = analyze(&Context::new(&document, &dictionary, profile, &options));
        let elapsed = started.elapsed().as_millis() as u64;
        Ok(to_result(
            &document,
            lint,
            elapsed,
            profile.map(|p| p.name.clone()),
        ))
    })
    .await
    .map_err(|_| "İnceleme tamamlanamadı.".to_string())?
}

/// Kaydetme penceresinde kullanıcının yazdığı adı güvenli bir dosya adına indir.
///
/// Yalnız ad bileşeni alınır: yol ayırıcısı taşıyan bir girdi (`../..`) hedef
/// klasörün dışına yazamaz. Uzantı kullanıcı yazmadıysa türetilmiş addan alınır,
/// böylece ".docx" kaybolup açılamayan bir dosya oluşmaz.
fn chosen_file_name(requested: &str, derived: &str) -> Option<String> {
    let base = requested
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if base.is_empty() || base == "." || base == ".." {
        return None;
    }
    let has_ext = base.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && !ext.is_empty() && !ext.contains(' ')
    });
    if has_ext {
        return Some(base);
    }
    match derived.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => Some(format!("{base}.{ext}")),
        _ => Some(base),
    }
}

/// Seçilen düzeltmeleri **bir kopyaya** uygula.
///
/// Kaynak yol asla yazmak için açılmaz. Yayın no-clobber + atomiktir
/// (`atomic::write_new_unique`): kaynak dahil var olan hiçbir dosyanın üzerine
/// yazılmaz, çakışan ad Dönüştür gibi " (2)" ile türetilir.
///
/// `file_name` verilirse KULLANICININ kaydetme penceresinde seçtiği addır ve
/// ona saygı duyulur. Eskiden pencere ad soruyor ama yalnız klasör kullanılıyor,
/// yazılan ad sessizce atılıyordu — kontrol yalan söylüyordu.
#[tauri::command]
pub async fn ikincigoz_apply_fixes(
    path: String,
    fixes: Vec<Fix>,
    output_dir: Option<String>,
    file_name: Option<String>,
) -> Result<WriteResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Parametre aşağıdaki yerel `file_name` ile gölgelenmesin.
        let requested_name = file_name;
        let source = PathBuf::from(&path);
        let bytes = read_document(&source)?;
        let file_name = parser::base_name(&source.to_string_lossy());

        let result = writeback::apply(&file_name, &bytes, &fixes).map_err(|e| e.message_tr())?;

        // Kopya, kullanıcı başka bir yer seçmedikçe orijinalin yanına gider.
        let directory = match output_dir {
            Some(d) => PathBuf::from(d),
            None => source
                .parent()
                .map(|p| p.to_path_buf())
                .ok_or_else(|| "Kaydedilecek klasör bulunamadı.".to_string())?,
        };
        // No-clobber + atomik yayın. Kaynak zaten adı değiştiği için korunur;
        // ama kullanıcının daha önce üretip düzenlediği bir "… - Düzeltilmiş"
        // kopyası aynı adı taşıyabilir. Düz fs::write onu sessizce eziyordu
        // (geri alınamaz kayıp) ve atomik değildi (yarım yazım → bozuk zip).
        // Çakışırsa Dönüştür gibi " (2)" türet; var olan hiçbir kopya kaybolmaz.
        let target_name = requested_name
            .as_deref()
            .and_then(|requested| chosen_file_name(requested, &result.file_name))
            .unwrap_or_else(|| result.file_name.clone());
        let written = crate::atomic::write_new_unique(&directory, &target_name, &result.bytes)
            .map_err(|_| "Yeni dosya kaydedilemedi.".to_string())?;

        Ok(WriteResult {
            file_name: written,
            directory: directory.to_string_lossy().to_string(),
            applied: result.applied,
        })
    })
    .await
    .map_err(|_| "Düzeltmeler uygulanamadı.".to_string())?
}

/// Bir dizi değişikliğin üreteceği metni, hiçbir şey yazmadan önizle.
#[tauri::command]
pub async fn ikincigoz_preview_fixes(path: String, fixes: Vec<Fix>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = PathBuf::from(&path);
        let bytes = read_document(&source)?;
        let file_name = parser::base_name(&source.to_string_lossy());
        let document = parser::parse(&file_name, &bytes).map_err(|e| e.message_tr().to_string())?;

        let mut out = Vec::with_capacity(fixes.len());
        for fix in &fixes {
            let block = document
                .block(&fix.block_id)
                .ok_or_else(|| "Belge değişmiş görünüyor.".to_string())?;
            let mut text: String = block.text.chars().take(fix.char_start).collect();
            text.push_str(&fix.replacement);
            text.extend(block.text.chars().skip(fix.char_end));
            out.push(text);
        }
        Ok(out)
    })
    .await
    .map_err(|_| "Önizleme oluşturulamadı.".to_string())?
}

#[tauri::command]
pub fn ikincigoz_list_rules() -> Vec<RuleView> {
    RULES.iter().map(RuleView::from).collect()
}

#[tauri::command]
pub fn ikincigoz_get_dictionary() -> DictionaryView {
    dictionary_view(&dictionary())
}

#[tauri::command]
pub fn ikincigoz_accept_word(word: String) -> Result<DictionaryView, String> {
    let mut d = dictionary();
    d.accept(&word);
    write_json(DICTIONARY_FILE, &d)?;
    Ok(dictionary_view(&d))
}

#[tauri::command]
pub fn ikincigoz_remove_accepted_word(word: String) -> Result<DictionaryView, String> {
    let mut d = dictionary();
    d.unaccept(&word);
    write_json(DICTIONARY_FILE, &d)?;
    Ok(dictionary_view(&d))
}

#[tauri::command]
pub fn ikincigoz_add_correction(rule: NewCorrection) -> Result<DictionaryView, String> {
    let mut d = dictionary();
    // Doğrulama çekirdeğin işidir; Türkçe mesajlar standalone uygulamadan birebir.
    d.add_correction(CorrectionRule {
        from: rule.from,
        to: rule.to,
        inflect: rule.inflect,
        note: rule.note,
    })
    .map_err(|e| match e {
        "empty" => "Her iki alan da doldurulmalı.".to_string(),
        "identical" => "İki alan aynı olamaz.".to_string(),
        "multiword_inflect" => {
            "Birden çok kelimeden oluşan terimler için çekim desteği kapatılmalı.".to_string()
        }
        _ => "Kural eklenemedi.".to_string(),
    })?;
    write_json(DICTIONARY_FILE, &d)?;
    Ok(dictionary_view(&d))
}

#[tauri::command]
pub fn ikincigoz_remove_correction(from: String) -> Result<DictionaryView, String> {
    let mut d = dictionary();
    d.remove_correction(&from);
    write_json(DICTIONARY_FILE, &d)?;
    Ok(dictionary_view(&d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_is_refused_before_any_parsing() {
        let e = read_document(&std::env::temp_dir()).unwrap_err();
        assert_eq!(e, "Seçilen öğe bir dosya değil.");
    }

    #[test]
    fn an_unsupported_extension_is_refused_with_the_formats_that_are_supported() {
        let f = std::env::temp_dir().join(format!("belge-ig-{}.txt", std::process::id()));
        std::fs::write(&f, b"metin").unwrap();
        let e = read_document(&f).unwrap_err();
        assert!(
            e.contains("DOCX veya UDF"),
            "mesaj yönlendirici olmalı: {e}"
        );
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn every_rule_exposes_a_severity_the_interface_can_render() {
        let views = ikincigoz_list_rules();
        assert!(!views.is_empty(), "kural listesi boş olmamalı");
        for v in &views {
            assert!(!v.id.is_empty());
            assert!(!v.title.is_empty());
            assert!(
                ["error", "warning", "review"].contains(&v.default_severity.as_str()),
                "beklenmeyen severity: {} ({})",
                v.default_severity,
                v.id
            );
        }
    }

    #[test]
    fn lint_options_follow_the_shared_settings_store() {
        // Ayar deposu kapalı bir kural listesi ve inceleme tercihi taşır;
        // bu ikisi standalone uygulamadaki alanların aynısıdır.
        let o = lint_options();
        let s = settings::load_from(&paths::app_config_dir());
        assert_eq!(o.include_review, s.include_review);
        assert_eq!(o.disabled_rules.len(), s.disabled_rules.len());
    }
}
