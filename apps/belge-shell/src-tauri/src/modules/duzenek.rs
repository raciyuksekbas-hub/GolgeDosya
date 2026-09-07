//! Düzenle — DüzenEk'in PDF ve Office işleme yüzeyi.
//!
//! Bu bir taşımadır. Komut GÖVDELERİ bağımsız uygulamadan **değiştirilmeden**
//! alındı; yalnız komut adları modül önekli hâle getirildi ve `pub` yapıldı.
//! Gövdelerdeki `ekler_core::` çağrıları özgün adlarını korur.
//!
//! Motor `ekler-core`'dur ve **kopyalanmadı**: kaynağı standalone deposunda
//! duruyor. Dondurulmuş baseline `duzenek-premerge-2026-09-07` parity
//! kaynağıdır ve DüzenEk emekliye ayrılana kadar korunacak.
//!
//! `pdf-core` ayrımı bu fazda YAPILMADI ve bu bilinçlidir. Gerçek bağımlılık
//! grafiği ölçüldü: `model` (domain) `optimizer`e bağlı (`ExportPlan`'in
//! `optimization: OptimizationLevel` alanı, model.rs:203), `pdf/stamp` ise
//! `model`e bağlı (`StampConfig`, `StampPosition`, stamp.rs:2). Ayırmak üç
//! dosyada tip taşıma refactor'u gerektirirdi; bu fazın hedefi mimari güzellik
//! değil migration parity. Tipler ortak bir katmana alındıktan sonra ayrı bir
//! işte yapılabilir.
//!
//! LibreOffice köprüsü `document-core` veya `pdf-core` içine GÖMÜLMEDİ; hâlâ
//! `ekler-core::office` içindedir ve `check-architecture.sh` çekirdeklerde
//! LibreOffice referansı bulunmadığını her sürüm kapısında doğrular.
//!
//! NO-TOUCH: raster.rs, CoreGraphics FFI, safe_io, tolerant PDF, optimizer,
//! signed-original yolu, kaynak SHA muhasebesi, branding Form XObject,
//! publication ve LibreOffice keşfi bu turda davranışsal olarak değişmedi.

use ekler_core::{
    convert_udf_to_markdown, delete_pages_from_file, detect_likely_blank_pages,
    execute_uyap_preparation, extract_pages_to_file, images_to_pdf_file, merge_pdf_files,
    rotate_pdf_pages, scan_source_files as core_scan_source_files, ExecutionContext,
    PipelineResult, Project, ScanBatchResult,
};
use std::path::{Path, PathBuf};

#[tauri::command]
pub async fn duzenek_preview_pdf_page(
    path: String,
    page: usize,
    dpi: u32,
    rotation: i32,
) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ekler_core::raster::preview_page(Path::new(&path), page, dpi, rotation)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn duzenek_renderer_status() -> Option<String> {
    ekler_core::office::find_renderer().map(|p| p.to_string_lossy().into_owned())
}
#[tauri::command]
pub fn duzenek_select_renderer(path: String) -> Result<String, String> {
    ekler_core::office::select_renderer(Path::new(&path))
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn duzenek_prepare_export_plan(
    project: Project,
) -> Result<ekler_core::ExportPlan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ekler_core::prepare_export_plan(&project).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn duzenek_execute_export_plan(
    plan: ekler_core::ExportPlan,
    output_dir: String,
) -> Result<PipelineResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ekler_core::execute_export_plan(
            &plan,
            &ExecutionContext {
                output_dir: output_dir.into(),
            },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn duzenek_split_into_new_exhibit(
    project: Project,
    exhibit_id: String,
    after: usize,
) -> Result<Project, String> {
    ekler_core::split_into_new_exhibit(&project, &exhibit_id, after).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn duzenek_scan_source_files(paths: Vec<String>) -> Result<ScanBatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || core_scan_source_files(&paths))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duzenek_prepare_uyap(
    project: Project,
    output_dir: String,
) -> Result<PipelineResult, String> {
    let ctx = ExecutionContext {
        output_dir: PathBuf::from(output_dir),
    };
    execute_uyap_preparation(&project, &ctx).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn duzenek_convert_office_pdf(
    path: String,
    output_path: String,
    approved: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        ekler_core::office::convert_to_pdf_file(Path::new(&path), Path::new(&output_path), approved)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn duzenek_convert_udf_to_md(
    udf_path: String,
    output_dir: String,
    approved: bool,
) -> Result<ekler_core::udf::MarkdownResult, String> {
    convert_udf_to_markdown(Path::new(&udf_path), Path::new(&output_dir), approved)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn duzenek_pdf_to_images(
    path: String,
    output_dir: String,
    format: String,
    dpi: u32,
    approved: bool,
) -> Result<PathBuf, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ekler_core::raster::pdf_to_images(
            Path::new(&path),
            Path::new(&output_dir),
            &format,
            dpi,
            approved,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn duzenek_run_pdf_tool(
    paths: Vec<String>,
    operation: ekler_core::toolbox::ToolOperation,
    output_path: String,
    approved: bool,
) -> Result<ekler_core::toolbox::ToolOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ekler_core::toolbox::run_tool_with_outcome(
            &paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
            &operation,
            Path::new(&output_path),
            approved,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn duzenek_merge_pdfs(paths: Vec<String>, output_path: String) -> Result<(), String> {
    let pb_vec: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    merge_pdf_files(&pb_vec, Path::new(&output_path)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duzenek_split_pdf(
    path: String,
    start: usize,
    end: usize,
    output_path: String,
) -> Result<(), String> {
    extract_pages_to_file(Path::new(&path), start, end, Path::new(&output_path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duzenek_delete_pdf_pages(
    path: String,
    pages: Vec<usize>,
    output_path: String,
) -> Result<(), String> {
    delete_pages_from_file(Path::new(&path), &pages, Path::new(&output_path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duzenek_rotate_pdf_pages_cmd(
    path: String,
    degrees: i32,
    output_path: String,
) -> Result<(), String> {
    rotate_pdf_pages(Path::new(&path), degrees, Path::new(&output_path)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duzenek_images_to_pdf(paths: Vec<String>, output_path: String) -> Result<(), String> {
    let pb_vec: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    images_to_pdf_file(&pb_vec, Path::new(&output_path)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duzenek_detect_blank_pages(path: String) -> Result<Vec<usize>, String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let name = Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("belge.pdf");
    let doc = ekler_core::load_pdf_tolerant(&bytes, name)
        .map_err(|e| e.to_string())?
        .document;
    Ok(detect_likely_blank_pages(&doc))
}

#[tauri::command]
pub fn duzenek_save_project_to_file(project: Project, path: String) -> Result<(), String> {
    ekler_core::persistence::save_project(&project, Path::new(&path)).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn duzenek_load_project_from_file(
    path: String,
) -> Result<ekler_core::persistence::ProjectLoadReport, String> {
    ekler_core::persistence::load_project(Path::new(&path)).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn duzenek_relink_source(
    project: Project,
    source_id: String,
    path: String,
) -> Result<ekler_core::persistence::ProjectLoadReport, String> {
    ekler_core::persistence::relink_source(&project, &source_id, Path::new(&path))
        .map_err(|e| e.to_string())
}
