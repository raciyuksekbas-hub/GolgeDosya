//! Değişmez denetimleri: çökme/sonsuz döngü koruması, katı yeniden açma,
//! sayfa modeli, kaynak özeti ve kısmi çıktı taraması.

use ekler_core::pdf::{self, load_pdf_tolerant};
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Tauri'nin engelleyen iş parçacıklarıyla aynı mertebede yığın. Üretimde
/// komutlar bu büyüklükte bir yığında koşar; testte daha büyüğüyle koşmak yığın
/// taşmasını gizlerdi.
pub const WORKER_STACK: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub enum Guarded<T> {
    Done(T),
    Panicked(String),
    TimedOut,
}

impl<T: std::fmt::Debug> Guarded<T> {
    pub fn expect_done(self, label: &str) -> T {
        match self {
            Guarded::Done(v) => v,
            Guarded::Panicked(p) => panic!("{label}: PANİK: {p}"),
            Guarded::TimedOut => panic!("{label}: ZAMAN AŞIMI (sonsuz döngü ya da kilitlenme)"),
        }
    }
}

/// `f`'yi ayrı bir iş parçacığında, `timeout` içinde koşar. Panik yakalanır;
/// süre aşılırsa iş parçacığı bırakılır (öldürülemez) ve TimedOut döner.
pub fn guarded<T, F>(timeout: Duration, f: F) -> Guarded<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .stack_size(WORKER_STACK)
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            let _ = tx.send(result);
        });
    if spawned.is_err() {
        return Guarded::Panicked("iş parçacığı başlatılamadı".into());
    }
    match rx.recv_timeout(timeout) {
        Ok(Ok(v)) => Guarded::Done(v),
        Ok(Err(p)) => Guarded::Panicked(
            p.downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "bilinmeyen panik".into()),
        ),
        Err(mpsc::RecvTimeoutError::Timeout) => Guarded::TimedOut,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Guarded::Panicked("iş parçacığı sonuç göndermeden bitti".into())
        }
    }
}

pub fn sha(path: &Path) -> String {
    ekler_core::calculate_sha256(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Klasördeki dosya adları (alt dizinler dâhil, göreli).
pub fn tree(dir: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.insert(p.strip_prefix(root).unwrap().display().to_string());
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(dir, dir, &mut out);
    out
}

/// Çıktıyı kabuğun yolundan katı biçimde yeniden açar: onarım gerekmemeli,
/// katı doğrulama geçmeli.
pub fn reopen_strict(path: &Path) -> Result<Document, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("okunamadı: {e}"))?;
    if bytes.is_empty() {
        return Err("0 bayt".into());
    }
    let loaded = load_pdf_tolerant(&bytes, "cikti.pdf").map_err(|e| e.to_string())?;
    if loaded.is_repaired {
        return Err(format!("çıktı onarım gerektirdi: {:?}", loaded.repair_note));
    }
    pdf::validate_document(&loaded.document).map_err(|e| e.to_string())?;
    Ok(loaded.document)
}

/// Sayfadaki `(Mk N)` işareti: corpus'un her sayfasına kimliği yazılır. Filigran
/// ve numara ayrı içerik akışı olarak eklendiğinden işaret korunur.
pub fn page_marker(doc: &Document, page: ObjectId) -> Option<u32> {
    let content = doc.get_page_content(page).ok()?;
    let text = String::from_utf8_lossy(&content);
    let after = text.split("(Mk ").nth(1)?;
    after.split(')').next()?.trim().parse().ok()
}

pub fn markers(doc: &Document) -> Vec<Option<u32>> {
    doc.get_pages()
        .into_values()
        .map(|id| page_marker(doc, id))
        .collect()
}

/// Sayfanın etkin (miras dâhil) dönüşü; dolaylı ya da ondalık değer çözülür.
pub fn rotation(doc: &Document, page: ObjectId) -> i64 {
    let resolved = pdf::resolved_page_dictionary(doc, page).expect("sayfa sözlüğü");
    match resolved.get(b"Rotate") {
        Ok(v) => match doc.dereference(v).map(|(_, v)| v) {
            Ok(Object::Integer(i)) => *i,
            Ok(Object::Real(r)) => *r as i64,
            _ => 0,
        },
        Err(_) => 0,
    }
}

pub type Rect = [f64; 4];

fn rect(doc: &Document, value: &Object) -> Option<Rect> {
    let (_, v) = doc.dereference(value).ok()?;
    let a = v.as_array().ok()?;
    if a.len() != 4 {
        return None;
    }
    let mut out = [0f64; 4];
    for (i, item) in a.iter().enumerate() {
        out[i] = match doc.dereference(item).ok()?.1 {
            Object::Integer(n) => *n as f64,
            Object::Real(r) => *r as f64,
            _ => return None,
        };
    }
    // Köşe sırası serbesttir (ISO 32000-1 §7.9.5); normalize edilir.
    Some([
        out[0].min(out[2]),
        out[1].min(out[3]),
        out[0].max(out[2]),
        out[1].max(out[3]),
    ])
}

/// (MediaBox, CropBox) — ikisi de normalize.
pub fn boxes(doc: &Document, page: ObjectId) -> (Option<Rect>, Option<Rect>) {
    let resolved = pdf::resolved_page_dictionary(doc, page).expect("sayfa sözlüğü");
    (
        resolved.get(b"MediaBox").ok().and_then(|v| rect(doc, v)),
        resolved.get(b"CropBox").ok().and_then(|v| rect(doc, v)),
    )
}

pub fn contains(outer: &Rect, inner: &Rect, tolerance: f64) -> bool {
    outer[0] <= inner[0] + tolerance
        && outer[1] <= inner[1] + tolerance
        && outer[2] + tolerance >= inner[2]
        && outer[3] + tolerance >= inner[3]
}

/// Geçici dizin; test bitince silinir.
pub struct Lab {
    pub dir: tempfile::TempDir,
}

impl Lab {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("geçici dizin"),
        }
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    pub fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.path(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, bytes).unwrap();
        p
    }
}
