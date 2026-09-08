use crate::error::{EklerError, Result};
use crate::model::{NormalizedPage, PhysicalOutput, SourceFile};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Result as IoResult};
use std::path::Path;

/// Dosyanın SHA-256 özetini (hex string) hesaplar.
pub fn calculate_sha256(path: &Path) -> IoResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Kaynak dosyanın mutasyona uğramadığını doğrular:
/// SHA-256(source_before) == SHA-256(source_after)
pub fn verify_source_integrity(source: &SourceFile) -> Result<()> {
    if !source.path.exists() {
        return Err(EklerError::FileNotFound(source.path.clone()));
    }
    let current_sha256 = calculate_sha256(&source.path).map_err(|e| EklerError::IoError {
        path: source.path.clone(),
        source: e,
    })?;

    if current_sha256 != source.sha256_before {
        return Err(EklerError::SourceIntegrityCompromised {
            path: source.path.clone(),
        });
    }
    Ok(())
}

/// Strict Page Accounting (Katı Sayfa Muhasebesi)
///
/// Bir Ek için kaynak sayfa sayısı ile fiziksel çıktılardaki toplam sayfa sayısının
/// birebir eşit olduğunu, hiçbir sayfanın kaybolmadığını ve mükerrer basılmadığını doğrular.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageAccountingReport {
    pub exhibit_id: String,
    pub input_page_count: usize,
    pub output_page_count: usize,
    pub missing_pages: Vec<usize>,
    pub duplicate_pages: Vec<usize>,
    pub is_valid: bool,
}

pub fn verify_page_accounting(
    exhibit_id: &str,
    normalized_pages: &[NormalizedPage],
    outputs: &[PhysicalOutput],
) -> Result<PageAccountingReport> {
    let input_pages_for_exhibit: Vec<&NormalizedPage> = normalized_pages
        .iter()
        .filter(|p| p.exhibit_id == exhibit_id)
        .collect();

    let input_count = input_pages_for_exhibit.len();

    let mut output_count = 0;
    let mut covered_pages = std::collections::HashSet::new();
    let mut duplicate_pages = Vec::new();

    for out in outputs.iter().filter(|o| o.exhibit_id == exhibit_id) {
        output_count += out.page_count;
        for page_num in out.page_start..=out.page_end {
            if !covered_pages.insert(page_num) {
                duplicate_pages.push(page_num);
            }
        }
    }

    let mut missing_pages = Vec::new();
    for i in 1..=input_count {
        if !covered_pages.contains(&i) {
            missing_pages.push(i);
        }
    }

    let is_valid =
        input_count == output_count && missing_pages.is_empty() && duplicate_pages.is_empty();

    let report = PageAccountingReport {
        exhibit_id: exhibit_id.to_string(),
        input_page_count: input_count,
        output_page_count: output_count,
        missing_pages,
        duplicate_pages,
        is_valid,
    };

    if !is_valid {
        return Err(EklerError::PageAccountingMismatch {
            expected: input_count,
            actual: output_count,
        });
    }

    Ok(report)
}
