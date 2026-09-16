use crate::accounting::calculate_sha256;
use crate::model::{SignedPolicy, SourceFile, SourceFormat};
use crate::pdf::inspect_pdf;
use crate::udf::inspect_udf;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanFileError {
    pub path: String,
    pub file_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanBatchResult {
    pub sources: Vec<SourceFile>,
    pub errors: Vec<ScanFileError>,
}

pub fn scan_source_files<P: AsRef<Path>>(paths: &[P]) -> ScanBatchResult {
    let mut sources = Vec::new();
    let mut errors = Vec::new();

    for p in paths {
        let path = p.as_ref();
        let path_str = path.to_string_lossy().to_string();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("adsiz")
            .to_string();

        if !path.exists() {
            errors.push(ScanFileError {
                path: path_str,
                file_name: file_name.clone(),
                reason: format!("'{}' dosyası bulunamadı veya erişilemez.", file_name),
            });
            continue;
        }

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                errors.push(ScanFileError {
                    path: path_str,
                    file_name: file_name.clone(),
                    reason: format!("'{}' dosya bilgileri okunamadı: {}", file_name, e),
                });
                continue;
            }
        };

        if metadata.is_dir() {
            errors.push(ScanFileError {
                path: path_str,
                file_name: file_name.clone(),
                reason: format!("'{}' bir klasördür, lütfen dosya seçin.", file_name),
            });
            continue;
        }

        let size_bytes = metadata.len();
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let format = SourceFormat::from_path(path);
        if format == SourceFormat::Unknown {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            errors.push(ScanFileError {
                path: path_str,
                file_name: file_name.clone(),
                reason: format!(
                    "'{}' için desteklenmeyen dosya biçimi (.{}).",
                    file_name, ext
                ),
            });
            continue;
        }

        let sha256_before = match calculate_sha256(path) {
            Ok(h) => h,
            Err(e) => {
                errors.push(ScanFileError {
                    path: path_str,
                    file_name: file_name.clone(),
                    reason: format!("'{}' sağlama toplamı hesaplanamadı: {}", file_name, e),
                });
                continue;
            }
        };

        // SHA-256'nın ilk 12 karakterinden türetilen deterministik ve tekil ID
        let id = format!("src-{}", sha256_before);

        // Gerçek format denetimi ve metadata tespiti
        let inspection = match format {
            SourceFormat::Pdf => match inspect_pdf(path) {
                Ok(info) => {
                    let note = if info.is_signed {
                        Some("Elektronik / Dijital imza tespit edildi".to_string())
                    } else if info.is_repaired {
                        Some("Bu PDF standart dışı bir yapıya sahip. GölgeDosya belgeyi değiştirmeden uyumlu bir çalışma kopyası oluşturdu.".to_string())
                    } else {
                        None
                    };
                    Ok((
                        info.page_count,
                        info.is_signed,
                        note,
                        info.is_repaired,
                        info.repair_note,
                    ))
                }
                Err(e) => Err(format!("'{}' okunamadı: {}.", file_name, e)),
            },
            SourceFormat::Udf => {
                match inspect_udf(path) {
                    Ok(info) => {
                        let note = if info.is_signed {
                            Some("UDF Elektronik İmza (sign.sgn) mevcut".to_string())
                        } else {
                            None
                        };
                        let estimated_pages = 0; // Layout is unknown until explicitly approved PDF conversion.
                        Ok((estimated_pages, info.is_signed, note, false, None))
                    }
                    Err(e) => Err(format!(
                        "'{}' okunamadı: geçersiz UDF yapısı ({}).",
                        file_name, e
                    )),
                }
            }
            SourceFormat::Tif | SourceFormat::Tiff => crate::image::image_file_to_pdf(path)
                .map(|doc| (doc.get_pages().len(), false, None, false, None))
                .map_err(|e| e.to_string()),
            SourceFormat::Doc | SourceFormat::Docx => crate::office::convert_to_pdf(path, false)
                .map(|doc| {
                    (
                        doc.get_pages().len(),
                        false,
                        Some("Yerel LibreOffice ile PDF dönüşümü doğrulandı".into()),
                        false,
                        None,
                    )
                })
                .map_err(|e| e.to_string()),
            SourceFormat::Heic => crate::image::image_file_to_pdf(path)
                .map(|doc| (doc.get_pages().len(), false, None, false, None))
                .map_err(|e| e.to_string()),
            _ if format.is_image() => match ::image::image_dimensions(path) {
                Ok(_) => Ok((1, false, None, false, None)),
                Err(e) => Err(format!(
                    "'{}' okunamadı: geçersiz görsel formatı ({}).",
                    file_name, e
                )),
            },
            _ => Ok((1, false, None, false, None)),
        };

        let (page_count, is_signed, signature_note, is_repaired, repair_note) = match inspection {
            Ok(data) => data,
            Err(reason) => {
                errors.push(ScanFileError {
                    path: path_str,
                    file_name: file_name.clone(),
                    reason,
                });
                continue;
            }
        };

        if calculate_sha256(path).ok().as_ref() != Some(&sha256_before) {
            errors.push(ScanFileError {
                path: path_str,
                file_name,
                reason: "Belge tarama sırasında değişti; yeniden ekleyin".into(),
            });
            continue;
        }
        sources.push(SourceFile {
            id,
            path: path.to_path_buf(),
            file_name,
            format,
            size_bytes,
            sha256_before,
            mtime,
            page_count,
            is_signed,
            signature_note,
            signed_policy: SignedPolicy::UseOriginalAsIs,
            is_approved_for_conversion: false,
            is_repaired,
            repair_note,
        });
    }

    ScanBatchResult { sources, errors }
}
