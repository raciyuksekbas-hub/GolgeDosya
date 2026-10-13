//! Uzun ama yasal çıktı adları yazılır; diskin sınırını aşan ad dürüstçe
//! söylenir.
//!
//! Denetim bulgusu (OR-05): atomik yazımın geçici adı ".{hedef ad}.tavzih-tmp"
//! idi, hedef adına 13 bayt ekliyordu. APFS'nin 255 baytlık sınırında 244–254
//! baytlık çıktı adları "File name too long (os error 63)" ile düşüyor,
//! kullanıcıya "Klasör izinlerini kontrol edin" deniyordu. Türkçe harfler
//! UTF-8'de iki bayt olduğu için eşik Türkçe adlarda daha az karakterde gelir.
//!
//! Bütün belgeler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::path::PathBuf;

use tavzih_core::convert::{self, Status};
use tavzih_core::docx;
use tavzih_core::model::*;
use tavzih_core::warnings::WarningSink;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tavzih-names-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn sample_docx() -> Vec<u8> {
    let d = Document {
        meta: Metadata::default(),
        sections: vec![Section {
            blocks: vec![Block::Paragraph(Paragraph::plain("Sayın Mahkemeye"))],
            ..Default::default()
        }],
        lists: vec![],
    };
    docx::writer::write_docx(&d, &mut WarningSink::new()).unwrap()
}

fn leftovers(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|r| {
            r.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().contains("tavzih-tmp"))
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn a_long_but_legal_output_name_is_written() {
    let d = tmpdir("long");
    let out = d.join("cikti");
    // 244 bayt + ".udf" = 248 baytlık çıktı adı: yasal. Türkçe biçimi:
    // 120 × "ş" (240 bayt) + ".udf" = 244 bayt.
    for stem in ["a".repeat(244), "ş".repeat(120)] {
        let src = d.join(format!("{stem}.docx"));
        std::fs::write(&src, sample_docx()).unwrap();
        let r = convert::convert_file_to(&src, &out);
        assert_eq!(
            r.status,
            Status::Success,
            "{} bayt: {:?}",
            stem.len(),
            r.error.map(|e| (e.code, e.detail))
        );
        assert!(PathBuf::from(r.output.unwrap()).exists());
    }
    assert_eq!(leftovers(&out), 0, "geçici dosya kaldı");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_name_too_long_for_the_disk_says_so() {
    // 250 bayt + ".docx" = 255: kaynak yasal, ilk çıktı (254 bayt) da. İkinci
    // dönüştürmenin adı " (2)" ile 258 bayta çıkar ve disk reddeder: kullanıcıya
    // klasör izinleri değil, adın uzunluğu söylenmeli.
    let d = tmpdir("toolong");
    let out = d.join("cikti");
    let src = d.join(format!("{}.docx", "a".repeat(250)));
    std::fs::write(&src, sample_docx()).unwrap();
    let first = convert::convert_file_to(&src, &out);
    assert_eq!(
        first.status,
        Status::Success,
        "{:?}",
        first.error.map(|e| (e.code, e.detail))
    );
    let second = convert::convert_file_to(&src, &out);
    let e = second.error.expect("ikinci ad diske sığmamalı");
    assert_eq!(e.code, "OUTPUT_NAME_TOO_LONG", "{}", e.detail);
    assert!(e.message.contains("çok uzun"), "{}", e.message);
    assert_eq!(leftovers(&out), 0, "geçici dosya kaldı");
    std::fs::remove_dir_all(&d).ok();
}
