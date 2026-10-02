//! Kaydetme yeri — Düzenle, Ekler ve Denetle'nin ortak kuralı.
//!
//! Saha: "Kayıt klasörleri arasında birlik yok. Farklı araçlarda farklı
//! klasörlere kaydediliyor sanki." Kaydetme ve klasör seçme pencerelerine
//! başlangıç klasörü verilmiyordu. Pencere işletim sisteminin en son
//! hatırladığı klasörde açılıyordu ve o klasör araçtan araca, oturumdan
//! oturuma değişiyordu.
//!
//! Tek kural: pencere KAYNAK belgenin klasöründe açılır, kullanıcı yine
//! istediği yeri seçer. Kaynak kullanıcıya ait bir klasörde değilse
//! (geçici klasör, e-posta eki önbelleği, uygulama verisi, arşivin içi) ya da
//! klasör yoksa veya yazılamıyorsa öneri verilmez ve işletim sisteminin
//! varsayılanı kalır: çıktıyı sonradan temizlenen gizli bir yere kaydetmeyi
//! önermek, rastgele bir klasörden kötüdür. Dönüştür bu kuralın dışındadır;
//! kendi çıktı klasörüne yazar (Ayarlar).
//!
//! Bu modül kabuğa aittir, bir modüle değil: `duzenek.rs` bağımsız
//! uygulamayla gövde eşitliği taşır (`check-command-parity.py`).
use std::path::{Component, Path, PathBuf};

/// Kaynağın klasörünün öneri olamayacağı yerler.
#[derive(Debug, Default)]
pub(crate) struct Places {
    /// Sistemin geçici klasörleri (her biçimiyle: `/var` ↔ `/private/var`).
    pub temp: Vec<PathBuf>,
    /// Kullanıcının ev klasörü.
    pub home: Option<PathBuf>,
    /// Uygulama verisi kökleri (Windows: `%APPDATA%`, `%LOCALAPPDATA%`).
    pub app_data: Vec<PathBuf>,
}

impl Places {
    fn current() -> Self {
        let mut temp = vec![std::env::temp_dir()];
        if let Ok(real) = std::env::temp_dir().canonicalize() {
            temp.push(real);
        }
        if cfg!(unix) {
            for fixed in [
                "/tmp",
                "/private/tmp",
                "/var/folders",
                "/private/var/folders",
            ] {
                temp.push(PathBuf::from(fixed));
            }
        }
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let app_data = ["APPDATA", "LOCALAPPDATA"]
            .iter()
            .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
            .collect();
        Places {
            temp,
            home,
            app_data,
        }
    }
}

/// Bileşen bileşen, büyük/küçük harfe duyarsız `starts_with`. Windows yolları
/// harfe duyarsızdır; macOS'un varsayılan dosya sistemi de öyle.
fn under(path: &Path, root: &Path) -> bool {
    let norm = |p: &Path| -> Vec<String> {
        p.components()
            .filter(|c| !matches!(c, Component::CurDir))
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect()
    };
    let (p, r) = (norm(path), norm(root));
    !r.is_empty() && p.len() >= r.len() && p[..r.len()] == r[..]
}

/// Arşiv gibi görünen bir bileşen: Windows Gezgini zip içinden açılan dosyayı
/// `%TEMP%\Temp1_ek.zip\…` altına çıkarır; arşivin içi de `….zip\…` olarak
/// görünür. İkisi de kalıcı bir yer değildir.
fn through_archive(dir: &Path) -> bool {
    dir.components().any(|c| {
        let name = c.as_os_str().to_string_lossy().to_lowercase();
        [".zip", ".rar", ".7z"]
            .iter()
            .any(|ext| name.ends_with(ext))
    })
}

/// Kullanıcının kendi klasörü sayılmayan yer: geçici, önbellek, uygulama
/// verisi ya da arşivin içi.
pub(crate) fn is_transient(dir: &Path, places: &Places) -> bool {
    if through_archive(dir) || places.temp.iter().any(|t| under(dir, t)) {
        return true;
    }
    if places.app_data.iter().any(|a| under(dir, a)) {
        return true;
    }
    let Some(home) = places.home.as_deref() else {
        return false;
    };
    // Windows: e-posta eki önbelleği (INetCache\Content.Outlook), %TEMP% ve
    // uygulama verisi AppData altındadır.
    if under(dir, &home.join("AppData")) {
        return true;
    }
    // macOS: Mail eki ("Mail Downloads"), Outlook önbelleği, uygulama verisi
    // ~/Library altındadır. İki istisna kullanıcının kendi bulut klasörüdür:
    // iCloud Drive (Mobile Documents) ve OneDrive/Dropbox/Google Drive
    // (CloudStorage). Onlar gerçek çalışma klasörleridir.
    let library = home.join("Library");
    under(dir, &library)
        && !under(dir, &library.join("Mobile Documents"))
        && !under(dir, &library.join("CloudStorage"))
}

/// Klasöre gerçekten yazılabiliyor mu? İzin bitleri salt okunur bir birimi
/// (DMG, kilitli ağ paylaşımı) söylemez; tek güvenilir yol denemektir.
/// Deneme adsız bir geçici dosyadır ve kapanınca kendiliğinden kaybolur;
/// kullanıcının hiçbir dosyasına dokunulmaz.
fn writable(dir: &Path) -> bool {
    tempfile::tempfile_in(dir).is_ok()
}

/// Kaydetme penceresinin açılacağı klasör: kaynak belgenin klasörü, ya da
/// öneri yok (`None`: işletim sisteminin varsayılanı).
pub(crate) fn start_folder(source: &Path, places: &Places) -> Option<PathBuf> {
    if !source.is_absolute() {
        return None;
    }
    let dir = source.parent()?;
    if is_transient(dir, places) || !dir.is_dir() || !writable(dir) {
        return None;
    }
    Some(dir.to_path_buf())
}

/// Kaydetme ve klasör seçme pencerelerinin başlangıç klasörü.
///
/// Ana iş parçacığında çalışmaz: Tauri eşzamanlı komutları orada çalıştırır
/// ve erişilemeyen bir ağ paylaşımındaki klasörü yoklamak pencereyi
/// dondururdu (bkz. `modules/ekler.rs`). Arayüz de uzun beklemez; öneri
/// gecikirse pencere işletim sisteminin varsayılanında açılır.
#[tauri::command]
pub async fn output_start_folder(source: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        start_folder(Path::new(&source), &Places::current())
            .map(|d| d.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unicode, boşluk, kesme işareti: sahadaki yolların biçimi.
    fn case_folder() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let dir = root
            .path()
            .join("Çağrı Şahin")
            .join("Dava Dosyaları")
            .join("Müvekkil'in işi");
        std::fs::create_dir_all(&dir).unwrap();
        (root, dir)
    }

    /// Test kökü sistemin geçici klasöründe durur; o yüzden "geçici" listesi
    /// testte yalnız açıkça verilen kökleri taşır.
    fn places(home: &Path, temp: &[&Path]) -> Places {
        Places {
            temp: temp.iter().map(|p| p.to_path_buf()).collect(),
            home: Some(home.to_path_buf()),
            app_data: vec![],
        }
    }

    #[test]
    fn the_dialog_starts_in_the_source_documents_folder() {
        let (root, dir) = case_folder();
        let source = dir.join("sözleşme.pdf");
        std::fs::write(&source, b"%PDF").unwrap();
        let p = places(&root.path().join("Çağrı Şahin"), &[]);
        assert_eq!(start_folder(&source, &p), Some(dir.clone()));
        // Kaynağın kendisine dokunulmaz; klasörde başka dosya da kalmaz.
        assert_eq!(std::fs::read(&source).unwrap(), b"%PDF");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[test]
    fn temp_mail_cache_app_data_and_archives_fall_back_to_the_os_default() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("Users").join("avukat");
        let temp = root.path().join("var").join("folders").join("T");
        let mac_mail =
            home.join("Library/Containers/com.apple.mail/Data/Library/Mail Downloads/1F2E");
        let mac_cache = home.join("Library").join("Caches").join("x");
        let win_outlook =
            home.join("AppData/Local/Microsoft/Windows/INetCache/Content.Outlook/AB12");
        let win_zip = temp.join("Temp1_ekler.zip");
        let in_zip = home.join("Downloads").join("ekler.ZIP").join("alt");
        for d in [
            &temp,
            &mac_mail,
            &mac_cache,
            &win_outlook,
            &win_zip,
            &in_zip,
        ] {
            std::fs::create_dir_all(d).unwrap();
            let source = d.join("ek.pdf");
            std::fs::write(&source, b"%PDF").unwrap();
            assert_eq!(
                start_folder(&source, &places(&home, &[&temp])),
                None,
                "{}",
                d.display()
            );
        }
    }

    #[test]
    fn cloud_folders_under_library_are_real_working_folders() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("avukat");
        for d in [
            home.join("Library/Mobile Documents/com~apple~CloudDocs/Dava"),
            home.join("Library/CloudStorage/OneDrive-Büro/Dava"),
        ] {
            std::fs::create_dir_all(&d).unwrap();
            let source = d.join("dilekçe.docx");
            std::fs::write(&source, b"PK").unwrap();
            assert_eq!(start_folder(&source, &places(&home, &[])), Some(d.clone()));
        }
    }

    #[test]
    fn a_missing_or_relative_source_folder_is_not_suggested() {
        let (root, dir) = case_folder();
        let p = places(root.path(), &[]);
        assert_eq!(start_folder(&dir.join("yok").join("belge.pdf"), &p), None);
        assert_eq!(start_folder(Path::new("belge.pdf"), &p), None);
        assert_eq!(start_folder(Path::new("Dava/belge.pdf"), &p), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_folder_is_not_suggested() {
        use std::os::unix::fs::PermissionsExt;
        let (root, dir) = case_folder();
        let source = dir.join("belge.pdf");
        std::fs::write(&source, b"%PDF").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let suggested = start_folder(&source, &places(root.path(), &[]));
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(suggested, None);
    }
}
