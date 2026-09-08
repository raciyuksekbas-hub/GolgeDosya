use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EklerError {
    #[error("Girdi dosyası bulunamadı: {0}")]
    FileNotFound(PathBuf),

    #[error("Dosya okunamadı veya erişilemedi: {path}: {source}")]
    IoError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Desteklenmeyen dosya biçimi: {0}")]
    UnsupportedFormat(String),

    #[error("Bozuk veya geçersiz PDF dosyası: {0}")]
    InvalidPdf(String),

    #[error("Bozuk veya geçersiz UDF dosyası: {0}")]
    InvalidUdf(String),

    #[error("Bozuk veya geçersiz görsel dosyası: {0}")]
    InvalidImage(String),

    #[error("Elektronik imzalı UDF için kullanıcı onayı verilmedi: {0}")]
    UnapprovedSignedUdf(PathBuf),

    #[error("Sayfa hesaplama uyumsuzluğu (Strict Page Accounting Failure): {expected} beklenen sayfa, ancak {actual} sayfa bulundu")]
    PageAccountingMismatch { expected: usize, actual: usize },

    #[error("Hedef dosya boyutu sınırı aşıldı: {actual_bytes} bayt > {limit_bytes} bayt sınırı")]
    SizeLimitExceeded { actual_bytes: u64, limit_bytes: u64 },

    #[error("Kaynak dosya bütünlük hatası! Kaynak değiştirilmiş: {path}")]
    SourceIntegrityCompromised { path: PathBuf },

    #[error("Doğrulama hatası (Validation Failed): {0}")]
    ValidationFailed(String),

    #[error("Serileştirme hatası: {0}")]
    SerializationError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, EklerError>;
