//! Eski uygulamaların kullanıcı verisinin okunması (talep §11).
//!
//! Üç değişmez:
//!
//! 1. **Eski dizinler asla silinmez, taşınmaz, değiştirilmez.** Bu modül eski
//!    konumları yalnız `read_to_string` ile açar.
//! 2. **Zaten ayarlanmış bir değer ezilmez.** Migration boş alanları doldurur;
//!    kullanıcının birleşik uygulamada yaptığı bir seçimi geri almaz. Birleşik
//!    depoda veri varken bağımsız uygulamalar hiç okunmaz (bkz. `migrate_into`).
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

#[derive(serde::Deserialize, Default)]
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
    respect_reduced_motion: Option<bool>,
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
    // Ayar dosyası hiç çözülemediyse nedeni burada tutulur; dosya devri (sözlük,
    // profiller) yine de çalışır.
    let mut unreadable_settings: Option<String> = None;

    if file.is_file() {
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) => {
                return SourceStatus::Unreadable {
                    detail: e.to_string(),
                }
            }
        };
        // Ayarlar çözülemezse BURADAN ÇIKILMAZ. Eskiden tek bir yanlış tipli
        // alan (`"textScale": "büyük"`) bütün fonksiyonu erken döndürüyordu ve
        // aşağıdaki SÖZLÜK/PROFİL kopyalaması hiç çalışmıyordu: kullanıcının
        // öğrettiği kelimeler ve profilleri, ilgisiz bir ayar hatası yüzünden
        // kalıcı olarak kayboluyordu. Ayar devri ile dosya devri bağımsızdır.
        //
        // Ayrıca tek bozuk alan bütün ayar devrini düşürmesin: önce katı,
        // olmazsa alan-bazlı kurtarma (settings.rs'teki `lenient` ile aynı
        // yaklaşım).
        let old: IkinciGozSettings = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(serde_json::Value::Object(map)) => {
                    // Alan-bazlı kurtarma: yalnız TİPİ TUTAN alanlar alınır,
                    // bozuk olan atlanır. Kapanış yerine makro, çünkü her alanın
                    // hedef tipi farklı.
                    macro_rules! take {
                        ($key:literal) => {
                            map.get($key)
                                .cloned()
                                .and_then(|v| serde_json::from_value(v).ok())
                        };
                    }
                    IkinciGozSettings {
                        disabled_rules: take!("disabledRules"),
                        include_review: take!("includeReview"),
                        source_read_only: take!("sourceReadOnly"),
                        theme: take!("theme"),
                        text_scale: take!("textScale"),
                        high_contrast: take!("highContrast"),
                        reduce_motion: take!("reduceMotion"),
                        respect_reduced_motion: take!("respectReducedMotion"),
                        linear_results: take!("linearResults"),
                    }
                }
                _ => {
                    unreadable_settings = Some(e.to_string());
                    IkinciGozSettings::default()
                }
            },
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
        if s.respect_reduced_motion == defaults.respect_reduced_motion {
            if let Some(v) = old.respect_reduced_motion {
                if v != defaults.respect_reduced_motion {
                    s.respect_reduced_motion = v;
                    fields.push("respectReducedMotion".to_string());
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
    // Ayar dosyası hiç çözülemediyse bunu bildir — ama sözlük/profil kopyalandıysa
    // o kazanç kaybolmaz, rapora yazılır.
    if let Some(detail) = unreadable_settings {
        if fields.is_empty() {
            return SourceStatus::Unreadable { detail };
        }
        return SourceStatus::Migrated { fields };
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
pub fn migrate_duzenek_renderer(settings: &mut Settings) -> SourceStatus {
    // Zaten bir değer varsa dokunma: migration boş alanı doldurur, seçimi ezmez.
    if settings
        .renderer_path
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty())
    {
        return SourceStatus::NothingToDo;
    }
    let dir = paths::legacy_webkit_dir(paths::LEGACY_DUZENEK);
    if !dir.exists() {
        return SourceStatus::NotFound;
    }
    let Some(db) = find_localstorage(&dir) else {
        return SourceStatus::NotFound;
    };
    match read_localstorage_value(&db, "duzenek-renderer") {
        Err(detail) => SourceStatus::Unreadable { detail },
        Ok(None) => SourceStatus::NothingToDo,
        Ok(Some(path)) => {
            if !is_usable_renderer(Path::new(&path)) {
                // SESSİZCE BAŞKA BİR KURULUM SEÇME: kullanıcı hangi kurulumu
                // seçtiğini biliyordu, biz bilmiyoruz. Anlamlı hata ve yeniden
                // seçim akışı, sessiz bir sürprizden iyidir.
                return SourceStatus::NeedsManualStep {
                    detail: format!(
                        "Önceden seçtiğiniz LibreOffice artık bu konumda değil: {path}. \
                         Otomatik olarak başka bir kurulum seçilmedi; Düzenle bölümünde \
                         yeniden seçmeniz gerekiyor."
                    ),
                };
            }
            settings.renderer_path = Some(path);
            SourceStatus::Migrated {
                fields: vec!["rendererPath".to_string()],
            }
        }
    }
}

fn is_usable_renderer(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Eski WebKit localStorage'ından tek bir anahtarı oku.
///
/// Üç değişmez:
/// * Dosya ASLA yerinde açılmaz; geçici bir kopya üzerinde çalışılır. Böylece
///   eski veriye yazma ihtimali kalmaz.
/// * Değerler UTF-16LE'dir; WebKit'in localStorage biçimi budur.
/// * Okuma başarısızsa hata döndürülür, tahmin yürütülmez.
///
/// macOS'un kendi `sqlite3` aracı kullanılır. Alternatif, tek seferlik bir okuma
/// için ~1,5 MB'lık gömülü SQLite bağımlılığını kalıcı olarak taşımaktı.
fn read_localstorage_value(db: &Path, key: &str) -> Result<Option<String>, String> {
    const SQLITE: &str = "/usr/bin/sqlite3";
    if !Path::new(SQLITE).is_file() {
        return Err("sqlite3 bulunamadı".into());
    }
    // Geçici kopya `NamedTempFile` ile tutulur: düşerken kendiliğinden silinir.
    // Böylece migration kodunda açık bir dosya silme çağrısı bulunmaz — panik
    // hâlinde bile artık dosya kalmaz ve sürüm kapısının "migration hiçbir şeyi
    // silmez" değişmezi harfiyen doğru kalır.
    let tmp = tempfile::Builder::new()
        .prefix("belge-legacy-ls-")
        .suffix(".sqlite3")
        .tempfile()
        .map_err(|e| e.to_string())?;
    std::fs::copy(db, tmp.path()).map_err(|e| e.to_string())?;
    let query = format!("SELECT hex(value) FROM ItemTable WHERE key='{key}';");
    // Dış süreç sınırından geçer. Eski verinin kopyası okunuyor; yine de
    // zaman aşımı ve ağ reddi uygulanır — migration okuması bir kullanıcı
    // işlemini süresiz bekletemez.
    let out = process_bridge::run(
        process_bridge::Spawn::new(std::path::Path::new(SQLITE))
            .arg(tmp.path())
            .arg(&query)
            .timeout(std::time::Duration::from_secs(20))
            .network(process_bridge::NetworkPolicy::Deny),
    )
    .map_err(|e| {
        let stderr = String::from_utf8_lossy(e.stderr()).trim().to_string();
        if stderr.is_empty() {
            e.to_string()
        } else {
            stderr
        }
    })?;
    let hex = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if hex.is_empty() {
        return Ok(None);
    }
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<_, _>>()
        .map_err(|_| "değer onaltılık olarak çözülemedi".to_string())?;
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units)
        .map(|s| if s.trim().is_empty() { None } else { Some(s) })
        .map_err(|_| "değer UTF-16 olarak çözülemedi".to_string())
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

// ------------------------------------------------------- önceki birleşik ad

/// Ürünün GölgeDosya adını almadan önceki birleşik deposu.
///
/// Şema aynı olduğu için bu bir alan eşlemesi değil, **devralmadır**: yeni
/// kimlik altında henüz bir `settings.json` yoksa eskisi olduğu gibi okunur.
/// Yeni dosya varsa hiçbir şey yapılmaz — kullanıcının yeni ad altında yaptığı
/// değişiklikler eski dosyayla ezilmez. Eski dizin okunur, **asla silinmez**;
/// ikinci çalıştırma sonucu değiştirmez.
fn migrate_belge(old_dir: &Path, target_dir: &Path, s: &mut Settings) -> SourceStatus {
    let old = old_dir.join(crate::settings::SETTINGS_FILE);
    if !old.is_file() {
        return SourceStatus::NotFound;
    }
    if crate::settings::settings_path(target_dir).is_file() {
        // Yeni ad altında zaten ayar var: devralma bitmiş.
        return SourceStatus::NothingToDo;
    }
    let text = match std::fs::read_to_string(&old) {
        Ok(t) => t,
        Err(e) => {
            return SourceStatus::Unreadable {
                detail: e.to_string(),
            }
        }
    };
    let inherited: Settings = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return SourceStatus::Unreadable {
                detail: e.to_string(),
            }
        }
    };
    *s = inherited;
    let mut fields = vec!["settings.json".to_string()];

    // Sözlük ve profiller de devralınır. Önceki birleşik "Belge" kurulumu
    // İkinciGöz'ün sözlüğünü KENDİ yapılandırma dizinine taşımıştı; buradan
    // kopyalanmazsa kalıcı olarak geride kalırdı: `migrate_belge` başarılı
    // olduğu an `pristine` false olur ve İkinciGöz kaynağı artık hiç
    // okunmaz (bkz. çağıran). Kullanıcının öğrettiği kelimeler ve kurduğu
    // profiller yükseltmede sessizce kaybolurdu.
    //
    // Ayrıştırılmaz, birebir kopyalanır — şemayı yeniden yorumlamak
    // kullanıcının emeğini bozmanın en kolay yoludur (bkz. IKINCIGOZ_VERBATIM).
    for name in IKINCIGOZ_VERBATIM {
        let src = old_dir.join(name);
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
    SourceStatus::Migrated { fields }
}

// ----------------------------------------------------------------- orchestrator

/// Eski uygulamaların ayarlarını **bir kez** birleşik depoya taşır.
///
/// Yeniden çalıştırmak güvenlidir: hiçbir değer ezilmediği için sonuç değişmez.
///
/// Bağımsız uygulamalar (Tavzih, İkinciGöz, DüzenEk) yalnız birleşik depo el
/// değmemişken okunur: açılışta birleşik `settings.json` yoksa ve önceki
/// birleşik addan devralma olmadıysa. Birleşik depoda veri varken varsayılana
/// eşit bir değer de kullanıcının seçimidir; ayar dosyası "hiç dokunulmadı" ile
/// "bilerek varsayılana getirildi"yi ayırt edemez. Bu kapı olmadan kullanıcının
/// varsayılana geri çektiği tema, inceleme ya da çıktı klasörü her açılışta eski
/// uygulamanın değerine dönüyordu.
pub fn migrate_into(target_dir: &Path, settings: &mut Settings) -> MigrationReport {
    let before = settings.clone();
    let mut sources = Vec::new();
    let unified_existed = crate::settings::settings_path(target_dir).is_file();

    // Önce önceki birleşik ad: aynı şema, doğrudan devralma. Diğer kaynaklar
    // yalnız boş alanları doldurur, bu yüzden sıralama önemlidir.
    let belge_dir = paths::legacy_config_dir(paths::LEGACY_BELGE);
    let belge_status = migrate_belge(&belge_dir, target_dir, settings);
    let pristine = !unified_existed && !matches!(belge_status, SourceStatus::Migrated { .. });
    sources.push(SourceReport {
        app: "Belge".into(),
        identifier: paths::LEGACY_BELGE.into(),
        path: belge_dir.display().to_string(),
        status: belge_status,
    });

    let tavzih_dir = paths::legacy_config_dir(paths::LEGACY_TAVZIH);
    sources.push(SourceReport {
        app: "Tavzih".into(),
        identifier: paths::LEGACY_TAVZIH.into(),
        path: tavzih_dir.display().to_string(),
        status: if pristine {
            migrate_tavzih(&tavzih_dir, settings)
        } else {
            left_untouched(&tavzih_dir)
        },
    });

    let ig_dir = paths::legacy_config_dir(paths::LEGACY_IKINCIGOZ);
    sources.push(SourceReport {
        app: "İkinciGöz".into(),
        identifier: paths::LEGACY_IKINCIGOZ.into(),
        path: ig_dir.display().to_string(),
        status: if pristine {
            migrate_ikincigoz(&ig_dir, target_dir, settings)
        } else {
            left_untouched(&ig_dir)
        },
    });

    let duzenek_dir = paths::legacy_webkit_dir(paths::LEGACY_DUZENEK);
    sources.push(SourceReport {
        app: "DüzenEk".into(),
        identifier: paths::LEGACY_DUZENEK.into(),
        path: duzenek_dir.display().to_string(),
        status: if pristine {
            migrate_duzenek_renderer(settings)
        } else {
            left_untouched(&duzenek_dir)
        },
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

/// Birleşik depoda veri varken bağımsız kaynak okunmaz; yalnız varlığı raporlanır.
fn left_untouched(path: &Path) -> SourceStatus {
    if path.exists() {
        SourceStatus::NothingToDo
    } else {
        SourceStatus::NotFound
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

    /// P1: Bozuk bir eski `settings.json` SÖZLÜK ve PROFİL devrini iptal
    /// ediyordu. Erken `return` dosya kopyalamasından ÖNCEydi; kullanıcının
    /// öğrettiği kelimeler ve profilleri, ilgisiz bir ayar tipi hatası
    /// yüzünden kalıcı olarak kayboluyordu.
    /// P1: Önceki birleşik "Belge" kurulumundan yükseltmede sözlük ve
    /// profiller devralınmıyordu. `migrate_belge` yalnız settings.json
    /// taşıyor, ama başarılı olduğu an `pristine` false oluyor ve İkinciGöz
    /// kaynağı bir daha hiç okunmuyordu — kullanıcının öğrettiği kelimeler
    /// yükseltmede geride kalıyordu.
    #[test]
    fn upgrading_from_the_previous_unified_install_carries_the_dictionary_and_profiles() {
        let d = tmp("belge");
        let t = tmp("belge-hedef");
        std::fs::write(
            d.join(crate::settings::SETTINGS_FILE),
            r#"{"theme":"dark","textScale":150}"#,
        )
        .unwrap();
        std::fs::write(d.join("dictionary.json"), r#"{"accepted":["tahkim"]}"#).unwrap();
        std::fs::write(d.join("profiles.json"), r#"{"profiles":[]}"#).unwrap();

        let mut s = Settings::default();
        let status = migrate_belge(&d, &t, &mut s);

        assert_eq!(s.theme, "dark", "ayarlar devralınmalı");
        assert!(
            t.join("dictionary.json").is_file(),
            "P1: sözlük devralınmadı (yükseltmede kayıp)"
        );
        assert!(t.join("profiles.json").is_file(), "P1: profiller devralınmadı");
        assert_eq!(
            std::fs::read_to_string(t.join("dictionary.json")).unwrap(),
            r#"{"accepted":["tahkim"]}"#,
            "sözlük birebir kopyalanmalı"
        );
        match status {
            SourceStatus::Migrated { fields } => {
                assert!(fields.contains(&"dictionary.json".to_string()), "{fields:?}");
            }
            other => panic!("beklenen Migrated, gelen {other:?}"),
        }
    }

    #[test]
    fn a_corrupt_legacy_settings_file_must_not_cancel_dictionary_and_profile_migration() {
        let d = tmp("ig-bozuk");
        let t = tmp("ig-bozuk-hedef");
        // Ayar dosyası tamamen bozuk (JSON değil).
        std::fs::write(d.join("settings.json"), "{bu json degil").unwrap();
        // Kullanıcının emeği: öğretilen kelimeler ve profiller.
        std::fs::write(d.join("dictionary.json"), r#"{"accepted":["tahkim"]}"#).unwrap();
        std::fs::write(d.join("profiles.json"), r#"{"profiles":[]}"#).unwrap();

        let mut s = Settings::default();
        let status = migrate_ikincigoz(&d, &t, &mut s);

        assert!(
            t.join("dictionary.json").is_file(),
            "P1: bozuk ayar dosyası SÖZLÜĞÜN devrini iptal etti (kalıcı kayıp)"
        );
        assert!(
            t.join("profiles.json").is_file(),
            "P1: bozuk ayar dosyası PROFİLLERİN devrini iptal etti"
        );
        assert_eq!(
            std::fs::read_to_string(t.join("dictionary.json")).unwrap(),
            r#"{"accepted":["tahkim"]}"#,
            "sözlük birebir kopyalanmalı"
        );
        // Dosyalar taşındıysa sonuç "aktarıldı" olmalı, "okunamadı" değil.
        match status {
            SourceStatus::Migrated { fields } => {
                assert!(fields.contains(&"dictionary.json".to_string()), "{fields:?}");
                assert!(fields.contains(&"profiles.json".to_string()), "{fields:?}");
            }
            other => panic!("beklenen Migrated, gelen {other:?}"),
        }
    }

    /// Tek bir yanlış tipli alan bütün AYAR devrini düşürmemeli: sağlam alanlar
    /// taşınır, bozuk alan atlanır (settings.rs'teki lenient ile aynı sözleşme).
    #[test]
    fn one_wrong_typed_legacy_field_does_not_discard_the_other_settings() {
        let d = tmp("ig-kismi");
        let t = tmp("ig-kismi-hedef");
        // textScale sayı olmalı ama metin verilmiş; diğerleri sağlam.
        std::fs::write(
            d.join("settings.json"),
            r#"{"textScale":"buyuk","highContrast":"on","disabledRules":["ORTHO_01"]}"#,
        )
        .unwrap();
        let mut s = Settings::default();
        migrate_ikincigoz(&d, &t, &mut s);
        assert_eq!(s.high_contrast, "on", "sağlam alan taşınmalı");
        assert_eq!(s.disabled_rules, vec!["ORTHO_01".to_string()]);
        assert_eq!(
            s.text_scale,
            Settings::default().text_scale,
            "bozuk alan varsayılanda kalmalı"
        );
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
    fn an_already_chosen_renderer_is_never_overwritten() {
        let mut s = Settings {
            renderer_path: Some("/kullanicinin/secimi/soffice".into()),
            ..Default::default()
        };
        assert_eq!(migrate_duzenek_renderer(&mut s), SourceStatus::NothingToDo);
        assert_eq!(
            s.renderer_path.as_deref(),
            Some("/kullanicinin/secimi/soffice")
        );
    }

    #[test]
    fn a_utf16_localstorage_value_is_decoded() {
        // WebKit localStorage değerleri UTF-16LE'dir. Bayt sırası yanlış
        // okunursa yol sessizce bozulur; bu test onu yakalar.
        let d = tmp("ls");
        let db = d.join("localstorage.sqlite3");
        let path = "/Uygulamalar/LibreOffice.app/Contents/MacOS/soffice";
        let hex: String = path
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .map(|b| format!("{b:02X}"))
            .collect();
        let sql = format!(
            "CREATE TABLE ItemTable(key TEXT, value BLOB); \
             INSERT INTO ItemTable VALUES('duzenek-renderer', x'{hex}');"
        );
        let ok = std::process::Command::new("/usr/bin/sqlite3")
            .arg(&db)
            .arg(&sql)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            assert_eq!(
                read_localstorage_value(&db, "duzenek-renderer")
                    .unwrap()
                    .as_deref(),
                Some(path)
            );
            assert_eq!(
                read_localstorage_value(&db, "olmayan-anahtar").unwrap(),
                None
            );
        }
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_renderer_that_no_longer_exists_asks_the_user_instead_of_guessing() {
        assert!(!is_usable_renderer(Path::new("/olmayan/soffice")));
        // Dizin de çalıştırılabilir bir ikili değildir.
        assert!(!is_usable_renderer(&std::env::temp_dir()));
        // Çalıştırma izni olmayan gerçek bir dosya da kabul edilmez.
        let f = std::env::temp_dir().join(format!("belge-notexec-{}", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        assert!(!is_usable_renderer(&f));
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn the_older_motion_switch_is_migrated_too() {
        // reduceMotion "system" iken respectReducedMotion=false, "sistem azaltma
        // dese bile animasyonları göster" demektir. Taşınmazsa kullanıcı sessizce
        // tersine bir davranışa geçerdi.
        let d = tmp("hareket");
        let t = tmp("hareket-hedef");
        std::fs::write(
            d.join("settings.json"),
            r#"{"reduceMotion":"system","respectReducedMotion":false}"#,
        )
        .unwrap();
        let mut s = Settings::default();
        migrate_ikincigoz(&d, &t, &mut s);
        assert!(!s.respect_reduced_motion);
        assert_eq!(s.reduce_motion, "system");
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
        // "Belge" = ürünün GölgeDosya adını almadan önceki birleşik deposu.
        // Aynı şemayı kullandığı için ilk sırada: devralma, diğer kaynakların
        // boş alan doldurmasından önce gerçekleşmeli.
        assert_eq!(
            apps,
            vec!["Belge", "Tavzih", "İkinciGöz", "DüzenEk", "Değişikİş"]
        );
        std::fs::remove_dir_all(&d).ok();
    }

    /// Eski uygulama ayarlarını testin kendi (iş parçacığına özel) kökündeki
    /// eski dizinlere yazar; kapanışta siler.
    struct LegacyFixture(Vec<PathBuf>);
    impl LegacyFixture {
        fn new(files: &[(&str, &str, &str)]) -> Self {
            let mut dirs = Vec::new();
            for (id, name, text) in files {
                let dir = paths::legacy_config_dir(id);
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join(name), text).unwrap();
                dirs.push(dir);
            }
            LegacyFixture(dirs)
        }
    }
    impl Drop for LegacyFixture {
        fn drop(&mut self) {
            for d in &self.0 {
                std::fs::remove_dir_all(d).ok();
            }
        }
    }

    /// Paketlenmiş uygulamada yeniden üretildi (2026-09-15): kullanıcı Ayarlar'da
    /// temayı "system"e, incelemeyi açığa, çıktı klasörünü varsayılana geri
    /// çekiyor; bir sonraki açılışta migration eski Tavzih/İkinciGöz değerlerini
    /// yeniden basıyordu. Varsayılana eşit değer "boş" sayılıyordu.
    #[test]
    fn a_preference_returned_to_its_default_is_not_reapplied_on_the_next_launch() {
        let _legacy = LegacyFixture::new(&[
            (
                paths::LEGACY_TAVZIH,
                "preferences.json",
                r#"{"output_dir":"/eski","accepted_terms":1}"#,
            ),
            (
                paths::LEGACY_IKINCIGOZ,
                "settings.json",
                r#"{"theme":"dark","includeReview":false}"#,
            ),
        ]);
        let target = tmp("geri-alma-hedef");
        // İlk açılış: taşınır ve kaydedilir (migrate_legacy_settings gibi).
        let mut s = crate::settings::load_from(&target);
        assert!(migrate_into(&target, &mut s).changed);
        assert_eq!(s.theme, "dark");
        crate::settings::save_to(&target, &s).unwrap();

        // Kullanıcı tercihlerini varsayılana geri çekip kaydediyor.
        s.theme = "system".into();
        s.include_review = true;
        s.output_dir = None;
        crate::settings::save_to(&target, &s).unwrap();

        // Sonraki açılış.
        let mut next = crate::settings::load_from(&target);
        let report = migrate_into(&target, &mut next);
        assert_eq!(
            next.theme, "system",
            "kullanıcının seçtiği tema geri alındı"
        );
        assert!(
            next.include_review,
            "kullanıcının seçtiği inceleme geri alındı"
        );
        assert_eq!(
            next.output_dir, None,
            "kullanıcının seçtiği klasör geri alındı"
        );
        assert!(!report.changed);
        std::fs::remove_dir_all(&target).ok();
    }

    /// Birleşik uygulamada ayar dosyası zaten varken, varsayılana eşit değerler
    /// de kullanıcının seçimidir: bağımsız uygulamaların eski değerleri onları ezmez.
    #[test]
    fn existing_unified_settings_are_never_overwritten_by_standalone_apps() {
        let _legacy = LegacyFixture::new(&[
            (
                paths::LEGACY_TAVZIH,
                "preferences.json",
                r#"{"output_dir":"/eski","accepted_terms":1}"#,
            ),
            (
                paths::LEGACY_IKINCIGOZ,
                "settings.json",
                r#"{"theme":"dark","includeReview":false,"textScale":150}"#,
            ),
        ]);
        let target = tmp("mevcut-hedef");
        crate::settings::save_to(&target, &Settings::default()).unwrap();
        let before = std::fs::read(crate::settings::settings_path(&target)).unwrap();

        let mut s = crate::settings::load_from(&target);
        let report = migrate_into(&target, &mut s);
        assert_eq!(s, Settings::default(), "mevcut birleşik ayarlar ezildi");
        assert!(!report.changed);
        assert_eq!(
            std::fs::read(crate::settings::settings_path(&target)).unwrap(),
            before
        );
        std::fs::remove_dir_all(&target).ok();
    }

    /// Önceki birleşik addan devralınan ayarlar da birleşik uygulamanın verisidir;
    /// aynı açılışta bağımsız uygulamaların eski değerleri onları ezmez.
    #[test]
    fn inherited_unified_settings_are_not_overwritten_by_standalone_apps() {
        let previous = Settings {
            theme: "light".into(),
            ..Default::default()
        };
        let _legacy = LegacyFixture::new(&[
            (
                paths::LEGACY_BELGE,
                crate::settings::SETTINGS_FILE,
                &serde_json::to_string(&previous).unwrap(),
            ),
            (
                paths::LEGACY_IKINCIGOZ,
                "settings.json",
                r#"{"theme":"dark","includeReview":false}"#,
            ),
        ]);
        let target = tmp("devralma-ezme-hedef");
        let mut s = crate::settings::load_from(&target);
        migrate_into(&target, &mut s);
        assert_eq!(s.theme, "light");
        assert!(s.include_review, "devralınan inceleme tercihi ezildi");
        std::fs::remove_dir_all(&target).ok();
    }

    /// Kimlik değişiminde ayarlar devralınır; ikinci çalıştırma bir şey yapmaz
    /// ve eski dizin yerinde kalır.
    #[test]
    fn previous_unified_store_is_inherited_once_and_never_deleted() {
        let target = tmp("devralma-hedef");
        let old_dir = paths::legacy_config_dir(paths::LEGACY_BELGE);
        std::fs::create_dir_all(&old_dir).unwrap();
        let previous = Settings {
            theme: "dark".into(),
            text_scale: 150,
            ..Default::default()
        };
        crate::settings::save_to(&old_dir, &previous).unwrap();

        let mut s = Settings::default();
        let first = migrate_into(&target, &mut s);
        assert_eq!(s.theme, "dark", "önceki ayarlar devralınmalı");
        assert_eq!(s.text_scale, 150);
        assert!(first.changed);

        // Devralınan ayarlar kaydedilir; ikinci geçiş artık dokunmaz.
        crate::settings::save_to(&target, &s).unwrap();
        s.theme = "light".into();
        let second = migrate_into(&target, &mut s);
        assert_eq!(s.theme, "light", "kullanıcının yeni tercihi ezilmemeli");
        assert!(!second.changed);

        // Eski dizin ve dosyası yerinde.
        assert!(crate::settings::settings_path(&old_dir).is_file());
        std::fs::remove_dir_all(&target).ok();
        std::fs::remove_dir_all(&old_dir).ok();
    }
}
