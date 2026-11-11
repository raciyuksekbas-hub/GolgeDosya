//! Sayfa önizlemesini ham bayt olarak döndürür.
//!
//! `duzenek_preview_pdf_page` PNG'yi JSON sayı dizisi olarak yollar: 5 MB'lık
//! Retina önizlemesi yaklaşık 20 MB metne dönüşür, WebView onu ayrıştırıp yeniden
//! bayta çevirir. O komut bağımsız DüzenEk ile gövde eşitliği kapısının
//! (`scripts/check-command-parity.py`) altında olduğu için dokunulmadı.

use std::path::Path;
use tauri::ipc::Response;

#[tauri::command]
pub async fn duzenek_preview_page_bytes(
    path: String,
    page: usize,
    dpi: u32,
    rotation: i32,
) -> Result<Response, String> {
    let png = tauri::async_runtime::spawn_blocking(move || {
        ekler_core::raster::preview_page(Path::new(&path), page, dpi, rotation)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(Response::new(png))
}
