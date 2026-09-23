//! Local Writer renderer. Sources are copied into a private work directory;
//! the external application is never handed a source path or a final path.
use crate::*;
use std::io::Read;
use std::path::{Path, PathBuf};

static SELECTED_RENDERER: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);
pub fn select_renderer(path: &Path) -> Result<PathBuf> {
    let path = if path.is_dir() {
        path.join("Contents/MacOS/soffice")
    } else {
        path.into()
    };
    if !path.is_file() {
        return Err(err("Seçilen konumda LibreOffice çalıştırıcısı bulunamadı. LibreOffice.app veya soffice çalıştırıcısını seçin."));
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !["soffice", "soffice.exe", "soffice.com", "soffice.bin"].contains(&name) {
        return Err(err("Bu dosya LibreOffice çalıştırıcısı değil. LibreOffice.app veya soffice dosyasını seçin."));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path
            .metadata()
            .map_err(|e| err(e.to_string()))?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err(err("LibreOffice dosyası bulundu, ancak çalıştırma izni yok. Kurulumu veya dosya izinlerini kontrol edin."));
        }
    }
    let path = path.canonicalize().map_err(|e| err(e.to_string()))?;
    *SELECTED_RENDERER
        .write()
        .map_err(|_| err("Renderer ayarı kilitli"))? = Some(path.clone());
    Ok(path)
}
pub fn find_renderer() -> Option<PathBuf> {
    if let Ok(selected) = SELECTED_RENDERER.read() {
        if let Some(path) = selected.as_ref() {
            return Some(path.clone());
        }
    }
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("DUZENEK_LIBREOFFICE") {
        candidates.push(path.into());
    }
    #[cfg(target_os = "macos")]
    candidates.push(PathBuf::from(
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ));
    #[cfg(target_os = "macos")]
    {
        for root in [
            Some(PathBuf::from("/Applications")),
            std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Applications")),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(entries) = std::fs::read_dir(root) {
                let mut paths = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().starts_with("LibreOffice"))
                            .unwrap_or(false)
                    })
                    .collect::<Vec<_>>();
                paths.sort();
                for p in paths {
                    candidates.push(p.join("Contents/MacOS/soffice"));
                }
            }
        }
        candidates.push("/opt/homebrew/bin/soffice".into());
        candidates.push("/usr/local/bin/soffice".into());
    }
    #[cfg(target_os = "windows")]
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(path) = std::env::var_os(key) {
            candidates.push(PathBuf::from(path).join("LibreOffice/program/soffice.com"));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join(if cfg!(windows) {
                "soffice.exe"
            } else {
                "soffice"
            }));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}
fn err(message: impl Into<String>) -> EklerError {
    EklerError::ValidationFailed(message.into())
}
fn local_only_docx(bytes: &[u8]) -> Result<()> {
    tavzih_core::docx::reader::probe(bytes).map_err(|e| err(e.to_string()))?;
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| err(e.to_string()))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| err(e.to_string()))?;
        if file.name().ends_with(".rels") {
            if file.size() > 8 * 1024 * 1024 {
                return Err(err("DOCX ilişki dosyası çok büyük"));
            }
            let mut xml = String::new();
            file.read_to_string(&mut xml)
                .map_err(|e| err(e.to_string()))?;
            // Conservative: external links are refused too, avoiding remote templates,
            // external images and linked OLE loads before the renderer is invoked.
            if xml.to_ascii_lowercase().contains("external") {
                return Err(err("Belge dış bağlantı içeriyor. Bağlantıları kaynak uygulamada kaldırıp yerel bir kopya ekleyin."));
            }
        }
    }
    Ok(())
}

pub fn convert_to_pdf(path: &Path, approved: bool) -> Result<lopdf::Document> {
    let before = calculate_sha256(path).map_err(|source| EklerError::IoError {
        path: path.into(),
        source,
    })?;
    let bytes = std::fs::read(path).map_err(|source| EklerError::IoError {
        path: path.into(),
        source,
    })?;
    let format = SourceFormat::from_path(path);
    let (input, extension) = match format {
        SourceFormat::Udf => {
            let info = inspect_udf(path)?;
            if info.is_signed && !approved {
                return Err(EklerError::UnapprovedSignedUdf(path.into()));
            }
            let mut warnings = tavzih_core::warnings::WarningSink::new();
            let model = tavzih_core::udf::reader::read_udf(&bytes, &mut warnings)
                .map_err(|e| err(e.to_string()))?;
            let docx = tavzih_core::docx::writer::write_docx(&model, &mut warnings)
                .map_err(|e| err(e.to_string()))?;
            let warnings = warnings.into_vec();
            if warnings.iter().any(|w| w.data_loss) {
                return Err(err(format!(
                    "İçerik kaybı riski nedeniyle UDF dönüşümü durduruldu: {}",
                    warnings
                        .iter()
                        .filter(|w| w.data_loss)
                        .map(|w| w.title.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                )));
            }
            local_only_docx(&docx)?;
            (docx, "docx")
        }
        SourceFormat::Docx => {
            local_only_docx(&bytes)?;
            (bytes, "docx")
        }
        SourceFormat::Doc => {
            if !bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
                return Err(err("Geçerli ikili DOC dosyası değil"));
            }
            let ascii = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
            let wide = bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            let wide = String::from_utf16_lossy(&wide).to_ascii_lowercase();
            if ["http:", "https:", "ftp:", "includepicture", "includetext"]
                .iter()
                .any(|t| ascii.contains(t) || wide.contains(t))
            {
                return Err(err(
                    "DOC dış bağlantı içerebilir; yerel DOCX kopyası kullanın",
                ));
            }
            (bytes, "doc")
        }
        _ => return Err(err("Office dönüştürücüsü bu formatı desteklemiyor")),
    };
    let renderer=find_renderer().ok_or_else(||err("PDF dönüşümü için LibreOffice bulunamadı. LibreOffice'i bu bilgisayara kurup yeniden deneyin. Microsoft Word gerekli değildir."))?;
    let work = tempfile::tempdir().map_err(|source| EklerError::IoError {
        path: std::env::temp_dir(),
        source,
    })?;
    let input_path = work.path().join(format!("input.{extension}"));
    safe_io::write_new_bytes(&input_path, &[], &input)?;
    let profile = work.path().join("profile");
    std::fs::create_dir_all(profile.join("user")).map_err(|e| err(e.to_string()))?;
    safe_io::write_new_bytes(&profile.join("user/registrymodifications.xcu"),&[],br#"<?xml version="1.0"?><oor:items xmlns:oor="http://openoffice.org/2001/registry"><item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop></item><item oor:path="/org.openoffice.Office.Common/Misc"><prop oor:name="FirstRun" oor:op="fuse"><value>false</value></prop></item><item oor:path="/org.openoffice.Office.Writer/Content/Update"><prop oor:name="Link" oor:op="fuse"><value>2</value></prop></item></oor:items>"#)?;
    let profile_url =
        url::Url::from_directory_path(&profile).map_err(|_| err("Geçersiz profil yolu"))?;
    // Süreç başlatma `process-bridge` üzerinden. Politika aynen korundu:
    // ağ reddi (macOS'ta sandbox ile), 120 saniye zaman aşımı, çıktı yutulur.
    let spawn = process_bridge::Spawn::new(&renderer)
        .arg(format!("-env:UserInstallation={profile_url}"))
        .args([
            "--headless",
            "--nologo",
            "--nodefault",
            "--norestore",
            "--unaccept=all",
            "--convert-to",
            "pdf:writer_pdf_Export",
            "--outdir",
        ])
        .arg(work.path())
        .arg(&input_path)
        .timeout(std::time::Duration::from_secs(120))
        .network(process_bridge::NetworkPolicy::Deny)
        .capture(false);
    process_bridge::run(spawn).map_err(|e| match e {
        process_bridge::BridgeError::Timeout { .. } => {
            err("LibreOffice dönüşümü zaman aşımına uğradı")
        }
        process_bridge::BridgeError::Launch { source, .. } => {
            err(format!("LibreOffice başlatılamadı: {source}"))
        }
        _ => err(
            "LibreOffice bulundu, fakat çalıştırma veya dönüşüm başarısız oldu; kurulumu kontrol edin",
        ),
    })?;
    let output = std::fs::read(work.path().join("input.pdf"))
        .map_err(|_| err("LibreOffice çalıştı, fakat geçerli PDF dosyası üretmedi"))?;
    let doc = load_pdf_tolerant(&output, "dönüştürülmüş.pdf")?.document;
    if before != calculate_sha256(path).map_err(|e| err(e.to_string()))? {
        return Err(EklerError::SourceIntegrityCompromised { path: path.into() });
    }
    Ok(doc)
}

pub fn convert_to_pdf_file(source: &Path, output: &Path, approved: bool) -> Result<()> {
    safe_io::ensure_new_destination(output, &[source.into()])?;
    let before = calculate_sha256(source).map_err(|e| err(e.to_string()))?;
    let mut doc = convert_to_pdf(source, approved)?;
    // LibreOffice'in ürettiği PDF de GölgeDosya çıktısıdır: aynı kapı.
    let pdf = crate::pdf::stamp::finalize_pdf_output(&mut doc)?;
    if before != calculate_sha256(source).map_err(|e| err(e.to_string()))? {
        return Err(EklerError::SourceIntegrityCompromised {
            path: source.into(),
        });
    }
    safe_io::publish_pdf(output, &[source.into()], &pdf)
}
