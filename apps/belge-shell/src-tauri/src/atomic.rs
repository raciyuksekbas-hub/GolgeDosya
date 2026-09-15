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
use std::path::Path;

pub fn write(destination: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = destination.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.as_file_mut().write_all(bytes)?;
    staged.as_file_mut().flush()?;
    staged.as_file().sync_all()?;
    // persist (noclobber DEĞİL): hedef varsa atomik olarak değiştirilir.
    staged.persist(destination).map_err(|e| e.error).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "belge-atomic-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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
}
