//! Stable, user-facing error taxonomy.
//!
//! Every variant carries a Turkish message written for a lawyer, plus optional technical
//! detail kept behind a disclosure in the UI. Raw panics and library errors never reach the
//! user as the primary message.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidExtension,
    MultipleFiles,
    InputNotFound,
    InputUnreadable,
    InputEmpty,
    InvalidDocx,
    InvalidUdf,
    ProtectedOrLegacyDocx,
    UnsupportedFeature,
    UnsafeArchive,
    SourceChanged,
    OutputExistsConflict,
    OutputWriteFailed,
    OutputValidationFailed,
    EngineFailure,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        use ErrorCode::*;
        match self {
            InvalidExtension => "INVALID_EXTENSION",
            MultipleFiles => "MULTIPLE_FILES",
            InputNotFound => "INPUT_NOT_FOUND",
            InputUnreadable => "INPUT_UNREADABLE",
            InputEmpty => "INPUT_EMPTY",
            InvalidDocx => "INVALID_DOCX",
            InvalidUdf => "INVALID_UDF",
            ProtectedOrLegacyDocx => "PROTECTED_OR_LEGACY_DOCX",
            UnsupportedFeature => "UNSUPPORTED_FEATURE",
            UnsafeArchive => "UNSAFE_ARCHIVE",
            SourceChanged => "SOURCE_CHANGED",
            OutputExistsConflict => "OUTPUT_EXISTS_CONFLICT",
            OutputWriteFailed => "OUTPUT_WRITE_FAILED",
            OutputValidationFailed => "OUTPUT_VALIDATION_FAILED",
            EngineFailure => "ENGINE_FAILURE",
        }
    }

    /// Message shown to the user. Turkish, plain, non-technical, and explicit about whether
    /// the source file was touched.
    pub fn message_tr(self) -> &'static str {
        use ErrorCode::*;
        match self {
            InvalidExtension => {
                "Bu dosya türü desteklenmiyor. Yalnızca .docx ve .udf dosyaları dönüştürülebilir."
            }
            MultipleFiles => {
                "Aynı anda yalnızca tek bir dosya dönüştürülebilir. Lütfen tek dosya bırakın."
            }
            InputNotFound => "Dosya bulunamadı. Taşınmış veya silinmiş olabilir.",
            InputUnreadable => "Dosya okunamadı. Dosya izinlerini kontrol edin.",
            // 0 baytlık dosya "geçerli bir belge değil ya da bozulmuş" diye
            // düşüyordu; yarım kalmış bir indirme ya da kayıt böyle görünür.
            InputEmpty => {
                "Dosya boş (0 bayt). Belgenin tamamen indirildiğinden ya da kaydedildiğinden emin olun."
            }
            InvalidDocx => "Bu dosya geçerli bir Word (.docx) belgesi değil ya da bozulmuş.",
            InvalidUdf => "Bu dosya geçerli bir UDF belgesi değil ya da bozulmuş.",
            // Word parola korumalı belgeyi bir OLE kabına koyar; eski .doc da
            // aynı kaptır. Eskiden "Word belgesi değil ya da bozulmuş" deniyordu;
            // kullanıcı parolayı kaldırması gerektiğini öğrenemiyordu (OR-07).
            ProtectedOrLegacyDocx => {
                "Belge parola korumalı ya da eski Word (.doc) biçiminde. Word'de açıp parolayı kaldırın ve Word Belgesi (.docx) olarak yeniden kaydedin."
            }
            UnsupportedFeature => "Belge, güvenle dönüştürülemeyecek bir özellik içeriyor.",
            UnsafeArchive => {
                "Belge güvenlik sınırlarını aşıyor ve güvenlik gerekçesiyle işlenmedi."
            }
            SourceChanged => {
                "Kaynak dosya dönüştürme sırasında değişti. İşlem güvenlik gerekçesiyle durduruldu."
            }
            OutputExistsConflict => {
                "Hedef dosya adı oluşturulamadı. Klasörde çok fazla aynı adlı dosya var."
            }
            OutputWriteFailed => {
                "Dönüştürülen belge kaydedilemedi. Klasör izinlerini kontrol edin."
            }
            OutputValidationFailed => {
                "Dönüştürülen belge doğrulamayı geçemedi ve bu nedenle kaydedilmedi."
            }
            EngineFailure => "Dönüştürme sırasında beklenmeyen bir hata oluştu.",
        }
    }

    /// Whether this failure can leave the source file in any way altered.
    /// Every variant answers `false`: Tavzih opens sources read-only, always.
    pub fn source_untouched(self) -> bool {
        true
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{}: {}", code.as_str(), detail)]
pub struct ConvError {
    pub code: ErrorCode,
    /// Technical detail for the disclosure panel and local diagnostics.
    /// Must never contain document text.
    pub detail: String,
}

impl ConvError {
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub fn invalid_docx(d: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidDocx, d)
    }

    pub fn invalid_udf(d: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidUdf, d)
    }

    pub fn unsafe_archive(d: impl Into<String>) -> Self {
        Self::new(ErrorCode::UnsafeArchive, d)
    }

    pub fn engine(d: impl Into<String>) -> Self {
        Self::new(ErrorCode::EngineFailure, d)
    }
}

pub type Result<T> = std::result::Result<T, ConvError>;
