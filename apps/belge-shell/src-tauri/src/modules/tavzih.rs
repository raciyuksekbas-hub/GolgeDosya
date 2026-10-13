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
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tavzih_core::convert::{self, BatchResult, ConversionResult, Direction};

/// Bu oturumda dönüştürmenin ÜRETTİĞİ belgeler (gerçek yollarıyla).
///
/// "Belgeyi Aç" yalnız bunları açar. Eskiden ölçüt "çıktı klasöründe ve
/// oturum başladıktan sonra yazılmış" idi: kullanıcı sonuç ekranı açıkken
/// Ayarlar'dan çıktı klasörünü değiştirince az önceki belge "bulunamadı"
/// oluyordu; aynı ölçüt o klasöre başka yoldan yeni düşen bir dosyayı da
/// açtırırdı. Kayıt bellekte durur; uygulama kapanınca biter.
#[derive(Default)]
pub struct Produced(pub Mutex<HashSet<PathBuf>>);

impl Produced {
    /// Başarılı bir sonucun çıktısını kaydet. Toplu işin arşivinde tekil
    /// çıktılar silinmiştir; var olmayan yol kaydedilmez.
    fn record(&self, result: &ConversionResult) {
        let Some(output) = result.output.as_deref() else {
            return;
        };
        if let (Ok(real), Ok(mut set)) = (std::fs::canonicalize(output), self.0.lock()) {
            set.insert(real);
        }
    }
}

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

const PACKAGE_DIR: &str = "Seçilen klasör bir uygulama ya da paket; çıktı klasörü olamaz.";

/// macOS'un tek dosya gibi davrandığı ve "açılınca" bir program çalıştırabilen
/// paket uzantıları (uygulama, eklenti, kurulum paketi…).
const PACKAGE_EXTENSIONS: [&str; 16] = [
    "app",
    "appex",
    "bundle",
    "component",
    "framework",
    "kext",
    "mdimporter",
    "mpkg",
    "pkg",
    "plugin",
    "prefpane",
    "qlgenerator",
    "saver",
    "service",
    "workflow",
    "xpc",
];

/// Klasör bir uygulama/paket mi?
///
/// Saha denetimi (OR-11): "Klasörde Göster" belge bulunamayınca çıktı
/// klasörünü `open_path` ile açar; macOS'ta `X.app` "açılınca" uygulama
/// çalışır. Çıktı klasörü arayüzden ayarlanabildiği için böyle bir klasör hiç
/// kabul edilmemeli (`output_place.rs::reveal_target` aynı kuralı uygular).
/// Uzantısı olan her klasör reddedilmez: "Dava 2024.10" ya da "Müvekkil A.Ş."
/// gerçek çalışma klasörleridir. Paket uzantısı ya da paket yapısı
/// (`Contents/Info.plist`) taşıyan klasör reddedilir.
fn is_package_dir(dir: &Path) -> bool {
    let by_name = dir
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PACKAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    by_name || dir.join("Contents").join("Info.plist").exists()
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
    if is_package_dir(dir) {
        return Err(PACKAGE_DIR.into());
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
            message: "Hâlihazırda bir dönüştürme sürüyor.".into(),
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

/// Dosyanın ilk baytları (en çok `SNIFF_LEN`). Yalnız başlığı okur, belgeyi
/// açmaz (inceleme ucuz kalmalı). Açılamayan dosya `None`.
fn read_head(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut head = Vec::new();
    f.take(tavzih_core::security::SNIFF_LEN as u64)
        .read_to_end(&mut head)
        .ok()?;
    Some(head)
}

/// Kabın ilk baytlarına göre dürüst karar; motorun okuyucularıyla aynı ölçüt
/// (`tavzih_core::security::sniff_container`).
///
/// DOCX ve UDF birer ZIP kabıdır. Eskiden ZIP olmayan her şey "Bu dosya türü
/// desteklenmiyor. Yalnızca .docx ve .udf…" diye reddediliyordu (OR-07): adı
/// .docx olan parola korumalı bir Word belgesi için çelişkili ve yanlış yönlendiren
/// bir cümle. Boş dosya boş, parola korumalı/eski Word öyle denir; sıkıştırılmamış
/// düz bir content.xml de UDF'tir (gerçek derlemde bir belge, UF-13).
fn check_container(head: &[u8], direction: Direction) -> Result<(), AppError> {
    use tavzih_core::security::Container;
    use tavzih_core::ErrorCode;
    let refuse = |code: ErrorCode, detail: &str| {
        Err(AppError::from(tavzih_core::ConvError::new(code, detail)))
    };
    match (tavzih_core::security::sniff_container(head), direction) {
        (Container::Zip, _) | (Container::Xml, Direction::UdfToDocx) => Ok(()),
        // Şifreli bir UDF arşivini motor "geçerli bir UDF değil" diye bildirir.
        (Container::EncryptedZip, Direction::UdfToDocx) => Ok(()),
        (Container::Empty, _) => refuse(ErrorCode::InputEmpty, "file is empty (0 bytes)"),
        (Container::OleCompound | Container::EncryptedZip, Direction::DocxToUdf) => refuse(
            ErrorCode::ProtectedOrLegacyDocx,
            "password-protected Word document or legacy .doc",
        ),
        _ => refuse(
            ErrorCode::InvalidExtension,
            "dosya bir DOCX/UDF konteyneri değil",
        ),
    }
}

/// Bırakılan veya seçilen dosyayı tanımla. Ucuzdur: dosyayı stat'lar, uzantısını ve
/// konteyner başlığını okur; belgenin tamamını asla okumaz.
///
/// Ana iş parçacığında çalışmaz (bkz. [`tavzih_open_output`]): dosya bir ağ
/// paylaşımında olabilir.
#[tauri::command]
pub async fn tavzih_inspect_file(path: String) -> Result<FileInfo, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        inspect_file_in(Path::new(&path), &effective_output_dir())
    })
    .await
    .map_err(|e| engine_failure(e.to_string()))?
}

/// [`tavzih_inspect_file`]'ın işi. Planlanan çıktı adı ve çakışma, dönüştürmenin
/// GERÇEKTEN yazacağı klasöre (`out_dir`) göre hesaplanır. Eskiden varsayılan
/// klasöre bakılıyordu; kullanıcı başka bir çıktı klasörü seçtiyse ad ve
/// "çakışır" bilgisi yanlış klasörü anlatıyordu (OR-13).
///
/// `pub`: kabul testleri komutun işini kendi klasörleriyle çağırır; komut
/// kullanıcının ayar dosyasını okur, test paketi onu okumamalı (bkz. `paths.rs`).
pub fn inspect_file_in(p: &Path, out_dir: &Path) -> Result<FileInfo, AppError> {
    let meta = std::fs::metadata(p).map_err(|e| {
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
    let direction = convert::detect_direction(p).map_err(AppError::from)?;
    // Konteyner başlığını gerçekten doğrula. `detect_direction` yalnız UZANTIYA
    // bakar; yanlış adlandırılmış ya da bozuk bir dosya (ör. .docx uzantılı düz
    // metin) arayüzde "Word (.docx) → UDF (.udf)" diye listelenip çıktı adı bile
    // vaat ediliyordu — dönüştürme ancak çok sonra düşüyordu. DOCX ve UDF'nin
    // ikisi de ZIP konteyneridir; dört baytlık bu denetim vaadi dürüst kılar
    // ve bu komutun sözleşmesi de zaten "konteyner başlığını okur" diyor.
    let head = read_head(p).ok_or_else(|| {
        AppError::from(tavzih_core::ConvError::new(
            tavzih_core::ErrorCode::InputUnreadable,
            "dosya açılamadı",
        ))
    })?;
    check_container(&head, direction)?;
    let planned = convert::unique_output_path_in(out_dir, p, direction.target_ext())
        .map_err(AppError::from)?;
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
pub async fn tavzih_inspect_files(paths: Vec<String>) -> Vec<InspectOutcome> {
    let fallback = paths.clone();
    tauri::async_runtime::spawn_blocking(move || inspect_all(paths, &effective_output_dir()))
        .await
        .unwrap_or_else(|e| {
            let detail = e.to_string();
            fallback
                .into_iter()
                .map(|path| InspectOutcome {
                    path,
                    info: None,
                    error: Some(engine_failure(detail.clone())),
                })
                .collect()
        })
}

fn inspect_all(paths: Vec<String>, out_dir: &Path) -> Vec<InspectOutcome> {
    paths
        .into_iter()
        .map(|p| match inspect_file_in(Path::new(&p), out_dir) {
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

/// Ayarlardaki dönüşüm tercihleri. Okuma her dönüştürmede yapılır: Tercihler
/// penceresinde değiştirilen ayar bir sonraki dönüştürmede geçerlidir.
fn convert_options() -> convert::ConvertOptions {
    let current = settings::load_from(&paths::app_config_dir());
    convert::ConvertOptions {
        udf_spacing: if current.udf_petition_spacing {
            convert::UdfSpacing::Petition
        } else {
            convert::UdfSpacing::Source
        },
    }
}

/// Tek dönüştürme. Bloklayan iş arayüz iş parçacığının dışında yapılır; nöbetçi
/// bayrak ikinci eşzamanlı isteği araya girmek yerine hızla başarısız kılar.
#[tauri::command]
pub async fn tavzih_convert_file(
    path: String,
    busy: tauri::State<'_, Busy>,
    produced: tauri::State<'_, Produced>,
) -> Result<ConversionResult, AppError> {
    claim_slot(&busy.0)?;
    let flag = busy.0.clone();
    let options = convert_options();
    let result = tauri::async_runtime::spawn_blocking(move || {
        convert::convert_file_to_with(Path::new(&path), &effective_output_dir(), &options)
    })
    .await;
    // Her yolda serbest bırakılır — panikleyen bir join dâhil — ki tek bir bozuk
    // dosya uygulamayı kalıcı olarak meşgul durumda kilitleyemesin.
    flag.store(false, Ordering::SeqCst);
    let result = result.map_err(|e| engine_failure(e.to_string()))?;
    produced.record(&result);
    Ok(result)
}

/// Bir dosya kümesini tek iş olarak dönüştür.
///
/// `stamp`, pencerenin ürettiği yerel `YYYY-MM-DD_HHMM` damgasıdır: motorda saat
/// dilimi veritabanı yoktur ve UTC bir dosya adı kullanıcıya yanlış saati gösterirdi.
/// Damga arşivin adına girer; tam olarak bu biçimde değilse yok sayılır ve motor
/// UTC damgası kullanır (OR-08: "x/../../kacis" arşivi çıktı klasörünün dışına
/// yazdırıyordu).
#[tauri::command]
pub async fn tavzih_convert_batch(
    paths: Vec<String>,
    stamp: Option<String>,
    busy: tauri::State<'_, Busy>,
    produced: tauri::State<'_, Produced>,
) -> Result<BatchResult, AppError> {
    let stamp = stamp.filter(|s| convert::is_valid_stamp(s));
    claim_slot(&busy.0)?;
    let flag = busy.0.clone();
    let sources: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let out_dir = effective_output_dir();
    let options = convert_options();
    let result = tauri::async_runtime::spawn_blocking(move || {
        convert::convert_batch_to_with(&sources, &out_dir, stamp.as_deref(), &options)
    })
    .await;
    flag.store(false, Ordering::SeqCst);
    let result = result.map_err(|e| engine_failure(e.to_string()))?;
    for item in &result.items {
        produced.record(item);
    }
    Ok(result)
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
///
/// Doğrulama klasöre yazar; erişilemeyen bir ağ paylaşımında bu, işletim
/// sisteminin zaman aşımı kadar sürer. Ana iş parçacığında çalışmaz (bkz.
/// [`tavzih_open_output`]).
#[tauri::command]
pub async fn tavzih_set_output_folder(path: Option<String>) -> Result<OutputFolder, AppError> {
    tauri::async_runtime::spawn_blocking(move || set_output_folder(path))
        .await
        .map_err(|e| engine_failure(e.to_string()))?
}

fn set_output_folder(path: Option<String>) -> Result<OutputFolder, AppError> {
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

/// "Klasörde Göster" neyi açacak?
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RevealPlan {
    /// Klasörün KENDİSİNİ aç.
    OpenFolder(PathBuf),
    /// Dosyaların bulunduğu klasörü aç ve onları seç.
    SelectItems(Vec<PathBuf>),
}

/// Dönüştürmenin ürettiği dosya türleri; başka bir yol seçtirilmez.
const OUTPUT_EXTENSIONS: [&str; 3] = ["udf", "docx", "zip"];

/// Karar: dönüştürmenin ürettiği dosyalar hâlâ yerindeyse ve hepsi aynı
/// klasördeyse o klasör açılır ve dosyalar seçilir. Aksi hâlde çıktı
/// klasörünün KENDİSİ açılır.
///
/// Saha hatası (Windows, madde 13): klasör, "öğeyi üst klasöründe göster"
/// API'sine veriliyordu. Bu API — Windows'ta SHOpenFolderAndSelectItems,
/// macOS'ta activateFileViewerSelectingURLs — verilen öğenin ÜST klasörünü
/// açar. Kullanıcı `Belgeler\GölgeDosya\Dönüştürülen Belgeler` yerine
/// `Belgeler\GölgeDosya`yı görüyordu. Bir klasör bu yüzden hiçbir zaman o
/// API'ye gitmez.
///
/// Ayrıca gösterilen yer, dönüştürmenin GERÇEKTEN yazdığı yerdir: tıklama
/// anındaki ayar değil. Kullanıcı arada çıktı klasörünü değiştirmişse eski
/// davranış yanlış klasörü açıyordu.
pub(crate) fn reveal_plan(dir: &Path, outputs: &[String]) -> RevealPlan {
    let files: Vec<PathBuf> = outputs.iter().map(PathBuf::from).collect();
    let is_output = |f: &PathBuf| {
        f.is_file()
            && f.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| OUTPUT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
    };
    let together = files.windows(2).all(|w| w[0].parent() == w[1].parent());
    if !files.is_empty() && together && files.iter().all(is_output) {
        RevealPlan::SelectItems(files)
    } else {
        RevealPlan::OpenFolder(dir.to_path_buf())
    }
}

/// Çıktıyı klasöründe göster.
///
/// `outputs`: az önce üretilen dosyalar (tek belge, toplu işin arşivi ya da
/// başarılı öğeler). Boşsa ya da dosyalar artık yoksa çıktı klasörü açılır;
/// klasör önce oluşturulur ki henüz kullanılmamış bir dizinde gösterme işlemi
/// başarısız olmasın.
///
/// Ana iş parçacığında çalışmaz (bkz. [`tavzih_open_output`]).
#[tauri::command]
pub async fn tavzih_reveal_output_folder(
    app: tauri::AppHandle,
    outputs: Option<Vec<String>>,
) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || reveal_output(&app, outputs))
        .await
        .map_err(|e| engine_failure(e.to_string()))?
}

fn reveal_output(app: &tauri::AppHandle, outputs: Option<Vec<String>>) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let dir = effective_output_dir();
    std::fs::create_dir_all(&dir).map_err(|e| AppError {
        code: "OUTPUT_WRITE_FAILED".into(),
        message: "Klasör oluşturulamadı.".into(),
        detail: e.to_string(),
    })?;
    let opened = match reveal_plan(&dir, &outputs.unwrap_or_default()) {
        RevealPlan::SelectItems(files) => app.opener().reveal_items_in_dir(files),
        // Yalnız arka ucun hesapladığı klasör açılır; arayüzden gelen bir yol
        // hiçbir zaman "aç"a verilmez (dosya olsaydı uygulamasıyla açılırdı).
        // Paket klasörü (macOS'ta `X.app`) "açılınca" program çalıştırır;
        // ayar elle değiştirilmiş olsa bile açılmaz (OR-11).
        RevealPlan::OpenFolder(dir) if is_package_dir(&dir) => {
            return Err(AppError {
                code: "OPEN_FAILED".into(),
                message: "Klasör açılamadı.".into(),
                detail: PACKAGE_DIR.into(),
            })
        }
        RevealPlan::OpenFolder(dir) => app.opener().open_path(dir.to_string_lossy(), None::<&str>),
    };
    opened.map_err(|e| AppError {
        code: "OPEN_FAILED".into(),
        message: "Klasör açılamadı.".into(),
        detail: e.to_string(),
    })
}

/// "Belgeyi Aç"ın açabileceği tek şey: Dönüştür'ün BU oturumda ürettiği bir
/// UDF ya da Word belgesi ([`Produced`]).
///
/// "Klasörde Göster"den farklı olarak bu, belgeyi uygulamasıyla AÇAR; arayüzden
/// gelen bir yol bu yüzden sıkı denetlenir: mutlak, bağlantı (symlink) değil,
/// gerçek bir dosya, uzantısı `.udf`/`.docx` ve gerçek yolu bu oturumun
/// dönüştürme kaydında. Arşiv (`.zip`) açılmaz; toplu işte "Klasörde Göster"
/// kullanılır.
pub(crate) fn open_target(path: &Path, produced: &HashSet<PathBuf>) -> Result<PathBuf, String> {
    const NOT_FOUND: &str = "Belge bulunamadı. Taşınmış veya silinmiş olabilir.";
    if !path.is_absolute() {
        return Err(NOT_FOUND.into());
    }
    let meta = std::fs::symlink_metadata(path).map_err(|_| NOT_FOUND.to_string())?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(NOT_FOUND.into());
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(ext.as_str(), "udf" | "docx") {
        return Err(NOT_FOUND.into());
    }
    let real = std::fs::canonicalize(path).map_err(|_| NOT_FOUND.to_string())?;
    if !produced.contains(&real) {
        return Err(NOT_FOUND.into());
    }
    Ok(real)
}

/// Az önce dönüştürülen belgeyi varsayılan uygulamasıyla aç (UDF için UYAP
/// Doküman Editörü, Word belgesi için Word…).
///
/// Ana iş parçacığında çalışmaz. Tauri eşzamanlı komutları orada çalıştırır
/// (`output_place.rs::output_start_folder` aynı sebeple async'tir): `open_target`
/// çıktı klasörünü yoklar (ağ paylaşımı olabilir), macOS'ta açma işlemi
/// `/usr/bin/open` bitene kadar bekler, Windows'ta ilişkilendirilmemiş tür
/// "Birlikte aç" penceresi açar. Eskiden her tıklama pencereyi bu süre boyunca
/// donduruyordu (OR-09). Oturum kaydı ([`Produced`]) iş başlamadan okunur.
#[tauri::command]
pub async fn tavzih_open_output(
    app: tauri::AppHandle,
    path: String,
    produced: tauri::State<'_, Produced>,
) -> Result<(), AppError> {
    let known = produced.0.lock().map(|set| set.clone()).unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || open_output(&app, &path, &known))
        .await
        .map_err(|e| engine_failure(e.to_string()))?
}

fn open_output(
    app: &tauri::AppHandle,
    path: &str,
    known: &HashSet<PathBuf>,
) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let file = open_target(Path::new(path), known).map_err(|message| AppError {
        code: "NOT_FOUND".into(),
        message,
        detail: String::new(),
    })?;
    app.opener()
        .open_path(file.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError {
            code: "OPEN_FAILED".into(),
            message: "Belge açılamadı. Bu türdeki belgeyi açacak bir uygulama yüklü olmayabilir."
                .into(),
            detail: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unicode profil, boşluk, kesme işareti: sahadaki yolun biçimi.
    fn output_tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let dir = root
            .path()
            .join("Çağrı Şahin")
            .join("Documents")
            .join("GölgeDosya")
            .join("Dönüştürülen Belgeler");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Müvekkil'in dilekçesi.udf");
        std::fs::write(&file, b"PK").unwrap();
        (root, dir, file)
    }

    fn test_result() -> ConversionResult {
        ConversionResult {
            contract_version: 1,
            status: convert::Status::Success,
            source: String::new(),
            source_name: String::new(),
            output: None,
            output_name: None,
            direction: Direction::DocxToUdf,
            source_format: String::new(),
            target_format: String::new(),
            warnings: Vec::new(),
            error: None,
            source_sha256_before: String::new(),
            source_sha256_after: String::new(),
            source_unchanged: true,
            elapsed_ms: 0,
        }
    }

    #[test]
    fn open_accepts_only_a_document_this_session_produced() {
        let (root, dir, udf) = output_tree();
        let produced = Produced::default();
        let record = |path: &Path| {
            produced.record(&ConversionResult {
                output: Some(path.to_string_lossy().into_owned()),
                ..test_result()
            })
        };
        record(&udf);
        let known = || produced.0.lock().unwrap().clone();
        assert_eq!(
            open_target(&udf, &known()),
            Ok(std::fs::canonicalize(&udf).unwrap())
        );

        // Aynı klasöre başka yoldan düşen belge açılmaz; kayıtlı olan açılır.
        let docx = dir.join("Dilekçe.DOCX");
        std::fs::write(&docx, b"PK").unwrap();
        assert!(open_target(&docx, &known()).is_err(), "dönüştürülmedi");
        record(&docx);
        assert!(
            open_target(&docx, &known()).is_ok(),
            "büyük harfli uzantı da belge"
        );

        // Çıktı klasörü sonradan değişse de az önceki belge açılır: ölçüt
        // klasör değil, bu oturumun ürettiği dosya.
        let elsewhere = root.path().join("Masaüstü");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let moved_out = elsewhere.join("Yeni klasörde.udf");
        std::fs::write(&moved_out, b"PK").unwrap();
        record(&moved_out);
        assert!(open_target(&moved_out, &known()).is_ok());

        // Klasör, arşiv, başka tür: kayıtlı olsalar bile açılmaz.
        record(&dir);
        assert!(open_target(&dir, &known()).is_err());
        for name in [
            "arsiv.zip",
            "betik.command",
            "uygulama.app",
            "kisayol.lnk",
            "belge.pdf",
        ] {
            let other = dir.join(name);
            std::fs::write(&other, b"x").unwrap();
            record(&other);
            assert!(open_target(&other, &known()).is_err(), "{name} açılmamalı");
        }

        // Silinmiş ya da taşınmış belge: "bulunamadı".
        std::fs::remove_file(&moved_out).unwrap();
        assert_eq!(
            open_target(&moved_out, &known()),
            Err("Belge bulunamadı. Taşınmış veya silinmiş olabilir.".to_string())
        );

        // Göreli yol ve bağlantı (symlink) açılmaz — bağlantının hedefi
        // kayıtlı bir belge olsa bile.
        assert!(open_target(Path::new("Müvekkil'in dilekçesi.udf"), &known()).is_err());
        #[cfg(unix)]
        {
            let link = dir.join("bag.udf");
            std::os::unix::fs::symlink(&udf, &link).unwrap();
            assert!(open_target(&link, &known()).is_err());
        }

        // `..` içeren yol gerçek yoluna çözülür; kayıtlı belgeye gidiyorsa açılır,
        // kayıt dışına çıkıyorsa açılmaz.
        let dotted = dir
            .join("..")
            .join("Dönüştürülen Belgeler")
            .join("Müvekkil'in dilekçesi.udf");
        assert!(open_target(&dotted, &known()).is_ok());
        let outside = root.path().join("baska.udf");
        std::fs::write(&outside, b"PK").unwrap();
        assert!(open_target(
            &dir.join("..")
                .join("..")
                .join("..")
                .join("..")
                .join("baska.udf"),
            &known()
        )
        .is_err());
        let _ = outside;
    }

    #[test]
    fn a_failed_or_zipped_result_registers_nothing() {
        let (_root, dir, _udf) = output_tree();
        let produced = Produced::default();
        produced.record(&ConversionResult {
            output: None,
            ..test_result()
        });
        // Toplu işin arşivinde tekil çıktı silinmiştir.
        produced.record(&ConversionResult {
            output: Some(dir.join("silindi.udf").to_string_lossy().into_owned()),
            ..test_result()
        });
        assert!(produced.0.lock().unwrap().is_empty());
    }

    #[test]
    fn a_folder_is_opened_itself_never_handed_to_select_in_parent() {
        // Madde 13: eski kod klasörü "üst klasörde seç" API'sine veriyordu.
        let (_root, dir, _file) = output_tree();
        assert_eq!(reveal_plan(&dir, &[]), RevealPlan::OpenFolder(dir.clone()));
        // Klasör yolu "çıktı" diye gelse bile seçtirilmez; klasör açılır.
        let as_output = [dir.display().to_string()];
        assert_eq!(
            reveal_plan(&dir, &as_output),
            RevealPlan::OpenFolder(dir.clone())
        );
    }

    #[test]
    fn a_produced_file_opens_its_own_folder_with_the_file_selected() {
        let (_root, dir, file) = output_tree();
        let plan = reveal_plan(&dir, &[file.display().to_string()]);
        assert_eq!(plan, RevealPlan::SelectItems(vec![file.clone()]));
        // Açılacak klasör, dosyanın klasörü = çıktı klasörü; üstü değil.
        assert_eq!(file.parent(), Some(dir.as_path()));
    }

    #[test]
    fn missing_foreign_or_scattered_outputs_fall_back_to_the_output_folder() {
        let (root, dir, file) = output_tree();
        let gone = dir.join("silinmiş.udf");
        assert_eq!(
            reveal_plan(&dir, &[gone.display().to_string()]),
            RevealPlan::OpenFolder(dir.clone())
        );
        // Dönüştürmenin üretmeyeceği bir tür seçtirilmez.
        let other = dir.join("not.txt");
        std::fs::write(&other, b"x").unwrap();
        assert_eq!(
            reveal_plan(&dir, &[other.display().to_string()]),
            RevealPlan::OpenFolder(dir.clone())
        );
        // Farklı klasörlerdeki dosyalar tek pencerede seçilemez.
        let elsewhere = root.path().join("başka.udf");
        std::fs::write(&elsewhere, b"PK").unwrap();
        assert_eq!(
            reveal_plan(
                &dir,
                &[file.display().to_string(), elsewhere.display().to_string()]
            ),
            RevealPlan::OpenFolder(dir.clone())
        );
    }

    #[test]
    fn the_single_conversion_slot_refuses_a_second_claim() {
        let flag = AtomicBool::new(false);
        assert!(claim_slot(&flag).is_ok());
        let second = claim_slot(&flag).unwrap_err();
        assert_eq!(second.code, "BUSY");
        assert_eq!(second.message, "Hâlihazırda bir dönüştürme sürüyor.");
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

    /// İncelemenin işi (`inspect`); komut artık async olduğu için testler işi
    /// doğrudan, çıktı klasörünü kendileri vererek çağırır.
    fn inspect_here(path: &Path) -> Result<FileInfo, AppError> {
        inspect_file_in(path, &std::env::temp_dir().join("belge-inspect-cikti-yok"))
    }

    #[test]
    fn a_directory_is_not_an_inspectable_document() {
        let e = inspect_here(&std::env::temp_dir()).unwrap_err();
        assert_eq!(e.code, "INVALID_EXTENSION");
    }

    #[test]
    fn the_planned_name_describes_the_folder_conversion_will_write_to() {
        // OR-13: planlanan ad ve çakışma varsayılan klasöre göre hesaplanıyordu;
        // dönüştürme ise kullanıcının seçtiği klasöre yazar.
        let src_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("Dilekçe.docx");
        std::fs::write(&src, b"PK\x03\x04\x14\x00\x00\x00").unwrap();
        let chosen = tempfile::tempdir().unwrap();
        let info = inspect_file_in(&src, chosen.path()).unwrap();
        assert_eq!(info.planned_output_name, "Dilekçe.udf");
        assert!(!info.output_would_collide);
        std::fs::write(chosen.path().join("Dilekçe.udf"), b"PK").unwrap();
        let info = inspect_file_in(&src, chosen.path()).unwrap();
        assert_eq!(info.planned_output_name, "Dilekçe (2).udf");
        assert!(info.output_would_collide);
    }

    #[test]
    fn a_package_is_never_an_output_folder() {
        // OR-11: macOS'ta `X.app` "açılınca" uygulama çalışır; çıktı klasörü
        // arayüzden ayarlanabilir ve "Klasörde Göster" onu açar.
        let root = tempfile::tempdir().unwrap();
        for name in ["Kur.app", "Eklenti.PLUGIN", "Paket.pkg"] {
            let dir = root.path().join(name);
            assert_eq!(validate_dir(&dir), Err(PACKAGE_DIR.to_string()), "{name}");
            assert!(!dir.exists(), "{name} oluşturulmamalı");
        }
        // Uzantısız ama paket yapısında (Contents/Info.plist) bir klasör de.
        let hidden = root.path().join("Sıradan görünen");
        std::fs::create_dir_all(hidden.join("Contents")).unwrap();
        std::fs::write(hidden.join("Contents").join("Info.plist"), b"<plist/>").unwrap();
        assert!(validate_dir(&hidden).is_err());
        assert!(is_package_dir(&hidden));
        // Noktalı gerçek çalışma klasörleri kabul edilir.
        for name in ["Dava 2024.10", "Müvekkil A.Ş.", "Av. Çağrı"] {
            let dir = root.path().join(name);
            assert!(validate_dir(&dir).is_ok(), "{name}");
            assert!(!is_package_dir(&dir), "{name}");
        }
    }

    #[test]
    fn commands_that_touch_the_disk_or_launch_apps_do_not_block_the_window() {
        // OR-09: eşzamanlı Tauri komutları ana iş parçacığında çalışır; ağ
        // paylaşımını yoklamak ya da `/usr/bin/open`'ı beklemek pencereyi
        // donduruyordu. Bu komutlar async olmalı ve işi spawn_blocking'e vermeli.
        let source = include_str!("tavzih.rs");
        for name in [
            "open_output",
            "reveal_output_folder",
            "set_output_folder",
            "inspect_file",
            "inspect_files",
        ] {
            // İğne çalışma anında kurulur ki bu testin kendi metnine takılmasın.
            let command = format!("pub async fn tavzih_{name}(");
            let at = source
                .find(&command)
                .unwrap_or_else(|| panic!("tavzih_{name} async değil"));
            let body: String = source[at..].chars().take(900).collect();
            assert!(body.contains("spawn_blocking"), "tavzih_{name}");
        }
    }

    /// Parola korumalı/eski Word kodunun adı. Enum'dan okunur: kodun metni eski
    /// uygulama sabitlerinin önekini içerir ve mimari kapısı (5d) kabukta bu
    /// metni eski veri yolu sanar.
    fn protected() -> &'static str {
        tavzih_core::ErrorCode::ProtectedOrLegacyDocx.as_str()
    }

    #[test]
    fn an_empty_or_password_protected_docx_says_why_it_cannot_be_converted() {
        // OR-07: adı .docx olan parola korumalı belgeye "Bu dosya türü
        // desteklenmiyor. Yalnızca .docx ve .udf…" deniyordu.
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("Boş dilekçe.docx");
        std::fs::write(&empty, b"").unwrap();
        let e = inspect_here(&empty).unwrap_err();
        assert_eq!(e.code, "INPUT_EMPTY");
        assert!(e.message.contains("boş"), "{}", e.message);

        let ole = dir.path().join("Şifreli dilekçe.docx");
        let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        bytes.resize(1024, 0);
        std::fs::write(&ole, &bytes).unwrap();
        let e = inspect_here(&ole).unwrap_err();
        assert_eq!(e.code, protected());
        assert!(e.message.contains("parola"), "{}", e.message);

        // Düz metin hâlâ "desteklenmeyen tür"dür.
        let text = dir.path().join("not.docx");
        std::fs::write(&text, b"merhaba").unwrap();
        let e = inspect_here(&text).unwrap_err();
        assert_eq!(e.code, "INVALID_EXTENSION");
    }

    #[test]
    fn the_container_check_matches_the_engine() {
        let zip = b"PK\x03\x04\x14\x00\x00\x00";
        let zip_crypto = b"PK\x03\x04\x14\x00\x01\x00";
        let bare_udf =
            b"<?xml version=\"1.0\" encoding=\"UTF-8\" ?> \n\n<template format_id=\"1.8\">";
        assert!(check_container(zip, Direction::DocxToUdf).is_ok());
        assert!(check_container(zip, Direction::UdfToDocx).is_ok());
        // Sıkıştırılmamış content.xml bir UDF'tir (UF-13), Word belgesi değildir.
        assert!(check_container(bare_udf, Direction::UdfToDocx).is_ok());
        assert_eq!(
            check_container(bare_udf, Direction::DocxToUdf)
                .unwrap_err()
                .code,
            "INVALID_EXTENSION"
        );
        assert_eq!(
            check_container(zip_crypto, Direction::DocxToUdf)
                .unwrap_err()
                .code,
            protected()
        );
    }

    #[test]
    fn a_missing_file_is_reported_as_not_found_not_as_unreadable() {
        let e = inspect_here(Path::new("/olmayan/dosya.docx")).unwrap_err();
        assert_eq!(e.code, "INPUT_NOT_FOUND");
    }
}
