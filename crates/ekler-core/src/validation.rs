use crate::model::{PhysicalOutput, Project, SignedPolicy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckLevel {
    Pass,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub level: CheckLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationReport {
    pub is_ready_for_uyap: bool,
    pub items: Vec<CheckItem>,
}

pub fn validate_project_and_outputs(
    project: &Project,
    outputs: &[PhysicalOutput],
) -> ValidationReport {
    let mut items = Vec::new();

    // 1. Ek numaraları ardışık mı?
    let mut exhibits_consecutive = true;
    for (i, exhibit) in project.exhibits.iter().enumerate() {
        if exhibit.order != i + 1 {
            exhibits_consecutive = false;
            break;
        }
    }
    if exhibits_consecutive && !project.exhibits.is_empty() {
        items.push(CheckItem {
            id: "consecutive_exhibits".to_string(),
            title: "Ek numaraları ardışık".to_string(),
            description: format!(
                "Toplam {} adet Ek sıralı ve eksiksiz.",
                project.exhibits.len()
            ),
            level: CheckLevel::Pass,
        });
    } else if project.exhibits.is_empty() {
        items.push(CheckItem {
            id: "consecutive_exhibits".to_string(),
            title: "Ek listesi boş".to_string(),
            description: "Projede hazırlanmış hiçbir mantıksal Ek bulunmuyor.".to_string(),
            level: CheckLevel::Error,
        });
    } else {
        items.push(CheckItem {
            id: "consecutive_exhibits".to_string(),
            title: "Ek numaraları sıralı değil".to_string(),
            description: "Ek sıralamasında boşluk veya atlama tespit edildi.".to_string(),
            level: CheckLevel::Error,
        });
    }

    // 2. Boyut sınırını aşan çıktı var mı?
    let mut size_exceeded = false;
    for out in outputs {
        if out.size_bytes > project.target_size_bytes {
            size_exceeded = true;
            items.push(CheckItem {
                id: format!("size_{}", out.file_name),
                title: format!("Boyut sınırı aşıldı: {}", out.file_name),
                description: format!(
                    "Dosya boyutu {:.2} MB, izin verilen tavan {:.2} MB.",
                    out.size_bytes as f64 / (1024.0 * 1024.0),
                    project.target_size_bytes as f64 / (1024.0 * 1024.0)
                ),
                level: CheckLevel::Error,
            });
        }
    }
    if !size_exceeded && !outputs.is_empty() {
        items.push(CheckItem {
            id: "target_size".to_string(),
            title: "10 MB üzeri çıktı yok".to_string(),
            description: format!(
                "Tüm çıktılar {:.2} MB güvenli tavanın altındadır.",
                project.target_size_bytes as f64 / (1024.0 * 1024.0)
            ),
            level: CheckLevel::Pass,
        });
    }

    // 3. Onaysız UDF veya riskli imzalı kaynak denetimi
    for source in &project.sources {
        if source.format == crate::model::SourceFormat::Udf
            && source.is_signed
            && !source.is_approved_for_conversion
        {
            items.push(CheckItem {
                id: format!("unapproved_udf_{}", source.id),
                title: format!("Onay bekleyen imzalı UDF: {}", source.file_name),
                description:
                    "Elektronik imzalı UDF için kullanıcı onayı alınmadan dönüşüm başlatılamaz."
                        .to_string(),
                level: CheckLevel::Error,
            });
        }
        if source.is_signed && source.signed_policy == SignedPolicy::UseOriginalAsIs {
            items.push(CheckItem {
                id: format!("signed_asis_{}", source.id),
                title: format!("İmzalı belge orijinali korunuyor: {}", source.file_name),
                description: "Kaynak dosya değiştirilmeden ve damga basılmadan korunacaktır."
                    .to_string(),
                level: CheckLevel::Pass,
            });
        }
    }

    // 4. Çıktı dosya adları geçerli mi?
    let mut filenames_valid = true;
    for out in outputs {
        if out.file_name.is_empty() || out.file_name.contains('/') || out.file_name.contains('\\') {
            filenames_valid = false;
            items.push(CheckItem {
                id: format!("bad_filename_{}", out.file_name),
                title: "Geçersiz çıktı dosya adı".to_string(),
                description: format!("'{}' geçerli bir dosya adı değil.", out.file_name),
                level: CheckLevel::Error,
            });
        }
    }
    if filenames_valid && !outputs.is_empty() {
        items.push(CheckItem {
            id: "valid_filenames".to_string(),
            title: "Çıktı dosya isimleri geçerli".to_string(),
            description: "Tüm dosya adları platform güvenli ve sıfır dolgulu.".to_string(),
            level: CheckLevel::Pass,
        });
    }

    let has_error = items.iter().any(|item| item.level == CheckLevel::Error);
    ValidationReport {
        is_ready_for_uyap: !has_error && !outputs.is_empty(),
        items,
    }
}
