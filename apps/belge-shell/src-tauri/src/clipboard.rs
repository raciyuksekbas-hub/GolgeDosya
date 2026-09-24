//! Panodan belge açmak (saha maddesi 21: "Ctrl+V aktif olsun").
//!
//! Gezgin'de/Finder'da kopyalanan bir dosya WebView'e YOLSUZ bir `File`
//! olarak gelir; GölgeDosya ise belgeleri yol ile açar (son belgeler, çıktı
//! klasörü, kipler arası taşıma). Bu yüzden pano yerelden okunur: dosya
//! listesi (Windows CF_HDROP, macOS dosya URL'leri) ve metin olarak
//! yapıştırılmış bir yol. Yalnız VAR OLAN, mutlak yollu DOSYALAR döner; tür
//! süzgeci arayüzde, açık kipe göre uygulanır.
//!
//! Komut yalnız kullanıcının yapıştırma kısayoluyla ve belge yüzeyinde
//! çağrılır; pano içeriği hiçbir yere gönderilmez ve saklanmaz.
use std::path::Path;

/// Tek yapıştırmada en çok bu kadar belge (iki belgelik kip için bol).
const MAX_PASTED: usize = 10;

/// Metinden yol çıkarır: her satır bir aday; tırnaklar ve boşluklar atılır,
/// yalnız var olan mutlak yollu dosyalar kalır.
pub fn paths_from_text(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim().trim_matches('"').trim())
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let p = Path::new(line);
            p.is_absolute() && p.is_file()
        })
        .map(str::to_string)
        .collect()
}

/// Sırayı koruyarak tekilleştirir ve sınırlar.
fn finish(candidates: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in candidates {
        if !out.contains(&c) && Path::new(&c).is_file() {
            out.push(c);
        }
        if out.len() == MAX_PASTED {
            break;
        }
    }
    out
}

#[tauri::command]
pub fn pasted_document_paths() -> Vec<String> {
    let (files, text) = native::read();
    finish(
        files
            .into_iter()
            .chain(paths_from_text(&text.unwrap_or_default())),
    )
}

#[cfg(windows)]
mod native {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT};
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    /// Pano: kopyalanan dosyalar ve düz metin.
    pub fn read() -> (Vec<String>, Option<String>) {
        let mut files = Vec::new();
        let mut text = None;
        unsafe {
            if OpenClipboard(None).is_err() {
                return (files, text);
            }
            if IsClipboardFormatAvailable(u32::from(CF_HDROP.0)).is_ok() {
                if let Ok(handle) = GetClipboardData(u32::from(CF_HDROP.0)) {
                    let drop = HDROP(handle.0);
                    let count = DragQueryFileW(drop, u32::MAX, None);
                    for i in 0..count {
                        let len = DragQueryFileW(drop, i, None) as usize;
                        let mut buf = vec![0u16; len + 1];
                        let written = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
                        files.push(String::from_utf16_lossy(&buf[..written]));
                    }
                }
            }
            if IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)).is_ok() {
                if let Ok(handle) = GetClipboardData(u32::from(CF_UNICODETEXT.0)) {
                    let global = HGLOBAL(handle.0);
                    let ptr = GlobalLock(global) as *const u16;
                    if !ptr.is_null() {
                        let mut len = 0usize;
                        // Metin panoda NUL ile biter; yol için 32 K karakter bol.
                        while len < 32_768 && *ptr.add(len) != 0 {
                            len += 1;
                        }
                        text = Some(String::from_utf16_lossy(std::slice::from_raw_parts(
                            ptr, len,
                        )));
                        let _ = GlobalUnlock(global);
                    }
                }
            }
            let _ = CloseClipboard();
        }
        (files, text)
    }
}

#[cfg(target_os = "macos")]
mod native {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypeString};
    use objc2_foundation::NSURL;

    /// Pano: kopyalanan dosyalar (her öğenin dosya URL'si) ve düz metin.
    pub fn read() -> (Vec<String>, Option<String>) {
        let board = NSPasteboard::generalPasteboard();
        let mut files = Vec::new();
        if let Some(items) = board.pasteboardItems() {
            for item in items.iter() {
                let Some(url) = item.stringForType(unsafe { NSPasteboardTypeFileURL }) else {
                    continue;
                };
                if let Some(path) = NSURL::URLWithString(&url).and_then(|u| u.path()) {
                    files.push(path.to_string());
                }
            }
        }
        let text = board
            .stringForType(unsafe { NSPasteboardTypeString })
            .map(|s| s.to_string());
        (files, text)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod native {
    pub fn read() -> (Vec<String>, Option<String>) {
        (Vec::new(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_path_opens_only_if_it_is_an_existing_absolute_file() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Çağrı Şahin").join("Müvekkil'in Dosyası");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("dilekçe (son).docx");
        std::fs::write(&file, b"PK").unwrap();
        let shown = file.display().to_string();
        let text = format!(
            "  \"{shown}\"  \r\n{folder}\r\ngöreli/yol.docx\r\n{missing}\r\nDavacı vekili",
            folder = folder.display(),
            missing = folder.join("yok.pdf").display(),
        );
        assert_eq!(paths_from_text(&text), vec![shown]);
    }

    #[test]
    fn duplicates_are_dropped_and_the_count_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<String> = (0..12)
            .map(|i| {
                let p = dir.path().join(format!("belge-{i}.pdf"));
                std::fs::write(&p, b"%PDF-").unwrap();
                p.display().to_string()
            })
            .collect();
        let mut candidates = files.clone();
        candidates.insert(1, files[0].clone());
        let out = finish(candidates);
        assert_eq!(out.len(), MAX_PASTED);
        assert_eq!(out[0], files[0]);
        assert_eq!(out[1], files[1]);
    }
}
