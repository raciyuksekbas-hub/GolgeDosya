//! Ekler — GölgeDosya'nın kendi ek paketi komutları.
//!
//! `duzenek.rs` bağımsız DüzenEk'ten TAŞINAN komutları tutar ve gövdeleri
//! dondurulmuş baseline ile birebir aynı kalmak zorundadır
//! (`scripts/check-command-parity.py`, sürüm kapısı). Ekler'in saha turunda
//! gereken iki davranışı o gövdelere dokunmadan burada yaşar:
//!
//! * Hazırlama ANA iş parçacığında çalışmaz. Tauri eşzamanlı komutları ana iş
//!   parçacığında çalıştırır; `duzenek_prepare_uyap` eşzamanlıdır ve yirmi ekli
//!   bir dosyada hazırlama boyunca pencere donuyor, "Ekler hazırlanıyor…"
//!   göstergesi dönemiyor, Windows pencereyi "Yanıt vermiyor" diye
//!   işaretleyebiliyordu. Motor çağrısı aynıdır.
//! * "Klasörü Aç" paketin KENDİSİNİ açar (madde 10). "Öğeyi üst klasöründe
//!   göster" kullanılmaz: o API klasörün ÜSTÜNÜ açar (madde 13'ün hatası).
use ekler_core::{execute_uyap_preparation, ExecutionContext, PipelineResult, Project};
use std::path::{Path, PathBuf};

/// Ekleri hazırla ve kaydet — motor çağrısı `duzenek_prepare_uyap` ile aynı,
/// yalnız engelleyici iş ayrı bir iş parçacığında.
#[tauri::command]
pub async fn ekler_prepare_package(
    project: Project,
    output_dir: String,
) -> Result<PipelineResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let ctx = ExecutionContext {
            output_dir: PathBuf::from(output_dir),
        };
        execute_uyap_preparation(&project, &ctx).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Açılabilecek tek klasör: Ekler'in yayımladığı paket.
///
/// Arayüzden gelen yol doğrudan "aç"a verilmez: dosya olsaydı uygulamasıyla
/// çalışırdı. Yalnız var olan, adı motorun paket adlandırmasına uyan
/// (`GolgeDosya-<ad>-<ek>`, bkz. `ekler_core::pipeline`) bir KLASÖR kabul edilir.
pub(crate) fn package_folder_to_open(path: &Path) -> Result<PathBuf, String> {
    let named = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("GolgeDosya-"));
    if !named || !path.is_dir() {
        return Err("Klasör bulunamadı. Taşınmış veya silinmiş olabilir.".into());
    }
    Ok(path.to_path_buf())
}

/// "Klasörü Aç": hazırlanan paketin kendisi.
#[tauri::command]
pub fn ekler_open_package_folder(path: String) -> Result<(), String> {
    let dir = package_folder_to_open(Path::new(&path))?;
    tauri_plugin_opener::open_path(&dir, None::<&str>).map_err(|_| "Klasör açılamadı.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_package_folder_itself_is_opened_not_its_parent() {
        let root = tempfile::tempdir().unwrap();
        let chosen = root
            .path()
            .join("Çağrı Şahin")
            .join("Documents")
            .join("GölgeDosya");
        let package = chosen.join("GolgeDosya-Dilekce_Ekleri-fCTl67");
        std::fs::create_dir_all(&package).unwrap();
        assert_eq!(package_folder_to_open(&package).unwrap(), package);
    }

    #[test]
    fn nothing_but_an_existing_package_folder_is_opened() {
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("GolgeDosya-Dilekce_Ekleri-abc");
        std::fs::create_dir_all(&package).unwrap();
        // Paketin içindeki bir dosya açılmaz (uygulamasıyla çalışırdı).
        let file = package.join("EKLER_LISTESI.txt");
        std::fs::write(&file, "EKLER").unwrap();
        assert!(package_folder_to_open(&file).is_err());
        // Motorun adlandırmasına uymayan klasör, üst klasör ve olmayan yol.
        assert!(package_folder_to_open(root.path()).is_err());
        assert!(package_folder_to_open(&root.path().join("GolgeDosya-yok")).is_err());
    }
}
