//! Dönüştür — Tavzih'in DOCX ↔ UDF dönüştürme yüzeyi.
//!
//! Bu bir taşımadır, yeniden yazım değildir. Komut imzaları, hata kodları,
//! Türkçe metinler ve eşzamanlılık kuralı bağımsız uygulamadan **birebir**
//! alındı; motor `document-core`'un `convert` modülüdür ve hiç değiştirilmedi.
//!
//! Tek adaptasyon: Tavzih'in kendi `prefs.rs`'i yerine kabuğun ortak ayar
//! deposu kullanılıyor. Alanlar aynıdır (`output_dir`, `accepted_terms`), yalnız
//! artık dört modülün paylaştığı tek dosyada duruyorlar.

use crate::{paths, settings};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tavzih_core::convert::{self, BatchResult, ConversionResult, Direction};

/// Aynı anda tek dönüştürme. İkinci istek kuyruğa alınmaz, reddedilir:
/// kullanıcının bir penceresi ve bir dosyası var; kuyruk yalnız bir hatayı gizler.
pub struct Busy(pub Arc<AtomicBool>);

impl Default for Busy {
    fn default() -> Self {
        Busy(Arc::new(AtomicBool::new(false)))
    }
}

#[derive(Debug, Serialize)]
pub struct AppError {
    code: String,
    message: String,
    detail: String,
}

impl From<tavzih_core::ConvError> for AppError {
    fn from(e: tavzih_core::ConvError) -> Self {
        AppError {
            code: e.code.as_str().to_string(),
            message: e.code.message_tr().to_string(),
            detail: e.detail,
        }
    }
}

/// Arayüzün "seçildi" durumunu çizmek için gereken bilgi; tam parse yapılmadan.
#[derive(Debug, Serialize)]
pub struct FileInfo {
    path: String,
    name: String,
    size_bytes: u64,
    size_label: String,
    direction: Direction,
    source_format: String,
    target_format: String,
    /// Çıktının alacağı ad, mevcut dosyalar hesaba katılarak.
    planned_output_name: String,
    /// Aynı adlı bir dosya zaten varsa arayüz bunu önden söyleyebilsin.
    output_would_collide: bool,
}

#[derive(Debug, Serialize)]
pub struct InspectOutcome {
    path: String,
    info: Option<FileInfo>,
    error: Option<AppError>,
}

#[derive(Debug, Serialize)]
pub struct OutputFolder {
    path: String,
    default_path: String,
    is_default: bool,
}

fn human_size(n: u64) -> String {
    // Türkçe ondalık ayracı.
    let f = |v: f64, u: &str| format!("{:.1} {}", v, u).replace('.', ",");
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1_048_575 => f(n as f64 / 1024.0, "KB"),
        1_048_576..=1_073_741_823 => f(n as f64 / 1_048_576.0, "MB"),
        _ => f(n as f64 / 1_073_741_824.0, "GB"),
    }
}

/// Dönüştürmelerin yazılacağı klasör: kullanıcının seçimi varsa o, yoksa varsayılan.
fn effective_output_dir() -> PathBuf {
    match settings::load_from(&paths::app_config_dir()).output_dir {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => convert::output_root(),
    }
}

/// Bu klasöre gerçekten yazabiliyor muyuz?
///
/// İzin bitlerine bakılmaz; bir deneme dosyası yazılıp silinir. Bitler salt-okunur
/// birimleri, tam disk erişimi kısıtlarını ve aslında dosya olan bir yolu hesaba katmaz.
fn validate_dir(dir: &Path) -> Result<(), String> {
    if dir.as_os_str().is_empty() {
        return Err("Klasör yolu boş.".into());
    }
    if dir.exists() && !dir.is_dir() {
        return Err("Seçilen yol bir klasör değil.".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Klasör oluşturulamadı: {e}"))?;
    let probe = dir.join(".belge-yazma-denemesi");
    std::fs::write(&probe, b"").map_err(|e| format!("Bu klasöre yazılamıyor: {e}"))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Tek dönüştürme yuvasını al, ya da neden dolu olduğunu açıkla.
///
/// Tauri komutundan ayrı tutuldu ki eşzamanlılık kuralı bir uygulama örneği
/// ayağa kaldırmadan test edilebilsin — yanlış yapılması en muhtemel parça budur.
fn claim_slot(flag: &AtomicBool) -> Result<(), AppError> {
    if flag.swap(true, Ordering::SeqCst) {
        return Err(AppError {
            code: "BUSY".into(),
            message: "Halihazırda bir dönüştürme sürüyor.".into(),
            detail: "a conversion is already running".into(),
        });
    }
    Ok(())
}

fn engine_failure(detail: String) -> AppError {
    AppError {
        code: "ENGINE_FAILURE".into(),
        message: tavzih_core::ErrorCode::EngineFailure
            .message_tr()
            .to_string(),
        detail,
    }
}

/// DOCX ve UDF birer ZIP konteyneridir; ilk baytlar `PK` olmalıdır. Yalnız
/// başlığı okur, belgeyi açmaz (inceleme ucuz kalmalı).
fn has_zip_header(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2];
    match f.read_exact(&mut magic) {
        Ok(()) => &magic == b"PK",
        Err(_) => false,
    }
}

/// Bırakılan veya seçilen dosyayı tanımla. Ucuzdur: dosyayı stat'lar, uzantısını ve
/// konteyner başlığını okur; belgenin tamamını asla okumaz.
#[tauri::command]
pub fn tavzih_inspect_file(path: String) -> Result<FileInfo, AppError> {
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p).map_err(|e| {
        let code = if e.kind() == std::io::ErrorKind::NotFound {
            tavzih_core::ErrorCode::InputNotFound
        } else {
            tavzih_core::ErrorCode::InputUnreadable
        };
        AppError::from(tavzih_core::ConvError::new(code, e.to_string()))
    })?;
    if meta.is_dir() {
        return Err(AppError::from(tavzih_core::ConvError::new(
            tavzih_core::ErrorCode::InvalidExtension,
            "path is a directory",
        )));
    }
    let direction = convert::detect_direction(&p).map_err(AppError::from)?;
    // Konteyner başlığını gerçekten doğrula. `detect_direction` yalnız UZANTIYA
    // bakar; yanlış adlandırılmış ya da bozuk bir dosya (ör. .docx uzantılı düz
    // metin) arayüzde "Word (.docx) → UDF (.udf)" diye listelenip çıktı adı bile
    // vaat ediliyordu — dönüştürme ancak çok sonra düşüyordu. DOCX ve UDF'nin
    // ikisi de ZIP konteyneridir; dört baytlık bu denetim vaadi dürüst kılar
    // ve bu komutun sözleşmesi de zaten "konteyner başlığını okur" diyor.
    if !has_zip_header(&p) {
        return Err(AppError::from(tavzih_core::ConvError::new(
            tavzih_core::ErrorCode::InvalidExtension,
            "dosya bir DOCX/UDF konteyneri değil",
        )));
    }
    let planned =
        convert::unique_output_path(&p, direction.target_ext()).map_err(AppError::from)?;
    let default_name = format!(
        "{}.{}",
        p.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
        direction.target_ext()
    );
    let planned_name = planned
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&default_name)
        .to_string();

    Ok(FileInfo {
        path: p.display().to_string(),
        name: p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string(),
        size_bytes: meta.len(),
        size_label: human_size(meta.len()),
        direction,
        source_format: direction.source_label().to_string(),
        target_format: direction.target_label().to_string(),
        output_would_collide: planned_name != default_name,
        planned_output_name: planned_name,
    })
}

/// Birden çok dosyayı tanımla; tek dosyanın hatası kümeyi düşürmez.
#[tauri::command]
pub fn tavzih_inspect_files(paths: Vec<String>) -> Vec<InspectOutcome> {
    paths
        .into_iter()
        .map(|p| match tavzih_inspect_file(p.clone()) {
            Ok(info) => InspectOutcome {
                path: p,
                info: Some(info),
                error: None,
            },
            Err(e) => InspectOutcome {
                path: p,
                info: None,
                error: Some(e),
            },
        })
        .collect()
}

/// Tek dönüştürme. Bloklayan iş arayüz iş parçacığının dışında yapılır; nöbetçi
/// bayrak ikinci eşzamanlı isteği araya girmek yerine hızla başarısız kılar.
#[tauri::command]
pub async fn tavzih_convert_file(
    path: String,
    busy: tauri::State<'_, Busy>,
) -> Result<ConversionResult, AppError> {
    claim_slot(&busy.0)?;
    let flag = busy.0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        convert::convert_file_to(Path::new(&path), &effective_output_dir())
    })
    .await;
    // Her yolda serbest bırakılır — panikleyen bir join dâhil — ki tek bir bozuk
    // dosya uygulamayı kalıcı olarak meşgul durumda kilitleyemesin.
    flag.store(false, Ordering::SeqCst);
    result.map_err(|e| engine_failure(e.to_string()))
}

/// Bir dosya kümesini tek iş olarak dönüştür.
///
/// `stamp`, pencerenin ürettiği yerel `YYYY-MM-DD_HHMM` damgasıdır: motorda saat
/// dilimi veritabanı yoktur ve UTC bir dosya adı kullanıcıya yanlış saati gösterirdi.
#[tauri::command]
pub async fn tavzih_convert_batch(
    paths: Vec<String>,
    stamp: Option<String>,
    busy: tauri::State<'_, Busy>,
) -> Result<BatchResult, AppError> {
    claim_slot(&busy.0)?;
    let flag = busy.0.clone();
    let sources: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let out_dir = effective_output_dir();
    let result = tauri::async_runtime::spawn_blocking(move || {
        convert::convert_batch_to(&sources, &out_dir, stamp.as_deref())
    })
    .await;
    flag.store(false, Ordering::SeqCst);
    result.map_err(|e| engine_failure(e.to_string()))
}

/// Kullanım koşullarının kabul edilip edilmediği. Kabul yalnız yerelde saklanır;
/// hiçbir zaman yüklenmez, telemetri değildir.
#[tauri::command]
pub fn tavzih_terms_accepted() -> bool {
    settings::load_from(&paths::app_config_dir()).accepted_terms == Some(settings::TERMS_VERSION)
}

/// Kabulü kaydet. Yalnız açık "Kabul Ediyorum" eyleminden çağrılır.
#[tauri::command]
pub fn tavzih_accept_terms() -> Result<(), AppError> {
    let dir = paths::app_config_dir();
    let mut current = settings::load_from(&dir);
    current.accepted_terms = Some(settings::TERMS_VERSION);
    settings::save_to(&dir, &current).map_err(|e| AppError {
        code: "PREFS_WRITE_FAILED".into(),
        message: "Onayınız kaydedilemedi.".into(),
        detail: e.to_string(),
    })
}

/// Dönüştürülen belgelerin yazıldığı yer. Arayüzde gösterilir ki konum hiçbir zaman
/// bir muamma olmasın.
#[tauri::command]
pub fn tavzih_output_folder() -> OutputFolder {
    let effective = effective_output_dir();
    let default = convert::output_root();
    OutputFolder {
        is_default: effective == default,
        path: effective.display().to_string(),
        default_path: default.display().to_string(),
    }
}

/// Kullanıcının seçtiği çıktı klasörünü benimse — gerçekten yazılabilir olduğunu
/// doğruladıktan sonra.
#[tauri::command]
pub fn tavzih_set_output_folder(path: Option<String>) -> Result<OutputFolder, AppError> {
    let chosen = path.filter(|p| !p.trim().is_empty());
    if let Some(p) = &chosen {
        validate_dir(Path::new(p)).map_err(|detail| AppError {
            code: "OUTPUT_DIR_UNUSABLE".into(),
            message: detail.clone(),
            detail,
        })?;
    }
    let dir = paths::app_config_dir();
    let mut current = settings::load_from(&dir);
    current.output_dir = chosen;
    settings::save_to(&dir, &current).map_err(|e| AppError {
        code: "PREFS_WRITE_FAILED".into(),
        message: "Tercih kaydedilemedi.".into(),
        detail: e.to_string(),
    })?;
    Ok(tavzih_output_folder())
}

/// Çıktı klasörünü Finder'da göster. Klasör önce oluşturulur ki henüz
/// kullanılmamış bir dizinde gösterme işlemi başarısız olmasın.
#[tauri::command]
pub fn tavzih_reveal_output_folder(app: tauri::AppHandle) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let dir = effective_output_dir();
    std::fs::create_dir_all(&dir).map_err(|e| AppError {
        code: "OUTPUT_WRITE_FAILED".into(),
        message: "Klasör oluşturulamadı.".into(),
        detail: e.to_string(),
    })?;
    app.opener().reveal_item_in_dir(&dir).map_err(|e| AppError {
        code: "OPEN_FAILED".into(),
        message: "Klasör açılamadı.".into(),
        detail: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_single_conversion_slot_refuses_a_second_claim() {
        let flag = AtomicBool::new(false);
        assert!(claim_slot(&flag).is_ok());
        let second = claim_slot(&flag).unwrap_err();
        assert_eq!(second.code, "BUSY");
        assert_eq!(second.message, "Halihazırda bir dönüştürme sürüyor.");
        // Serbest bırakıldıktan sonra yuva yeniden alınabilmeli.
        flag.store(false, Ordering::SeqCst);
        assert!(claim_slot(&flag).is_ok());
    }

    #[test]
    fn sizes_are_formatted_with_the_turkish_decimal_separator() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1,5 KB");
        assert_eq!(human_size(1_572_864), "1,5 MB");
        assert!(!human_size(1536).contains('.'));
    }

    #[test]
    fn a_path_that_is_a_file_is_not_a_usable_output_directory() {
        let f = std::env::temp_dir().join(format!("belge-probe-{}", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        assert!(validate_dir(&f).is_err());
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn a_writable_directory_validates_and_leaves_no_probe_behind() {
        let d = std::env::temp_dir().join(format!("belge-outdir-{}", std::process::id()));
        assert!(validate_dir(&d).is_ok());
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_directory_is_not_an_inspectable_document() {
        let e = tavzih_inspect_file(std::env::temp_dir().display().to_string()).unwrap_err();
        assert_eq!(e.code, "INVALID_EXTENSION");
    }

    #[test]
    fn a_missing_file_is_reported_as_not_found_not_as_unreadable() {
        let e = tavzih_inspect_file("/olmayan/dosya.docx".into()).unwrap_err();
        assert_eq!(e.code, "INPUT_NOT_FOUND");
    }
}
