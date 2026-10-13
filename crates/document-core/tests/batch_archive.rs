//! Toplu işin arşivi çıktı klasöründe kalır; yazılamazsa söylenir.
//!
//! Denetim bulgusu (OR-08): pencerenin gönderdiği `stamp` doğrulanmadan arşiv
//! adına giriyordu. Çıktı klasöründe "GölgeDosya_Dönüşüm_x" adlı bir klasör
//! varken "x/../../kacis" damgası arşivi çıktı klasörünün DIŞINA
//! (<kök>/kacis.zip) yazdırdı. Arşiv yazılamadığında da hata `.ok()` ile
//! yutuluyordu: is_zip=false, output=None, hiçbir uyarı yok; 999 arşiv adının
//! tükenmesi de aynı sessizlikle bitiyordu.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::path::{Path, PathBuf};

use tavzih_core::convert::{self, ConvertOptions, Status};
use tavzih_core::docx;
use tavzih_core::model::*;
use tavzih_core::warnings::WarningSink;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tavzih-batch-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn sources(d: &Path) -> Vec<PathBuf> {
    let doc = Document {
        meta: Metadata::default(),
        sections: vec![Section {
            blocks: vec![Block::Paragraph(Paragraph::plain("Sayın Mahkemeye"))],
            ..Default::default()
        }],
        lists: vec![],
    };
    let bytes = docx::writer::write_docx(&doc, &mut WarningSink::new()).unwrap();
    ["Dilekçe A.docx", "Dilekçe B.docx"]
        .iter()
        .map(|n| {
            let p = d.join(n);
            std::fs::write(&p, &bytes).unwrap();
            p
        })
        .collect()
}

#[test]
fn a_window_stamp_cannot_move_the_archive_out_of_the_output_folder() {
    let d = tmpdir("escape");
    let out = d.join("cikti");
    std::fs::create_dir_all(out.join("GölgeDosya_Dönüşüm_x")).unwrap();
    let r = convert::convert_batch_to_with(
        &sources(&d),
        &out,
        Some("x/../../kacis"),
        &ConvertOptions::default(),
    );
    let zip = PathBuf::from(r.output.expect("arşiv yazılmalı"));
    let real = std::fs::canonicalize(&zip).unwrap();
    assert_eq!(
        real.parent(),
        Some(std::fs::canonicalize(&out).unwrap().as_path()),
        "arşiv çıktı klasöründen çıktı: {}",
        zip.display()
    );
    assert!(!d.join("kacis.zip").exists());
    // Geçersiz damga yerine saat damgası kullanılır: GölgeDosya_Dönüşüm_YYYY-MM-DD_HHMM.zip
    let name = zip.file_name().unwrap().to_string_lossy().to_string();
    let stamp = name
        .strip_prefix("GölgeDosya_Dönüşüm_")
        .and_then(|s| s.strip_suffix(".zip"))
        .unwrap_or_default();
    let shape: Vec<bool> = stamp.chars().map(|c| c.is_ascii_digit()).collect();
    assert_eq!(stamp.len(), 15, "{name}");
    assert_eq!(
        shape,
        [
            true, true, true, true, false, true, true, false, true, true, false, true, true, true,
            true
        ],
        "{name}"
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_well_formed_window_stamp_is_kept() {
    // Pencerenin yerel saati: dosya adında kullanıcının saati görünsün.
    let d = tmpdir("stamp");
    let out = d.join("cikti");
    let r = convert::convert_batch_to_with(
        &sources(&d),
        &out,
        Some("2026-10-13_0930"),
        &ConvertOptions::default(),
    );
    assert_eq!(
        r.output_name.as_deref(),
        Some("GölgeDosya_Dönüşüm_2026-10-13_0930.zip")
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn an_archive_that_cannot_be_written_is_reported_not_swallowed() {
    // 999 arşiv adının hepsi dolu: arşiv yazılamaz. Dönüştürülen belgeler
    // klasörde tek tek kalır, ama kullanıcıya bunun neden olduğu söylenmeli.
    let d = tmpdir("nozip");
    let out = d.join("cikti");
    std::fs::create_dir_all(&out).unwrap();
    let stamp = "2026-10-13_0930";
    let base = format!("GölgeDosya_Dönüşüm_{stamp}");
    std::fs::write(out.join(format!("{base}.zip")), b"").unwrap();
    for n in 2..=999 {
        std::fs::write(out.join(format!("{base} ({n}).zip")), b"").unwrap();
    }
    let r =
        convert::convert_batch_to_with(&sources(&d), &out, Some(stamp), &ConvertOptions::default());
    assert!(!r.is_zip);
    assert!(r.output.is_none());
    let j = serde_json::to_value(&r).unwrap();
    assert_eq!(
        j["zip_error"]["code"], "OUTPUT_EXISTS_CONFLICT",
        "arşiv hatası sonuçta yok: {}",
        j["zip_error"]
    );
    assert_eq!(
        r.status,
        Status::Warning,
        "arşivsiz sonuç başarı sayılmamalı"
    );
    // Tekil çıktılar yerinde.
    for item in &r.items {
        assert!(PathBuf::from(item.output.as_ref().unwrap()).exists());
    }
    std::fs::remove_dir_all(&d).ok();
}
