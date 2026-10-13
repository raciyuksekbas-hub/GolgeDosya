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
//! İkinci yarı sonucun yeridir: kaydedilen dosya "Klasörde Göster" ile
//! klasöründe seçili gösterilir. Arayüzün opener izni yoktur ve olmayacaktır;
//! yalnız bu oturumda yazılmış, var olan ve uygulamanın ürettiği türde bir
//! yol kabul edilir. Dosya HİÇBİR ZAMAN uygulamasıyla açılmaz.
//!
//! Bu modül kabuğa aittir, bir modüle değil: Denetle'nin Rust tarafına opener
//! girmez (`check-architecture.sh`, 5c) ve `duzenek.rs` bağımsız uygulamayla
//! gövde eşitliği taşır (`check-command-parity.py`).
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

/// Uygulamanın bu açılışı. "Bu oturumda yazıldı" bundan sonra değişmiş demektir.
static SESSION_START: OnceLock<SystemTime> = OnceLock::new();

/// Uygulama açılırken bir kez çağrılır (`run`).
pub fn mark_session_start() {
    let _ = SESSION_START.set(SystemTime::now());
}

/// Bu açılışın zamanı. İşaretlenmemişse (olmamalı) "şimdi": yalnız bundan
/// sonra yazılan dosya bu oturumun sayılır.
pub(crate) fn session_start() -> SystemTime {
    SESSION_START.get().copied().unwrap_or_else(SystemTime::now)
}

/// Saat kayması payıyla "bu oturumda yazıldı" alt sınırı.
pub(crate) fn session_floor(session_start: SystemTime) -> SystemTime {
    session_start
        .checked_sub(CLOCK_SKEW)
        .unwrap_or(session_start)
}

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

/// "Klasörde Göster" neyi yapacak?
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reveal {
    /// Dosyanın klasörünü aç ve dosyayı seç.
    Select(PathBuf),
    /// Paket klasörünün KENDİSİNİ aç (Görsel paketi; Ekler ile aynı karar,
    /// saha #10). Klasör "üst klasörde seç"e verilmez (saha #13).
    OpenFolder(PathBuf),
}

/// Düzenle (PDF) ve Denetle (DOCX, UDF) çıktılarının türleri.
const SAVED_EXTENSIONS: [&str; 3] = ["pdf", "docx", "udf"];

const NOT_FOUND: &str = "Dosya bulunamadı. Taşınmış veya silinmiş olabilir.";

/// Ağ paylaşımındaki dosyanın zamanını sunucunun saati yazar; o saat bu
/// bilgisayarınkinden biraz geri olabilir. Açılıştan hemen sonra kaydedilen
/// dosya bu yüzden "eski" sayılmasın.
const CLOCK_SKEW: Duration = Duration::from_secs(120);

/// Yalnız bu oturumda yazılmış, var olan ve uygulamanın ürettiği türde yol.
///
/// Kullanıcı kaydetme penceresinde adı değiştirebilir; bu yüzden ad önekine
/// değil türe ve ZAMANA bakılır: dosya uygulama açıldıktan sonra yazılmış
/// olmalıdır. Klasör ise yalnız uygulamanın paket adını (`GolgeDosya-…`)
/// taşıyorsa ve uzantısızsa kabul edilir: macOS'ta `….app` gibi bir klasör
/// "açılınca" bir program çalışır. Bağlantı (symlink) izlenmez.
pub(crate) fn reveal_target(path: &Path, session_start: SystemTime) -> Result<Reveal, String> {
    if !path.is_absolute() {
        return Err(NOT_FOUND.into());
    }
    let meta = std::fs::symlink_metadata(path).map_err(|_| NOT_FOUND.to_string())?;
    let since = session_start
        .checked_sub(CLOCK_SKEW)
        .unwrap_or(session_start);
    let fresh = meta.modified().is_ok_and(|m| m >= since);
    if !fresh || meta.file_type().is_symlink() {
        return Err(NOT_FOUND.into());
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if meta.is_file() {
        let ext = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if SAVED_EXTENSIONS.contains(&ext.as_str()) {
            return Ok(Reveal::Select(path.to_path_buf()));
        }
    } else if meta.is_dir()
        && name.starts_with("GolgeDosya-")
        && Path::new(name).extension().is_none()
    {
        return Ok(Reveal::OpenFolder(path.to_path_buf()));
    }
    Err(NOT_FOUND.into())
}

/// Az önce kaydedilen dosyayı klasöründe göster.
#[tauri::command]
pub fn output_reveal(path: String) -> Result<(), String> {
    // Başlangıç işaretlenmemişse (olmamalı) yalnız az önce yazılmış yol sayılır.
    let since = SESSION_START.get().copied().unwrap_or_else(SystemTime::now);
    let opened = match reveal_target(Path::new(&path), since)? {
        Reveal::Select(file) => tauri_plugin_opener::reveal_item_in_dir(file),
        Reveal::OpenFolder(dir) => tauri_plugin_opener::open_path(dir, None::<&str>),
    };
    opened.map_err(|_| "Klasör açılamadı.".to_string())
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

    #[test]
    fn a_file_saved_in_this_session_is_shown_selected_in_its_folder() {
        let (_root, dir) = case_folder();
        let start = SystemTime::now() - Duration::from_secs(5);
        for name in [
            "GolgeDosya-compress-2026-10-01_213331.pdf",
            "Ek 3 bilirkişi.PDF",
            "dava - Düzeltilmiş.docx",
            "dava - Düzeltilmiş (2).udf",
        ] {
            let file = dir.join(name);
            std::fs::write(&file, b"x").unwrap();
            assert_eq!(
                reveal_target(&file, start),
                Ok(Reveal::Select(file.clone())),
                "{name}"
            );
        }
    }

    #[test]
    fn a_network_share_clock_a_little_behind_is_tolerated() {
        // Sunucunun saati bir dakika geri: dosya açılıştan "önce" yazılmış
        // görünür ama bu oturumun çıktısıdır.
        let (_root, dir) = case_folder();
        let file = dir.join("GolgeDosya-merge-2026-10-02_093000.pdf");
        std::fs::write(&file, b"%PDF").unwrap();
        let start = SystemTime::now() + Duration::from_secs(60);
        assert_eq!(
            reveal_target(&file, start),
            Ok(Reveal::Select(file.clone()))
        );
    }

    #[test]
    fn an_image_package_folder_is_opened_itself() {
        let (_root, dir) = case_folder();
        let package = dir.join("GolgeDosya-Gorseller-jyT24K");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join("sayfa-001.png"), b"x").unwrap();
        let start = SystemTime::now() - Duration::from_secs(5);
        assert_eq!(
            reveal_target(&package, start),
            Ok(Reveal::OpenFolder(package.clone()))
        );
    }

    #[test]
    fn nothing_else_is_revealed() {
        let (_root, dir) = case_folder();
        let start = SystemTime::now() - Duration::from_secs(5);
        // Var olmayan yol.
        assert!(reveal_target(&dir.join("silinmiş.pdf"), start).is_err());
        // Göreli yol.
        assert!(reveal_target(Path::new("GolgeDosya-a.pdf"), start).is_err());
        // Uygulamanın üretmediği tür: çalıştırılabilir ya da betik seçtirilmez.
        for name in ["kur.app", "betik.sh", "not.txt", "program.exe"] {
            let other = dir.join(name);
            std::fs::write(&other, b"x").unwrap();
            assert!(reveal_target(&other, start).is_err(), "{name}");
        }
        // Paket adını taşımayan klasör.
        assert!(reveal_target(&dir, start).is_err());
        // Paket adını taşısa da uzantılı klasör: macOS'ta açılınca çalışan
        // bir uygulama paketi olabilir.
        for name in ["GolgeDosya-kur.app", "GolgeDosya-x.bundle"] {
            let bundle = dir.join(name);
            std::fs::create_dir_all(&bundle).unwrap();
            assert!(reveal_target(&bundle, start).is_err(), "{name}");
        }
        // Uygulama açılmadan ÖNCE yazılmış dosya: bu oturumun çıktısı değil.
        let old = dir.join("eski.pdf");
        std::fs::write(&old, b"%PDF").unwrap();
        let later = SystemTime::now() + CLOCK_SKEW + Duration::from_secs(60);
        assert!(reveal_target(&old, later).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_followed() {
        let (_root, dir) = case_folder();
        let start = SystemTime::now() - Duration::from_secs(5);
        let target = dir.join("GolgeDosya-hedef.pdf");
        std::fs::write(&target, b"%PDF").unwrap();
        let link = dir.join("GolgeDosya-link.pdf");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(reveal_target(&link, start).is_err());
    }
}
