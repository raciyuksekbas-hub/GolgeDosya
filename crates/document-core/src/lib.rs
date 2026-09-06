//! Tavzih conversion engine.
//!
//! DOCX <-> UDF through a format-neutral canonical document model:
//!
//! ```text
//!   DOCX ──reader──▶ Document ──writer──▶ UDF
//!   UDF  ──reader──▶ Document ──writer──▶ DOCX
//! ```
//!
//! Neither format module depends on the other. The model is the only shared vocabulary,
//! so each side can be tested on its own and a fidelity bug has exactly one home.
//!
//! The engine performs no network I/O of any kind and touches only the two files it is
//! given: it reads the source and writes the target. It never modifies the source.

pub mod convert;
pub mod docx;
pub mod error;
pub mod model;
pub mod security;
pub mod udf;
pub mod units;
pub mod warnings;
pub mod xml;

#[cfg(test)]
pub(crate) mod testutil;

pub use convert::{
    convert_bytes, convert_file, detect_direction, ConversionResult, Direction, Status,
};
pub use error::{ConvError, ErrorCode};
pub use model::Document;
pub use warnings::{Severity, Warning, WarningCode};

/// Engine version, reported in the UI's About surface and in the release gate.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
