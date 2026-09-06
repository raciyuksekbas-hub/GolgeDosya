//! Canonical, format-neutral document model.
//!
//! Both converters go through this type. Neither `docx` nor `udf` may depend on the other;
//! the model is the only shared vocabulary. It is deliberately a plain data structure with
//! no behaviour beyond construction helpers, so it can be unit-tested on its own.
//!
//! Geometry is `f32` **PostScript points** throughout (see docs/format-research/EXP-005).

use std::sync::Arc;

/// A whole document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub meta: Metadata,
    /// At least one section. UDF supports exactly one; DOCX may have several.
    pub sections: Vec<Section>,
    /// List definitions keyed by `ListId`.
    pub lists: Vec<ListDef>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    /// Default font for the document body, used when a run says nothing.
    pub default_font: FontSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontSpec {
    pub family: String,
    pub size_pt: f32,
}

impl Default for FontSpec {
    /// The UYAP body default observed in every specimen (EXP-003).
    fn default() -> Self {
        Self {
            family: "Times New Roman".to_string(),
            size_pt: 12.0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Section {
    pub page: PageSetup,
    pub header: Option<HeaderFooter>,
    pub footer: Option<HeaderFooter>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageSetup {
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_left_pt: f32,
    pub margin_right_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub header_offset_pt: f32,
    pub footer_offset_pt: f32,
    pub orientation: Orientation,
}

impl PageSetup {
    /// A4 portrait with UYAP's mandated 2.5 cm margins (EXP-005, EXP-009).
    pub fn a4_uyap() -> Self {
        Self {
            width_pt: 595.276,
            height_pt: 841.89,
            margin_left_pt: 70.875,
            margin_right_pt: 70.875,
            margin_top_pt: 70.875,
            margin_bottom_pt: 70.875,
            header_offset_pt: 20.0,
            footer_offset_pt: 20.0,
            orientation: Orientation::Portrait,
        }
    }

    /// Portrait-normalised dimensions, so a size is recognised in either orientation.
    fn portrait_wh(&self) -> (f32, f32) {
        match self.orientation {
            Orientation::Portrait => (self.width_pt, self.height_pt),
            Orientation::Landscape => (self.height_pt, self.width_pt),
        }
    }

    /// True when the page is A4 within half a point on both axes.
    pub fn is_a4(&self) -> bool {
        matches!(self.paper_size(), PaperSize::A4)
    }

    /// Identify the paper size. Used so a warning can name what the page actually is —
    /// calling 612x792 pt "A4" would be simply false.
    pub fn paper_size(&self) -> PaperSize {
        let (w, h) = self.portrait_wh();
        const TOL: f32 = 3.0;
        let near = |a: f32, b: f32| (a - b).abs() < TOL;
        for (ps, pw, ph) in PaperSize::KNOWN {
            if near(w, *pw) && near(h, *ph) {
                return *ps;
            }
        }
        PaperSize::Custom
    }

    /// Human-readable size for a warning message, always including the real dimensions.
    pub fn size_label(&self) -> String {
        let (w, h) = (self.width_pt, self.height_pt);
        match self.paper_size() {
            PaperSize::Custom => format!("{w:.0}x{h:.0} pt"),
            other => format!("{} ({w:.0}x{h:.0} pt)", other.as_str()),
        }
    }
}

/// Paper sizes Tavzih can name. Dimensions are portrait, in points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperSize {
    A3,
    A4,
    A5,
    UsLetter,
    UsLegal,
    Custom,
}

impl PaperSize {
    pub const KNOWN: &'static [(PaperSize, f32, f32)] = &[
        (PaperSize::A3, 841.89, 1190.55),
        (PaperSize::A4, 595.276, 841.89),
        (PaperSize::A5, 419.528, 595.276),
        (PaperSize::UsLetter, 612.0, 792.0),
        (PaperSize::UsLegal, 612.0, 1008.0),
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PaperSize::A3 => "A3",
            PaperSize::A4 => "A4",
            PaperSize::A5 => "A5",
            PaperSize::UsLetter => "US Letter",
            PaperSize::UsLegal => "US Legal",
            PaperSize::Custom => "özel boyut",
        }
    }

    /// Portrait dimensions in points, if this is a named size.
    pub fn dimensions(self) -> Option<(f32, f32)> {
        Self::KNOWN
            .iter()
            .find(|(p, _, _)| *p == self)
            .map(|(_, w, h)| (*w, *h))
    }
}

impl Default for PageSetup {
    fn default() -> Self {
        Self::a4_uyap()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

/// A page-number field carried by a UDF header/footer (EXP-013).
///
/// Reconstructed into a real, updatable Word `PAGE` field rather than a frozen number.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageNumberField {
    /// Text shown before the number (`pageNumber-foreStr`), e.g. "Sayfa ".
    pub prefix: Option<String>,
    /// Separator between current and total (`pageNumber-seperator`), e.g. "/".
    /// Its presence is what indicates a "current / total" layout.
    pub separator: Option<String>,
    /// Starting page number (`pageNumber-pageStartNumStr`).
    pub start_at: Option<u32>,
    pub font_family: Option<String>,
    pub font_size_pt: Option<f32>,
    pub bold: bool,
    pub italic: bool,
    pub color: Option<Color>,
    /// `pageNumber-spec` verbatim. Its encoding is not determinable from the corpus, so it
    /// is preserved rather than interpreted.
    pub raw_spec: Option<String>,
}

impl PageNumberField {
    /// True when the field shows "current / total" rather than just the current page.
    pub fn has_total(&self) -> bool {
        self.separator.as_deref().is_some_and(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeaderFooter {
    pub blocks: Vec<Block>,
    /// Present when the source restricts the header/footer to a page range.
    /// `stop_page = Some(1)` is UDF's "first page only" (EXP-009).
    pub stop_page: Option<u32>,
    pub start_page: Option<u32>,
    /// Verbatim `pageNumber-*` attributes, preserved so a UDF -> UDF path loses nothing.
    pub page_number_attrs: Vec<(String, String)>,
    /// The same information, parsed, for emitting a real Word field.
    pub page_number: Option<PageNumberField>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
    /// An explicit hard page break between blocks.
    PageBreak,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Paragraph {
    pub props: ParaProps,
    pub runs: Vec<Run>,
}

impl Paragraph {
    pub fn text(&self) -> String {
        let mut s = String::new();
        for r in &self.runs {
            match r {
                Run::Text { text, .. } => s.push_str(text),
                Run::Tab { .. } => s.push('\t'),
                Run::LineBreak { .. } => s.push('\n'),
                Run::Image(_) => {}
            }
        }
        s
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Convenience for tests and corpus generation.
    pub fn plain(text: &str) -> Self {
        Paragraph {
            props: ParaProps::default(),
            runs: vec![Run::Text {
                text: text.to_string(),
                props: RunProps::default(),
            }],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParaProps {
    pub alignment: Alignment,
    pub left_indent_pt: f32,
    pub right_indent_pt: f32,
    /// Positive first-line indent. Mutually exclusive with `hanging_pt` in DOCX,
    /// but UDF carries both attributes, so the model keeps them apart.
    pub first_line_indent_pt: f32,
    pub hanging_pt: f32,
    pub space_before_pt: f32,
    pub space_after_pt: f32,
    /// Extra line spacing as a fraction of line height: 0.0 = single, 0.5 = 1.5 lines (EXP-004).
    pub line_spacing_extra: f32,
    pub tab_stops: Vec<TabStop>,
    pub list: Option<ListRef>,
    /// Style name from the source, kept for diagnostics and DOCX style emission.
    pub style_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabStop {
    pub pos_pt: f32,
    pub align: TabAlign,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TabAlign {
    #[default]
    Left,
    Center,
    Right,
    Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRef {
    pub list_id: u32,
    /// 1-based nesting depth, matching UDF's `ListLevel`.
    pub level: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListDef {
    pub list_id: u32,
    pub kind: ListKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListKind {
    Bullet(BulletKind),
    Number(NumberKind),
}

/// Only tokens actually observed in the UDF corpus (EXP-007). We do not invent tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletKind {
    Ellipse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberKind {
    /// `1.` — UDF `NUMBER_TYPE_NUMBER_DOT`
    DecimalDot,
    /// `a)` — UDF `NUMBER_TYPE_CHAR_SMALL_PARANTHESE`
    LowerAlphaParen,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Run {
    Text { text: String, props: RunProps },
    Tab { props: RunProps },
    LineBreak { props: RunProps },
    Image(Image),
}

impl Run {
    pub fn props(&self) -> Option<&RunProps> {
        match self {
            Run::Text { props, .. } | Run::Tab { props } | Run::LineBreak { props } => Some(props),
            Run::Image(_) => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunProps {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Not representable in UDF (EXP-003); carried so the converter can warn precisely.
    pub strike: bool,
    pub vert_align: VertAlign,
    pub font_family: Option<String>,
    pub font_size_pt: Option<f32>,
    pub color: Option<Color>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VertAlign {
    #[default]
    Baseline,
    Superscript,
    Subscript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };

    /// Java `Color.getRGB()`: opaque alpha in the high byte, read as a signed i32 (EXP-003).
    pub fn to_java_argb(self) -> i32 {
        (0xFF00_0000u32 | ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32) as i32
    }

    pub fn from_java_argb(v: i32) -> Color {
        let u = v as u32;
        Color {
            r: ((u >> 16) & 0xFF) as u8,
            g: ((u >> 8) & 0xFF) as u8,
            b: (u & 0xFF) as u8,
        }
    }

    pub fn to_hex(self) -> String {
        format!("{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn from_hex(s: &str) -> Option<Color> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        Some(Color {
            r: u8::from_str_radix(&s[0..2], 16).ok()?,
            g: u8::from_str_radix(&s[2..4], 16).ok()?,
            b: u8::from_str_radix(&s[4..6], 16).ok()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// Shared so the bytes are never copied between model, reader and writer.
    pub data: Arc<Vec<u8>>,
    pub format: ImageFormat,
    pub width_pt: f32,
    pub height_pt: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    /// Recognised but not decodable by us (EMF/WMF/SVG/TIFF).
    Unsupported,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::Gif => "gif",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Unsupported => "bin",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Unsupported => "application/octet-stream",
        }
    }

    /// Sniff by magic number rather than trusting a filename extension.
    pub fn sniff(bytes: &[u8]) -> ImageFormat {
        if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            ImageFormat::Png
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            ImageFormat::Jpeg
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            ImageFormat::Gif
        } else if bytes.starts_with(b"BM") {
            ImageFormat::Bmp
        } else {
            ImageFormat::Unsupported
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TableBorder {
    None,
    #[default]
    Cell,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    pub name: Option<String>,
    /// Relative widths of the table's grid columns.
    pub column_widths: Vec<f32>,
    pub border: TableBorder,
    pub rows: Vec<TableRow>,
    pub props: ParaProps,
}

impl Table {
    pub fn column_count(&self) -> usize {
        self.column_widths.len().max(
            self.rows
                .iter()
                .map(|r| r.grid_span_total())
                .max()
                .unwrap_or(0),
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
    pub is_header: bool,
    pub height_pt: Option<f32>,
}

impl TableRow {
    pub fn grid_span_total(&self) -> usize {
        self.cells.iter().map(|c| c.grid_span.max(1)).sum()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableCell {
    pub blocks: Vec<Block>,
    /// How many grid columns this cell covers horizontally (DOCX `w:gridSpan`).
    pub grid_span: usize,
    pub vertical_align: CellVAlign,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CellVAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_argb_roundtrip() {
        // The exact value observed in every UDF specimen for black text.
        assert_eq!(Color::BLACK.to_java_argb(), -16_777_216);
        assert_eq!(Color::from_java_argb(-16_777_216), Color::BLACK);
        for c in [
            Color {
                r: 255,
                g: 255,
                b: 255,
            },
            Color { r: 1, g: 2, b: 3 },
            Color {
                r: 0x12,
                g: 0xAB,
                b: 0xFF,
            },
        ] {
            assert_eq!(Color::from_java_argb(c.to_java_argb()), c);
        }
        // White, as observed on <header background="-1">.
        assert_eq!(
            Color {
                r: 255,
                g: 255,
                b: 255
            }
            .to_java_argb(),
            -1
        );
    }

    #[test]
    fn hex_roundtrip() {
        let c = Color {
            r: 0x1A,
            g: 0x2B,
            b: 0x3C,
        };
        assert_eq!(Color::from_hex(&c.to_hex()), Some(c));
        assert_eq!(
            Color::from_hex("#FF0000"),
            Some(Color { r: 255, g: 0, b: 0 })
        );
        assert_eq!(Color::from_hex("nope"), None);
    }

    #[test]
    fn image_sniffing_ignores_extensions() {
        assert_eq!(
            ImageFormat::sniff(&[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0]),
            ImageFormat::Png
        );
        assert_eq!(
            ImageFormat::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]),
            ImageFormat::Jpeg
        );
        assert_eq!(ImageFormat::sniff(b"GIF89a..."), ImageFormat::Gif);
        assert_eq!(ImageFormat::sniff(b"BM......"), ImageFormat::Bmp);
        assert_eq!(
            ImageFormat::sniff(b"<?xml version"),
            ImageFormat::Unsupported
        );
    }

    #[test]
    fn us_letter_is_never_called_a4() {
        let letter = PageSetup {
            width_pt: 612.0,
            height_pt: 792.0,
            ..PageSetup::a4_uyap()
        };
        assert_eq!(letter.paper_size(), PaperSize::UsLetter);
        assert!(!letter.is_a4());
        assert_eq!(letter.size_label(), "US Letter (612x792 pt)");
        // The A4 label must state A4's real dimensions, not Letter's.
        assert_eq!(PageSetup::a4_uyap().size_label(), "A4 (595x842 pt)");
    }

    #[test]
    fn known_sizes_are_recognised_in_both_orientations() {
        for (ps, w, h) in PaperSize::KNOWN {
            let portrait = PageSetup {
                width_pt: *w,
                height_pt: *h,
                ..PageSetup::a4_uyap()
            };
            assert_eq!(portrait.paper_size(), *ps, "portrait {ps:?}");
            let landscape = PageSetup {
                width_pt: *h,
                height_pt: *w,
                orientation: Orientation::Landscape,
                ..PageSetup::a4_uyap()
            };
            assert_eq!(landscape.paper_size(), *ps, "landscape {ps:?}");
        }
    }

    #[test]
    fn an_unrecognised_size_reports_its_real_dimensions() {
        let odd = PageSetup {
            width_pt: 500.0,
            height_pt: 700.0,
            ..PageSetup::a4_uyap()
        };
        assert_eq!(odd.paper_size(), PaperSize::Custom);
        assert_eq!(odd.size_label(), "500x700 pt");
    }

    #[test]
    fn a4_detection_is_orientation_aware() {
        let p = PageSetup::a4_uyap();
        assert!(p.is_a4());
        let land = PageSetup {
            width_pt: 841.89,
            height_pt: 595.276,
            orientation: Orientation::Landscape,
            ..p
        };
        assert!(land.is_a4());
        let letter = PageSetup {
            width_pt: 612.0,
            height_pt: 792.0,
            ..PageSetup::a4_uyap()
        };
        assert!(!letter.is_a4());
    }

    #[test]
    fn paragraph_text_includes_tabs_not_images() {
        let p = Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run::Text {
                    text: "a".into(),
                    props: RunProps::default(),
                },
                Run::Tab {
                    props: RunProps::default(),
                },
                Run::Image(Image {
                    data: Arc::new(vec![]),
                    format: ImageFormat::Png,
                    width_pt: 1.0,
                    height_pt: 1.0,
                }),
                Run::Text {
                    text: "b".into(),
                    props: RunProps::default(),
                },
            ],
        };
        assert_eq!(p.text(), "a\tb");
    }

    #[test]
    fn grid_span_totals_account_for_merges() {
        let row = TableRow {
            cells: vec![
                TableCell {
                    grid_span: 2,
                    ..Default::default()
                },
                TableCell {
                    grid_span: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(row.grid_span_total(), 3);
    }
}
