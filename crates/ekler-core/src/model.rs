use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 9.5 MB varsayılan güvenli boyut (10 MB = 10,485,760 bayt; %5 güvenlik marjı bırakılır)
pub const DEFAULT_TARGET_SIZE_BYTES: u64 = 9_961_472;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Pdf,
    Doc,
    Docx,
    Jpg,
    Jpeg,
    Png,
    Tif,
    Tiff,
    Heic,
    Udf,
    Unknown,
}

impl SourceFormat {
    pub fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "pdf" => SourceFormat::Pdf,
            "doc" => SourceFormat::Doc,
            "docx" => SourceFormat::Docx,
            "jpg" => SourceFormat::Jpg,
            "jpeg" => SourceFormat::Jpeg,
            "png" => SourceFormat::Png,
            "tif" => SourceFormat::Tif,
            "tiff" => SourceFormat::Tiff,
            "heic" => SourceFormat::Heic,
            "udf" => SourceFormat::Udf,
            _ => SourceFormat::Unknown,
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(
            self,
            SourceFormat::Jpg
                | SourceFormat::Jpeg
                | SourceFormat::Png
                | SourceFormat::Tif
                | SourceFormat::Tiff
                | SourceFormat::Heic
        )
    }

    pub fn is_tiff(&self) -> bool {
        matches!(self, SourceFormat::Tif | SourceFormat::Tiff)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignedPolicy {
    /// Orijinali olduğu gibi kullan (hiçbir mutasyon, damga veya sıkıştırma yapılmaz)
    UseOriginalAsIs,
    /// Türetilmiş kopya oluştur (damgalanabilir, ancak kaynak imzayı taşımaz uyarısıyla)
    CreateDerivedCopy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceFile {
    pub id: String,
    pub path: PathBuf,
    pub file_name: String,
    pub format: SourceFormat,
    pub size_bytes: u64,
    pub sha256_before: String,
    pub mtime: u64,
    pub page_count: usize,
    pub is_signed: bool,
    pub signature_note: Option<String>,
    pub signed_policy: SignedPolicy,
    pub is_approved_for_conversion: bool,
    #[serde(default)]
    pub is_repaired: bool,
    #[serde(default)]
    pub repair_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExhibitSourceRef {
    pub source_id: String,
    /// İsteğe bağlı kaynak sayfa aralığı (boş ise tüm sayfalar)
    pub page_range: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogicalExhibit {
    pub id: String,
    pub order: usize,
    pub name: String,
    pub sources: Vec<ExhibitSourceRef>,
}

impl LogicalExhibit {
    pub fn new(id: String, order: usize, name: String) -> Self {
        Self {
            id,
            order,
            name,
            sources: Vec::new(),
        }
    }

    /// Kullanıcıya dönük başlık: örn. "Ek-1: İş Sözleşmesi"
    pub fn display_title(&self) -> String {
        format!("Ek-{}: {}", self.order, self.name)
    }

    /// Çıktı dosya adı için sanitize edilmiş etiket: örn. "EK-01_Sozlesme"
    pub fn file_stem_prefix(&self, max_exhibit_digits: usize) -> String {
        let clean_name = sanitize_for_filename(&self.name);
        let digits = max_exhibit_digits.max(2);
        format!("EK-{:0width$}_{}", self.order, clean_name, width = digits)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizedPage {
    pub source_id: String,
    pub source_page_index: usize,
    pub exhibit_id: String,
    pub exhibit_page_number: usize,
    pub total_exhibit_pages: usize,
    pub width_pt: f32,
    pub height_pt: f32,
    pub orientation: PageOrientation,
    pub is_blank: bool,
}

impl NormalizedPage {
    /// Türkçe damga metni: örn. "Ek-1 / 1. Sayfa"
    pub fn default_stamp_text(exhibit_order: usize, page_num: usize) -> String {
        format!("Ek-{} / {}. Sayfa", exhibit_order, page_num)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhysicalOutput {
    pub file_name: String,
    pub exhibit_id: String,
    pub exhibit_order: usize,
    pub is_continuation: bool,
    pub continuation_index: usize,
    pub page_start: usize,
    pub page_end: usize,
    pub page_count: usize,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StampPosition {
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StampConfig {
    pub enabled: bool,
    pub position: StampPosition,
    pub font_size: f32,
    pub margin_pt: f32,
    pub show_badge: bool,
}

impl Default for StampConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: StampPosition::TopRight,
            font_size: 10.0,
            margin_pt: 20.0,
            show_badge: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    #[serde(default)]
    pub optimization: crate::optimizer::OptimizationLevel,
    pub version: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub sources: Vec<SourceFile>,
    pub exhibits: Vec<LogicalExhibit>,
    pub target_size_bytes: u64,
    pub stamp_config: StampConfig,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            optimization: Default::default(),
            version: "1.0.0".to_string(),
            name: "Yeni Dilekçe Ekleri Projesi".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            sources: Vec::new(),
            exhibits: Vec::new(),
            target_size_bytes: DEFAULT_TARGET_SIZE_BYTES,
            stamp_config: StampConfig::default(),
        }
    }
}

impl Project {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    pub fn add_source(&mut self, source: SourceFile) {
        if !self.sources.iter().any(|s| s.id == source.id) {
            self.sources.push(source);
        }
    }

    pub fn create_exhibit(&mut self, name: &str) -> &mut LogicalExhibit {
        let order = self.exhibits.len() + 1;
        let mut suffix = order;
        while self
            .exhibits
            .iter()
            .any(|e| e.id == format!("exhibit-{}", suffix))
        {
            suffix += 1;
        }
        let id = format!("exhibit-{}", suffix);
        let exhibit = LogicalExhibit::new(id, order, name.to_string());
        self.exhibits.push(exhibit);
        self.exhibits.last_mut().unwrap()
    }

    pub fn renumber_exhibits(&mut self) {
        for (i, exhibit) in self.exhibits.iter_mut().enumerate() {
            exhibit.order = i + 1;
        }
    }

    pub fn remove_exhibit(&mut self, exhibit_id: &str) {
        self.exhibits.retain(|e| e.id != exhibit_id);
        self.renumber_exhibits();
    }

    pub fn get_source(&self, source_id: &str) -> Option<&SourceFile> {
        self.sources.iter().find(|s| s.id == source_id)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

pub fn sanitize_for_filename(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        match c {
            ' ' | '-' | '_' => {
                if !out.ends_with('_') && !out.is_empty() {
                    out.push('_');
                }
            }
            'ı' => out.push('i'),
            'i' => out.push('i'),
            'İ' => out.push('I'),
            'I' => out.push('I'),
            'ş' => out.push('s'),
            'Ş' => out.push('S'),
            'ğ' => out.push('g'),
            'Ğ' => out.push('G'),
            'ü' => out.push('u'),
            'Ü' => out.push('U'),
            'ö' => out.push('o'),
            'Ö' => out.push('O'),
            'ç' => out.push('c'),
            'Ç' => out.push('C'),
            c if c.is_ascii_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "Ek".to_string()
    } else {
        let name = trimmed.chars().take(96).collect::<String>();
        let upper = name.to_ascii_uppercase();
        if [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&upper.as_str())
        {
            format!("Belge_{name}")
        } else {
            name
        }
    }
}
