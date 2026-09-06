//! Platform yolları.
//!
//! Dört bağımsız uygulamanın hepsi aynı şemayı kullanıyor: uygulama kimliği
//! adında tek bir dizin, platformun kendi yapılandırma kökünün altında.
//! Birleşik uygulama aynı şemayı kendi kimliğiyle sürdürür.

use std::path::PathBuf;

/// Birleşik uygulamanın kimliği. `tauri.conf.json` ile aynı olmalıdır.
pub const APP_ID: &str = "tr.yuksekbas.belge";

/// Migration kaynağı olan eski uygulama kimlikleri.
pub const LEGACY_TAVZIH: &str = "tr.yuksekbas.tavzih";
pub const LEGACY_IKINCIGOZ: &str = "tr.yuksekbas.ikincigoz";
pub const LEGACY_DUZENEK: &str = "tr.yuksekbas.duzenek";
pub const LEGACY_DEGISIKIS: &str = "tr.degisikis.desktop";

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "macos")]
pub fn config_root() -> PathBuf {
    home().join("Library").join("Application Support")
}

#[cfg(target_os = "windows")]
pub fn config_root() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("AppData").join("Roaming"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn config_root() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}

/// Birleşik uygulamanın kendi yapılandırma dizini.
pub fn app_config_dir() -> PathBuf {
    config_root().join(APP_ID)
}

/// Eski bir uygulamanın yapılandırma dizini. Bu dizinler **hiçbir zaman silinmez**.
pub fn legacy_config_dir(identifier: &str) -> PathBuf {
    config_root().join(identifier)
}

/// Bir Tauri/WKWebView uygulamasının localStorage kökü (yalnız macOS).
///
/// DüzenEk ve Değişikİş tercihlerinin bir kısmını burada tutuyor. localStorage
/// webview origin'ine bağlıdır: bundle kimliği değişince **sessizce boşalır**.
/// Bu yüzden yol yalnız *tespit* için döndürülür; içeriği okumak ayrı bir iştir
/// (bkz. `legacy::duzenek_localstorage_status`).
#[cfg(target_os = "macos")]
pub fn legacy_webkit_dir(identifier: &str) -> PathBuf {
    home().join("Library").join("WebKit").join(identifier)
}

#[cfg(not(target_os = "macos"))]
pub fn legacy_webkit_dir(identifier: &str) -> PathBuf {
    config_root().join(identifier).join("EBWebView")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_dir_sits_under_the_platform_config_root() {
        assert!(app_config_dir().starts_with(config_root()));
        assert!(app_config_dir().ends_with(APP_ID));
    }

    #[test]
    fn legacy_dirs_are_siblings_of_the_app_dir_not_children() {
        // Migration okuma yönlüdür: eski dizin yeni dizinin İÇİNDE değildir,
        // dolayısıyla yeni dizini temizlemek eskisine asla dokunamaz.
        let app = app_config_dir();
        for id in [
            LEGACY_TAVZIH,
            LEGACY_IKINCIGOZ,
            LEGACY_DUZENEK,
            LEGACY_DEGISIKIS,
        ] {
            let legacy = legacy_config_dir(id);
            assert!(
                !legacy.starts_with(&app),
                "{id} yeni dizinin altında olmamalı"
            );
            assert_eq!(legacy.parent(), app.parent());
        }
    }
}
