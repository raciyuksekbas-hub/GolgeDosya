//! Conversion orchestration: direction detection, safe output naming, source immutability,
//! atomic writes, and the versioned result contract the UI consumes.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::docx;
use crate::error::{ConvError, ErrorCode, Result};
use crate::model::Document;
use crate::udf;
use crate::warnings::{Warning, WarningSink};

/// Result contract version. Bump on any breaking change to the shape below.
pub const CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    DocxToUdf,
    UdfToDocx,
}

impl Direction {
    pub fn source_ext(self) -> &'static str {
        match self {
            Direction::DocxToUdf => "docx",
            Direction::UdfToDocx => "udf",
        }
    }

    pub fn target_ext(self) -> &'static str {
        match self {
            Direction::DocxToUdf => "udf",
            Direction::UdfToDocx => "docx",
        }
    }

    pub fn source_label(self) -> &'static str {
        match self {
            Direction::DocxToUdf => "Word (.docx)",
            Direction::UdfToDocx => "UDF (.udf)",
        }
    }

    pub fn target_label(self) -> &'static str {
        match self {
            Direction::DocxToUdf => "UDF (.udf)",
            Direction::UdfToDocx => "Word (.docx)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Success,
    Warning,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    /// Technical detail for the disclosure panel. Never contains document text.
    pub detail: String,
    pub source_untouched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionResult {
    pub contract_version: u32,
    pub status: Status,
    pub source: String,
    pub source_name: String,
    pub output: Option<String>,
    pub output_name: Option<String>,
    pub direction: Direction,
    pub source_format: String,
    pub target_format: String,
    pub warnings: Vec<Warning>,
    pub error: Option<ErrorInfo>,
    pub source_sha256_before: String,
    pub source_sha256_after: String,
    /// The release-critical invariant: the source file is byte-identical afterwards.
    pub source_unchanged: bool,
    pub elapsed_ms: u128,
}

/// Where Tavzih writes its results.
///
/// Outputs no longer land beside the source: a converted copy appearing silently next to a
/// case file is easy to mistake for the original, and it writes into folders the user may
/// not own. Everything goes to one predictable place instead.
pub fn output_root() -> PathBuf {
    documents_dir().join("Tavzih").join("Dönüştürülen Belgeler")
}

/// The user's Documents directory.
///
/// Windows exposes the profile as `USERPROFILE` and usually has no `HOME`; Unix has `HOME`.
/// Reading only `HOME` made the whole output root collapse to a relative `.\Documents\…`
/// on Windows, which wrote converted documents into whatever the working directory happened
/// to be. Kept in the core so the CLI, the tests and the app all resolve the same place.
fn documents_dir() -> PathBuf {
    let home = if cfg!(target_os = "windows") {
        std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
    } else {
        std::env::var_os("HOME")
    };
    home.map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Documents")
}

/// True when `path` ends with `Documents/Tavzih/Dönüştürülen Belgeler`.
///
/// Compares path components rather than a string suffix, because the separator differs
/// between platforms — a hardcoded `/` passes on macOS and fails on Windows.
pub fn ends_with_output_root(path: &Path) -> bool {
    let tail: Vec<String> = path
        .components()
        .rev()
        .take(3)
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    tail == ["Dönüştürülen Belgeler", "Tavzih", "Documents"]
}

/// Create the output root if it does not exist yet.
pub fn ensure_output_root() -> Result<PathBuf> {
    let d = output_root();
    std::fs::create_dir_all(&d).map_err(|e| {
        ConvError::new(
            ErrorCode::OutputWriteFailed,
            format!("{}: {e}", d.display()),
        )
    })?;
    Ok(d)
}

/// `YYYY-MM-DD_HHMM` in UTC, used when the caller supplies no local label.
///
/// The desktop shell passes its own locally-formatted stamp, because a filename showing
/// UTC would read as the wrong time to the user. The core has no timezone database and
/// deliberately does not gain a dependency for one.
pub fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days, which is exact and needs no table.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}_{:02}{:02}",
        tod / 3600,
        (tod % 3600) / 60
    )
}

/// Infer the direction from the extension, case-insensitively.
pub fn detect_direction(path: &Path) -> Result<Direction> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "docx" => Ok(Direction::DocxToUdf),
        "udf" => Ok(Direction::UdfToDocx),
        _ => Err(ConvError::new(
            ErrorCode::InvalidExtension,
            format!("unsupported extension {ext:?}"),
        )),
    }
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|e| map_read_error(path, e))?;
    Ok(sha256_bytes(&bytes))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn map_read_error(path: &Path, e: std::io::Error) -> ConvError {
    let code = match e.kind() {
        std::io::ErrorKind::NotFound => ErrorCode::InputNotFound,
        std::io::ErrorKind::PermissionDenied => ErrorCode::InputUnreadable,
        _ => ErrorCode::InputUnreadable,
    };
    ConvError::new(code, format!("{}: {e}", path.display()))
}

/// Pick an output path that never overwrites an existing file:
/// `Belge.udf`, then `Belge (2).udf`, `Belge (3).udf`, ...
pub fn unique_output_path(source: &Path, target_ext: &str) -> Result<PathBuf> {
    unique_output_path_in(&output_root(), source, target_ext)
}

/// Pick a collision-free name for `source` inside `dir`.
pub fn unique_output_path_in(dir: &Path, source: &Path, target_ext: &str) -> Result<PathBuf> {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ConvError::new(ErrorCode::OutputWriteFailed, "source has no file name"))?;

    let first = dir.join(format!("{stem}.{target_ext}"));
    if !first.exists() {
        return Ok(first);
    }
    for n in 2..=999u32 {
        let cand = dir.join(format!("{stem} ({n}).{target_ext}"));
        if !cand.exists() {
            return Ok(cand);
        }
    }
    Err(ConvError::new(
        ErrorCode::OutputExistsConflict,
        "exhausted 999 candidate output names",
    ))
}

/// Write bytes atomically: into a temporary file in the destination directory, fsync, then
/// rename into place. A partially written file can never be mistaken for a finished one.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let stem = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("tavzih");
    let tmp = dir.join(format!(".{stem}.tavzih-tmp"));

    let write = || -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(())
    };
    if let Err(e) = write() {
        let _ = std::fs::remove_file(&tmp);
        return Err(ConvError::new(
            ErrorCode::OutputWriteFailed,
            format!("{}: {e}", target.display()),
        ));
    }
    // Refuse to clobber if something appeared at the target since we chose the name.
    if target.exists() {
        let _ = std::fs::remove_file(&tmp);
        return Err(ConvError::new(
            ErrorCode::OutputExistsConflict,
            format!("{} appeared while converting", target.display()),
        ));
    }
    if let Err(e) = std::fs::rename(&tmp, target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(ConvError::new(
            ErrorCode::OutputWriteFailed,
            format!("{}: {e}", target.display()),
        ));
    }
    Ok(())
}

/// In-memory conversion. Used by the file path below and directly by tests.
pub fn convert_bytes(
    input: &[u8],
    direction: Direction,
    warn: &mut WarningSink,
) -> Result<Vec<u8>> {
    match direction {
        Direction::DocxToUdf => {
            let doc: Document = docx::reader::read_docx(input, warn)?;
            let xml = udf::writer::write_content_xml(&doc, warn)?;
            udf::writer::validate(&xml)?;
            udf::writer::package(&xml)
        }
        Direction::UdfToDocx => {
            let doc: Document = udf::reader::read_udf(input, warn)?;
            let bytes = docx::writer::write_docx(&doc, warn)?;
            docx::writer::validate(&bytes)?;
            Ok(bytes)
        }
    }
}

/// Convert a file on disk.
///
/// Invariants enforced here, in order:
///  1. the source is opened read-only and hashed **before** anything else;
///  2. conversion happens entirely in memory — the source handle is closed first;
///  3. the source is hashed **again** and compared; a mismatch aborts with `SOURCE_CHANGED`
///     and nothing is written;
///  4. the output name is chosen so no existing file is ever overwritten;
///  5. the output is validated by re-parsing it before it reaches the disk;
///  6. the write is atomic.
pub fn convert_file(source: &Path) -> ConversionResult {
    match ensure_output_root() {
        Ok(dir) => convert_file_to(source, &dir),
        Err(e) => {
            let name = source
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let dir = detect_direction(source).unwrap_or(Direction::DocxToUdf);
            failure(
                source,
                &name,
                dir,
                e,
                String::new(),
                String::new(),
                Instant::now(),
            )
        }
    }
}

/// Convert a file into an explicit output directory.
///
/// The directory is a parameter rather than a global so the behaviour is testable without
/// mutating process-wide state, and so a future "save as" can reuse the same path.
pub fn convert_file_to(source: &Path, out_dir: &Path) -> ConversionResult {
    let started = Instant::now();
    let source_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();

    let direction = match detect_direction(source) {
        Ok(d) => d,
        Err(e) => {
            return failure(
                source,
                &source_name,
                Direction::DocxToUdf,
                e,
                String::new(),
                String::new(),
                started,
            )
        }
    };

    let bytes = match std::fs::read(source) {
        Ok(b) => b,
        Err(e) => {
            return failure(
                source,
                &source_name,
                direction,
                map_read_error(source, e),
                String::new(),
                String::new(),
                started,
            )
        }
    };
    let before = sha256_bytes(&bytes);

    let mut warn = WarningSink::new();
    let out_bytes = match convert_bytes(&bytes, direction, &mut warn) {
        Ok(b) => b,
        Err(e) => {
            let after = sha256_file(source).unwrap_or_else(|_| before.clone());
            return failure(source, &source_name, direction, e, before, after, started);
        }
    };

    // Re-hash the source. Conversion is pure and in-memory, so any change came from
    // outside; continuing would mean converting something the user no longer has.
    let after = match sha256_file(source) {
        Ok(h) => h,
        Err(e) => {
            return failure(
                source,
                &source_name,
                direction,
                e,
                before.clone(),
                String::new(),
                started,
            )
        }
    };
    if after != before {
        return failure(
            source,
            &source_name,
            direction,
            ConvError::new(
                ErrorCode::SourceChanged,
                "source hash changed during conversion",
            ),
            before,
            after,
            started,
        );
    }

    if let Err(e) = std::fs::create_dir_all(out_dir) {
        return failure(
            source,
            &source_name,
            direction,
            ConvError::new(
                ErrorCode::OutputWriteFailed,
                format!("{}: {e}", out_dir.display()),
            ),
            before,
            after,
            started,
        );
    }
    let target = match unique_output_path_in(out_dir, source, direction.target_ext()) {
        Ok(p) => p,
        Err(e) => return failure(source, &source_name, direction, e, before, after, started),
    };
    if let Err(e) = write_atomic(&target, &out_bytes) {
        return failure(source, &source_name, direction, e, before, after, started);
    }

    // Only a real fidelity cost makes a conversion "warning". An expected transformation —
    // an e-signature deliberately not copied, a form staticized with its text intact — is
    // reported but does not downgrade the result, or the signal stops meaning anything.
    let worst = warn.max_severity();
    let warnings = warn.into_vec();
    ConversionResult {
        contract_version: CONTRACT_VERSION,
        status: match worst {
            None | Some(crate::warnings::Severity::Info) => Status::Success,
            Some(_) => Status::Warning,
        },
        source: source.display().to_string(),
        source_name,
        output: Some(target.display().to_string()),
        output_name: target
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string()),
        direction,
        source_format: direction.source_label().to_string(),
        target_format: direction.target_label().to_string(),
        warnings,
        error: None,
        source_sha256_before: before.clone(),
        source_sha256_after: after.clone(),
        source_unchanged: before == after,
        elapsed_ms: started.elapsed().as_millis(),
    }
}

#[allow(clippy::too_many_arguments)]
fn failure(
    source: &Path,
    source_name: &str,
    direction: Direction,
    e: ConvError,
    before: String,
    after: String,
    started: Instant,
) -> ConversionResult {
    let unchanged = before.is_empty() || after.is_empty() || before == after;
    ConversionResult {
        contract_version: CONTRACT_VERSION,
        status: Status::Failure,
        source: source.display().to_string(),
        source_name: source_name.to_string(),
        output: None,
        output_name: None,
        direction,
        source_format: direction.source_label().to_string(),
        target_format: direction.target_label().to_string(),
        warnings: vec![],
        error: Some(ErrorInfo {
            code: e.code.as_str().to_string(),
            message: e.code.message_tr().to_string(),
            detail: e.detail,
            source_untouched: e.code.source_untouched(),
        }),
        source_sha256_before: before,
        source_sha256_after: after,
        source_unchanged: unchanged,
        elapsed_ms: started.elapsed().as_millis(),
    }
}

// ============================================================ batch conversion

/// The outcome of converting a set of files as one job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub contract_version: u32,
    /// Worst outcome across the batch.
    pub status: Status,
    pub total: usize,
    pub succeeded: usize,
    pub warned: usize,
    pub failed: usize,
    /// Per-file results, in the order they were given.
    pub items: Vec<ConversionResult>,
    /// The single artifact to open: one converted file, or the ZIP.
    pub output: Option<String>,
    pub output_name: Option<String>,
    /// True when `output` is a ZIP holding several converted documents.
    pub is_zip: bool,
    /// Every source file was byte-identical afterwards. A release-critical invariant.
    pub all_sources_unchanged: bool,
    pub elapsed_ms: u128,
}

/// Convert several files as one job.
///
/// Sequential on purpose: one document at a time is predictable, keeps peak memory to a
/// single document, and makes a failure attributable. A file that fails does not abort the
/// rest — its result is recorded and the batch continues.
///
/// One file produces a plain converted document. Two or more produce a single ZIP, so the
/// user gets one artifact to hand on rather than a scattered folder.
pub fn convert_batch(sources: &[PathBuf], stamp: Option<&str>) -> BatchResult {
    let dir = match ensure_output_root() {
        Ok(d) => d,
        Err(_) => output_root(),
    };
    convert_batch_to(sources, &dir, stamp)
}

/// Batch conversion into an explicit output directory.
pub fn convert_batch_to(sources: &[PathBuf], out_dir: &Path, stamp: Option<&str>) -> BatchResult {
    let started = Instant::now();

    if sources.len() == 1 {
        let one = convert_file_to(&sources[0], out_dir);
        return BatchResult {
            contract_version: CONTRACT_VERSION,
            status: one.status,
            total: 1,
            succeeded: usize::from(one.status == Status::Success),
            warned: usize::from(one.status == Status::Warning),
            failed: usize::from(one.status == Status::Failure),
            output: one.output.clone(),
            output_name: one.output_name.clone(),
            is_zip: false,
            all_sources_unchanged: one.source_unchanged,
            elapsed_ms: started.elapsed().as_millis(),
            items: vec![one],
        };
    }

    // Convert everything first; only then decide what to package.
    let mut items: Vec<ConversionResult> = Vec::with_capacity(sources.len());
    let mut produced: Vec<(PathBuf, Direction)> = Vec::new();
    for src in sources {
        let r = convert_file_to(src, out_dir);
        if let Some(out) = &r.output {
            produced.push((PathBuf::from(out), r.direction));
        }
        items.push(r);
    }

    let succeeded = items.iter().filter(|r| r.status == Status::Success).count();
    let warned = items.iter().filter(|r| r.status == Status::Warning).count();
    let failed = items.iter().filter(|r| r.status == Status::Failure).count();
    let all_unchanged = items.iter().all(|r| r.source_unchanged);

    // A ZIP is still produced when only some files converted: the user should receive the
    // work that succeeded rather than nothing.
    let zip = if produced.is_empty() {
        None
    } else {
        // A ZIP that cannot be written leaves the individual outputs in place rather than
        // failing the whole batch: the user still has the converted documents.
        build_batch_zip(out_dir, &produced, stamp).ok()
    };

    // Once the outputs are inside the ZIP, the loose copies would be duplicates.
    if zip.is_some() {
        for (p, _) in &produced {
            let _ = std::fs::remove_file(p);
        }
    }

    let status = if failed == items.len() {
        Status::Failure
    } else if failed > 0 || warned > 0 {
        Status::Warning
    } else {
        Status::Success
    };

    BatchResult {
        contract_version: CONTRACT_VERSION,
        status,
        total: items.len(),
        succeeded,
        warned,
        failed,
        output: zip.as_ref().map(|p| p.display().to_string()),
        output_name: zip
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(|s| s.to_string()),
        is_zip: zip.is_some(),
        all_sources_unchanged: all_unchanged,
        elapsed_ms: started.elapsed().as_millis(),
        items,
    }
}

/// Package converted outputs into one ZIP under the output root.
///
/// Layout: outputs are grouped by target format, so a mixed batch is navigable.
/// Only outputs go in — never the user's source documents.
fn build_batch_zip(
    out_dir: &Path,
    produced: &[(PathBuf, Direction)],
    stamp: Option<&str>,
) -> Result<PathBuf> {
    use std::io::Write;
    let dir = out_dir.to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| {
        ConvError::new(
            ErrorCode::OutputWriteFailed,
            format!("{}: {e}", dir.display()),
        )
    })?;
    let stamp = stamp.map(|s| s.to_string()).unwrap_or_else(utc_stamp);
    let base = format!("Tavzih_Dönüşüm_{stamp}");

    let mut zip_path = dir.join(format!("{base}.zip"));
    let mut n = 2;
    while zip_path.exists() {
        zip_path = dir.join(format!("{base} ({n}).zip"));
        n += 1;
        if n > 999 {
            return Err(ConvError::new(
                ErrorCode::OutputExistsConflict,
                "exhausted candidate ZIP names",
            ));
        }
    }

    // A single target format needs no folder split, but keeping one folder is consistent
    // with the mixed case and makes the ZIP self-describing either way.
    let mut buf = Vec::new();
    {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut used: Vec<String> = Vec::new();
        for (path, dir_kind) in produced {
            let folder = dir_kind.target_ext().to_uppercase();
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("belge")
                .to_string();
            // Two source folders can contain the same file name; keep both.
            let mut entry = format!("{folder}/{name}");
            let mut k = 2;
            while used.contains(&entry) {
                let stem = name.rsplit_once('.').map(|(a, _)| a).unwrap_or(&name);
                let ext = name.rsplit_once('.').map(|(_, b)| b).unwrap_or("");
                entry = format!("{folder}/{stem} ({k}).{ext}");
                k += 1;
            }
            used.push(entry.clone());

            let bytes = std::fs::read(path).map_err(|e| {
                ConvError::new(
                    ErrorCode::OutputWriteFailed,
                    format!("{}: {e}", path.display()),
                )
            })?;
            z.start_file(entry, opts)
                .map_err(|e| ConvError::new(ErrorCode::OutputWriteFailed, e.to_string()))?;
            z.write_all(&bytes)
                .map_err(|e| ConvError::new(ErrorCode::OutputWriteFailed, e.to_string()))?;
        }
        z.finish()
            .map_err(|e| ConvError::new(ErrorCode::OutputWriteFailed, e.to_string()))?;
    }
    write_atomic(&zip_path, &buf)?;
    Ok(zip_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "tavzih-test-{tag}-{}",
            std::process::id() as u64 * 1000 + tag.len() as u64
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample_docx() -> Vec<u8> {
        let d = Document {
            meta: Metadata::default(),
            sections: vec![Section {
                blocks: vec![Block::Paragraph(Paragraph::plain(
                    "Çağdaş Türkiye Cumhuriyeti — İİK, HMK, ıİğĞüÜşŞöÖçÇ",
                ))],
                ..Default::default()
            }],
            lists: vec![],
        };
        let mut w = WarningSink::new();
        docx::writer::write_docx(&d, &mut w).unwrap()
    }

    #[test]
    fn direction_detection_is_case_insensitive() {
        assert_eq!(
            detect_direction(Path::new("a.docx")).unwrap(),
            Direction::DocxToUdf
        );
        assert_eq!(
            detect_direction(Path::new("a.DOCX")).unwrap(),
            Direction::DocxToUdf
        );
        assert_eq!(
            detect_direction(Path::new("a.Udf")).unwrap(),
            Direction::UdfToDocx
        );
        assert_eq!(
            detect_direction(Path::new("a.pdf")).unwrap_err().code,
            ErrorCode::InvalidExtension
        );
        assert_eq!(
            detect_direction(Path::new("noext")).unwrap_err().code,
            ErrorCode::InvalidExtension
        );
    }

    #[test]
    fn output_names_never_collide() {
        let d = tmpdir("collide");
        let out = d.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let src = d.join("Belge.docx");
        std::fs::write(&src, b"x").unwrap();
        let p1 = unique_output_path_in(&out, &src, "udf").unwrap();
        assert_eq!(p1.file_name().unwrap(), "Belge.udf");
        std::fs::write(&p1, b"x").unwrap();
        let p2 = unique_output_path_in(&out, &src, "udf").unwrap();
        assert_eq!(p2.file_name().unwrap(), "Belge (2).udf");
        std::fs::write(&p2, b"x").unwrap();
        let p3 = unique_output_path_in(&out, &src, "udf").unwrap();
        assert_eq!(p3.file_name().unwrap(), "Belge (3).udf");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn output_goes_to_the_given_folder_not_beside_the_source() {
        let d = tmpdir("central-src");
        let out = d.join("cikti");
        let src = d.join("Dilekçe.docx");
        std::fs::write(&src, sample_docx()).unwrap();

        let r = convert_file_to(&src, &out);
        assert_eq!(r.status, Status::Success, "{:?}", r.error);
        let produced = PathBuf::from(r.output.unwrap());
        assert!(
            produced.starts_with(&out),
            "output landed at {}",
            produced.display()
        );
        assert!(produced.exists());
        // Nothing new appeared next to the source.
        assert!(
            !d.join("Dilekçe.udf").exists(),
            "output was written beside the source"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_default_output_root_is_the_tavzih_folder() {
        let r = output_root();
        assert!(ends_with_output_root(&r), "{}", r.display());
        // A relative root would write converted documents into whatever the working
        // directory happens to be. This is what the Windows runner caught.
        assert!(
            r.is_absolute(),
            "output root must be absolute, got {}",
            r.display()
        );
    }

    #[test]
    fn utc_stamp_is_well_formed() {
        let s = utc_stamp();
        assert_eq!(s.len(), 15, "expected YYYY-MM-DD_HHMM, got {s}");
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "_");
        assert!(s[..4].parse::<u32>().unwrap() >= 2024);
    }

    #[test]
    fn source_file_is_never_modified() {
        let d = tmpdir("immutable");
        let src = d.join("Dilekçe.docx");
        std::fs::write(&src, sample_docx()).unwrap();
        let before = sha256_file(&src).unwrap();
        let mtime_before = std::fs::metadata(&src).unwrap().modified().unwrap();

        let r = convert_file_to(&src, &d.join("cikti"));
        assert_eq!(r.status, Status::Success, "{:?}", r.error);
        assert!(r.source_unchanged, "RELEASE BLOCKER: source changed");
        assert_eq!(r.source_sha256_before, before);
        assert_eq!(r.source_sha256_after, before);

        let after = sha256_file(&src).unwrap();
        assert_eq!(before, after, "RELEASE BLOCKER: source bytes differ");
        assert_eq!(
            mtime_before,
            std::fs::metadata(&src).unwrap().modified().unwrap()
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn conversion_writes_a_readable_output_and_reports_it() {
        let d = tmpdir("output");
        let src = d.join("Dava Dilekçesi.docx");
        std::fs::write(&src, sample_docx()).unwrap();
        let r = convert_file_to(&src, &d.join("cikti"));
        assert_eq!(r.status, Status::Success, "{:?}", r.error);
        let out = PathBuf::from(r.output.unwrap());
        assert_eq!(out.file_name().unwrap(), "Dava Dilekçesi.udf");
        assert!(out.exists());
        // And the produced UDF must be readable back.
        let mut w = WarningSink::new();
        let back = udf::reader::read_udf(&std::fs::read(&out).unwrap(), &mut w).unwrap();
        let Block::Paragraph(p) = &back.sections[0].blocks[0] else {
            panic!()
        };
        assert!(p.text().contains("Çağdaş Türkiye"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_existing_output_is_never_overwritten() {
        let d = tmpdir("nooverwrite");
        let out = d.join("cikti");
        std::fs::create_dir_all(&out).unwrap();
        let src = d.join("Belge.docx");
        std::fs::write(&src, sample_docx()).unwrap();
        let existing = out.join("Belge.udf");
        std::fs::write(&existing, b"ONCEDEN VAR OLAN ICERIK").unwrap();

        let r = convert_file_to(&src, &out);
        assert_eq!(r.status, Status::Success);
        assert_eq!(
            std::fs::read(&existing).unwrap(),
            b"ONCEDEN VAR OLAN ICERIK",
            "an existing user file was overwritten"
        );
        assert_eq!(
            PathBuf::from(r.output.unwrap()).file_name().unwrap(),
            "Belge (2).udf"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_corrupt_file_fails_without_writing_anything() {
        let d = tmpdir("corrupt");
        let src = d.join("Bozuk.docx");
        std::fs::write(&src, b"PK\x03\x04 this is not really a docx").unwrap();
        let r = convert_file_to(&src, &d.join("cikti"));
        assert_eq!(r.status, Status::Failure);
        assert_eq!(r.error.as_ref().unwrap().code, "INVALID_DOCX");
        assert!(r.error.as_ref().unwrap().source_untouched);
        assert!(r.output.is_none());
        assert!(
            !d.join("Bozuk.udf").exists(),
            "a failed conversion left an output behind"
        );
        // No temporary file left behind either.
        let leftovers: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tavzih-tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary file left behind");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn unsupported_extension_fails_cleanly() {
        let d = tmpdir("ext");
        let src = d.join("belge.pdf");
        std::fs::write(&src, b"%PDF-1.4").unwrap();
        let r = convert_file_to(&src, &d.join("cikti"));
        assert_eq!(r.status, Status::Failure);
        assert_eq!(r.error.unwrap().code, "INVALID_EXTENSION");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn result_serialises_to_the_documented_contract() {
        let d = tmpdir("contract");
        let src = d.join("Belge.docx");
        std::fs::write(&src, sample_docx()).unwrap();
        let r = convert_file_to(&src, &d.join("cikti"));
        let j: serde_json::Value = serde_json::to_value(&r).unwrap();
        for k in [
            "contract_version",
            "status",
            "source",
            "output",
            "direction",
            "warnings",
            "error",
            "source_sha256_before",
            "source_sha256_after",
            "source_unchanged",
            "elapsed_ms",
        ] {
            assert!(j.get(k).is_some(), "contract field {k} missing");
        }
        assert_eq!(j["contract_version"], 1);
        assert_eq!(j["direction"], "docx_to_udf");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn atomic_write_leaves_no_temp_on_success() {
        let d = tmpdir("atomic");
        let t = d.join("out.bin");
        write_atomic(&t, b"hello").unwrap();
        assert_eq!(std::fs::read(&t).unwrap(), b"hello");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
        std::fs::remove_dir_all(&d).ok();
    }
}
