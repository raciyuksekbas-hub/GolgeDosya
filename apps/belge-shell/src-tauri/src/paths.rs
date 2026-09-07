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

/// Uygulamanın gerçekten okuyup yazdığı yapılandırma kökü.
///
/// Sürüm derlemesinde platformun kendi kökü. Test derlemesinde ise sürece özel,
/// geçici bir kök: test paketi kullanıcının kurulu ayarlarına **yapı gereği**
/// erişemez. Bu bir üslup tercihi değil; Tavzih'te tam bu eksiklik yüzünden
/// testler gerçek `preferences.json` dosyasını bozdu ve kullanıcının kabul
/// ettiği kullanım koşulları kaydı silindi. Aynı kusur birleşik uygulamaya
/// taşınmıyor.
///
/// `legacy_config_dir` ve `legacy_webkit_dir` de bu köke dayandığı için test
/// paketi eski uygulamaların gerçek veri dizinlerini de göremez.
#[cfg(not(test))]
fn effective_config_root() -> PathBuf {
    config_root()
}

#[cfg(test)]
fn effective_config_root() -> PathBuf {
    test_config_root()
}

/// Test süreci başına bir, test iş parçacığı başına bir geçici kök.
///
/// İş parçacığı başına: ayar deposu oku-değiştir-yaz döngüsüdür, tek dosyayı
/// paylaşan testler birbiriyle yarışır. Tavzih'teki kusuru gizleyen de buydu.
#[cfg(test)]
fn test_config_root() -> PathBuf {
    use std::cell::OnceCell;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    static BASE: OnceLock<PathBuf> = OnceLock::new();
    let base = BASE.get_or_init(|| {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("belge-test-config-{}-{stamp}", std::process::id()))
    });

    thread_local! {
        static DIR: OnceCell<PathBuf> = const { OnceCell::new() };
    }
    DIR.with(|cell| {
        cell.get_or_init(|| {
            let thread: String = format!("{:?}", std::thread::current().id())
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect();
            let dir = base.join(thread);
            std::fs::create_dir_all(&dir).expect("test yapılandırma kökü oluşturulamadı");
            dir
        })
        .clone()
    })
}

/// Birleşik uygulamanın kendi yapılandırma dizini.
pub fn app_config_dir() -> PathBuf {
    effective_config_root().join(APP_ID)
}

/// Eski bir uygulamanın yapılandırma dizini. Bu dizinler **hiçbir zaman silinmez**.
pub fn legacy_config_dir(identifier: &str) -> PathBuf {
    effective_config_root().join(identifier)
}

/// Bir Tauri/WKWebView uygulamasının localStorage kökü (yalnız macOS).
///
/// DüzenEk ve Değişikİş tercihlerinin bir kısmını burada tutuyor. localStorage
/// webview origin'ine bağlıdır: bundle kimliği değişince **sessizce boşalır**.
/// Bu yüzden yol yalnız *tespit* için döndürülür; içeriği okumak ayrı bir iştir
/// (bkz. `legacy::duzenek_localstorage_status`).
#[cfg(target_os = "macos")]
pub fn legacy_webkit_dir(identifier: &str) -> PathBuf {
    webkit_root().join(identifier)
}

/// WebKit localStorage kökü. `Application Support` altında **değildir**, bu yüzden
/// ayrı bir yönlendirme gerekir — testler burayı da görmemeli.
#[cfg(all(target_os = "macos", not(test)))]
fn webkit_root() -> PathBuf {
    home().join("Library").join("WebKit")
}

#[cfg(all(target_os = "macos", test))]
fn webkit_root() -> PathBuf {
    test_config_root().join("WebKit")
}

#[cfg(not(target_os = "macos"))]
pub fn legacy_webkit_dir(identifier: &str) -> PathBuf {
    effective_config_root().join(identifier).join("EBWebView")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_dir_sits_under_the_platform_config_root() {
        // Kurulu uygulamanın yeri: test derlemesinin yazdığı yer değil.
        let installed = config_root().join(APP_ID);
        assert!(installed.starts_with(config_root()));
        assert!(installed.ends_with(APP_ID));
        if cfg!(target_os = "macos") {
            assert!(installed.to_string_lossy().contains("Application Support"));
        }
    }

    /// Yalıtımın kendisi: varsayılmıyor, doğrulanıyor.
    ///
    /// Tavzih'te tam bu güvence yoktu; testler kullanıcının gerçek ayar dosyasını
    /// yazdı ve kabul kaydını sildi. Bu test, aynı kusur birleşik uygulamaya
    /// sızdığı anda başarısız olur.
    #[test]
    fn the_suite_cannot_reach_the_installed_data() {
        let installed = config_root();
        for dir in [
            app_config_dir(),
            legacy_config_dir(LEGACY_TAVZIH),
            legacy_config_dir(LEGACY_IKINCIGOZ),
            legacy_config_dir(LEGACY_DUZENEK),
            legacy_config_dir(LEGACY_DEGISIKIS),
            legacy_webkit_dir(LEGACY_DUZENEK),
        ] {
            assert!(
                !dir.starts_with(&installed),
                "{} kurulu veri kökünün altında",
                dir.display()
            );
            assert!(
                dir.starts_with(std::env::temp_dir()),
                "{} geçici bir dizin değil",
                dir.display()
            );
        }
    }

    /// Eski dizinler *okunur*, asla yazılmaz — ama test paketi onları hiç görmemeli.
    #[test]
    fn the_suite_cannot_read_the_real_legacy_directories() {
        let real_tavzih = config_root().join(LEGACY_TAVZIH);
        assert_ne!(legacy_config_dir(LEGACY_TAVZIH), real_tavzih);
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
