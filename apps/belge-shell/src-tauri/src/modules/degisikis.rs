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

pub mod legacy_doc;

use std::path::{Path, PathBuf};
use tauri::Manager;

/// Rapor dosyalarının zorunlu ad öneki. Değişikİş'teki değerle aynı: eski
/// uygulamanın ürettiği raporlar da aynı adla açılabilmeli.
const REPORT_FILE_PREFIX: &str = "DegisikIs-Rapor-";

/// Eski ikili `.doc` dosyasını `.docx`'e çevir.
///
/// Baytlar arayüzden gelir ve arayüze döner; dosya sistemine kalıcı bir şey
/// yazılmaz. Dönüşüm geçici bir dizinde yapılır ve dizin her koşulda silinir.
#[tauri::command]
pub async fn degisikis_convert_legacy_doc(contents: Vec<u8>) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || legacy_doc::convert(&contents))
        .await
        .map_err(|_| "DOC_CONVERSION".to_string())?
        .map_err(str::to_string)
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
    if !name.starts_with(REPORT_FILE_PREFIX) {
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

#[tauri::command]
pub fn degisikis_save_report(
    app: tauri::AppHandle,
    file_name: String,
    contents: Vec<u8>,
) -> Result<String, String> {
    let name = sanitize_report_file_name(&file_name).map_err(str::to_string)?;
    let directory = report_directory(&app);
    std::fs::create_dir_all(&directory).map_err(|_| "REPORT_DIR".to_string())?;
    let path = directory.join(name);
    std::fs::write(&path, contents).map_err(|_| "REPORT_WRITE".to_string())?;
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
    fn a_report_name_must_carry_the_expected_prefix_and_extension() {
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
