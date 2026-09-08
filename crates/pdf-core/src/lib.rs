//! PDF alanı.
//!
//! Bu crate, `ekler-core` içinden çıkarıldı. Kesim hattı ölçülerek seçildi:
//! taşımadan önce `pdf` modülü crate içinde yalnız `error` (3 kullanım) ve
//! `model` (1 kullanım — `StampConfig`/`StampPosition`) modüllerine, `optimizer`
//! ise yalnız `error`'a bağlıydı.
//!
//! Modül grafiği `pdf → model → optimizer → error` bir DAG'dır; döngü yoktu.
//! Döngü ancak crate sınırı `model` uygulama tarafında kalacak şekilde
//! çizilince doğuyordu: `pdf-core::pdf → app::model → pdf-core::optimizer`.
//! Bu yüzden üç tip — `StampConfig`, `StampPosition`, `OptimizationLevel` —
//! buraya taşındı. Üçü de PDF alanının kavramıdır; `model` içinde durmaları
//! tarihsel bir kazaydı. `ekler-core` onları yeniden dışa aktardığı için genel
//! API değişmedi.
//!
//! Mimari değişmezler (`scripts/check-architecture.sh` ile denetlenir):
//!   * `document-core -> pdf-core`   YASAK
//!   * `pdf-core -> document-core`   serbest
//!   * `pdf-core` **hiçbir dış süreç başlatmaz** — süreç yüzeyi
//!     `process-bridge` crate'inde yaşar.

pub mod error;
pub mod optimizer;
pub mod pdf;

pub use error::{EklerError, Result};
pub use optimizer::{optimize_pdf, OptimizationLevel, OptimizationResult};
pub use pdf::{load_pdf_tolerant, TolerantLoadResult};
