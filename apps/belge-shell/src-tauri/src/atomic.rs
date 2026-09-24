//! Yapılandırma dosyalarının atomik yazımı.
//!
//! Ayar ve sözlük dosyaları yerinde `std::fs::write` ile yazılıyordu: yazma
//! yarıda kesilirse (çökme, güç kaybı, disk dolması) dosya yarım kalıyor,
//! sonraki okuma sessizce varsayılana düşüyor ve ilk kayıt bu kaybı kalıcı
//! yapıyordu. Burada geçici dosya AYNI dizinde yazılır, diske işlenir ve tek bir
//! `rename` ile yerine konur: okuyucular ya eski ya yeni tam dosyayı görür,
//! asla yarısını görmez.
//!
//! Migration bu modülü kullanmaz (o modül dosya SİLMEZ; bkz. legacy.rs). Bu
//! yalnız üzerine yazan yolları atomikleştirir.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Hazırlık klasörü (oluşturulur) ve yayın hedefi, klasörün kanonik yolu
/// üzerinden. Windows'ta `canonicalize` `\\?\` önekli (uzun yol) biçimi döner;
/// `tempfile` yayında yolu Win32'ye öneksiz verdiği için 260 karakteri aşan
/// bir klasörde yayın "os error 3" ile düşüyordu. Kanonik klasör aynı yerdir.
/// Kanonik yol alınamayan birimlerde (bazı RAM diskleri) eski davranış sürer.
fn staging_target(destination: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let name = destination.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "hedefin dosya adı yok")
    })?;
    let dir = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    let target = dir.join(name);
    Ok((dir, target))
}

pub fn write(destination: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let (dir, target) = staging_target(destination)?;
    let mut staged = tempfile::NamedTempFile::new_in(&dir)?;
    staged.as_file_mut().write_all(bytes)?;
    staged.as_file_mut().flush()?;
    staged.as_file().sync_all()?;
    // persist (noclobber DEĞİL): hedef varsa atomik olarak değiştirilir.
    staged.persist(&target).map_err(|e| e.error).map(|_| ())
}

/// No-clobber + atomik yazım. Var olan bir dosyanın üzerine **asla** yazmaz;
/// hedef zaten varsa hiçbir şey yazmadan `ErrorKind::AlreadyExists` döner.
///
/// Bu, "bir kopyaya uygula" yayınları içindir (İkinciGöz düzeltme çıktısı gibi):
/// kaynak korunur diye çıktı adı zaten değişir, ama kullanıcının daha önce
/// oluşturup ELİYLE DÜZENLEDİĞİ bir kopya aynı adı taşıyabilir. Düz
/// `fs::write` onu sessizce eziyordu (geri alınamaz veri kaybı) ve atomik
/// olmadığı için yarıda kesilen yazım bozuk bir zip (docx/udf) bırakıyordu.
///
/// `symlink_metadata` ön kontrolü kimlik karşılaştırmasından güçlüdür (dangling
/// symlink ve hardlink'leri de yakalar); `persist_noclobber` ise ön kontrol ile
/// yazım arasındaki yarışta bile üzerine yazmayı engelleyen gerçek settir.
pub fn write_new(destination: &Path, bytes: &[u8]) -> std::io::Result<()> {
    match std::fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("hedef zaten mevcut: {}", destination.display()),
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let (dir, target) = staging_target(destination)?;
    let mut staged = tempfile::NamedTempFile::new_in(&dir)?;
    staged.as_file_mut().write_all(bytes)?;
    staged.as_file_mut().flush()?;
    staged.as_file().sync_all()?;
    staged
        .persist_noclobber(&target)
        .map_err(|e| e.error)
        .map(|_| ())
}

/// `dir/file_name` altına no-clobber + atomik yaz; ad çakışırsa uzantıdan önce
/// ` (2)`, ` (3)`… ekleyerek çakışmayan ilk adı bulur — Dönüştür'ün
/// `unique_output_path_in` davranışıyla aynı, ama yarış-güvenli (ad seçimi ile
/// yazım tek `write_new` çağrısında birleşiktir). Yazılan gerçek dosya adını
/// döner. Var olan hiçbir dosya kaybolmaz.
pub fn write_new_unique(dir: &Path, file_name: &str, bytes: &[u8]) -> std::io::Result<String> {
    let (stem, ext) = split_name(file_name);
    for n in 1..=999u32 {
        let candidate = if n == 1 {
            file_name.to_string()
        } else if let Some(e) = ext {
            format!("{stem} ({n}).{e}")
        } else {
            format!("{stem} ({n})")
        };
        match write_new(&dir.join(&candidate), bytes) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "999 aday adın tümü mevcut",
    ))
}

/// Dosya adını (kök, uzantı) olarak ayır. Yalnız son noktada böler; baştaki
/// nokta (gizli dosya) ya da uzantısız ad için uzantı `None`.
fn split_name(file_name: &str) -> (&str, Option<&str>) {
    match file_name.rfind('.') {
        Some(i) if i > 0 && i + 1 < file_name.len() => (&file_name[..i], Some(&file_name[i + 1..])),
        _ => (file_name, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        // Süreç-genelinde artan sayaç: testler paralel koşarken aynı nanosaniyede
        // çakışan iki dizin üretmesin (yoksa testler birbirinin dosyasını görür).
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "belge-atomic-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            N.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn overwrites_atomically_and_leaves_no_temp() {
        let d = tmp();
        let f = d.join("x.json");
        write(&f, b"eski").unwrap();
        write(&f, b"yeni tam icerik").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"yeni tam icerik");
        let leftovers: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "x.json")
            .collect();
        assert!(leftovers.is_empty(), "geçici dosya kaldı: {leftovers:?}");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn creates_missing_parent_directories() {
        let d = tmp();
        let f = d.join("alt/dizin/x.json");
        write(&f, b"veri").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"veri");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn write_new_refuses_to_overwrite_and_preserves_the_existing_file() {
        let d = tmp();
        let f = d.join("Dilekçe - Düzeltilmiş.docx");
        // Kullanıcının elle düzenlemiş olabileceği önceki kopya.
        std::fs::write(&f, b"kullanicinin duzenledigi kopya").unwrap();
        let err = write_new(&f, b"yeni tam icerik cok daha uzun").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        // Var olan dosya bit bit korunmalı.
        assert_eq!(
            std::fs::read(&f).unwrap(),
            b"kullanicinin duzenledigi kopya"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn write_new_unique_never_clobbers_and_derives_numbered_names() {
        let d = tmp();
        // Aynı ada üç kez yaz; her biri farklı içerik.
        let n1 = write_new_unique(&d, "Dilekçe - Düzeltilmiş.docx", b"birinci").unwrap();
        let n2 = write_new_unique(&d, "Dilekçe - Düzeltilmiş.docx", b"ikinci").unwrap();
        let n3 = write_new_unique(&d, "Dilekçe - Düzeltilmiş.docx", b"ucuncu").unwrap();
        assert_eq!(n1, "Dilekçe - Düzeltilmiş.docx");
        assert_eq!(n2, "Dilekçe - Düzeltilmiş (2).docx");
        assert_eq!(n3, "Dilekçe - Düzeltilmiş (3).docx");
        // Üçü de var ve içerikleri karışmamış: hiçbiri kaybolmadı.
        assert_eq!(std::fs::read(d.join(&n1)).unwrap(), b"birinci");
        assert_eq!(std::fs::read(d.join(&n2)).unwrap(), b"ikinci");
        assert_eq!(std::fs::read(d.join(&n3)).unwrap(), b"ucuncu");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn write_new_is_atomic_and_leaves_no_temporary_file() {
        let d = tmp();
        let f = d.join("cikti.udf");
        write_new(&f, b"tam ve butun").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"tam ve butun");
        let leftovers: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "cikti.udf")
            .collect();
        assert!(leftovers.is_empty(), "geçici dosya kaldı: {leftovers:?}");
        std::fs::remove_dir_all(&d).ok();
    }

    /// §55: Windows'un eski 260 karakterlik sınırını aşan klasörde de yazar.
    /// Windows'ta `tempfile`'ın yayını yolu Win32'ye öneksiz veriyordu ve bu
    /// test "os error 3" ile düşüyordu.
    #[test]
    fn writes_beyond_the_windows_path_limit() {
        let d = tmp();
        let mut deep = d.join("Çağrı Şahin").join("Müvekkil'in Dosyası (2026)");
        while deep.as_os_str().len() < 300 {
            deep = deep.join("Ayrıntılı alt klasör adı");
        }
        let f = deep.join("Dilekçe — Çağrı'nın düzeltilmiş kopyası.docx");
        write_new(&f, b"ilk").unwrap();
        assert_eq!(
            write_new(&f, b"ikinci").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        write(&f, b"yeni").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"yeni");
        assert_eq!(
            write_new_unique(&deep, "Dilekçe — Çağrı'nın düzeltilmiş kopyası.docx", b"x").unwrap(),
            "Dilekçe — Çağrı'nın düzeltilmiş kopyası (2).docx"
        );
        assert_eq!(
            std::fs::read_dir(&deep).unwrap().count(),
            2,
            "geçici dosya kaldı"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn split_name_handles_extension_dotfiles_and_bare_names() {
        assert_eq!(split_name("a.docx"), ("a", Some("docx")));
        assert_eq!(
            split_name("a - Düzeltilmiş.udf"),
            ("a - Düzeltilmiş", Some("udf"))
        );
        assert_eq!(split_name("noext"), ("noext", None));
        assert_eq!(split_name(".gizli"), (".gizli", None));
        assert_eq!(split_name("bitiyor."), ("bitiyor.", None));
    }
}
