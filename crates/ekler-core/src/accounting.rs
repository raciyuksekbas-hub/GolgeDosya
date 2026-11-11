use crate::error::{EklerError, Result};
use crate::model::{NormalizedPage, PhysicalOutput, SourceFile};
#[cfg(not(target_os = "macos"))]
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Result as IoResult};
use std::path::Path;

/// Dosyanın SHA-256 özetini (hex string) hesaplar.
pub fn calculate_sha256(path: &Path) -> IoResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = FileHasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finish_hex())
}

#[cfg(not(target_os = "macos"))]
struct FileHasher(Sha256);

#[cfg(not(target_os = "macos"))]
impl FileHasher {
    fn new() -> Self {
        Self(Sha256::new())
    }
    fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }
    fn finish_hex(self) -> String {
        format!("{:x}", self.0.finalize())
    }
}

/// macOS'ta CommonCrypto: işlemcinin SHA komutlarını kullanır. `sha2` aarch64'te
/// yalnız ayrı bir crate (`sha2-asm`) ile donanıma iner; onsuz yazılımla hesaplar
/// ve her araç kaynağı iki kez özetlediği için büyük dosyada asıl bekleme buydu.
/// Yeni bir bağımlılık değil: işlev libSystem'in içinde.
#[cfg(target_os = "macos")]
struct FileHasher(CcSha256Ctx);

#[cfg(target_os = "macos")]
#[repr(C)]
struct CcSha256Ctx {
    count: [u32; 2],
    hash: [u32; 8],
    wbuf: [u32; 16],
}

#[cfg(target_os = "macos")]
extern "C" {
    fn CC_SHA256_Init(c: *mut CcSha256Ctx) -> i32;
    fn CC_SHA256_Update(c: *mut CcSha256Ctx, data: *const u8, len: u32) -> i32;
    fn CC_SHA256_Final(md: *mut u8, c: *mut CcSha256Ctx) -> i32;
}

#[cfg(target_os = "macos")]
impl FileHasher {
    fn new() -> Self {
        let mut ctx = CcSha256Ctx {
            count: [0; 2],
            hash: [0; 8],
            wbuf: [0; 16],
        };
        // SAFETY: `ctx`, CommonDigest.h'deki CC_SHA256_CTX ile aynı düzende.
        unsafe { CC_SHA256_Init(&mut ctx) };
        Self(ctx)
    }
    fn update(&mut self, data: &[u8]) {
        // Tek çağrıdaki uzunluk u32'dir; dilim her zaman okuma tamponundan küçük.
        // SAFETY: işaretçi ve uzunluk aynı dilimden geliyor.
        unsafe { CC_SHA256_Update(&mut self.0, data.as_ptr(), data.len() as u32) };
    }
    fn finish_hex(mut self) -> String {
        let mut digest = [0u8; 32];
        // SAFETY: `digest` CC_SHA256_DIGEST_LENGTH (32) bayttır.
        unsafe { CC_SHA256_Final(digest.as_mut_ptr(), &mut self.0) };
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Kaynak dosyanın mutasyona uğramadığını doğrular:
/// SHA-256(source_before) == SHA-256(source_after)
pub fn verify_source_integrity(source: &SourceFile) -> Result<()> {
    if !source.path.exists() {
        return Err(EklerError::FileNotFound(source.path.clone()));
    }
    let current_sha256 = calculate_sha256(&source.path).map_err(|e| EklerError::IoError {
        path: source.path.clone(),
        source: e,
    })?;

    if current_sha256 != source.sha256_before {
        return Err(EklerError::SourceIntegrityCompromised {
            path: source.path.clone(),
        });
    }
    Ok(())
}

/// Strict Page Accounting (Katı Sayfa Muhasebesi)
///
/// Bir Ek için kaynak sayfa sayısı ile fiziksel çıktılardaki toplam sayfa sayısının
/// birebir eşit olduğunu, hiçbir sayfanın kaybolmadığını ve mükerrer basılmadığını doğrular.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageAccountingReport {
    pub exhibit_id: String,
    pub input_page_count: usize,
    pub output_page_count: usize,
    pub missing_pages: Vec<usize>,
    pub duplicate_pages: Vec<usize>,
    pub is_valid: bool,
}

pub fn verify_page_accounting(
    exhibit_id: &str,
    normalized_pages: &[NormalizedPage],
    outputs: &[PhysicalOutput],
) -> Result<PageAccountingReport> {
    let input_pages_for_exhibit: Vec<&NormalizedPage> = normalized_pages
        .iter()
        .filter(|p| p.exhibit_id == exhibit_id)
        .collect();

    let input_count = input_pages_for_exhibit.len();

    let mut output_count = 0;
    let mut covered_pages = std::collections::HashSet::new();
    let mut duplicate_pages = Vec::new();

    for out in outputs.iter().filter(|o| o.exhibit_id == exhibit_id) {
        output_count += out.page_count;
        for page_num in out.page_start..=out.page_end {
            if !covered_pages.insert(page_num) {
                duplicate_pages.push(page_num);
            }
        }
    }

    let mut missing_pages = Vec::new();
    for i in 1..=input_count {
        if !covered_pages.contains(&i) {
            missing_pages.push(i);
        }
    }

    let is_valid =
        input_count == output_count && missing_pages.is_empty() && duplicate_pages.is_empty();

    let report = PageAccountingReport {
        exhibit_id: exhibit_id.to_string(),
        input_page_count: input_count,
        output_page_count: output_count,
        missing_pages,
        duplicate_pages,
        is_valid,
    };

    if !is_valid {
        return Err(EklerError::PageAccountingMismatch {
            expected: input_count,
            actual: output_count,
        });
    }

    Ok(report)
}
