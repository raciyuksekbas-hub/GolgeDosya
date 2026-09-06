//! Yüksekbaş Belge — birleşik kabuk.
//!
//! Phase 2: kabuk boş. Rota sistemi, ortak yerleşim, tema, erişilebilirlik,
//! ayar deposu, eski ayar migration'ı ve feature flag altyapısı burada; hiçbir
//! belge motoru henüz taşınmadı. Yeni özellik yazılmaz.

pub mod features;
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            app_info,
            enabled_features,
            get_settings,
            save_settings,
            remember_documents,
            forget_documents,
            migrate_legacy_settings
        ])
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
    fn the_shell_ships_no_engine_yet() {
        // Phase 2 sözleşmesi: kabuk boş. Bu test, bir modül yanlışlıkla
        // default feature'a eklendiğinde kırılır.
        assert!(
            features::states().iter().all(|f| !f.compiled),
            "Phase 2'de hiçbir modül derlenmiş olmamalı"
        );
    }
}
