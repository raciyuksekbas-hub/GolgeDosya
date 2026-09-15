use crate::error::{EklerError, Result};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct UdfInfo {
    pub is_signed: bool,
    pub title: Option<String>,
    pub author: Option<String>,
    pub paragraph_count: usize,
    pub table_count: usize,
    pub image_count: usize,
}

/// UDF dosyasını inceler, imza durumunu ve yapısal istatistiklerini döner.
pub fn inspect_udf(path: &Path) -> Result<UdfInfo> {
    let bytes = std::fs::read(path).map_err(|e| EklerError::IoError {
        path: path.to_path_buf(),
        source: e,
    })?;

    // 1. ZIP arşivinde sign.sgn var mı kontrol et
    let mut is_signed = false;
    let mut cursor = std::io::Cursor::new(&bytes);
    if let Ok(mut zip) = zip::ZipArchive::new(&mut cursor) {
        for i in 0..zip.len() {
            if let Ok(file) = zip.by_index(i) {
                if file.name().eq_ignore_ascii_case("sign.sgn") {
                    is_signed = true;
                    break;
                }
            }
        }
    } else {
        return Err(EklerError::InvalidUdf(
            "Dosya geçerli bir UDF (ZIP) arşivi değil".to_string(),
        ));
    }

    // 2. Tavzih çekirdeği ile Document modelini ayrıştır
    let mut warn = tavzih_core::warnings::WarningSink::new();
    let doc = tavzih_core::udf::reader::read_udf(&bytes, &mut warn)
        .map_err(|e| EklerError::InvalidUdf(format!("UDF ayrıştırma hatası: {}", e)))?;

    let mut p_count = 0;
    let mut tbl_count = 0;
    let mut img_count = 0;

    for section in &doc.sections {
        for block in &section.blocks {
            match block {
                tavzih_core::model::Block::Paragraph(p) => {
                    p_count += 1;
                    for run in &p.runs {
                        if let tavzih_core::model::Run::Image(_) = run {
                            img_count += 1;
                        }
                    }
                }
                tavzih_core::model::Block::Table(_) => {
                    tbl_count += 1;
                }
                _ => {}
            }
        }
    }

    Ok(UdfInfo {
        is_signed,
        title: doc.meta.title,
        author: doc.meta.author,
        paragraph_count: p_count,
        table_count: tbl_count,
        image_count: img_count,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct MarkdownResult {
    pub markdown: String,
    pub package_dir: std::path::PathBuf,
    pub warnings: Vec<String>,
}

/// Markdown and every asset become visible together in one new package.
pub fn convert_udf_to_markdown(
    udf_path: &Path,
    output_dir: &Path,
    approved: bool,
) -> Result<MarkdownResult> {
    use crate::{calculate_sha256, safe_io};
    let fail = |e: std::io::Error| EklerError::IoError {
        path: udf_path.into(),
        source: e,
    };
    let before = calculate_sha256(udf_path).map_err(fail)?;
    let info = inspect_udf(udf_path)?;
    if info.is_signed && !approved {
        return Err(EklerError::UnapprovedSignedUdf(udf_path.into()));
    }
    let bytes = std::fs::read(udf_path).map_err(fail)?;
    let mut warn = tavzih_core::warnings::WarningSink::new();
    let doc = tavzih_core::udf::reader::read_udf(&bytes, &mut warn)
        .map_err(|e| EklerError::InvalidUdf(e.to_string()))?;
    let warnings = warn.into_vec();
    if warnings.iter().any(|w| w.data_loss) {
        return Err(EklerError::InvalidUdf(format!(
            "İçerik kaybı riski: {}",
            warnings
                .iter()
                .filter(|w| w.data_loss)
                .map(|w| w.title.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    let staging = tempfile::Builder::new()
        .prefix(".duzenek-markdown-")
        .tempdir_in(output_dir)
        .map_err(fail)?;
    let mut writer = MarkdownWriter {
        dir: staging.path(),
        source: udf_path,
        doc: &doc,
        image_count: 0,
        counters: std::collections::HashMap::new(),
    };
    let mut markdown = String::new();
    if info.is_signed {
        markdown.push_str("> Bu türetilmiş metin kaynak elektronik imzanın doğrulanabilirliğini taşımaz. Kaynak değiştirilmemiştir.\n\n");
    }
    for section in &doc.sections {
        if let Some(h) = &section.header {
            markdown.push_str("<!-- Üst bilgi -->\n");
            markdown.push_str(&writer.blocks(&h.blocks, false)?);
        }
        markdown.push_str(&writer.blocks(&section.blocks, false)?);
        if let Some(f) = &section.footer {
            markdown.push_str("<!-- Alt bilgi -->\n");
            markdown.push_str(&writer.blocks(&f.blocks, false)?);
        }
    }
    let warnings: Vec<String> = warnings
        .into_iter()
        .map(|w| format!("{}: {}", w.title, w.detail.unwrap_or_default()))
        .collect();
    safe_io::write_new_bytes(
        &staging.path().join("duzenek-logo.svg"),
        &[],
        include_bytes!("../../assets/brand-logo.svg"),
    )?;
    markdown.push_str("\n<p style=\"text-align:right\"><img src=\"duzenek-logo.svg\" alt=\"DüzenEk\" width=\"96\" /></p>\n");
    safe_io::write_new_bytes(&staging.path().join("belge.md"), &[], markdown.as_bytes())?;
    safe_io::write_new_bytes(
        &staging.path().join("conversion-notes.json"),
        &[],
        &serde_json::to_vec_pretty(&warnings)?,
    )?;
    if before != calculate_sha256(udf_path).map_err(fail)? {
        return Err(EklerError::SourceIntegrityCompromised {
            path: udf_path.into(),
        });
    }
    let suffix = staging
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace(".duzenek-markdown-", "");
    let package_dir = output_dir.join(format!("GolgeDosya-Markdown-{suffix}"));
    safe_io::publish_directory(staging.path(), &package_dir)?;
    Ok(MarkdownResult {
        markdown,
        package_dir,
        warnings,
    })
}
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn md_escape(s: &str) -> String {
    let mut r = String::new();
    for c in html_escape(s).chars() {
        if "\\`*_{}[]()#+-.!|".contains(c) {
            r.push('\\');
        }
        r.push(c);
    }
    r
}
struct MarkdownWriter<'a> {
    dir: &'a Path,
    source: &'a Path,
    doc: &'a tavzih_core::model::Document,
    image_count: usize,
    counters: std::collections::HashMap<(u32, u32), usize>,
}
impl MarkdownWriter<'_> {
    fn blocks(&mut self, blocks: &[tavzih_core::model::Block], html: bool) -> Result<String> {
        use tavzih_core::model::*;
        let mut out = String::new();
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    let mut text = String::new();
                    for run in &p.runs {
                        match run {
                            Run::Text { text: t, props } => {
                                let mut t = if html { html_escape(t) } else { md_escape(t) };
                                if props.bold {
                                    t = format!("<strong>{t}</strong>");
                                }
                                if props.italic {
                                    t = format!("<em>{t}</em>");
                                }
                                if props.underline {
                                    t = format!("<u>{t}</u>");
                                }
                                text.push_str(&t);
                            }
                            Run::Tab { .. } => text.push_str(if html { "&#9;" } else { "\t" }),
                            Run::LineBreak { .. } => text.push_str("<br />\n"),
                            Run::Image(img) => {
                                let format = ImageFormat::sniff(&img.data);
                                if format == ImageFormat::Unsupported {
                                    return Err(EklerError::InvalidUdf(
                                        "Desteklenmeyen gömülü görsel".into(),
                                    ));
                                }
                                ::image::load_from_memory(&img.data).map_err(|e| {
                                    EklerError::InvalidUdf(format!("Bozuk görsel: {e}"))
                                })?;
                                self.image_count += 1;
                                let name =
                                    format!("image-{:03}.{}", self.image_count, format.extension());
                                crate::safe_io::write_new_bytes(
                                    &self.dir.join(&name),
                                    &[self.source.into()],
                                    &img.data,
                                )?;
                                text.push_str(&format!(
                                    "<img src=\"{name}\" alt=\"Belge görseli {}\" />",
                                    self.image_count
                                ));
                            }
                        }
                    }
                    let heading = p.props.style_name.as_deref().and_then(|s| {
                        let s = s.to_lowercase();
                        ["heading ", "heading", "başlık "]
                            .iter()
                            .find_map(|prefix| {
                                s.strip_prefix(prefix)
                                    .and_then(|n| n.parse::<usize>().ok())
                                    .filter(|n| (1..=6).contains(n))
                            })
                    });
                    if let Some(n) = heading {
                        if html {
                            out.push_str(&format!("<h{n}>{text}</h{n}>\n"));
                        } else {
                            out.push_str(&format!("{} {text}\n\n", "#".repeat(n)));
                        }
                    } else if let Some(list) = &p.props.list {
                        let def = self
                            .doc
                            .lists
                            .iter()
                            .find(|d| d.list_id == list.list_id)
                            .ok_or_else(|| {
                                EklerError::InvalidUdf("Liste tanımı bulunamadı".into())
                            })?;
                        let count = self.counters.entry((list.list_id, list.level)).or_default();
                        *count += 1;
                        let marker = match def.kind {
                            ListKind::Bullet(_) => "•".to_string(),
                            ListKind::Number(NumberKind::LowerAlphaParen) => format!(
                                "{})",
                                char::from_u32(97 + ((*count - 1) % 26) as u32).unwrap()
                            ),
                            ListKind::Number(_) => format!("{count}."),
                        };
                        if html {
                            out.push_str(&format!("<p>{marker} {text}</p>\n"));
                        } else {
                            let marker = if matches!(def.kind, ListKind::Bullet(_)) {
                                "-".into()
                            } else {
                                marker
                            };
                            out.push_str(&format!(
                                "{}{marker} {text}\n",
                                "    ".repeat(list.level.saturating_sub(1).min(16) as usize)
                            ));
                        }
                    } else if html {
                        out.push_str(&format!("<p>{text}</p>\n"));
                    } else {
                        out.push_str(&format!("{text}\n\n"));
                    }
                }
                Block::Table(t) => {
                    out.push_str("\n<table>\n");
                    for row in &t.rows {
                        out.push_str("<tr>\n");
                        for cell in &row.cells {
                            let tag = if row.is_header { "th" } else { "td" };
                            out.push_str(&format!(
                                "<{tag} colspan=\"{}\">\n",
                                cell.grid_span.max(1)
                            ));
                            out.push_str(&self.blocks(&cell.blocks, true)?);
                            out.push_str(&format!("</{tag}>\n"));
                        }
                        out.push_str("</tr>\n");
                    }
                    out.push_str("</table>\n\n");
                }
                Block::PageBreak => out.push_str(if html { "<hr />\n" } else { "\n---\n\n" }),
            }
        }
        Ok(out)
    }
}
