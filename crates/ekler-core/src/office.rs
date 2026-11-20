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

/// LibreOffice'in yüklerken BAŞKA bir dosyayı belgeye çekebildiği alan kodları:
/// INCLUDETEXT bölüm bağlantısı, INCLUDEPICTURE bağlı görsel, LINK/DDE bağlı
/// nesne, IMPORT. Kötü niyetli bir belge bunlarla kullanıcının yerel bir
/// dosyasını (ör. `~/.ssh/...`) PDF'e gömdürebilir; o PDF de mahkemeye gider.
/// Profil bağlantı güncellemeyi kapatır; bu liste ondan bağımsız ikinci kattır.
const LINKING_FIELDS: &[&str] = &[
    "INCLUDETEXT",
    "INCLUDEPICTURE",
    "INCLUDE",
    "LINK",
    "DDE",
    "DDEAUTO",
    "IMPORT",
];
const LINKING_FIELD_MESSAGE: &str = "Belge başka bir dosyayı içeri alan bir alan kodu (INCLUDETEXT, INCLUDEPICTURE, LINK, DDE…) içeriyor. Alanı kaynak uygulamada kaldırıp (Word'de seçip Ctrl+Shift+F9) yerel bir kopya ekleyin.";
/// İlişki dosyası için gerçek (açılmış) bayt sınırı.
const MAX_RELS_BYTES: u64 = 8 * 1024 * 1024;
/// `word/*.xml` parçası için gerçek bayt sınırı (document-core ile aynı).
const MAX_PART_BYTES: u64 = 128 * 1024 * 1024;

/// Girdiyi en çok `cap` bayt okur. Zip'in BİLDİRDİĞİ boyuta güvenilmez: zip
/// 2.4 bildirilen boyutu uygulamaz, CRC'yi ancak girdi bittikten sonra
/// denetler; 100 bayt bildiren bir girdi gigabaytlarca açılabilir.
fn read_capped(entry: &mut impl Read, cap: u64, what: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    entry
        .take(cap + 1)
        .read_to_end(&mut buf)
        .map_err(|e| err(e.to_string()))?;
    if buf.len() as u64 > cap {
        return Err(err(format!("DOCX {what} çok büyük")));
    }
    Ok(buf)
}

/// Alan talimatının ilk sözcüğü içeri alan bir alan mı?
fn is_linking_instruction(instruction: &str) -> bool {
    let word: String = instruction
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    LINKING_FIELDS.iter().any(|f| f.eq_ignore_ascii_case(&word))
}

/// Bir WordprocessingML parçasındaki alanlardan içeri alan ilkini döndürür.
///
/// Karmaşık alanın talimatı birden çok `w:instrText`e bölünebilir
/// (`INCLU` + `DETEXT`); bu yüzden `begin` ile `separate`/`end` arasındaki
/// bütün talimat metni birleştirilip öyle sınanır. İç içe alanlar yığında
/// tutulur. Talimatı doğrudan iç içe bir alanla BAŞLAYAN alan (anahtar sözcüğü
/// çalışma anında üretilebilir) da reddedilir.
fn linking_field(xml: &[u8]) -> Result<Option<String>> {
    use quick_xml::events::Event;
    struct Field {
        instruction: String,
        checked: bool,
    }
    let mut rd = quick_xml::Reader::from_reader(xml);
    rd.config_mut().check_end_names = false;
    let mut stack: Vec<Field> = Vec::new();
    let mut in_instr = false;
    let found = |instruction: &str| -> Option<String> {
        is_linking_instruction(instruction).then(|| instruction.trim().to_string())
    };
    loop {
        let event = rd
            .read_event()
            .map_err(|e| err(format!("DOCX XML okunamadı: {e}")))?;
        let is_start = matches!(event, Event::Start(_));
        match event {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => match e.local_name().as_ref() {
                b"fldSimple" => {
                    for a in e.attributes().with_checks(false).flatten() {
                        if a.key.local_name().as_ref() == b"instr" {
                            let v = a.unescape_value().map_err(|e| err(e.to_string()))?;
                            if let Some(f) = found(&v) {
                                return Ok(Some(f));
                            }
                        }
                    }
                }
                b"fldChar" => {
                    let kind = e
                        .attributes()
                        .with_checks(false)
                        .flatten()
                        .find(|a| a.key.local_name().as_ref() == b"fldCharType")
                        .map(|a| a.value.into_owned());
                    match kind.as_deref() {
                        Some(b"begin") => {
                            if let Some(parent) = stack.last() {
                                if !parent.checked && parent.instruction.trim().is_empty() {
                                    return Ok(Some("(iç içe alanla başlayan alan)".into()));
                                }
                            }
                            stack.push(Field {
                                instruction: String::new(),
                                checked: false,
                            });
                        }
                        Some(b"separate") | Some(b"end") => {
                            if let Some(top) = stack.last_mut() {
                                if !top.checked {
                                    if let Some(f) = found(&top.instruction) {
                                        return Ok(Some(f));
                                    }
                                    top.checked = true;
                                }
                            }
                            if kind.as_deref() == Some(b"end") {
                                stack.pop();
                            }
                        }
                        _ => {}
                    }
                }
                b"instrText" => in_instr = is_start,
                _ => {}
            },
            Event::End(e) if e.local_name().as_ref() == b"instrText" => in_instr = false,
            Event::Text(t) if in_instr => {
                let text = t.unescape().map_err(|e| err(e.to_string()))?;
                match stack.last_mut() {
                    Some(top) if !top.checked => top.instruction.push_str(&text),
                    // `begin` olmadan talimat: kendi başına sınanır.
                    _ => {
                        if let Some(f) = found(&text) {
                            return Ok(Some(f));
                        }
                    }
                }
            }
            Event::CData(t) if in_instr => {
                let text = String::from_utf8_lossy(&t).into_owned();
                if let Some(top) = stack.last_mut().filter(|t| !t.checked) {
                    top.instruction.push_str(&text);
                } else if let Some(f) = found(&text) {
                    return Ok(Some(f));
                }
            }
            _ => {}
        }
    }
    // Kapanmamış alanlar da sınanır.
    for field in stack.iter().filter(|f| !f.checked) {
        if let Some(f) = found(&field.instruction) {
            return Ok(Some(f));
        }
    }
    Ok(None)
}

/// İkili DOC'ta alan başlangıcı (0x13) ardından gelen talimat sözcüğü.
fn doc_has_linking_field(text: &str) -> bool {
    text.match_indices('\u{13}')
        .any(|(at, _)| is_linking_instruction(&text[at + 1..].chars().take(64).collect::<String>()))
}

fn local_only_docx(bytes: &[u8]) -> Result<()> {
    tavzih_core::docx::reader::probe(bytes).map_err(|e| err(e.to_string()))?;
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| err(e.to_string()))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| err(e.to_string()))?;
        let name = file.name().to_string();
        if name.ends_with(".rels") {
            let xml = read_capped(&mut file, MAX_RELS_BYTES, "ilişki dosyası")?;
            let xml = String::from_utf8_lossy(&xml).to_ascii_lowercase();
            // Conservative: external links are refused too, avoiding remote templates,
            // external images and linked OLE loads before the renderer is invoked.
            if xml.contains("external") {
                return Err(err("Belge dış bağlantı içeriyor. Bağlantıları kaynak uygulamada kaldırıp yerel bir kopya ekleyin."));
            }
            // altChunk: pakete gömülü BAŞKA bir belge (RTF/HTML/DOCX). Writer onu
            // kendi alanlarıyla birlikte içeri alır; yukarıdaki alan denetimi o
            // biçimlerin içini görmez.
            if xml.contains("/afchunk") {
                return Err(err("Belge gömülü başka bir belge (altChunk) içeriyor. Belgeyi kaynak uygulamada açıp DOCX olarak yeniden kaydedin."));
            }
        } else if name.starts_with("word/") && name.ends_with(".xml") {
            let xml = read_capped(&mut file, MAX_PART_BYTES, "parçası")?;
            if linking_field(&xml)?.is_some() {
                return Err(err(LINKING_FIELD_MESSAGE));
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
            if [
                "http:",
                "https:",
                "ftp:",
                "file:",
                "includepicture",
                "includetext",
            ]
            .iter()
            .any(|t| ascii.contains(t) || wide.contains(t))
                || doc_has_linking_field(&ascii)
                || doc_has_linking_field(&wide)
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
    // Writer'ın `Content/Update/Link` değeri sw/inc/linkenum.hxx'teki sırayla
    // okunur: 0 = NEVER, 1 = MANUAL, 2 = AUTOMATIC. Burada eskiden 2 vardı, yani
    // bağlantılar her yüklemede güncelleniyordu. (Calc'ın aynı adlı ayarı başka
    // bir sıra kullanır: 0 = her zaman, 1 = asla; karışıklık oradan.)
    // `BlockUntrustedRefererLinks`: güvenilir konumlar dışındaki belgelerden
    // bağlı kaynak yüklenmez (profil boş, dolayısıyla hiçbir konum güvenilir değil).
    safe_io::write_new_bytes(&profile.join("user/registrymodifications.xcu"),&[],br#"<?xml version="1.0"?><oor:items xmlns:oor="http://openoffice.org/2001/registry"><item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop></item><item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="BlockUntrustedRefererLinks" oor:op="fuse"><value>true</value></prop></item><item oor:path="/org.openoffice.Office.Common/Misc"><prop oor:name="FirstRun" oor:op="fuse"><value>false</value></prop></item><item oor:path="/org.openoffice.Office.Writer/Content/Update"><prop oor:name="Link" oor:op="fuse"><value>0</value></prop></item></oor:items>"#)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

    fn body(inner: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><w:document {W}><w:body><w:p>{inner}</w:p></w:body></w:document>"#
        )
    }

    fn complex(parts: &[&str]) -> String {
        let instr: String = parts
            .iter()
            .map(|p| format!(r#"<w:r><w:instrText xml:space="preserve">{p}</w:instrText></w:r>"#))
            .collect();
        format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>{instr}<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>sonuç</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
        )
    }

    fn docx(parts: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut cur);
            let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            for (name, data) in parts {
                z.start_file(*name, o).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
        }
        cur.into_inner()
    }

    fn with_document(xml: &str) -> Vec<u8> {
        docx(&[("word/document.xml", xml.as_bytes())])
    }

    #[test]
    fn plain_document_and_harmless_fields_pass() {
        assert!(local_only_docx(&with_document(&body("<w:r><w:t>Dilekçe</w:t></w:r>"))).is_ok());
        let page = format!(
            "{}{}",
            complex(&[" PAGE "]),
            r#"<w:fldSimple w:instr=" NUMPAGES "/>"#
        );
        assert!(local_only_docx(&with_document(&body(&page))).is_ok());
        let link = complex(&[r#" HYPERLINK "https://ornek.tr" "#]);
        assert!(local_only_docx(&with_document(&body(&link))).is_ok());
    }

    #[test]
    fn includetext_split_across_runs_is_rejected() {
        let xml = body(&complex(&[
            " INCLU",
            r#"DETEXT "/Users/x/.ssh/known_hosts" "#,
        ]));
        let e = local_only_docx(&with_document(&xml))
            .unwrap_err()
            .to_string();
        assert!(e.contains("INCLUDETEXT"), "{e}");
    }

    #[test]
    fn simple_field_entity_escaped_and_lowercase_keywords_are_rejected() {
        for instr in [
            r#" INCLUDEPICTURE &quot;/etc/hosts&quot; \d "#,
            r#"includetext "C:\\gizli.txt""#,
            r#" IN&#67;LUDETEXT "x" "#,
            r#" LINK Word.Document.12 "x.docx" "" "#,
            r#" DDEAUTO excel "x" "#,
        ] {
            let xml = body(&format!(
                r#"<w:fldSimple w:instr="{instr}"><w:r><w:t>s</w:t></w:r></w:fldSimple>"#
            ));
            assert!(local_only_docx(&with_document(&xml)).is_err(), "{instr}");
            let xml = body(&complex(&[instr]));
            assert!(local_only_docx(&with_document(&xml)).is_err(), "{instr}");
        }
    }

    #[test]
    fn field_whose_keyword_comes_from_a_nested_field_is_rejected() {
        let nested = format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>{}<w:r><w:instrText> "x" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
            complex(&[r#" QUOTE "INCLUDETEXT" "#])
        );
        assert!(local_only_docx(&with_document(&body(&nested))).is_err());
        // Anahtar sözcükle başlayan dış alanın içindeki iç içe alan sorun değil.
        let ok = format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> IF </w:instrText></w:r>{}<w:r><w:instrText> = 1 "a" "b" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
            complex(&[" PAGE "])
        );
        assert!(local_only_docx(&with_document(&body(&ok))).is_ok());
    }

    #[test]
    fn linking_fields_in_headers_and_footnotes_are_rejected() {
        let doc = body("<w:r><w:t>a</w:t></w:r>");
        let header = format!(
            r#"<w:hdr {W}><w:p>{}</w:p></w:hdr>"#,
            complex(&[r#" INCLUDETEXT "x" "#])
        );
        let pkg = docx(&[
            ("word/document.xml", doc.as_bytes()),
            ("word/header1.xml", header.as_bytes()),
        ]);
        assert!(local_only_docx(&pkg).is_err());
    }

    #[test]
    fn alt_chunk_and_external_relationships_are_rejected() {
        let doc = body("<w:r><w:t>a</w:t></w:r>");
        let alt = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/aFChunk" Target="ek.rtf"/></Relationships>"#;
        let pkg = docx(&[
            ("word/document.xml", doc.as_bytes()),
            ("word/_rels/document.xml.rels", alt),
            ("word/ek.rtf", br"{\rtf1{\field{\*\fldinst INCLUDETEXT x}}}"),
        ]);
        assert!(local_only_docx(&pkg)
            .unwrap_err()
            .to_string()
            .contains("altChunk"));
        let ext = br#"<Relationships><Relationship Id="r1" Type="x/image" Target="file:///etc/hosts" TargetMode="External"/></Relationships>"#;
        let pkg = docx(&[
            ("word/document.xml", doc.as_bytes()),
            ("word/_rels/document.xml.rels", ext),
        ]);
        assert!(local_only_docx(&pkg).is_err());
    }

    #[test]
    fn oversized_relationship_part_is_refused_by_bytes_read_not_declared_size() {
        let doc = body("<w:r><w:t>a</w:t></w:r>");
        let huge = vec![b' '; (MAX_RELS_BYTES + 1024) as usize];
        let mut pkg = docx(&[
            ("word/document.xml", doc.as_bytes()),
            ("word/_rels/document.xml.rels", &huge),
        ]);
        // Bildirilen açılmış boyutu (yerel başlık +22, merkez dizin +24) 100
        // bayta indir: eski denetim yalnız bu sayıya bakıyordu.
        let name = b"word/_rels/document.xml.rels";
        for (sig, field) in [(&b"PK\x03\x04"[..], 22usize), (&b"PK\x01\x02"[..], 24usize)] {
            let at = pkg
                .windows(4)
                .enumerate()
                .filter(|(_, w)| *w == sig)
                .map(|(i, _)| i)
                .find(|&i| {
                    let name_at = i + if field == 22 { 30 } else { 46 };
                    pkg.get(name_at..name_at + name.len()) == Some(&name[..])
                })
                .unwrap();
            pkg[at + field..at + field + 4].copy_from_slice(&100u32.to_le_bytes());
        }
        assert!(local_only_docx(&pkg).is_err());
    }

    #[test]
    fn doc_field_begin_followed_by_linking_keyword_is_detected() {
        assert!(doc_has_linking_field(
            "metin \u{13} includetext \"c:\\\\x\" \u{14}sonuç\u{15}"
        ));
        assert!(doc_has_linking_field(
            "\u{13}LINK Excel.Sheet.8 \"x\"\u{14}"
        ));
        assert!(doc_has_linking_field("\u{13} ddeauto excel x\u{14}"));
        assert!(!doc_has_linking_field(
            "\u{13} PAGE \u{14}1\u{15} link sözcüğü metinde"
        ));
        assert!(!doc_has_linking_field(
            "\u{13} HYPERLINK \"https://x\" \u{14}"
        ));
    }

    #[test]
    fn repository_sample_documents_are_not_falsely_rejected() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for docx in [
            "ekler-core/tests/corpus/corpus.docx",
            "ikincigoz-core/tests/samples/ornek-dilekce-hatali.docx",
            "ikincigoz-core/tests/samples/ornek-dilekce-temiz.docx",
        ] {
            let bytes = std::fs::read(root.join(docx)).unwrap();
            assert!(local_only_docx(&bytes).is_ok(), "{docx}");
        }
        let doc = std::fs::read(root.join("ekler-core/tests/corpus/corpus.doc")).unwrap();
        let wide: Vec<u16> = doc
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert!(!doc_has_linking_field(
            &String::from_utf8_lossy(&doc).to_ascii_lowercase()
        ));
        assert!(!doc_has_linking_field(
            &String::from_utf16_lossy(&wide).to_ascii_lowercase()
        ));
    }

    #[test]
    fn docx_written_from_a_udf_still_passes() {
        let mut w = tavzih_core::warnings::WarningSink::new();
        let mut doc = tavzih_core::model::Document::default();
        doc.sections.push(Default::default());
        let bytes = tavzih_core::docx::writer::write_docx(&doc, &mut w).unwrap();
        assert!(local_only_docx(&bytes).is_ok());
    }
}
