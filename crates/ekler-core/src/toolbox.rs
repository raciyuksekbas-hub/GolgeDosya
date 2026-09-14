use crate::error::{EklerError, Result};
use crate::image::image_file_to_pdf;
use crate::pdf::{extract_page_range, load_pdf_tolerant, merge_documents};
use lopdf::{Document as LopdfDoc, Object};
use std::path::{Path, PathBuf};

fn load_single_pdf_approved(p: &Path) -> Result<LopdfDoc> {
    let bytes = std::fs::read(p).map_err(|e| EklerError::IoError {
        path: p.to_path_buf(),
        source: e,
    })?;
    let file_name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("belge.pdf");
    Ok(load_pdf_tolerant(&bytes, file_name)?.document)
}

fn validated_pdf_bytes(doc: &mut LopdfDoc) -> Result<Vec<u8>> {
    let expected = doc.get_pages().len();
    if expected == 0 {
        return Err(EklerError::InvalidPdf(
            "PDF en az bir sayfa içermeli".into(),
        ));
    }
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
    let reopened = LopdfDoc::load_mem(&bytes).map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
    crate::pdf::validate_document(&reopened)?;
    if reopened.get_pages().len() != expected {
        return Err(EklerError::InvalidPdf("Sayfa sayısı değişti".into()));
    }
    Ok(bytes)
}

/// Compatibility entry points route through the same validated tool operation.
pub fn merge_pdf_files(paths: &[PathBuf], output: &Path) -> Result<()> {
    run_tool(paths, &ToolOperation::Merge, output, false)
}
pub fn extract_pages_to_file(source: &Path, start: usize, end: usize, output: &Path) -> Result<()> {
    if start == 0 || end < start || end - start > 100_000 {
        return Err(EklerError::ValidationFailed(
            "Geçersiz sayfa aralığı".into(),
        ));
    }
    run_tool(
        &[source.into()],
        &ToolOperation::Select {
            pages: (start..=end).collect(),
        },
        output,
        false,
    )
}
pub fn delete_pages_from_file(source: &Path, pages: &[usize], output: &Path) -> Result<()> {
    run_tool(
        &[source.into()],
        &ToolOperation::Delete {
            pages: pages.to_vec(),
        },
        output,
        false,
    )
}
pub fn rotate_pdf_pages(source: &Path, degrees: i32, output: &Path) -> Result<()> {
    run_tool(
        &[source.into()],
        &ToolOperation::Rotate { degrees },
        output,
        false,
    )
}
pub fn images_to_pdf_file(paths: &[PathBuf], output: &Path) -> Result<()> {
    run_tool(paths, &ToolOperation::Images, output, false)
}

/// Boş Sayfa Tespiti:
/// İçerik akışı (Content Stream) boyutu aşırı küçük olan veya hiç içerik/nesne
/// barındırmayan sayfaları deterministik olarak tespit eder.
/// Otomatik silmez, kullanıcıya aday liste olarak döner.
pub fn detect_likely_blank_pages(doc: &LopdfDoc) -> Vec<usize> {
    let pages = doc.get_pages();
    let mut blank_pages = Vec::new();

    for (&num, &page_id) in pages.iter() {
        if let Ok(page_dict) = doc.get_object(page_id).and_then(|o| o.as_dict()) {
            // Contents akışını denetle
            let mut total_content_bytes = 0;
            if let Ok(contents) = page_dict.get(b"Contents") {
                match contents {
                    Object::Reference(ref_id) => {
                        if let Ok(stream) = doc.get_object(*ref_id).and_then(|o| o.as_stream()) {
                            total_content_bytes += stream.content.len();
                        }
                    }
                    Object::Array(arr) => {
                        for item in arr {
                            if let Ok(ref_id) = item.as_reference() {
                                if let Ok(stream) =
                                    doc.get_object(ref_id).and_then(|o| o.as_stream())
                                {
                                    total_content_bytes += stream.content.len();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Eğer içerik akışı yoksa veya 15 bayttan küçükse (yalnızca q Q gibi boş akışlar)
            if total_content_bytes <= 15 {
                blank_pages.push(num as usize);
            }
        }
    }

    blank_pages.sort();
    blank_pages
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolOperation {
    Merge,
    Images,
    Select { pages: Vec<usize> },
    Reorder { pages: Vec<usize> },
    Delete { pages: Vec<usize> },
    Rotate { degrees: i32 },
    RotateSelected { degrees: i32, pages: Vec<usize> },
    RotatePages { rotations: Vec<PageRotation> },
    Compress { level: crate::OptimizationLevel },
    Crop { margin_pt: f32 },
    Watermark { text: String },
    Number { start: usize },
}
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolOutcome {
    Published {
        output_bytes: u64,
    },
    Compressed {
        source_bytes: u64,
        body_bytes: u64,
        output_bytes: u64,
        images_found: usize,
        images_recompressed: usize,
    },
    NoBenefit {
        source_bytes: u64,
        body_bytes: u64,
        candidate_bytes: u64,
        images_found: usize,
        images_recompressed: usize,
    },
    Failed {
        reason: String,
    },
}

pub fn run_tool(
    paths: &[PathBuf],
    operation: &ToolOperation,
    output: &Path,
    approved: bool,
) -> Result<()> {
    match run_tool_with_outcome(paths, operation, output, approved)? {
        ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. } => Ok(()),
        ToolOutcome::Failed { reason } => Err(EklerError::ValidationFailed(reason)),
        ToolOutcome::NoBenefit { .. } => Err(EklerError::ValidationFailed(
            "Bu belge zaten yeterince optimize. Daha küçük bir kopya oluşturulamadı.".into(),
        )),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PageRotation {
    pub page: usize,
    pub degrees: i32,
}

/// All general tools publish one validated, no-clobber derived PDF.
pub fn run_tool_with_outcome(
    paths: &[PathBuf],
    operation: &ToolOperation,
    output: &Path,
    approved: bool,
) -> Result<ToolOutcome> {
    let result = run_tool_inner(paths, operation, output, approved);
    if matches!(operation, ToolOperation::Compress { .. }) {
        Ok(result.unwrap_or_else(|e| ToolOutcome::Failed {
            reason: e.to_string(),
        }))
    } else {
        result
    }
}
fn run_tool_inner(
    paths: &[PathBuf],
    operation: &ToolOperation,
    output: &Path,
    approved: bool,
) -> Result<ToolOutcome> {
    if paths.is_empty() {
        return Err(EklerError::ValidationFailed("Belge seçin".into()));
    }
    crate::safe_io::ensure_new_destination(output, paths)?;
    let hashes: Vec<_> = paths
        .iter()
        .map(|p| {
            crate::calculate_sha256(p).map_err(|source| EklerError::IoError {
                path: p.clone(),
                source,
            })
        })
        .collect::<Result<_>>()?;
    let mut docs = Vec::new();
    for path in paths {
        if matches!(operation, ToolOperation::Images) {
            docs.push(image_file_to_pdf(path)?);
        } else {
            let info = crate::pdf::inspect_pdf(path)?;
            if info.is_signed && !approved {
                return Err(EklerError::ValidationFailed(
                    "İmzalı PDF için türetilmiş kopya onayı gerekli".into(),
                ));
            }
            docs.push(load_single_pdf_approved(path)?);
        }
    }
    if docs.len() > 1 && !matches!(operation, ToolOperation::Merge | ToolOperation::Images) {
        return Err(EklerError::ValidationFailed(
            "Bu araç için tek PDF seçin".into(),
        ));
    }
    let mut doc = if docs.len() == 1 {
        docs.remove(0)
    } else {
        merge_documents(&docs)?
    };
    let total = doc.get_pages().len();
    let original = if matches!(operation, ToolOperation::Compress { .. }) {
        Some(doc.clone())
    } else {
        None
    };
    let source_bytes = std::fs::metadata(&paths[0])
        .map_err(|source| EklerError::IoError {
            path: paths[0].clone(),
            source,
        })?
        .len();
    let mut compression = None;
    match operation {
        ToolOperation::Select { pages }
        | ToolOperation::Reorder { pages }
        | ToolOperation::Delete { pages } => {
            let unique: std::collections::HashSet<_> = pages.iter().collect();
            if pages.is_empty()
                || pages.iter().any(|p| *p == 0 || *p > total)
                || unique.len() != pages.len()
            {
                return Err(EklerError::ValidationFailed(
                    "Sayfa listesi boş, yinelenmiş veya sınır dışında".into(),
                ));
            }
            if matches!(operation, ToolOperation::Reorder { .. }) && pages.len() != total {
                return Err(EklerError::ValidationFailed(
                    "Sıralama her sayfayı tam bir kez içermeli".into(),
                ));
            }
            let selected = if matches!(operation, ToolOperation::Delete { .. }) {
                (1..=total)
                    .filter(|p| !pages.contains(p))
                    .collect::<Vec<_>>()
            } else {
                pages.clone()
            };
            if selected.is_empty() {
                return Err(EklerError::ValidationFailed(
                    "En az bir sayfa kalmalı".into(),
                ));
            }
            let parts = selected
                .iter()
                .map(|p| extract_page_range(&doc, *p, *p))
                .collect::<Result<Vec<_>>>()?;
            doc = merge_documents(&parts)?;
        }
        ToolOperation::Rotate { degrees } | ToolOperation::RotateSelected { degrees, .. } => {
            if let ToolOperation::RotateSelected { pages, .. } = operation {
                if pages.is_empty() || pages.iter().any(|p| *p == 0 || *p > total) {
                    return Err(EklerError::ValidationFailed(
                        "Geçersiz dönüş sayfaları".into(),
                    ));
                }
            }
            if degrees % 90 != 0 {
                return Err(EklerError::ValidationFailed(
                    "Dönüş 90 derece katı olmalı".into(),
                ));
            }
            for (page, id) in doc.get_pages() {
                if let ToolOperation::RotateSelected { pages, .. } = operation {
                    if !pages.contains(&(page as usize)) {
                        continue;
                    }
                }
                let resolved = crate::pdf::resolved_page_dictionary(&doc, id)?;
                let old = resolved
                    .get(b"Rotate")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(0);
                doc.get_dictionary_mut(id)
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
                    .set("Rotate", (old + i64::from(*degrees)).rem_euclid(360));
            }
        }
        ToolOperation::RotatePages { rotations } => {
            let mut seen = std::collections::HashSet::new();
            if rotations.is_empty()
                || rotations.iter().any(|r| {
                    r.page == 0 || r.page > total || r.degrees % 90 != 0 || !seen.insert(r.page)
                })
            {
                return Err(EklerError::ValidationFailed(
                    "Geçersiz dönüş sayfaları".into(),
                ));
            }
            let ids = doc.get_pages();
            for r in rotations {
                let id = ids[&(r.page as u32)];
                let resolved = crate::pdf::resolved_page_dictionary(&doc, id)?;
                let old = resolved
                    .get(b"Rotate")
                    .ok()
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0);
                doc.get_dictionary_mut(id)
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
                    .set("Rotate", (old + i64::from(r.degrees)).rem_euclid(360));
            }
        }
        ToolOperation::Compress { level } => {
            use crate::OptimizationLevel::*;
            let levels: &[crate::OptimizationLevel] = match level {
                LowRiskCleanup => &[LowRiskCleanup],
                GentleCompression => &[LowRiskCleanup, GentleCompression],
                BalancedCompression => &[LowRiskCleanup, GentleCompression, BalancedCompression],
                AggressiveCompression => &[
                    LowRiskCleanup,
                    GentleCompression,
                    BalancedCompression,
                    AggressiveCompression,
                ],
            };
            let mut best = None;
            let mut quality_errors = Vec::new();
            for preset in levels {
                let mut candidate = original.as_ref().unwrap().clone();
                // Kapsam dışı görsel (ICC dışı renk uzayı, maske, öngörücü) artık
                // optimize_pdf içinde ATLANIR, hata döndürmez. Buradan çıkan hata
                // gerçek bir bozukluktur (çözülemeyen JPEG, uyuşmayan örnek
                // uzunluğu): temizlik adayı ne kadar kazandırırsa kazandırsın
                // bozuk görseli içinde taşıyan bir çıktı yayınlanmaz.
                let stats = crate::optimize_pdf(&mut candidate, *preset)?;
                if let Err(e) =
                    validate_compression_quality(original.as_ref().unwrap(), &candidate, *preset)
                {
                    quality_errors.push(e.to_string());
                    continue;
                }
                let body = validated_pdf_bytes(&mut candidate)?.len() as u64;
                crate::pdf::stamp::apply_branding(&mut candidate)?;
                let final_size = validated_pdf_bytes(&mut candidate)?.len() as u64;
                if best
                    .as_ref()
                    .is_none_or(|(_, _, _, size)| final_size < *size)
                {
                    best = Some((candidate, stats, body, final_size));
                }
            }
            let (candidate, stats, body, _) = best.ok_or_else(|| {
                quality_errors.dedup();
                EklerError::ValidationFailed(quality_errors.join("; "))
            })?;
            doc = candidate;
            compression = Some((body, stats));
        }
        ToolOperation::Crop { margin_pt } => {
            if !margin_pt.is_finite() || *margin_pt < 0. {
                return Err(EklerError::ValidationFailed(
                    "Geçersiz kırpma mesafesi".into(),
                ));
            }
            for id in doc.get_pages().into_values() {
                let r = crate::pdf::resolved_page_dictionary(&doc, id)?;
                let obj = r
                    .get(b"CropBox")
                    .or_else(|_| r.get(b"MediaBox"))
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
                let (_, obj) = doc
                    .dereference(obj)
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
                let a = obj
                    .as_array()
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
                if a.len() != 4 {
                    return Err(EklerError::InvalidPdf("Geçersiz sayfa kutusu".into()));
                }
                let mut a = a
                    .iter()
                    .map(|v| {
                        v.as_float()
                            .map_err(|e| EklerError::InvalidPdf(e.to_string()))
                    })
                    .collect::<Result<Vec<_>>>()?;
                a[0] += margin_pt;
                a[1] += margin_pt;
                a[2] -= margin_pt;
                a[3] -= margin_pt;
                if a[2] <= a[0] || a[3] <= a[1] {
                    return Err(EklerError::ValidationFailed(
                        "Kırpma bütün sayfayı kaldırıyor".into(),
                    ));
                }
                doc.get_dictionary_mut(id)
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
                    .set(
                        "CropBox",
                        a.into_iter().map(Object::Real).collect::<Vec<_>>(),
                    );
            }
        }
        ToolOperation::Watermark { text } => {
            if text.is_empty() || !text.is_ascii() || text.len() > 60 {
                return Err(EklerError::ValidationFailed("Filigran 1–60 temel Latin karakteri içermeli; bu sürümde Unicode filigran desteklenmiyor".into()));
            }
            crate::pdf::stamp::apply_text_marks(&mut doc, Some(text), 1)?;
        }
        ToolOperation::Number { start } => {
            if *start == 0 {
                return Err(EklerError::ValidationFailed(
                    "Başlangıç sayfası en az 1 olmalı".into(),
                ));
            }
            crate::pdf::stamp::apply_text_marks(&mut doc, None, *start)?;
        }
        _ => {}
    }
    for (p, hash) in paths.iter().zip(hashes) {
        if crate::calculate_sha256(p).map_err(|source| EklerError::IoError {
            path: p.clone(),
            source,
        })? != hash
        {
            return Err(EklerError::SourceIntegrityCompromised { path: p.clone() });
        }
    }
    if compression.is_none() {
        crate::pdf::stamp::apply_branding(&mut doc)?;
    }
    let bytes = validated_pdf_bytes(&mut doc)?;
    if let Some((body_bytes, stats)) = compression {
        // Kalan quality_errors yalnız daha sert bir ön ayarın PİKSEL kalitesi
        // nedeniyle elendiğini söyler; kazanan aday geçerlidir. Kazanç %3'ün
        // altındaysa bu dürüstçe "kazanç yok"tur, hata değil.
        if (bytes.len() as u128) * 100 > (source_bytes as u128) * 97 {
            return Ok(ToolOutcome::NoBenefit {
                source_bytes,
                body_bytes,
                candidate_bytes: bytes.len() as u64,
                images_found: stats.images_found,
                images_recompressed: stats.images_recompressed_count,
            });
        }
        crate::safe_io::write_new_bytes(output, paths, &bytes)?;
        return Ok(ToolOutcome::Compressed {
            source_bytes,
            body_bytes,
            output_bytes: bytes.len() as u64,
            images_found: stats.images_found,
            images_recompressed: stats.images_recompressed_count,
        });
    }
    crate::safe_io::write_new_bytes(output, paths, &bytes)?;
    Ok(ToolOutcome::Published {
        output_bytes: bytes.len() as u64,
    })
}

/// Compression preserves page operators; only supported JPEG samples may change.
/// Compare decoded pixels at candidate resolution before any branding is added.
fn validate_compression_quality(
    source: &LopdfDoc,
    candidate: &LopdfDoc,
    level: crate::OptimizationLevel,
) -> Result<()> {
    let fail = || {
        EklerError::ValidationFailed(
            "Sıkıştırma adayı kalite kontrolünü geçemedi; çıktı oluşturulmadı.".into(),
        )
    };
    if source.get_pages().len() != candidate.get_pages().len() {
        return Err(fail());
    }
    for (n, id) in source.get_pages() {
        let other = candidate.get_pages()[&n];
        if source.get_page_content(id).map_err(|_| fail())?
            != candidate.get_page_content(other).map_err(|_| fail())?
        {
            return Err(fail());
        }
    }
    for (id, obj) in &candidate.objects {
        let Ok(after) = obj.as_stream() else { continue };
        if after
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|v| v.as_name().ok())
            != Some(b"Image")
        {
            continue;
        }
        let before = source
            .get_object(*id)
            .and_then(|v| v.as_stream())
            .map_err(|_| fail())?;
        if before.content == after.content {
            continue;
        }
        if let (Ok(a), Ok(b)) = (before.decompressed_content(), after.decompressed_content()) {
            if a == b {
                continue;
            }
        }
        if level == crate::OptimizationLevel::LowRiskCleanup {
            return Err(fail());
        }
        let resolve = |doc: &LopdfDoc, stream: &lopdf::Stream| -> Result<lopdf::Stream> {
            let mut s = stream.clone();
            // Optimizer ile aynı çözümleme; dolaylı boyut girdisi dâhil.
            for key in [
                b"Filter".as_slice(),
                b"DecodeParms",
                b"Width",
                b"Height",
                b"BitsPerComponent",
            ] {
                if let Ok(v) = s.dict.get(key).cloned() {
                    s.dict
                        .set(key, doc.dereference(&v).map_err(|_| fail())?.1.clone());
                }
            }
            // Optimizer ile aynı indirgeme: ICCBased görsel Device adıyla çözülür.
            if let Some(space) = s
                .dict
                .get(b"ColorSpace")
                .ok()
                .and_then(|v| crate::optimizer::decodable_colorspace(doc, v))
            {
                s.dict.set("ColorSpace", space);
            }
            Ok(s)
        };
        let a = crate::optimizer::decode_image(&resolve(source, before)?)?
            .ok_or_else(fail)?
            .to_rgb8();
        let b = crate::optimizer::decode_image(&resolve(candidate, after)?)?
            .ok_or_else(fail)?
            .to_rgb8();
        let reference = ::image::imageops::resize(
            &a,
            b.width(),
            b.height(),
            ::image::imageops::FilterType::Lanczos3,
        );
        let sum: u64 = reference
            .as_raw()
            .iter()
            .zip(b.as_raw())
            .map(|(a, b)| a.abs_diff(*b) as u64)
            .sum();
        let limit = if level == crate::OptimizationLevel::GentleCompression {
            12.0
        } else {
            20.0
        };
        if sum as f64 / b.as_raw().len() as f64 > limit {
            return Err(fail());
        }
    }
    Ok(())
}
