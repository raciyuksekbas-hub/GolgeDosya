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

pub const REL_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const REL_IMAGE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
pub const REL_STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
pub const REL_NUMBERING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
pub const REL_HEADER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
pub const REL_FOOTER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";
