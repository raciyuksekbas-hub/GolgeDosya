//! Yüksekbaş Belge — birleşik kabuk.
//!
//! Phase 2: kabuk boş. Rota sistemi, ortak yerleşim, tema, erişilebilirlik,
//! ayar deposu, eski ayar migration'ı ve feature flag altyapısı burada; hiçbir
//! belge motoru henüz taşınmadı. Yeni özellik yazılmaz.

pub mod features;

/// Modül motorları. Her biri yalnız kendi cargo feature'ı açıkken derlenir.
pub mod modules {
    #[cfg(feature = "feature_ikincigoz")]
    pub mod ikincigoz;
    #[cfg(feature = "feature_tavzih")]
    pub mod tavzih;
}
pub mod legacy;
pub mod paths;
pub mod settings;

use serde::Serialize;
use settings::Settings;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    /// Ürün markası henüz kesinleşmedi; bu ad geçicidir.
    pub name_is_provisional: bool,
    pub config_dir: String,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Yüksekbaş Belge",
        version: env!("CARGO_PKG_VERSION"),
        name_is_provisional: true,
        config_dir: paths::app_config_dir().display().to_string(),
    }
}

#[tauri::command]
fn enabled_features() -> Vec<features::FeatureState> {
    features::states()
}

#[tauri::command]
fn get_settings() -> Settings {
    settings::load_from(&paths::app_config_dir())
}

#[tauri::command]
fn save_settings(next: Settings) -> Result<Settings, String> {
    let dir = paths::app_config_dir();
    settings::save_to(&dir, &next).map_err(|e| e.to_string())?;
    Ok(settings::load_from(&dir))
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Açılan belgeleri son kullanılanlar listesine yaz.
///
/// Yalnız yol saklanır; belge açılmaz, okunmaz, içeriğinden hiçbir şey tutulmaz.
#[tauri::command]
fn remember_documents(paths: Vec<String>) -> Result<Settings, String> {
    let dir = paths::app_config_dir();
    let mut current = settings::load_from(&dir);
    let now = now_seconds();
    // Ters sırada işlenir ki çağrının ilk yolu listenin başında kalsın.
    for path in paths.iter().rev() {
        current.remember(path, now);
    }
    settings::save_to(&dir, &current).map_err(|e| e.to_string())?;
    Ok(current)
}

#[tauri::command]
fn forget_documents() -> Result<Settings, String> {
    let dir = paths::app_config_dir();
    let mut current = settings::load_from(&dir);
    current.recent_documents.clear();
    settings::save_to(&dir, &current).map_err(|e| e.to_string())?;
    Ok(current)
}

/// Eski uygulamaların ayarlarını okur ve raporu döndürür.
///
/// Açılışta bir kez çağrılır. Yeniden çağrılması güvenlidir: hiçbir değer
/// ezilmez, hiçbir eski dosya silinmez.
#[tauri::command]
fn migrate_legacy_settings() -> Result<legacy::MigrationReport, String> {
    let dir = paths::app_config_dir();
    let mut current = settings::load_from(&dir);
    let report = legacy::migrate_into(&dir, &mut current);
    if report.changed {
        settings::save_to(&dir, &current).map_err(|e| e.to_string())?;
    }
    Ok(report)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());

    #[cfg(feature = "feature_tavzih")]
    let builder = builder
        .plugin(tauri_plugin_opener::init())
        .manage(modules::tavzih::Busy::default());

    // Komut listesi derleme zamanında feature'a göre seçilir: kapalı bir modülün
    // komutu binary'de hiç bulunmaz ve arayüz onu çağıramaz.
    //
    // `generate_handler!` listesinin içinde `#[cfg]` kullanılamadığı için her
    // kombinasyon ayrı bir koldur. Kol sayısı modül sayısıyla üstel büyür; bu
    // yüzden derleme zamanı bayrağı SÜRÜM ŞEKLİNİ belirlemek içindir, günlük
    // geri alma için değil. Bir modülü yeniden derlemeden kapatmak gerekirse
    // çalışma zamanı bayrağı kullanılır: BELGE_DISABLE_<AD>=1 (bkz. features.rs).
    macro_rules! shell_commands {
        ($($extra:path),* $(,)?) => {
            tauri::generate_handler![
                app_info,
                enabled_features,
                get_settings,
                save_settings,
                remember_documents,
                forget_documents,
                migrate_legacy_settings
                $(, $extra)*
            ]
        };
    }

    #[cfg(all(feature = "feature_tavzih", feature = "feature_ikincigoz"))]
    let builder = builder.invoke_handler(shell_commands![
        modules::tavzih::tavzih_inspect_file,
        modules::tavzih::tavzih_inspect_files,
        modules::tavzih::tavzih_convert_file,
        modules::tavzih::tavzih_convert_batch,
        modules::tavzih::tavzih_terms_accepted,
        modules::tavzih::tavzih_accept_terms,
        modules::tavzih::tavzih_output_folder,
        modules::tavzih::tavzih_set_output_folder,
        modules::tavzih::tavzih_reveal_output_folder,
        modules::ikincigoz::ikincigoz_analyze_document,
        modules::ikincigoz::ikincigoz_apply_fixes,
        modules::ikincigoz::ikincigoz_preview_fixes,
        modules::ikincigoz::ikincigoz_list_rules,
        modules::ikincigoz::ikincigoz_get_dictionary,
        modules::ikincigoz::ikincigoz_accept_word,
        modules::ikincigoz::ikincigoz_remove_accepted_word,
        modules::ikincigoz::ikincigoz_add_correction,
        modules::ikincigoz::ikincigoz_remove_correction,
    ]);

    #[cfg(all(feature = "feature_tavzih", not(feature = "feature_ikincigoz")))]
    let builder = builder.invoke_handler(shell_commands![
        modules::tavzih::tavzih_inspect_file,
        modules::tavzih::tavzih_inspect_files,
        modules::tavzih::tavzih_convert_file,
        modules::tavzih::tavzih_convert_batch,
        modules::tavzih::tavzih_terms_accepted,
        modules::tavzih::tavzih_accept_terms,
        modules::tavzih::tavzih_output_folder,
        modules::tavzih::tavzih_set_output_folder,
        modules::tavzih::tavzih_reveal_output_folder,
    ]);

    #[cfg(all(not(feature = "feature_tavzih"), feature = "feature_ikincigoz"))]
    let builder = builder.invoke_handler(shell_commands![
        modules::ikincigoz::ikincigoz_analyze_document,
        modules::ikincigoz::ikincigoz_apply_fixes,
        modules::ikincigoz::ikincigoz_preview_fixes,
        modules::ikincigoz::ikincigoz_list_rules,
        modules::ikincigoz::ikincigoz_get_dictionary,
        modules::ikincigoz::ikincigoz_accept_word,
        modules::ikincigoz::ikincigoz_remove_accepted_word,
        modules::ikincigoz::ikincigoz_add_correction,
        modules::ikincigoz::ikincigoz_remove_correction,
    ]);

    #[cfg(all(not(feature = "feature_tavzih"), not(feature = "feature_ikincigoz")))]
    let builder = builder.invoke_handler(shell_commands![]);

    builder
        .run(tauri::generate_context!())
        .expect("Yüksekbaş Belge başlatılamadı");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_the_name_as_provisional() {
        let info = app_info();
        assert!(
            info.name_is_provisional,
            "marka kararı verilene kadar ad geçici işaretlenmeli"
        );
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn a_module_is_reachable_only_when_its_engine_is_compiled_in() {
        // Feature-level rollback sözleşmesi: bir modül linklenmemişse arayüzde
        // etkin görünemez. Bir modül default'a eklenip motoru bağlanmazsa kırılır.
        for f in features::states() {
            if !f.compiled {
                assert!(!f.enabled, "{} linklenmemişken etkin görünüyor", f.key);
            }
        }
    }

    #[cfg(feature = "feature_ikincigoz")]
    #[test]
    fn the_review_module_is_live_in_this_build() {
        let ig = features::states()
            .into_iter()
            .find(|f| f.key == "ikincigoz")
            .expect("Denetle bölümü tanımlı olmalı");
        assert!(ig.compiled);
        assert_eq!(ig.label, "Denetle");
    }

    #[cfg(feature = "feature_tavzih")]
    #[test]
    fn the_convert_module_is_live_in_this_build() {
        let tavzih = features::states()
            .into_iter()
            .find(|f| f.key == "tavzih")
            .expect("Dönüştür bölümü tanımlı olmalı");
        assert!(tavzih.compiled);
        assert_eq!(tavzih.label, "Dönüştür");
    }
}
