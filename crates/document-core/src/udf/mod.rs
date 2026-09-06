//! UDF (UYAP Doküman Formatı) codec.
//!
//! A UDF file is a ZIP archive whose only required member is `content.xml` (EXP-001).
//! `content.xml` holds a flat character buffer plus an element tree that addresses it by
//! `(startOffset, length)` (EXP-002).
//!
//! Everything in this module is derived from the black-box observations recorded in
//! `docs/format-research/`. Nothing here is derived from any other implementation.

pub mod reader;
pub mod writer;

/// `format_id` written by Tavzih. `1.8` is the version the current UYAP editor produces
/// and the majority of observed specimens carry (EXP-001).
pub const FORMAT_ID: &str = "1.8";

/// The style name every observed specimen uses as its root resolver (EXP-003).
pub const DEFAULT_RESOLVER: &str = "hvl-default";

/// The single character an `<image>` occupies in the content buffer (EXP-002).
pub const IMAGE_PLACEHOLDER: char = '\u{00B8}';

/// UDF alignment enumeration — `javax.swing.text.StyleConstants` (EXP-004).
pub mod align {
    pub const LEFT: u8 = 0;
    pub const CENTER: u8 = 1;
    pub const RIGHT: u8 = 2;
    pub const JUSTIFIED: u8 = 3;
}

/// `paperOrientation` — `java.awt.print.PageFormat` (EXP-009).
pub mod orientation {
    pub const LANDSCAPE: u8 = 0;
    pub const PORTRAIT: u8 = 1;
}

/// `mediaSizeName` — only the A4 value is observed (EXP-009).
pub const MEDIA_A4: u8 = 1;

/// List style tokens actually observed in the corpus (EXP-007).
pub mod list_tokens {
    pub const NUMBER_DOT: &str = "NUMBER_TYPE_NUMBER_DOT";
    pub const CHAR_SMALL_PAREN: &str = "NUMBER_TYPE_CHAR_SMALL_PARANTHESE";
    pub const BULLET_ELLIPSE: &str = "BULLET_TYPE_ELLIPSE";
}
