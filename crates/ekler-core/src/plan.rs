use crate::*;
use sha2::{Digest, Sha256};
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SplitProposal {
    pub exhibit_id: String,
    pub exhibit_order: usize,
    pub first_part_end: usize,
    pub total_pages: usize,
    pub first_part_bytes: u64,
    pub parts: usize,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportPlan {
    pub project: Project,
    pub fingerprint: String,
    pub outputs: Vec<PhysicalOutput>,
    pub splits: Vec<SplitProposal>,
    pub warnings: Vec<String>,
}
fn fingerprint(p: &Project) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(p)?)))
}
pub fn prepare_export_plan(project: &Project) -> Result<ExportPlan> {
    let mut normalized = project.clone();
    for source in &mut normalized.sources {
        if source.format == SourceFormat::Udf {
            crate::verify_source_integrity(source)?;
            source.page_count =
                crate::office::convert_to_pdf(&source.path, source.is_approved_for_conversion)?
                    .get_pages()
                    .len();
        }
    }
    let project = &normalized;
    let temp = tempfile::tempdir().map_err(|source| EklerError::IoError {
        path: std::env::temp_dir(),
        source,
    })?;
    let result = execute_uyap_preparation(
        project,
        &ExecutionContext {
            output_dir: temp.path().into(),
        },
    )?;
    let mut splits = Vec::new();
    for exhibit in &project.exhibits {
        let outputs: Vec<_> = result
            .outputs
            .iter()
            .filter(|o| o.exhibit_id == exhibit.id)
            .collect();
        if outputs.len() > 1 {
            splits.push(SplitProposal {
                exhibit_id: exhibit.id.clone(),
                exhibit_order: exhibit.order,
                first_part_end: outputs[0].page_end,
                total_pages: outputs.iter().map(|o| o.page_count).sum(),
                first_part_bytes: outputs[0].size_bytes,
                parts: outputs.len(),
            });
        }
    }
    let warnings = project
        .sources
        .iter()
        .filter_map(|s| s.repair_note.clone())
        .collect();
    Ok(ExportPlan {
        project: project.clone(),
        fingerprint: fingerprint(project)?,
        outputs: result.outputs,
        splits,
        warnings,
    })
}
pub fn execute_export_plan(plan: &ExportPlan, ctx: &ExecutionContext) -> Result<PipelineResult> {
    if fingerprint(&plan.project)? != plan.fingerprint {
        return Err(EklerError::ValidationFailed(
            "Plan değişmiş; yeniden hesaplayın".into(),
        ));
    }
    // Source fingerprints and every invariant are revalidated by execution.
    crate::pipeline::execute_with_expected_outputs(&plan.project, ctx, Some(&plan.outputs))
}
/// Split an exhibit's source ranges at a logical page boundary. No source file is edited.
pub fn split_into_new_exhibit(
    project: &Project,
    exhibit_id: &str,
    after: usize,
) -> Result<Project> {
    let index = project
        .exhibits
        .iter()
        .position(|e| e.id == exhibit_id)
        .ok_or_else(|| EklerError::ValidationFailed("Ek bulunamadı".into()))?;
    let exhibit = &project.exhibits[index];
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut offset = 0;
    for reference in &exhibit.sources {
        let source = project
            .get_source(&reference.source_id)
            .ok_or_else(|| EklerError::ValidationFailed("Kaynak bulunamadı".into()))?;
        if source.is_signed && source.signed_policy == SignedPolicy::UseOriginalAsIs {
            return Err(EklerError::ValidationFailed(
                "İmzalı orijinal bölünemez".into(),
            ));
        }
        let (start, end) = reference.page_range.unwrap_or((1, source.page_count));
        if start == 0 || end < start || end > source.page_count {
            return Err(EklerError::ValidationFailed(
                "Geçersiz sayfa aralığı".into(),
            ));
        }
        let count = end - start + 1;
        let in_left = after.saturating_sub(offset).min(count);
        if in_left > 0 {
            left.push(ExhibitSourceRef {
                source_id: source.id.clone(),
                page_range: Some((start, start + in_left - 1)),
            });
        }
        if in_left < count {
            right.push(ExhibitSourceRef {
                source_id: source.id.clone(),
                page_range: Some((start + in_left, end)),
            });
        }
        offset += count;
    }
    if left.is_empty() || right.is_empty() {
        return Err(EklerError::ValidationFailed(
            "Bölme sınırı Ekin içinde olmalı".into(),
        ));
    }
    let mut result = project.clone();
    result.exhibits[index].sources = left;
    let base = format!("{}-split-{}", exhibit.id, after);
    let mut id = base.clone();
    let mut n = 2;
    while result.exhibits.iter().any(|e| e.id == id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    result.exhibits.insert(
        index + 1,
        LogicalExhibit {
            id,
            order: index + 2,
            name: format!("{} (ayrılan sayfalar)", exhibit.name),
            sources: right,
        },
    );
    result.renumber_exhibits();
    Ok(result)
}
