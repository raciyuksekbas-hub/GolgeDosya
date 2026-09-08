use crate::accounting::{calculate_sha256, verify_page_accounting, verify_source_integrity};
use crate::error::{EklerError, Result};
use crate::exhibits_list::ExhibitsListGenerator;
use crate::image::image_file_to_pdf;
use crate::model::*;
use crate::pdf::{
    extract_page_range, load_pdf_tolerant, merge_documents, stamp::apply_stamp_to_document,
};
use crate::validation::{validate_project_and_outputs, ValidationReport};
use lopdf::Document as LopdfDoc;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineResult {
    pub normalized_pages: Vec<NormalizedPage>,
    pub package_dir: PathBuf,
    pub validation_report: ValidationReport,
    pub outputs: Vec<PhysicalOutput>,
    pub exhibits_list_plain: String,
    pub exhibits_list_markdown: String,
}

pub struct ExecutionContext {
    pub output_dir: PathBuf,
}

/// Prepare and verify every artifact before one no-clobber package commit.
pub fn execute_uyap_preparation(
    project: &Project,
    ctx: &ExecutionContext,
) -> Result<PipelineResult> {
    execute_with_expected_outputs(project, ctx, None)
}

pub(crate) fn execute_with_expected_outputs(
    project: &Project,
    ctx: &ExecutionContext,
    expected: Option<&[PhysicalOutput]>,
) -> Result<PipelineResult> {
    crate::persistence::validate_project_structure(project)?;
    if project.exhibits.is_empty() || project.exhibits.iter().any(|e| e.sources.is_empty()) {
        return Err(EklerError::ValidationFailed(
            "Her Ek en az bir belge içermeli".into(),
        ));
    }
    for source in &project.sources {
        verify_source_integrity(source)?;
        if SourceFormat::from_path(&source.path) != source.format {
            return Err(EklerError::ValidationFailed(
                "Kaynak biçimi proje kaydıyla uyuşmuyor".into(),
            ));
        }
        let signed = match source.format {
            SourceFormat::Pdf => crate::pdf::inspect_pdf(&source.path)?.is_signed,
            SourceFormat::Udf => crate::inspect_udf(&source.path)?.is_signed,
            _ => false,
        };
        if signed && !source.is_signed {
            return Err(EklerError::ValidationFailed(
                "Kaynağın imza bilgisi proje kaydıyla uyuşmuyor; belgeyi yeniden ekleyin".into(),
            ));
        }
    }
    std::fs::create_dir_all(&ctx.output_dir).map_err(|source| EklerError::IoError {
        path: ctx.output_dir.clone(),
        source,
    })?;
    let staging = tempfile::Builder::new()
        .prefix(".duzenek-export-")
        .suffix(".tmp")
        .tempdir_in(&ctx.output_dir)
        .map_err(|source| EklerError::IoError {
            path: ctx.output_dir.clone(),
            source,
        })?;
    let staged = run_pipeline_in_staging(project, staging.path())?;
    for source in &project.sources {
        verify_source_integrity(source)?;
    }
    let validation = validate_project_and_outputs(project, &staged.outputs);
    if !validation.is_ready_for_uyap {
        return Err(EklerError::ValidationFailed(
            validation
                .items
                .iter()
                .filter(|i| i.level == crate::validation::CheckLevel::Error)
                .map(|i| i.description.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    if let Some(expected) = expected {
        if expected.len() != staged.outputs.len()
            || expected.iter().zip(&staged.outputs).any(|(a, b)| {
                a.file_name != b.file_name
                    || a.exhibit_id != b.exhibit_id
                    || a.page_start != b.page_start
                    || a.page_end != b.page_end
                    || a.page_count != b.page_count
                    || b.size_bytes > a.size_bytes
            })
        {
            return Err(EklerError::ValidationFailed(
                "Çıktı onaylanan planla uyuşmuyor; planı yeniden hesaplayın. Paket kaydedilmedi."
                    .into(),
            ));
        }
    }
    let suffix = staging
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace(".duzenek-export-", "")
        .replace(".tmp", "");
    let package_dir = ctx.output_dir.join(format!(
        "DuzenEk-{}-{}",
        sanitize_for_filename(&project.name),
        suffix
    ));
    let manifest = serde_json::to_vec_pretty(
        &serde_json::json!({"schema_version":1,"outputs":staged.outputs,"normalized_pages":staged.normalized_pages,"sources":project.sources.iter().map(|s|serde_json::json!({"id":s.id,"file_name":s.file_name,"sha256":s.sha256_before})).collect::<Vec<_>>()}),
    )?;
    crate::safe_io::write_new_bytes(&staging.path().join("manifest.json"), &[], &manifest)?;
    #[cfg(unix)]
    File::open(staging.path())
        .and_then(|f| f.sync_all())
        .map_err(|source| EklerError::IoError {
            path: staging.path().into(),
            source,
        })?;
    crate::safe_io::publish_directory(staging.path(), &package_dir)?;
    Ok(PipelineResult {
        normalized_pages: staged.normalized_pages,
        package_dir,
        validation_report: validation,
        outputs: staged.outputs,
        exhibits_list_plain: staged.list_plain,
        exhibits_list_markdown: staged.list_md,
    })
}

struct StagedExecutionResult {
    outputs: Vec<PhysicalOutput>,
    normalized_pages: Vec<NormalizedPage>,
    list_plain: String,
    list_md: String,
}

fn run_pipeline_in_staging(project: &Project, staging_dir: &Path) -> Result<StagedExecutionResult> {
    let mut all_outputs = Vec::new();
    let mut all_normalized_pages = Vec::new();
    let mut exhibit_page_counts = Vec::new();

    let max_digits = project.exhibits.len().to_string().len().max(2);

    for exhibit in &project.exhibits {
        let mut exhibit_docs = Vec::new();
        let mut provenance = Vec::new();

        for source_ref in &exhibit.sources {
            let source = project
                .get_source(&source_ref.source_id)
                .ok_or_else(|| EklerError::FileNotFound(PathBuf::from(&source_ref.source_id)))?;

            if source.is_signed
                && source.signed_policy == SignedPolicy::CreateDerivedCopy
                && !source.is_approved_for_conversion
            {
                return Err(EklerError::ValidationFailed(format!(
                    "Türetilmiş kopya için açık onay gerekli: {}",
                    source.file_name
                )));
            }
            // Original PDFs are indivisible and must occupy their own exhibit.
            if source.is_signed && source.signed_policy == SignedPolicy::UseOriginalAsIs {
                if source.format != SourceFormat::Pdf
                    || source_ref.page_range.is_some()
                    || exhibit.sources.len() != 1
                {
                    return Err(EklerError::ValidationFailed("İmzalı orijinal PDF tek başına bir Ek olmalı ve sayfa aralığı seçilmemeli. Birleştirmek için onaylı türetilmiş kopya kullanın.".into()));
                }
                let stem = Path::new(&source.file_name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Belge");
                let clean_src_name = format!("{}.pdf", crate::model::sanitize_for_filename(stem));
                let out_name = format!(
                    "{}_{}",
                    exhibit.file_stem_prefix(max_digits),
                    clean_src_name
                );
                let staged_path = staging_dir.join(&out_name);
                let original =
                    std::fs::read(&source.path).map_err(|source_error| EklerError::IoError {
                        path: source.path.clone(),
                        source: source_error,
                    })?;
                let checked = load_pdf_tolerant(&original, &source.file_name)?;
                if checked.document.get_pages().len() != source.page_count {
                    return Err(EklerError::ValidationFailed(
                        "İmzalı PDF sayfa sayısı değişmiş".into(),
                    ));
                }
                crate::safe_io::write_new_bytes(
                    &staged_path,
                    std::slice::from_ref(&source.path),
                    &original,
                )?;
                let size = original.len() as u64;
                let hash =
                    calculate_sha256(&staged_path).map_err(|source_error| EklerError::IoError {
                        path: staged_path.clone(),
                        source: source_error,
                    })?;
                if hash != source.sha256_before {
                    return Err(EklerError::SourceIntegrityCompromised {
                        path: source.path.clone(),
                    });
                }
                exhibit_page_counts.push((exhibit.id.clone(), source.page_count));
                for (index, id) in checked.document.get_pages().values().enumerate() {
                    let (width_pt, height_pt) =
                        crate::pdf::get_page_dimensions(&checked.document, *id)
                            .ok_or_else(|| EklerError::InvalidPdf("Sayfa boyutu yok".into()))?;
                    all_normalized_pages.push(NormalizedPage {
                        source_id: source.id.clone(),
                        source_page_index: index + 1,
                        exhibit_id: exhibit.id.clone(),
                        exhibit_page_number: index + 1,
                        total_exhibit_pages: source.page_count,
                        width_pt,
                        height_pt,
                        orientation: if width_pt > height_pt {
                            PageOrientation::Landscape
                        } else {
                            PageOrientation::Portrait
                        },
                        is_blank: false,
                    });
                }
                all_outputs.push(PhysicalOutput {
                    file_name: out_name,
                    exhibit_id: exhibit.id.clone(),
                    exhibit_order: exhibit.order,
                    is_continuation: false,
                    continuation_index: 0,
                    page_start: 1,
                    page_end: source.page_count,
                    page_count: source.page_count,
                    size_bytes: size,
                    sha256: hash,
                });
                continue;
            }

            // Kaynağı ara PDF formatına dönüştür / yükle (Çok katmanlı toleranslı yükleme)
            let mut doc = if source.format == SourceFormat::Pdf {
                let bytes = std::fs::read(&source.path).map_err(|e| EklerError::IoError {
                    path: source.path.clone(),
                    source: e,
                })?;
                load_pdf_tolerant(&bytes, &source.file_name)?.document
            } else if matches!(
                source.format,
                SourceFormat::Doc | SourceFormat::Docx | SourceFormat::Udf
            ) {
                crate::office::convert_to_pdf(&source.path, source.is_approved_for_conversion)?
            } else if source.format.is_image() {
                image_file_to_pdf(&source.path)?
            } else {
                return Err(EklerError::UnsupportedFormat(format!(
                    "{:?} henüz doğrudan PDF hattına bağlanmadı",
                    source.format
                )));
            };

            // Sayfa alt aralığı varsa filtrele
            if let Some((start, end)) = source_ref.page_range {
                doc = extract_page_range(&doc, start, end)?;
            }

            let source_start = source_ref.page_range.map(|(s, _)| s).unwrap_or(1);
            for (index, id) in doc.get_pages().values().enumerate() {
                let (w, h) = crate::pdf::get_page_dimensions(&doc, *id)
                    .ok_or_else(|| EklerError::InvalidPdf("Sayfa boyutu okunamadı".into()))?;
                provenance.push((source.id.clone(), source_start + index, w, h));
            }
            exhibit_docs.push(doc);
        }

        if exhibit_docs.is_empty() {
            verify_page_accounting(&exhibit.id, &all_normalized_pages, &all_outputs)?;
            continue;
        }

        // Ek altındaki tüm kaynakları tek bir mantıksal dokümanda birleştir
        let mut unified_doc = if exhibit_docs.len() == 1 {
            exhibit_docs.remove(0)
        } else {
            merge_documents(&exhibit_docs)?
        };

        crate::optimizer::optimize_pdf(
            &mut unified_doc,
            crate::optimizer::OptimizationLevel::LowRiskCleanup,
        )?;
        if project.optimization != crate::optimizer::OptimizationLevel::LowRiskCleanup {
            let count = unified_doc.get_pages().len();
            if measure_candidate_size(&unified_doc, exhibit.order, 1, count, &project.stamp_config)?
                > project.target_size_bytes
            {
                crate::optimizer::optimize_pdf(&mut unified_doc, project.optimization)?;
            }
        }
        let total_exhibit_pages = unified_doc.get_pages().len();
        exhibit_page_counts.push((exhibit.id.clone(), total_exhibit_pages));

        // Normalleştirilmiş sayfa kayıtlarını oluştur
        for (index, (source_id, source_page_index, width_pt, height_pt)) in
            provenance.into_iter().enumerate()
        {
            let p_idx = index + 1;
            all_normalized_pages.push(NormalizedPage {
                source_id,
                source_page_index,
                exhibit_id: exhibit.id.clone(),
                exhibit_page_number: p_idx,
                total_exhibit_pages,
                width_pt,
                height_pt,
                orientation: if width_pt > height_pt {
                    PageOrientation::Landscape
                } else {
                    PageOrientation::Portrait
                },
                is_blank: false,
            });
        }

        // Akıllı bölme ve damgalama ile çıktıları staging içine oluştur
        let exhibit_outputs = process_exhibit_with_smart_split(
            &unified_doc,
            exhibit,
            max_digits,
            project.target_size_bytes,
            &project.stamp_config,
            staging_dir,
        )?;

        all_outputs.extend(exhibit_outputs);

        // Strict Page Accounting: Ek düzeyinde sayfa mutabakatı
        verify_page_accounting(&exhibit.id, &all_normalized_pages, &all_outputs)?;
    }

    // Ekler Listesi oluştur ve staging içine yaz
    let list_plain = ExhibitsListGenerator::generate_plain_text(project, &exhibit_page_counts);
    let list_md = ExhibitsListGenerator::generate_markdown(project, &exhibit_page_counts);

    let list_path = staging_dir.join("EKLER_LISTESI.txt");
    let mut f = File::create(&list_path).map_err(|e| EklerError::IoError {
        path: list_path.clone(),
        source: e,
    })?;
    f.write_all(list_plain.as_bytes())
        .map_err(|e| EklerError::IoError {
            path: list_path.clone(),
            source: e,
        })?;
    f.flush().map_err(|e| EklerError::IoError {
        path: list_path.clone(),
        source: e,
    })?;
    f.sync_all().map_err(|e| EklerError::IoError {
        path: list_path.clone(),
        source: e,
    })?;
    drop(f);

    Ok(StagedExecutionResult {
        outputs: all_outputs,
        normalized_pages: all_normalized_pages,
        list_plain,
        list_md,
    })
}

fn process_exhibit_with_smart_split(
    unified_doc: &LopdfDoc,
    exhibit: &LogicalExhibit,
    max_digits: usize,
    target_size_bytes: u64,
    stamp_config: &StampConfig,
    output_dir: &Path,
) -> Result<Vec<PhysicalOutput>> {
    let total_pages = unified_doc.get_pages().len();
    let mut outputs = Vec::new();

    let mut current_page = 1;
    let mut continuation_idx = 0;

    while current_page <= total_pages {
        // En büyük uygun sayfa aralığını bul (current_page ..= best_end)
        let best_end = find_maximum_fitting_range(
            unified_doc,
            exhibit.order,
            current_page,
            total_pages,
            target_size_bytes,
            stamp_config,
        )?;

        // Çıktı belgesini üret ve damgala
        let mut slice_doc = extract_page_range(unified_doc, current_page, best_end)?;
        apply_stamp_to_document(&mut slice_doc, exhibit.order, current_page, stamp_config)?;
        crate::pdf::stamp::apply_branding(&mut slice_doc)?;

        // Dosya adını oluştur:
        // EK-01_Sozlesme_S001-S018.pdf veya EK-01_DEVAM_Sozlesme_S019-S020.pdf
        let file_stem = exhibit.file_stem_prefix(max_digits);
        let file_name = if continuation_idx == 0 {
            format!("{}_S{:03}-S{:03}.pdf", file_stem, current_page, best_end)
        } else {
            let part_label = if continuation_idx == 1 {
                "DEVAM".to_string()
            } else {
                format!("DEVAM_{}", continuation_idx)
            };
            format!(
                "{}_{}_S{:03}-S{:03}.pdf",
                file_stem, part_label, current_page, best_end
            )
        };

        let out_path = output_dir.join(&file_name);
        let mut out_file = File::create(&out_path).map_err(|e| EklerError::IoError {
            path: out_path.clone(),
            source: e,
        })?;

        slice_doc
            .save_to(&mut out_file)
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        out_file.flush().map_err(|e| EklerError::IoError {
            path: out_path.clone(),
            source: e,
        })?;
        out_file.sync_all().map_err(|e| EklerError::IoError {
            path: out_path.clone(),
            source: e,
        })?;
        drop(out_file);

        // Strict Post-Write Invariant Validation
        let actual_size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
        if actual_size == 0 {
            return Err(EklerError::InvalidPdf(format!(
                "Üretilen '{}' PDF dosyası 0 bayt (boş) olduğu için dışa aktarma reddedildi.",
                file_name
            )));
        }

        let verified_doc = LopdfDoc::load(&out_path).map_err(|e| {
            EklerError::InvalidPdf(format!(
                "Üretilen '{}' PDF çıktısı doğrulanamadı (geçersiz yapı): {}",
                file_name, e
            ))
        })?;
        crate::pdf::validate_document(&verified_doc)?;
        let expected_page_count = best_end - current_page + 1;
        let actual_page_count = verified_doc.get_pages().len();
        if actual_page_count != expected_page_count {
            return Err(EklerError::InvalidPdf(format!(
                "Üretilen '{}' sayfa sayısı uyuşmazlığı: beklenen {}, üretilen {}.",
                file_name, expected_page_count, actual_page_count
            )));
        }

        let hash = calculate_sha256(&out_path).map_err(|source| EklerError::IoError {
            path: out_path.clone(),
            source,
        })?;

        outputs.push(PhysicalOutput {
            file_name,
            exhibit_id: exhibit.id.clone(),
            exhibit_order: exhibit.order,
            is_continuation: continuation_idx > 0,
            continuation_index: continuation_idx,
            page_start: current_page,
            page_end: best_end,
            page_count: expected_page_count,
            size_bytes: actual_size,
            sha256: hash,
        });

        current_page = best_end + 1;
        continuation_idx += 1;
    }

    Ok(outputs)
}

/// Binary search ile 9.5 MB güvenli tavanı aşmayan en büyük sayfa aralığını bulur
fn find_maximum_fitting_range(
    doc: &LopdfDoc,
    exhibit_order: usize,
    start_page: usize,
    total_pages: usize,
    target_size_bytes: u64,
    stamp_config: &StampConfig,
) -> Result<usize> {
    // Önce tüm kalan sayfaları dene
    let candidate_size =
        measure_candidate_size(doc, exhibit_order, start_page, total_pages, stamp_config)?;
    if candidate_size <= target_size_bytes || start_page == total_pages {
        return Ok(total_pages);
    }

    // Hedefi aşıyorsa binary search ile sınırlandır
    let mut low = start_page;
    let mut high = total_pages - 1;
    let mut best = start_page;

    while low <= high {
        let mid = low + (high - low) / 2;
        let size = measure_candidate_size(doc, exhibit_order, start_page, mid, stamp_config)?;

        if size <= target_size_bytes {
            best = mid;
            low = mid + 1; // Daha fazla sayfa sığabilir mi?
        } else {
            if mid == start_page {
                // Tek sayfa bile sınırı aşıyorsa mecburen 1 sayfa üretilir
                best = start_page;
                break;
            }
            high = mid - 1;
        }
    }

    Ok(best)
}

fn measure_candidate_size(
    doc: &LopdfDoc,
    exhibit_order: usize,
    start_page: usize,
    end_page: usize,
    stamp_config: &StampConfig,
) -> Result<u64> {
    let mut slice_doc = extract_page_range(doc, start_page, end_page)?;
    apply_stamp_to_document(&mut slice_doc, exhibit_order, start_page, stamp_config)?;
    crate::pdf::stamp::apply_branding(&mut slice_doc)?;

    let mut buf = Vec::new();
    slice_doc
        .save_to(&mut buf)
        .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
    Ok(buf.len() as u64)
}
