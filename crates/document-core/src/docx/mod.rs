//! DOCX (OOXML WordprocessingML) codec.
//!
//! Implemented from ECMA-376 / the published Microsoft OOXML documentation. Reads and
//! writes the package directly on top of `zip` + `quick-xml` rather than adopting a heavy
//! OOXML library, because Tavzih needs precise control in both directions and a small
//! dependency surface.

pub mod reader;
pub mod writer;

pub const NS_W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const NS_WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
pub const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const NS_PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
pub const NS_CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
pub const NS_PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// Word'ün vurgulayıcı renkleri (`ST_HighlightColor`) ve RGB karşılıkları.
/// Okurken ad → renk; yazarken tam eşleşen renk vurgulayıcı olur, öteki renk
/// run gölgelemesi (`w:shd`) olarak yazılır.
pub const HIGHLIGHT_COLORS: &[(&str, &str)] = &[
    ("yellow", "FFFF00"),
    ("green", "00FF00"),
    ("cyan", "00FFFF"),
    ("magenta", "FF00FF"),
    ("blue", "0000FF"),
    ("red", "FF0000"),
    ("darkBlue", "000080"),
    ("darkCyan", "008080"),
    ("darkGreen", "008000"),
    ("darkMagenta", "800080"),
    ("darkRed", "800000"),
    ("darkYellow", "808000"),
    ("darkGray", "808080"),
    ("lightGray", "C0C0C0"),
    ("black", "000000"),
    ("white", "FFFFFF"),
];

pub const REL_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const REL_IMAGE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
pub const REL_STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
pub const REL_SETTINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
pub const REL_NUMBERING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
pub const REL_HEADER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
pub const REL_FOOTER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";
