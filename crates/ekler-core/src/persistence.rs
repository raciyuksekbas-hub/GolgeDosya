use crate::*;
use std::collections::HashSet;
use std::path::Path;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceIssue {
    pub source_id: String,
    pub file_name: String,
    pub message: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectLoadReport {
    pub project: Project,
    pub issues: Vec<SourceIssue>,
}
pub fn validate_project_structure(p: &Project) -> Result<()> {
    if p.version != "1.0.0" {
        return Err(EklerError::ValidationFailed(format!(
            "Desteklenmeyen proje sürümü: {}",
            p.version
        )));
    }
    if p.target_size_bytes == 0
        || !p.stamp_config.font_size.is_finite()
        || !(4.0..=72.0).contains(&p.stamp_config.font_size)
        || !p.stamp_config.margin_pt.is_finite()
        || p.stamp_config.margin_pt < 0.0
    {
        return Err(EklerError::ValidationFailed(
            "Geçersiz boyut veya damga ayarı".into(),
        ));
    }
    let mut ids = HashSet::new();
    for s in &p.sources {
        if s.id.is_empty() || !ids.insert(&s.id) {
            return Err(EklerError::ValidationFailed(
                "Mükerrer veya boş kaynak kimliği".into(),
            ));
        }
    }
    let mut ids = HashSet::new();
    for (i, e) in p.exhibits.iter().enumerate() {
        if e.id.is_empty() || !ids.insert(&e.id) || e.order != i + 1 {
            return Err(EklerError::ValidationFailed(
                "Ek kimliği veya sırası tutarsız".into(),
            ));
        }
        for r in &e.sources {
            let s = p.get_source(&r.source_id).ok_or_else(|| {
                EklerError::ValidationFailed("Ek kaynak referansı çözümlenemiyor".into())
            })?;
            if let Some((start, end)) = r.page_range {
                if start == 0 || end < start || end > s.page_count {
                    return Err(EklerError::ValidationFailed(
                        "Geçersiz kaynak sayfa aralığı".into(),
                    ));
                }
            }
        }
    }
    Ok(())
}
pub fn source_issues(p: &Project) -> Vec<SourceIssue> {
    p.sources
        .iter()
        .filter_map(|s| {
            verify_source_integrity(s).err().map(|e| SourceIssue {
                source_id: s.id.clone(),
                file_name: s.file_name.clone(),
                message: e.to_string(),
            })
        })
        .collect()
}
pub fn load_project(path: &Path) -> Result<ProjectLoadReport> {
    let bytes = std::fs::read(path).map_err(|source| EklerError::IoError {
        path: path.into(),
        source,
    })?;
    let project: Project = serde_json::from_slice(&bytes)?;
    validate_project_structure(&project)?;
    let issues = source_issues(&project);
    Ok(ProjectLoadReport { project, issues })
}
pub fn save_project(project: &Project, path: &Path) -> Result<()> {
    validate_project_structure(project)?;
    safe_io::write_new_bytes(
        path,
        &project
            .sources
            .iter()
            .map(|s| s.path.clone())
            .collect::<Vec<_>>(),
        &serde_json::to_vec_pretty(project)?,
    )
}
pub fn relink_source(project: &Project, source_id: &str, path: &Path) -> Result<ProjectLoadReport> {
    let expected = project
        .get_source(source_id)
        .ok_or_else(|| EklerError::ValidationFailed("Kaynak bulunamadı".into()))?;
    let hash = calculate_sha256(path).map_err(|source| EklerError::IoError {
        path: path.into(),
        source,
    })?;
    if hash != expected.sha256_before {
        return Err(EklerError::ValidationFailed("Seçilen belge orijinal kaynakla aynı değil. Değişmiş belgeyi yeni belge olarak ekleyin.".into()));
    }
    let mut updated = project.clone();
    let source = updated
        .sources
        .iter_mut()
        .find(|s| s.id == source_id)
        .unwrap();
    source.path = path.canonicalize().map_err(|source| EklerError::IoError {
        path: path.into(),
        source,
    })?;
    source.file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into();
    let issues = source_issues(&updated);
    Ok(ProjectLoadReport {
        project: updated,
        issues,
    })
}
