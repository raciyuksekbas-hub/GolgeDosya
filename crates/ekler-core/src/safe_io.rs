//! Shared no-clobber publication. Existing files (including aliases of sources)
//! are never overwritten. Staging stays on the destination filesystem.
//!
//! PDF ÇIKIŞ SINIRI. Ürün kuralı: GölgeDosya'nın ürettiği ya da değiştirdiği
//! her PDF, canonical GölgeDosya işaretini taşır. Bu, araç geliştiricisinin
//! hatırlamasına bırakılmaz: genel yazıcı (`write_new_file`,
//! `write_new_bytes`) PDF baytını REDDEDER. PDF yalnız iki yoldan çıkar:
//!
//! * `publish_pdf` — yalnız `finalize_pdf_output`'un ürettiği `BrandedPdf`'i
//!   kabul eder (işaretlenmiş, yeniden açılıp doğrulanmış baytlar);
//! * `publish_signed_original` — kuralın tek istisnası: imzalı bir PDF'in
//!   bayt bayt kopyası (GölgeDosya onu üretmez, değiştirmez; işaret e-imzayı
//!   bozardı). Baytların kaynakla aynı ve kaynağın imzalı olduğu doğrulanır.
//!
//! Yerel renderer'ın geçici çalışma kopyası (`write_render_scratch`) bir
//! `TempDir` ister: kullanıcıya teslim edilen bir dosya olamaz.
use crate::pdf::stamp::BrandedPdf;
use crate::{EklerError, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

fn io(path: &Path, source: std::io::Error) -> EklerError {
    EklerError::IoError {
        path: path.into(),
        source,
    }
}

pub fn ensure_new_destination(destination: &Path, sources: &[PathBuf]) -> Result<()> {
    // symlink_metadata also catches dangling symlinks; no-clobber is deliberately
    // stronger than an identity comparison and covers hardlinks on every OS.
    match std::fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(EklerError::ValidationFailed(format!(
                "Hedef zaten mevcut; üzerine yazılmaz: {}",
                destination.display()
            )))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(destination, e)),
    }
    let parent = destination.parent().unwrap_or(Path::new("."));
    let canonical_parent = parent.canonicalize().map_err(|e| io(parent, e))?;
    let name = destination
        .file_name()
        .ok_or_else(|| EklerError::ValidationFailed("Geçersiz hedef".into()))?;
    let resolved = canonical_parent.join(name);
    for source in sources {
        let canonical_source = source.canonicalize().map_err(|e| io(source, e))?;
        if canonical_source == resolved {
            return Err(EklerError::ValidationFailed(
                "Kaynağın üzerine yazılamaz".into(),
            ));
        }
    }
    Ok(())
}

/// Yazılan içeriğin türü. Genel yazıcıdan PDF geçemez.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Payload {
    NotPdf,
    Pdf,
}

/// lopdf'in yazdığı her PDF ilk bayttan `%PDF-` ile başlar. Yalnız başa
/// bakılır: içinde bu diziyi ANAN bir metin (ör. bir belgenin Markdown
/// dökümü) yanlışlıkla reddedilmesin.
fn starts_like_pdf(file: &mut std::fs::File) -> std::io::Result<bool> {
    use std::io::{Read, Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    let mut head = [0u8; 5];
    let mut read = 0;
    while read < head.len() {
        match file.read(&mut head[read..])? {
            0 => break,
            n => read += n,
        }
    }
    Ok(read == head.len() && &head == b"%PDF-")
}

fn persist_new<F>(destination: &Path, sources: &[PathBuf], write: F, payload: Payload) -> Result<()>
where
    F: FnOnce(&mut std::fs::File) -> Result<()>,
{
    ensure_new_destination(destination, sources)?;
    let parent = destination.parent().unwrap_or(Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| io(parent, e))?;
    write(staged.as_file_mut())?;
    staged
        .as_file_mut()
        .flush()
        .map_err(|e| io(destination, e))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|e| io(destination, e))?;
    // Denetim YAYIMDAN ÖNCE ve yazılmış baytlar üzerinde: kapanış hangi yoldan
    // gelirse gelsin (bayt dizisi ya da yazıcı işlev) aynı kural uygulanır.
    // Reddedilen geçici dosya düşürülünce silinir; hedefte hiçbir şey kalmaz.
    if payload == Payload::NotPdf
        && starts_like_pdf(staged.as_file_mut()).map_err(|e| io(destination, e))?
    {
        return Err(EklerError::ValidationFailed(
            "PDF yalnız GölgeDosya işaret kapısından geçerek yayımlanabilir".into(),
        ));
    }
    staged
        .persist_noclobber(destination)
        .map_err(|e| io(destination, e.error))?;
    Ok(())
}

/// Genel, üzerine-yazmayan yazıcı. PDF baytını reddeder; PDF için
/// `publish_pdf` ya da `publish_signed_original` kullanılır.
pub fn write_new_file<F>(destination: &Path, sources: &[PathBuf], write: F) -> Result<()>
where
    F: FnOnce(&mut std::fs::File) -> Result<()>,
{
    persist_new(destination, sources, write, Payload::NotPdf)
}

pub fn write_new_bytes(destination: &Path, sources: &[PathBuf], bytes: &[u8]) -> Result<()> {
    write_new_file(destination, sources, |f| {
        f.write_all(bytes).map_err(|e| io(destination, e))
    })
}

/// GölgeDosya'nın ürettiği ya da değiştirdiği PDF'in TEK yayın yolu.
pub fn publish_pdf(destination: &Path, sources: &[PathBuf], pdf: &BrandedPdf) -> Result<()> {
    persist_new(
        destination,
        sources,
        |f| f.write_all(pdf.bytes()).map_err(|e| io(destination, e)),
        Payload::Pdf,
    )
}

/// Kuralın tek istisnası: imzalı bir PDF'in BAYT BAYT kopyası.
///
/// GölgeDosya bu dosyayı üretmez ve değiştirmez; üstüne işaret basmak
/// e-imzayı geçersiz kılar ve UYAP'taki delil değerini yok eder. İstisnanın
/// boşluğa dönüşmemesi için yazmadan önce doğrulanır: baytlar diskteki
/// kaynakla birebir aynı ve kaynak gerçekten imza taşıyor.
pub fn publish_signed_original(destination: &Path, source: &Path, bytes: &[u8]) -> Result<()> {
    let on_disk = std::fs::read(source).map_err(|e| io(source, e))?;
    if on_disk != bytes {
        return Err(EklerError::ValidationFailed(
            "İmzalı orijinal kaynakla bayt bayt aynı değil; işaretsiz yayımlanamaz".into(),
        ));
    }
    if !crate::pdf::inspect_pdf(source)?.is_signed {
        return Err(EklerError::ValidationFailed(
            "Kaynak imza taşımıyor; GölgeDosya işareti olmadan yayımlanamaz".into(),
        ));
    }
    persist_new(
        destination,
        &[source.to_path_buf()],
        |f| f.write_all(bytes).map_err(|e| io(destination, e)),
        Payload::Pdf,
    )
}

/// Yerel renderer'a verilecek GEÇİCİ çalışma kopyası.
///
/// Yalnız bir `TempDir` içine, düz bir dosya adıyla yazar: tür, dosyanın
/// kullanıcıya teslim edilmediğini ve dizinle birlikte silineceğini kanıtlar.
pub fn write_render_scratch(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    if Path::new(name).file_name().and_then(|n| n.to_str()) != Some(name) {
        return Err(EklerError::ValidationFailed(
            "Geçersiz geçici dosya adı".into(),
        ));
    }
    let path = dir.path().join(name);
    persist_new(
        &path,
        &[],
        |f| f.write_all(bytes).map_err(|e| io(&path, e)),
        Payload::Pdf,
    )?;
    Ok(path)
}

/// A single no-replace directory rename publishes an entire package. No copy
/// fallback: a filesystem that cannot provide this contract returns an error.
pub fn publish_directory(staged: &Path, destination: &Path) -> Result<()> {
    ensure_new_destination(destination, &[])?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::os::unix::ffi::OsStrExt;
        let from = std::ffi::CString::new(staged.as_os_str().as_bytes())
            .map_err(|_| EklerError::ValidationFailed("Geçersiz yol".into()))?;
        let to = std::ffi::CString::new(destination.as_os_str().as_bytes())
            .map_err(|_| EklerError::ValidationFailed("Geçersiz yol".into()))?;
        // SAFETY: both C strings are NUL-terminated and live throughout the call.
        #[cfg(target_os = "macos")]
        let status = unsafe {
            libc::renameatx_np(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(target_os = "linux")]
        let status = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if status != 0 {
            return Err(io(destination, std::io::Error::last_os_error()));
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        std::fs::rename(staged, destination).map_err(|e| io(destination, e))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(EklerError::ValidationFailed(
            "Atomik paket yayını bu platformda desteklenmiyor".into(),
        ))
    }
}
