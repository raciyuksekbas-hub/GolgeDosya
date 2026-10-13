//! Defensive limits for untrusted archives.
//!
//! Both DOCX and UDF are ZIP archives supplied by third parties (courts, opposing counsel,
//! e-mail attachments). They are treated as hostile input.
//!
//! Threats addressed here:
//!   * zip bombs (compression ratio and absolute decompressed size),
//!   * Zip Slip / path traversal via crafted entry names,
//!   * absolute paths and Windows drive letters in entry names,
//!   * entry-count exhaustion,
//!   * unbounded XML nesting and entity expansion,
//!   * oversized media.
//!
//! Not addressed here because they are handled by construction:
//!   * XXE — the XML reader is a non-validating pull parser that never resolves
//!     DOCTYPE/ENTITY declarations; `reject_doctype` refuses documents that contain one,
//!   * network access — the engine performs no I/O other than the two files it is given.

use crate::error::{ConvError, ErrorCode, Result};

/// Total bytes any single archive may decompress to.
pub const MAX_TOTAL_UNCOMPRESSED: u64 = 512 * 1024 * 1024;
/// Bytes any single archive entry may decompress to.
pub const MAX_ENTRY_UNCOMPRESSED: u64 = 128 * 1024 * 1024;
/// Entries an archive may contain.
pub const MAX_ENTRIES: usize = 4_096;
/// Maximum ratio of decompressed to compressed bytes before we call it a bomb.
pub const MAX_COMPRESSION_RATIO: u64 = 200;
/// Below this, a high ratio is unremarkable (tiny highly-compressible XML).
pub const RATIO_CHECK_FLOOR: u64 = 1024 * 1024;
/// Maximum XML element nesting depth.
pub const MAX_XML_DEPTH: usize = 256;
/// Maximum decoded bytes for one image.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// Maximum decoded bytes for all images in one document.
pub const MAX_TOTAL_IMAGE_BYTES: usize = 256 * 1024 * 1024;
/// Maximum characters in a UDF content buffer.
pub const MAX_CONTENT_CHARS: usize = 64 * 1024 * 1024;
/// Maximum structural elements we will build from one document.
pub const MAX_ELEMENTS: usize = 2_000_000;
/// UDF öğelerinin içerik tamponundan toplam en çok kaç katını okuyabileceği.
///
/// Saha denetimi (OR-03): tampon boyutu sınırlıydı ama aynı tampona yapılan
/// başvuruların toplamı değildi. 1 milyon karakterlik tamponu 300 paragrafta
/// baştan sona okutan 1,5 KB'lık bir UDF 1,5 GB bellek tüketti. Gerçek UYAP
/// belgesi her birime en çok bir kez başvurur: 294 gerçek belgede en yüksek
/// oran 1,0. İki kat, gerçek belgelere geniş pay bırakır.
pub const MAX_CONTENT_REFERENCE_RATIO: usize = 2;
/// Bu kadar UTF-16 birime kadar başvuru oranına bakılmaz (küçük tampon).
pub const CONTENT_REFERENCE_FLOOR: usize = 1024 * 1024;

/// Reject an archive entry name that could escape the extraction root, even though Tavzih
/// never extracts to disk. Enforced anyway: defence in depth costs nothing here, and it
/// also rejects archives that are malformed in ways worth refusing outright.
pub fn check_entry_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(ConvError::unsafe_archive("empty archive entry name"));
    }
    if name.len() > 1024 {
        return Err(ConvError::unsafe_archive(
            "archive entry name exceeds 1024 bytes",
        ));
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return Err(ConvError::unsafe_archive(
            "absolute path in archive entry name",
        ));
    }
    // Windows drive letter, e.g. "C:\evil".
    let b = name.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return Err(ConvError::unsafe_archive(
            "drive-qualified path in archive entry name",
        ));
    }
    for seg in name.split(['/', '\\']) {
        if seg == ".." {
            return Err(ConvError::unsafe_archive(
                "path traversal segment in archive entry name",
            ));
        }
    }
    if name.contains('\0') {
        return Err(ConvError::unsafe_archive("NUL byte in archive entry name"));
    }
    Ok(())
}

/// Running budget for one archive.
#[derive(Debug)]
pub struct ArchiveBudget {
    /// Merkezi dizinde BEYAN edilen boyutların toplamı (erken ret için).
    total_uncompressed: u64,
    /// Gerçekten açılıp okunan baytların toplamı. Beyan saldırganın elindedir;
    /// sınırın asıl dayanağı budur.
    actual_uncompressed: u64,
    /// Gerçek baytlar için toplam sınır ([`MAX_TOTAL_UNCOMPRESSED`]; testte küçültülür).
    total_limit: u64,
    entries: usize,
    image_bytes: usize,
    /// Okunamayan (bozuk) bir girdinin hangi biçim hatasıyla bildirileceği.
    /// Eskiden her okuma hatası "geçerli bir Word belgesi değil" oluyordu;
    /// bozuk bir .udf de kullanıcıya Word dosyası diye anlatılıyordu (OR-06).
    corrupt: ErrorCode,
}

impl Default for ArchiveBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveBudget {
    /// DOCX okuyucusunun bütçesi: bozuk girdi `INVALID_DOCX` olarak bildirilir.
    pub fn new() -> Self {
        Self::for_format(ErrorCode::InvalidDocx)
    }

    /// Bozuk bir girdiyi `corrupt` koduyla bildiren bütçe (UDF: `INVALID_UDF`).
    pub fn for_format(corrupt: ErrorCode) -> Self {
        Self {
            total_uncompressed: 0,
            actual_uncompressed: 0,
            total_limit: MAX_TOTAL_UNCOMPRESSED,
            entries: 0,
            image_bytes: 0,
            corrupt,
        }
    }

    /// Toplam sınırı küçük bir bütçe; yalnız testlerde, sınır davranışını
    /// yüzlerce MiB açmadan sınamak için.
    #[cfg(test)]
    fn with_total_limit(total_limit: u64) -> Self {
        Self {
            total_limit,
            ..Self::new()
        }
    }

    /// Şimdiye kadar gerçekten açılan bayt sayısı.
    pub fn actual_uncompressed(&self) -> u64 {
        self.actual_uncompressed
    }

    pub fn count_entry(&mut self) -> Result<()> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(ConvError::unsafe_archive(format!(
                "archive declares more than {MAX_ENTRIES} entries"
            )));
        }
        Ok(())
    }

    /// Bir arşiv girdisini BEYAN EDİLEN boyuta güvenmeden, sert bir sınıra kadar
    /// okur. Merkezi dizindeki boyut saldırganın denetimindedir: küçük bir boyut
    /// beyan edip (denetimi geçip) deflate akışında gigabaytlarca genişleyen bir
    /// girdi, `read_to_end` ile sınırsız açılıp belleği tüketiyordu. (İkinciGöz
    /// ayrıştırıcısı bunu zaten böyle yapıyordu; dönüşüm motoru yapmıyordu.)
    ///
    /// Okuma üç sınırın en küçüğünde kesilir: beyan edilen boyut, girdi başına
    /// `cap` ve arşivin kalan toplam bütçesi. Saha denetimi (OR-02): yalnız
    /// `cap` ile kesmek yetmiyordu. 100 bayt beyan edip 120 MiB'a açılan 16
    /// üst bilgi parçası, her biri girdi sınırının altında kaldığı için
    /// 1,9 MB'lık bir dosyadan 2,3 GB bellek tüketti; toplam sınıra yalnız
    /// BEYANLAR sayılıyordu. Geçerli bir ZIP girdisi beyanından büyük açılmaz,
    /// açılan girdi yalan söylüyordur ve bir bayt ötesinde reddedilir.
    pub fn read_entry_capped<R: std::io::Read>(
        &mut self,
        mut reader: R,
        declared: u64,
        cap: u64,
    ) -> Result<Vec<u8>> {
        use std::io::Read as _;
        let remaining = self.total_limit.saturating_sub(self.actual_uncompressed);
        let limit = declared.min(cap).min(remaining);
        let mut buf = Vec::with_capacity(limit.min(16 * 1024 * 1024) as usize);
        let read = reader
            .by_ref()
            .take(limit.saturating_add(1))
            .read_to_end(&mut buf)
            .map_err(|e| ConvError::new(self.corrupt, format!("archive entry unreadable: {e}")))?
            as u64;
        if read > limit {
            return Err(ConvError::unsafe_archive(if read > declared {
                format!("archive entry inflates past its declared {declared} bytes")
            } else if read > cap {
                format!("archive entry exceeds {cap} bytes when decompressed")
            } else {
                format!(
                    "archive decompresses to more than {} bytes in total",
                    self.total_limit
                )
            }));
        }
        self.actual_uncompressed = self.actual_uncompressed.saturating_add(read);
        Ok(buf)
    }

    /// Check one entry's declared sizes before reading it.
    pub fn check_entry_size(&mut self, compressed: u64, uncompressed: u64) -> Result<()> {
        if uncompressed > MAX_ENTRY_UNCOMPRESSED {
            return Err(ConvError::unsafe_archive(format!(
                "archive entry decompresses to {uncompressed} bytes, limit {MAX_ENTRY_UNCOMPRESSED}"
            )));
        }
        if uncompressed >= RATIO_CHECK_FLOOR && compressed > 0 {
            let ratio = uncompressed / compressed.max(1);
            if ratio > MAX_COMPRESSION_RATIO {
                return Err(ConvError::unsafe_archive(format!(
                    "archive entry compression ratio {ratio}:1 exceeds {MAX_COMPRESSION_RATIO}:1"
                )));
            }
        }
        self.total_uncompressed = self.total_uncompressed.saturating_add(uncompressed);
        if self.total_uncompressed > MAX_TOTAL_UNCOMPRESSED {
            return Err(ConvError::unsafe_archive(format!(
                "archive decompresses to more than {MAX_TOTAL_UNCOMPRESSED} bytes in total"
            )));
        }
        Ok(())
    }

    pub fn add_image(&mut self, len: usize) -> Result<()> {
        if len > MAX_IMAGE_BYTES {
            return Err(ConvError::unsafe_archive(format!(
                "single image of {len} bytes exceeds limit {MAX_IMAGE_BYTES}"
            )));
        }
        self.image_bytes = self.image_bytes.saturating_add(len);
        if self.image_bytes > MAX_TOTAL_IMAGE_BYTES {
            return Err(ConvError::unsafe_archive(format!(
                "images total more than {MAX_TOTAL_IMAGE_BYTES} bytes"
            )));
        }
        Ok(())
    }
}

/// Refuse XML that carries a DOCTYPE. No legitimate DOCX or UDF has one, and its presence
/// is the entry point for entity-expansion and external-entity attacks. Refusing beats
/// relying on a parser flag.
pub fn reject_doctype(xml: &[u8]) -> Result<()> {
    // Only scan the prolog; a later "<!DOCTYPE" inside text content is harmless.
    let head = &xml[..xml.len().min(8192)];
    let needle = b"<!DOCTYPE";
    let ncase = b"<!doctype";
    for w in head.windows(needle.len()) {
        if w == needle || w == ncase {
            return Err(ConvError::unsafe_archive("XML declares a DOCTYPE; refused"));
        }
    }
    Ok(())
}

/// Depth guard used by both XML readers.
#[derive(Debug, Default)]
pub struct DepthGuard {
    depth: usize,
    elements: usize,
}

impl DepthGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        self.elements += 1;
        if self.depth > MAX_XML_DEPTH {
            return Err(ConvError::unsafe_archive(format!(
                "XML nesting deeper than {MAX_XML_DEPTH}"
            )));
        }
        if self.elements > MAX_ELEMENTS {
            return Err(ConvError::unsafe_archive(format!(
                "XML contains more than {MAX_ELEMENTS} elements"
            )));
        }
        Ok(())
    }

    pub fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        for bad in [
            "../../etc/passwd",
            "word/../../evil.xml",
            "/etc/passwd",
            "\\windows\\system32",
            "C:\\evil",
            "a/../../b",
            "",
        ] {
            let e = check_entry_name(bad).unwrap_err();
            assert_eq!(e.code, ErrorCode::UnsafeArchive, "should reject {bad:?}");
        }
    }

    #[test]
    fn accepts_normal_ooxml_names() {
        for good in [
            "word/document.xml",
            "[Content_Types].xml",
            "word/media/image1.png",
            "_rels/.rels",
            "content.xml",
        ] {
            assert!(check_entry_name(good).is_ok(), "should accept {good}");
        }
    }

    #[test]
    fn catches_a_zip_bomb_by_ratio() {
        let mut b = ArchiveBudget::new();
        // 2 MiB from 1 KiB is 2048:1.
        let e = b.check_entry_size(1024, 2 * 1024 * 1024).unwrap_err();
        assert_eq!(e.code, ErrorCode::UnsafeArchive);
    }

    #[test]
    fn read_cap_rejects_an_entry_that_lies_about_its_size() {
        // Saldırgan küçük bir boyut beyan eder (denetimi geçer) ama akış çok daha
        // büyüktür. Sert sınır gerçek okumayı yakalar; beyan edilene güvenilmez.
        let mut b = ArchiveBudget::new();
        let actual = vec![0u8; 5000];
        // Beyan 10 bayt (yalan), sınır 1000: 5000 baytlık gerçek akış reddedilir.
        let e = b
            .read_entry_capped(actual.as_slice(), 10, 1000)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::UnsafeArchive);
    }

    /// Sonsuz bir akış; kaç bayt verildiğini sayar.
    struct Endless {
        pulled: u64,
    }

    impl std::io::Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            buf.fill(b' ');
            self.pulled += buf.len() as u64;
            Ok(buf.len())
        }
    }

    #[test]
    fn a_lying_entry_is_cut_one_byte_past_its_declaration() {
        // OR-02: 100 bayt beyan eden girdi gerçekte sınırsız açılıyor.
        // Eskiden okuma girdi sınırına (128 MiB) kadar sürüyordu; şimdi
        // beyanın bir bayt ötesinde durur.
        let mut b = ArchiveBudget::new();
        let mut endless = Endless { pulled: 0 };
        let e = b
            .read_entry_capped(&mut endless, 100, MAX_ENTRY_UNCOMPRESSED)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::UnsafeArchive);
        assert!(endless.pulled <= 101, "{} bayt açıldı", endless.pulled);
        assert_eq!(b.actual_uncompressed(), 0);
    }

    #[test]
    fn actual_bytes_count_against_the_total_budget() {
        // Dürüst beyanlı girdiler de toplamda sınırı aşamaz; sayılan GERÇEK
        // baytlardır ve okuma kalan bütçenin bir bayt ötesinde kesilir.
        let mut b = ArchiveBudget::with_total_limit(1000);
        let first = vec![1u8; 600];
        assert_eq!(
            b.read_entry_capped(first.as_slice(), 600, 10_000)
                .unwrap()
                .len(),
            600
        );
        assert_eq!(b.actual_uncompressed(), 600);
        let mut endless = Endless { pulled: 0 };
        let e = b.read_entry_capped(&mut endless, 600, 10_000).unwrap_err();
        assert_eq!(e.code, ErrorCode::UnsafeArchive);
        assert!(endless.pulled <= 401, "{} bayt açıldı", endless.pulled);
    }

    #[test]
    fn a_damaged_entry_is_reported_as_the_format_being_read() {
        // OR-06: bozuk bir .udf "geçerli bir Word belgesi değil" diye
        // bildiriliyordu.
        struct Broken;
        impl std::io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid checksum",
                ))
            }
        }
        let e = ArchiveBudget::for_format(ErrorCode::InvalidUdf)
            .read_entry_capped(Broken, 10, 1000)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidUdf);
        let e = ArchiveBudget::new()
            .read_entry_capped(Broken, 10, 1000)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidDocx);
    }

    #[test]
    fn read_cap_allows_an_honest_entry() {
        let mut b = ArchiveBudget::new();
        let data = vec![7u8; 500];
        let out = b.read_entry_capped(data.as_slice(), 500, 1000).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn read_cap_allows_an_entry_exactly_at_the_limit() {
        let mut b = ArchiveBudget::new();
        let data = vec![1u8; 1000];
        let out = b.read_entry_capped(data.as_slice(), 1000, 1000).unwrap();
        assert_eq!(out.len(), 1000);
    }

    #[test]
    fn tiny_highly_compressible_xml_is_not_a_bomb() {
        let mut b = ArchiveBudget::new();
        // 300 KiB of XML from 1 KiB is a 300:1 ratio but under the floor, so allowed.
        assert!(b.check_entry_size(1024, 300 * 1024).is_ok());
    }

    #[test]
    fn total_budget_is_enforced_across_entries() {
        let mut b = ArchiveBudget::new();
        for _ in 0..4 {
            // Each is under the per-entry cap and at a safe ratio.
            let _ = b.check_entry_size(64 * 1024 * 1024, 100 * 1024 * 1024);
        }
        let e = b
            .check_entry_size(64 * 1024 * 1024, 200 * 1024 * 1024)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::UnsafeArchive);
    }

    #[test]
    fn doctype_is_refused() {
        assert!(
            reject_doctype(b"<?xml version=\"1.0\"?><!DOCTYPE foo [<!ENTITY x \"y\">]><a/>")
                .is_err()
        );
        assert!(reject_doctype(b"<?xml version=\"1.0\"?><!doctype html><a/>").is_err());
        assert!(reject_doctype(b"<?xml version=\"1.0\"?><template/>").is_ok());
    }

    #[test]
    fn depth_guard_trips() {
        let mut g = DepthGuard::new();
        for _ in 0..MAX_XML_DEPTH {
            g.enter().unwrap();
        }
        assert!(g.enter().is_err());
    }

    #[test]
    fn entry_count_is_bounded() {
        let mut b = ArchiveBudget::new();
        for _ in 0..MAX_ENTRIES {
            b.count_entry().unwrap();
        }
        assert!(b.count_entry().is_err());
    }
}
