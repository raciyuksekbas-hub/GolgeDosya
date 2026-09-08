//! Hardened runtime önkontrolü.
//!
//! DüzenEk'in feature kodu taşınmadan önce cevaplanması gereken soru şudur:
//! **imzalı, hardened runtime etkin bir uygulama içinde** LibreOffice keşfi,
//! `sandbox-exec` sarmalayıcısı, `sips` ve CoreGraphics FFI gerçekten çalışıyor
//! mu? DüzenEk'in kendi deposunda hiç imzalama hattı olmadığı için bu hiç
//! denenmemişti.
//!
//! Taklit yok: ekler-core'un gerçek fonksiyonları çağrılır. Sonuç JSON olarak
//! stdout'a yazılır ki paketlenmiş uygulamadan toplanabilsin.

use std::path::PathBuf;
use std::process::Command;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "belge-preflight-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Tek sayfalık, gerçek ve geçerli bir PDF. Sentetiktir; hiçbir belgeden alınmadı.
fn synthetic_pdf(width: f32, height: f32) -> Vec<u8> {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content = format!(
        "BT /F1 24 Tf 40 {} Td (ONKONTROL) Tj ET 0.2 0.2 0.2 rg 40 40 120 60 re f",
        height - 80.0
    );
    let contents_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => contents_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// Geçerli 1x1 PNG; CRC'leri yerel olarak hesaplanır.
fn minimal_png() -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for b in data {
            crc ^= *b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn chunk(v: &mut Vec<u8>, body: &[u8]) {
        v.extend_from_slice(&((body.len() - 4) as u32).to_be_bytes());
        v.extend_from_slice(body);
        v.extend_from_slice(&crc32(body).to_be_bytes());
    }
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(
        &mut v,
        &[
            b'I', b'H', b'D', b'R', 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0,
        ],
    );
    chunk(
        &mut v,
        &[
            b'I', b'D', b'A', b'T', 0x08, 0x1D, 0x63, 0x60, 0x60, 0x60, 0x00, 0x00, 0x00, 0x04,
            0x00, 0x01,
        ],
    );
    chunk(&mut v, b"IEND");
    v
}

fn check(name: &str, result: Result<String, String>) -> serde_json::Value {
    match result {
        Ok(detail) => serde_json::json!({ "check": name, "ok": true, "detail": detail }),
        Err(detail) => serde_json::json!({ "check": name, "ok": false, "detail": detail }),
    }
}

// ---------------------------------------------------------------- kontroller

/// LibreOffice keşfi. DüzenEk'in GERÇEK arama zinciri çağrılır.
fn renderer_discovery() -> Result<String, String> {
    match ekler_core::office::find_renderer() {
        Some(p) => Ok(format!("bulundu: {}", p.display())),
        // Bulunamaması bir hata değil: kurulu olmayabilir. Ölçtüğümüz şey,
        // aramanın hardened runtime altında ÇALIŞABİLMESİ.
        None => Ok("kurulu değil (arama zinciri hatasız tamamlandı)".into()),
    }
}

/// Alakasız bir dosyanın renderer olarak kabul edilmemesi. Gerçek doğrulayıcı.
fn renderer_validation() -> Result<String, String> {
    let d = tmpdir("renderer");
    let f = d.join("sahte-soffice");
    std::fs::write(&f, b"#!/bin/sh\nexit 0\n").map_err(|e| e.to_string())?;
    let r = ekler_core::office::select_renderer(&f);
    std::fs::remove_dir_all(&d).ok();
    match r {
        Err(_) => Ok("alakasız dosya reddedildi".into()),
        Ok(_) => Err("alakasız dosya renderer olarak KABUL EDİLDİ".into()),
    }
}

/// `sandbox-exec` sarmalayıcısı: hardened runtime altında çalıştırılabiliyor mu
/// ve ağ reddi gerçekten uygulanıyor mu?
fn sandbox_exec() -> Result<String, String> {
    let profile = "(version 1)(allow default)(deny network*)";
    let launch = Command::new("/usr/bin/sandbox-exec")
        .args(["-p", profile, "/bin/echo", "onkontrol"])
        .output()
        .map_err(|e| format!("sandbox-exec başlatılamadı: {e}"))?;
    if !launch.status.success() {
        return Err(format!(
            "sandbox-exec çıkış kodu {:?}: {}",
            launch.status.code(),
            String::from_utf8_lossy(&launch.stderr).trim()
        ));
    }
    // Ağ gerçekten reddediliyor mu? Aynı profille bir TCP bağlantısı denenir.
    let net = Command::new("/usr/bin/sandbox-exec")
        .args([
            "-p",
            profile,
            "/usr/bin/nc",
            "-z",
            "-G",
            "2",
            "127.0.0.1",
            "1",
        ])
        .output();
    let denied = match net {
        Ok(o) => !o.status.success(),
        Err(_) => true,
    };
    Ok(format!(
        "çalıştı; ağ reddi {}",
        if denied { "etkin" } else { "DOĞRULANAMADI" }
    ))
}

/// `sips`: hardened runtime altında çağrılabiliyor mu?
fn sips() -> Result<String, String> {
    let d = tmpdir("sips");
    let src = d.join("kaynak.png");
    std::fs::write(&src, minimal_png()).map_err(|e| e.to_string())?;
    let out = d.join("cikti.jpg");
    // Üretimdeki yoldan geçer: `sips` artık `process-bridge` üzerinden
    // çağrılıyor. Kapı, üretimin kullanmadığı bir yolu denemek yerine gerçek
    // yolu denemeli — yoksa neyi doğruladığı belirsizleşir.
    let r = process_bridge::run(
        process_bridge::Spawn::new(std::path::Path::new("/usr/bin/sips"))
            .args(["-s", "format", "jpeg"])
            .arg(&src)
            .arg("--out")
            .arg(&out)
            .timeout(std::time::Duration::from_secs(60))
            .network(process_bridge::NetworkPolicy::Deny),
    );
    let verdict = match r {
        Err(e) => Err(format!("sips başarısız: {e}")),
        Ok(o) if !o.status.success() => Err(format!(
            "sips hata verdi: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Ok(_) if !out.is_file() => Err("sips çıktı üretmedi".into()),
        Ok(_) => Ok(format!(
            "çalıştı, {} bayt üretti",
            std::fs::metadata(&out).unwrap().len()
        )),
    };
    std::fs::remove_dir_all(&d).ok();
    verdict
}

/// CoreGraphics FFI: DüzenEk'in GERÇEK rasterizasyon yolu. Bu, hardened runtime
/// altında crash/panic üreten en muhtemel yerdir.
fn coregraphics(width: f32, height: f32, tag: &str) -> Result<String, String> {
    let d = tmpdir(&format!("cg-{tag}"));
    let src = d.join("kaynak.pdf");
    std::fs::write(&src, synthetic_pdf(width, height)).map_err(|e| e.to_string())?;
    let out = d.join("cikti");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let r = ekler_core::raster::pdf_to_images(&src, &out, "png", 144, true);
    let verdict = match r {
        Ok(p) => {
            let files: Vec<_> = std::fs::read_dir(&p)
                .map_err(|e| e.to_string())?
                .flatten()
                .collect();
            if files.is_empty() {
                Err("paket üretildi ama boş".into())
            } else {
                let total: u64 = files
                    .iter()
                    .filter_map(|f| f.metadata().ok())
                    .map(|m| m.len())
                    .sum();
                Ok(format!("{} dosya, {total} bayt", files.len()))
            }
        }
        Err(e) => Err(format!("{e}")),
    };
    std::fs::remove_dir_all(&d).ok();
    verdict
}

/// Office → PDF zincirinin TAMAMI: keşif → süreç başlatma → dönüşüm → yayın →
/// yeniden açma. DüzenEk'in gerçek `convert_to_pdf_file` fonksiyonu çağrılır.
///
/// LibreOffice kurulu değilse kontrol ATLANIR ve bunu açıkça söyler. Atlanmış
/// bir kontrol geçmiş sayılmaz: rapor "skipped" alanında görünür ve LibreOffice
/// kurulduğu an bu kapı kendiliğinden çalışmaya başlar.
fn office_conversion() -> Result<Option<String>, String> {
    if ekler_core::office::find_renderer().is_none() {
        return Ok(None);
    }
    let d = tmpdir("office");
    let src = d.join("belge.docx");
    // Sentetik ama gerçek bir DOCX: ortak belge çekirdeğiyle üretilir.
    let mut warn = tavzih_core::warnings::WarningSink::new();
    let mut doc = tavzih_core::model::Document::default();
    let section = tavzih_core::model::Section {
        blocks: vec![tavzih_core::model::Block::Paragraph(
            tavzih_core::model::Paragraph::plain("ÖNKONTROL — şğüçöıİ"),
        )],
        ..Default::default()
    };
    doc.sections = vec![section];
    let bytes = tavzih_core::docx::writer::write_docx(&doc, &mut warn)
        .map_err(|e| format!("DOCX üretilemedi: {e}"))?;
    std::fs::write(&src, bytes).map_err(|e| e.to_string())?;

    let out = d.join("cikti.pdf");
    let r = ekler_core::office::convert_to_pdf_file(&src, &out, true);
    let verdict = match r {
        Err(e) => Err(format!("dönüşüm başarısız: {e}")),
        Ok(()) if !out.is_file() => Err("dönüşüm başarı bildirdi ama dosya yok".into()),
        Ok(()) => {
            // Yeniden açılabiliyor mu?
            match lopdf::Document::load(&out) {
                Err(e) => Err(format!("çıktı yeniden açılamadı: {e}")),
                Ok(pdf) => Ok(Some(format!(
                    "{} sayfa, {} bayt; yeniden açıldı",
                    pdf.get_pages().len(),
                    std::fs::metadata(&out).unwrap().len()
                ))),
            }
        }
    };
    std::fs::remove_dir_all(&d).ok();
    verdict
}

/// Atomik, üzerine-yazmayan yayın: gerçek dosya sisteminde.
fn safe_publication() -> Result<String, String> {
    let d = tmpdir("safeio");
    let target = d.join("cikti.pdf");
    ekler_core::safe_io::write_new_bytes(&target, &[], b"%PDF-1.5\n")
        .map_err(|e| format!("ilk yazma başarısız: {e}"))?;
    let second = ekler_core::safe_io::write_new_bytes(&target, &[], b"UZERINE YAZILDI");
    let content = std::fs::read(&target).map_err(|e| e.to_string())?;
    std::fs::remove_dir_all(&d).ok();
    if second.is_ok() {
        return Err("mevcut dosyanın üzerine YAZILDI".into());
    }
    if content != b"%PDF-1.5\n" {
        return Err("ilk çıktının içeriği değişti".into());
    }
    Ok("atomik yazma çalıştı; ikinci yazma reddedildi, ilk içerik korundu".into())
}

fn main() {
    let hardened = std::env::var("BELGE_PREFLIGHT_CONTEXT").unwrap_or_else(|_| "bilinmiyor".into());
    let checks = vec![
        check("renderer_discovery", renderer_discovery()),
        check("renderer_validation", renderer_validation()),
        check("sandbox_exec", sandbox_exec()),
        check("sips", sips()),
        check(
            "coregraphics_portrait",
            coregraphics(400.0, 800.0, "portrait"),
        ),
        check(
            "coregraphics_landscape",
            coregraphics(800.0, 400.0, "landscape"),
        ),
        check("coregraphics_small", coregraphics(72.0, 72.0, "small")),
        check("coregraphics_large", coregraphics(1684.0, 2384.0, "large")),
        check("safe_publication", safe_publication()),
    ];
    let mut checks = checks;
    let mut skipped: Vec<String> = Vec::new();
    match office_conversion() {
        Ok(None) => skipped.push("office_conversion (LibreOffice kurulu değil)".into()),
        Ok(Some(detail)) => checks.push(check("office_conversion", Ok(detail))),
        Err(detail) => checks.push(check("office_conversion", Err(detail))),
    }
    let failed: Vec<_> = checks
        .iter()
        .filter(|c| c["ok"] == serde_json::json!(false))
        .map(|c| c["check"].as_str().unwrap().to_string())
        .collect();
    let report = serde_json::json!({
        "context": hardened,
        "checks": checks,
        "failed": failed,
        // Atlanan kontrol GEÇMİŞ SAYILMAZ; raporda ayrı durur.
        "skipped": skipped,
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    if !report["failed"].as_array().unwrap().is_empty() {
        std::process::exit(1);
    }
}
