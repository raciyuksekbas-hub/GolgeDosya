//! Karşılaştır — Değişikİş'in belge karşılaştırma yüzeyi.
//!
//! Bu bir taşımadır, yeniden yazım değildir. **Karşılaştırma motoru
//! TypeScript'te kalır**: `compare.ts` ve pdfjs tabanlı çıkarma yolu webview
//! içinde çalışmaya devam eder. Rust tarafı yalnız üç şey yapar ve üçü de
//! bağımsız uygulamadan birebir alındı:
//!
//! * eski ikili `.doc` dosyasını platformun kendi aracıyla `.docx`'e çevirmek
//!   (macOS `textutil`, Windows Word COM) — saf TypeScript'te yapılamayan tek iş
//! * üretilen raporu indirilenler klasörüne yazmak
//! * yazılan raporu işletim sisteminin varsayılan uygulamasıyla açmak
//!
//! Rapor yolu iki kez doğrulanır: yazarken ad süzgecinden geçer, açarken
//! kanonikleştirilip rapor dizininin doğrudan çocuğu olduğu yeniden sınanır.
//! Böylece arayüzden gelen bir yol dizin dışına çıkamaz.
//!
//! Belge baytları IPC'yi HAM geçer. Eskiden `Vec<u8>` JSON sayı dizisi olarak
//! gidip geliyordu: 20 MiB'lık bir belge 71 MiB metne dönüşüyor, ayrıştırması
//! ~1,1 sn ve 425 MiB bellek tutuyordu. Yanıtlar `tauri::ipc::Response`
//! (arayüzde `ArrayBuffer`), istek gövdeleri `tauri::ipc::Request`'in ham
//! gövdesidir; küçük üst veri (rapor adı) yüzde-kodlu bir başlıkta gelir.

pub mod legacy_doc;

use serde::Deserialize;
use std::path::{Path, PathBuf};
use tauri::http::HeaderMap;
use tauri::ipc::{InvokeBody, InvokeResponseBody, IpcResponse, Request, Response};
use tauri::Manager;

/// Rapor dosyalarının kabul edilen ad önekleri.
///
/// İlk değer canonical GölgeDosya önekidir ve yeni raporlar onunla üretilir.
/// Eski Değişikİş öneki KABUL EDİLMEYE DEVAM EDER: kullanıcının daha önce
/// ürettiği raporlar hâlâ açılabilmeli. (Önek denetimi güvenlik içindir —
/// arayüzden gelen ad dizin dışına çıkmasın; markayı değiştirmek o denetimi
/// gevşetmemeli.)
const REPORT_FILE_PREFIXES: [&str; 2] = ["GolgeDosya-Karsilastirma-", "DegisikIs-Rapor-"];

/// Karşılaştırılacak en büyük dosya. Bağımsız uygulama belgeyi webview'e
/// olduğu gibi aldığı için örtük bir sınırı yoktu; burada açık bir sınır var.
const MAX_DOCUMENT_BYTES: u64 = 128 * 1024 * 1024;

/// Rapor adını taşıyan istek başlığı. Değer `encodeURIComponent` ile
/// yüzde-kodlanır: başlıklar yalnız ASCII taşır.
const REPORT_NAME_HEADER: &str = "x-degisikis-report-name";

/// İstek gövdesindeki baytlar.
///
/// Arayüz baytları `Uint8Array` olarak yollar; gövde `InvokeBody::Raw` gelir.
/// Webview özel IPC protokolünü engellerse Tauri postMessage yoluna düşer ve
/// aynı baytları JSON sayı dizisi olarak taşır; o yol da kabul edilir (yavaş
/// ama doğru). `limit`'ten büyük gövde reddedilir.
fn body_bytes(body: &InvokeBody, limit: u64) -> Option<Vec<u8>> {
    let within = |length: usize| length as u64 <= limit;
    match body {
        // Sınır kopyalamadan ÖNCE denetlenir.
        InvokeBody::Raw(bytes) => within(bytes.len()).then(|| bytes.clone()),
        InvokeBody::Json(value) => Vec::<u8>::deserialize(value)
            .ok()
            .filter(|bytes| within(bytes.len())),
    }
}

/// `encodeURIComponent` çıktısını çözer; bozuk kaçış ya da geçersiz UTF-8
/// `None` döner.
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            decoded.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// İstekteki rapor adı (henüz süzülmemiş).
fn report_name_from(headers: &HeaderMap) -> Option<String> {
    let header = headers.get(REPORT_NAME_HEADER)?.to_str().ok()?;
    percent_decode(header)
}

/// Eski ikili `.doc` dosyasını `.docx`'e çevir.
///
/// Baytlar arayüzden gelir ve arayüze döner; dosya sistemine kalıcı bir şey
/// yazılmaz. Dönüşüm geçici bir dizinde yapılır ve dizin her koşulda silinir.
/// Girdi istek gövdesinin ham baytlarıdır (`invoke(cmd, Uint8Array)`), çıktı
/// ham yanıttır (arayüzde `ArrayBuffer`). Girdi okunabilir belge sınırını
/// (`MAX_DOCUMENT_BYTES`) aşamaz: baytlar zaten o sınırla okunmuş belgeden gelir.
#[tauri::command]
pub async fn degisikis_convert_legacy_doc(request: Request<'_>) -> Result<Response, String> {
    let contents = body_bytes(request.body(), MAX_DOCUMENT_BYTES)
        .ok_or_else(|| "DOC_CONVERSION".to_string())?;
    let converted = tauri::async_runtime::spawn_blocking(move || legacy_doc::convert(&contents))
        .await
        .map_err(|_| "DOC_CONVERSION".to_string())?
        .map_err(str::to_string)?;
    Ok(Response::new(converted))
}

/// Kullanıcının seçtiği belgeyi oku ve baytları arayüze ver.
///
/// Bağımsız Değişikİş belgeyi webview'in kendi dosya girdisinden `File` olarak
/// alıyordu; birleşik kabuk native seçiciyi kullandığı için elde yol var.
/// Motoru değiştirmemek adına baytlar burada okunup arayüzde yeniden `File`
/// nesnesine sarılıyor: `extractDocument` imzası ve davranışı aynı kalıyor.
///
/// Bu komut Tauri'nin filesystem eklentisini AÇMAZ. Yol yalnız native seçiciden
/// veya sürükle-bırak olayından gelir, uzantı süzgecinden geçer ve boyut
/// sınırlanır; dizin veya desteklenmeyen tür okunmaz.
///
/// Baytlar ham yanıt olarak döner (arayüzde `ArrayBuffer`), JSON sayı dizisi
/// olarak değil.
#[tauri::command]
pub async fn degisikis_read_document(path: String) -> Result<DocumentBytes, String> {
    read_document_bytes(path).await.map(DocumentBytes)
}

/// Okunan belgenin baytları: IPC'de ham yanıt (`tauri::ipc::Response` gibi),
/// Rust'ta bayt dilimi. Kabul testleri komutu doğrudan çağırıp içeriği
/// okuyabilsin diye `Response` yerine bu tür döner.
#[derive(Debug)]
pub struct DocumentBytes(Vec<u8>);

impl std::ops::Deref for DocumentBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl IpcResponse for DocumentBytes {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

async fn read_document_bytes(path: String) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let meta = std::fs::metadata(&p).map_err(|_| "DOCUMENT_UNREADABLE".to_string())?;
        if !meta.is_file() {
            return Err("DOCUMENT_NOT_A_FILE".to_string());
        }
        if meta.len() > MAX_DOCUMENT_BYTES {
            return Err("DOCUMENT_TOO_LARGE".to_string());
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "pdf" | "doc" | "docx" | "udf") {
            return Err("DOCUMENT_UNSUPPORTED".to_string());
        }
        std::fs::read(&p).map_err(|_| "DOCUMENT_UNREADABLE".to_string())
    })
    .await
    .map_err(|_| "DOCUMENT_UNREADABLE".to_string())?
}

fn report_directory(app: &tauri::AppHandle) -> PathBuf {
    let resolver = app.path();
    resolver
        .download_dir()
        .or_else(|_| resolver.document_dir())
        .or_else(|_| resolver.desktop_dir())
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// Rapor adını süz.
///
/// Arayüzden gelen bir ad dizin dışına çıkamamalı ve beklenmedik bir uzantı
/// taşımamalı. Değişikİş'teki kurallarla birebir aynıdır.
fn sanitize_report_file_name(raw: &str) -> Result<&str, &'static str> {
    let name = raw.trim();
    if name.is_empty() || name.len() > 128 {
        return Err("REPORT_NAME");
    }
    if !REPORT_FILE_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return Err("REPORT_NAME");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("REPORT_NAME");
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err("REPORT_NAME");
    }
    if !(name.ends_with(".html")
        || name.ends_with(".docx")
        || name.ends_with(".md")
        || name.ends_with(".json"))
    {
        return Err("REPORT_NAME");
    }
    Ok(name)
}

/// En büyük rapor. Rapor karşılaştırmanın metin özetidir; belgenin kendisinden
/// büyük olması için bir neden yok.
const MAX_REPORT_BYTES: u64 = MAX_DOCUMENT_BYTES;

/// Raporu rapor dizinine yaz.
///
/// Ad `x-degisikis-report-name` başlığında yüzde-kodlu, içerik istek
/// gövdesinin ham baytlarıdır. Komut eşzamansızdır ve yazma
/// `spawn_blocking`'te yapılır: eskiden eşzamanlı komuttu, dosya yazımı ana
/// iş parçacığını tutuyordu.
#[tauri::command]
pub async fn degisikis_save_report(
    app: tauri::AppHandle,
    request: Request<'_>,
) -> Result<String, String> {
    let file_name = report_name_from(request.headers()).ok_or_else(|| "REPORT_NAME".to_string())?;
    let name = sanitize_report_file_name(&file_name)
        .map_err(str::to_string)?
        .to_string();
    let contents =
        body_bytes(request.body(), MAX_REPORT_BYTES).ok_or_else(|| "REPORT_WRITE".to_string())?;
    let directory = report_directory(&app);
    tauri::async_runtime::spawn_blocking(move || write_report(&directory, &name, &contents))
        .await
        .map_err(|_| "REPORT_WRITE".to_string())?
        .map_err(str::to_string)
}

/// Süzülmüş adı rapor dizinine yazar ve tam yolu döndürür.
fn write_report(directory: &Path, name: &str, contents: &[u8]) -> Result<String, &'static str> {
    std::fs::create_dir_all(directory).map_err(|_| "REPORT_DIR")?;
    let path = directory.join(name);
    std::fs::write(&path, contents).map_err(|_| "REPORT_WRITE")?;
    Ok(path.to_string_lossy().into_owned())
}

/// Açılacak yolun gerçekten rapor dizininin doğrudan çocuğu olduğunu doğrula.
///
/// Yol kanonikleştirilir, dolayısıyla `..` ve symlink ile dizin dışına çıkma
/// denemeleri burada durur.
fn ensure_report_path(directory: &Path, requested: &Path) -> Result<PathBuf, &'static str> {
    let canonical = requested.canonicalize().map_err(|_| "REPORT_MISSING")?;
    if !canonical.is_file() {
        return Err("REPORT_MISSING");
    }
    let root = directory.canonicalize().map_err(|_| "REPORT_DIR")?;
    if canonical.parent() != Some(root.as_path()) {
        return Err("REPORT_SCOPE");
    }
    let name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("REPORT_NAME")?;
    sanitize_report_file_name(name)?;
    Ok(canonical)
}

#[tauri::command]
pub async fn degisikis_open_report(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let directory = report_directory(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let target = ensure_report_path(&directory, Path::new(&path))?;
        open_with_default_application(target.as_os_str())
    })
    .await
    .map_err(|_| "REPORT_OPEN".to_string())?
    .map_err(str::to_string)
}

fn open_with_default_application(target: &std::ffi::OsStr) -> Result<(), &'static str> {
    let path = target;
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("/usr/bin/open");
        command.arg(path);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        // explorer.exe yolu varsayılan uygulamaya devreder ve kabuk ayrıştırması
        // yapmaz. Başarıda dahi sıfır olmayan çıkış kodu döndürebildiği için
        // durum kodu değil yalnız süreç başlatma hatası dikkate alınır.
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(path);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };

    #[cfg(target_os = "windows")]
    {
        command.spawn().map(|_| ()).map_err(|_| "REPORT_OPEN")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = command.status().map_err(|_| "REPORT_OPEN")?;
        if status.success() {
            Ok(())
        } else {
            Err("REPORT_OPEN")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_four_comparable_formats_are_readable() {
        let dir = std::env::temp_dir();
        // Dizin okunmaz.
        let e = tauri::async_runtime::block_on(read_document_bytes(dir.display().to_string()))
            .unwrap_err();
        assert_eq!(e, "DOCUMENT_NOT_A_FILE");

        // Desteklenmeyen uzantı okunmaz — dosya var olsa bile.
        let f = dir.join(format!("belge-dg-{}.txt", std::process::id()));
        std::fs::write(&f, b"metin").unwrap();
        let e = tauri::async_runtime::block_on(read_document_bytes(f.display().to_string()))
            .unwrap_err();
        assert_eq!(e, "DOCUMENT_UNSUPPORTED");
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn a_supported_document_is_returned_as_a_raw_ipc_body() {
        let f = std::env::temp_dir().join(format!("belge-dg-{}.docx", std::process::id()));
        let bytes: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
        std::fs::write(&f, &bytes).unwrap();
        let read = tauri::async_runtime::block_on(degisikis_read_document(f.display().to_string()))
            .unwrap();
        std::fs::remove_file(&f).ok();
        assert_eq!(&*read, bytes.as_slice());
        // JSON sayı dizisi değil, ham gövde.
        assert!(matches!(read.body().unwrap(), InvokeResponseBody::Raw(raw) if raw == bytes));
    }

    // `tauri::ipc::Request` yalnız Tauri'nin komut çağrısından kurulur; komutlar
    // gövdeyi ve başlıkları aşağıdaki okuyuculara verir, testler okuyucuları
    // doğrudan sınar.

    #[test]
    fn a_raw_body_crosses_byte_for_byte_and_the_json_fallback_is_accepted() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        assert_eq!(
            body_bytes(&InvokeBody::Raw(bytes.clone()), 1024),
            Some(bytes.clone())
        );
        // Özel protokol engellendiğinde Tauri aynı baytları sayı dizisi taşır.
        let json = InvokeBody::Json(serde_json::json!(bytes));
        assert_eq!(body_bytes(&json, 1024), Some(bytes));
        // Bayt olmayan JSON gövdesi kabul edilmez.
        assert_eq!(
            body_bytes(&InvokeBody::Json(serde_json::json!([256])), 1024),
            None
        );
        assert_eq!(
            body_bytes(
                &InvokeBody::Json(serde_json::json!({ "contents": [1] })),
                1024
            ),
            None
        );
    }

    #[test]
    fn a_body_over_the_limit_is_refused() {
        assert_eq!(body_bytes(&InvokeBody::Raw(vec![0; 11]), 10), None);
        assert_eq!(
            body_bytes(&InvokeBody::Raw(vec![0; 10]), 10),
            Some(vec![0; 10])
        );
    }

    fn headers_with(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(REPORT_NAME_HEADER, value.parse().unwrap());
        headers
    }

    #[test]
    fn the_report_name_header_is_percent_decoded_then_sanitised() {
        let name = report_name_from(&headers_with("GolgeDosya-Karsilastirma-2026.html")).unwrap();
        assert!(sanitize_report_file_name(&name).is_ok());
        // Başlık adı büyük/küçük harfe duyarsızdır.
        let mut upper = HeaderMap::new();
        upper.insert(
            "X-Degisikis-Report-Name",
            "DegisikIs-Rapor-1.md".parse().unwrap(),
        );
        assert_eq!(
            report_name_from(&upper).as_deref(),
            Some("DegisikIs-Rapor-1.md")
        );
        // Türkçe karakter yüzde-kodlu gelir; çözülür, ama süzgeç yine reddeder.
        let turkish = report_name_from(&headers_with("DegisikIs-Rapor-%C3%B6zet.html")).unwrap();
        assert_eq!(turkish, "DegisikIs-Rapor-özet.html");
        assert!(sanitize_report_file_name(&turkish).is_err());
        // Kodlanmış kaçış denemesi çözüldükten SONRA süzülür.
        let escape =
            report_name_from(&headers_with("DegisikIs-Rapor-..%2F..%2Fgizli.html")).unwrap();
        assert!(sanitize_report_file_name(&escape).is_err());
        // Eksik başlık ve bozuk kodlama ad sayılmaz.
        assert_eq!(report_name_from(&HeaderMap::new()), None);
        assert_eq!(
            report_name_from(&headers_with("DegisikIs-Rapor-%G1.html")),
            None
        );
        assert_eq!(
            report_name_from(&headers_with("DegisikIs-Rapor-%+1.html")),
            None
        );
        assert_eq!(
            report_name_from(&headers_with("DegisikIs-Rapor-%C3.html")),
            None
        );
        assert_eq!(report_name_from(&headers_with("DegisikIs-Rapor-%4")), None);
    }

    #[test]
    fn a_report_is_written_into_its_directory() {
        let dir = std::env::temp_dir().join(format!("belge-rapor-yaz-{}", std::process::id()));
        let path =
            write_report(&dir, "GolgeDosya-Karsilastirma-test.html", b"<html></html>").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"<html></html>");
        assert_eq!(Path::new(&path).parent(), Some(dir.as_path()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_report_name_must_carry_the_expected_prefix_and_extension() {
        // Canonical GölgeDosya öneki — arayüzün BUGÜN ürettiği ad.
        assert!(sanitize_report_file_name("GolgeDosya-Karsilastirma-2026.html").is_ok());
        assert!(sanitize_report_file_name("GolgeDosya-Karsilastirma-2026.docx").is_ok());
        // Eski önek geriye dönük kabul edilir: kullanıcının önceden ürettiği
        // raporlar açılabilmeye devam etmeli.
        assert!(sanitize_report_file_name("DegisikIs-Rapor-2026.html").is_ok());
        assert!(sanitize_report_file_name("DegisikIs-Rapor-2026.docx").is_ok());
        assert!(
            sanitize_report_file_name("rapor.html").is_err(),
            "önek zorunlu"
        );
        assert!(
            sanitize_report_file_name("DegisikIs-Rapor-2026.exe").is_err(),
            "uzantı süzgeci"
        );
    }

    #[test]
    fn a_report_name_cannot_escape_its_directory() {
        for bad in [
            "DegisikIs-Rapor-../gizli.html",
            "DegisikIs-Rapor-/etc/passwd.html",
            "DegisikIs-Rapor-..\\x.html",
        ] {
            assert!(
                sanitize_report_file_name(bad).is_err(),
                "kaçış kabul edildi: {bad}"
            );
        }
    }

    #[test]
    fn a_file_outside_the_report_directory_is_refused_even_if_it_exists() {
        let dir = std::env::temp_dir().join(format!("belge-rapor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let outside = std::env::temp_dir().join(format!(
            "DegisikIs-Rapor-disarida-{}.html",
            std::process::id()
        ));
        std::fs::write(&outside, b"<html></html>").unwrap();
        assert_eq!(
            ensure_report_path(&dir, &outside).unwrap_err(),
            "REPORT_SCOPE"
        );
        std::fs::remove_file(&outside).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_report_is_reported_as_missing_not_as_a_scope_error() {
        let dir = std::env::temp_dir();
        assert_eq!(
            ensure_report_path(&dir, Path::new("/olmayan/DegisikIs-Rapor-x.html")).unwrap_err(),
            "REPORT_MISSING"
        );
    }
}
