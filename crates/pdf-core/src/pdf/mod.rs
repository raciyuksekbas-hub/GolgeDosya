pub mod stamp;
pub mod tolerant;
pub mod validate;
pub use validate::validate_document;

pub use tolerant::{load_pdf_tolerant, TolerantLoadResult};

use crate::error::{EklerError, Result};
use lopdf::{dictionary, Document as LopdfDoc, Object, ObjectId};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct PdfInfo {
    pub page_count: usize,
    pub is_signed: bool,
    pub pages_dimensions: Vec<(f32, f32)>, // (width_pt, height_pt)
    pub is_repaired: bool,
    pub repair_note: Option<String>,
}

/// PDF dosyasını okur, sayfa sayısını, sayfa boyutlarını, imza ve onarım durumunu tespit eder.
pub fn inspect_pdf(path: &Path) -> Result<PdfInfo> {
    let bytes = std::fs::read(path).map_err(|e| EklerError::IoError {
        path: path.to_path_buf(),
        source: e,
    })?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("belge.pdf");

    let load_res = load_pdf_tolerant(&bytes, file_name)?;
    let doc = load_res.document;
    let is_signed = detect_signature(&doc);
    let pages = doc.get_pages();
    let page_count = pages.len();

    let mut dimensions = Vec::with_capacity(page_count);
    for page_num in 1..=(page_count as u32) {
        if let Some(&page_id) = pages.get(&page_num) {
            let (w, h) = get_page_dimensions(&doc, page_id).unwrap_or((595.28, 841.89));
            dimensions.push((w, h));
        } else {
            dimensions.push((595.28, 841.89));
        }
    }

    Ok(PdfInfo {
        page_count,
        is_signed,
        pages_dimensions: dimensions,
        is_repaired: load_res.is_repaired,
        repair_note: load_res.repair_note,
    })
}

pub fn get_page_dimensions(doc: &LopdfDoc, page_id: ObjectId) -> Option<(f32, f32)> {
    let page = resolved_page_dictionary(doc, page_id).ok()?;
    let bounds = page
        .get(b"CropBox")
        .or_else(|_| page.get(b"MediaBox"))
        .ok()?;
    let (_, bounds) = doc.dereference(bounds).ok()?;
    let a = bounds.as_array().ok()?;
    if a.len() != 4 {
        return None;
    }
    let width = a[2].as_float().ok()? - a[0].as_float().ok()?;
    let height = a[3].as_float().ok()? - a[1].as_float().ok()?;
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return None;
    }
    let rotate = page
        .get(b"Rotate")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    match rotate {
        0 | 180 => Some((width, height)),
        90 | 270 => Some((height, width)),
        _ => None,
    }
}

/// PDF'deki dijital imza (/Type /Sig veya /SigFlags) varlığını tespit eder.
pub fn detect_signature(doc: &LopdfDoc) -> bool {
    if let Ok(root) = doc.catalog() {
        if let Ok(acro_form) = root.get(b"AcroForm") {
            let acro_dict = match acro_form {
                Object::Reference(id) => doc.get_dictionary(*id).ok(),
                Object::Dictionary(ref d) => Some(d),
                _ => None,
            };
            if let Some(dict) = acro_dict {
                if let Ok(sig_flags) = dict.get(b"SigFlags") {
                    if let Ok(flags) = sig_flags.as_i64() {
                        if flags & 3 != 0 {
                            return true;
                        }
                    }
                }
            }
        }
    }

    for obj in doc.objects.values() {
        if let Ok(dict) = obj.as_dict() {
            if dict.has(b"ByteRange") && dict.has(b"Contents") {
                return true;
            }
            if let Ok(type_val) = dict.get(b"Type") {
                if let Ok(name) = type_val.as_name() {
                    if name == b"Sig" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Belirtilen sayfa aralığını (1-indexed [start_page, end_page]) yeni bir PDF olarak çıkarır.
pub fn extract_page_range(doc: &LopdfDoc, start_page: usize, end_page: usize) -> Result<LopdfDoc> {
    let pages = doc.get_pages();
    let total_pages = pages.len();
    if start_page < 1 || end_page > total_pages || start_page > end_page {
        return Err(EklerError::InvalidPdf(format!(
            "Geçersiz sayfa aralığı: {}-{} (Toplam: {})",
            start_page, end_page, total_pages
        )));
    }

    let mut new_doc = LopdfDoc::with_version("1.7");
    let pages_id = new_doc.new_object_id();
    let mut new_page_ids = Vec::new();

    for p in start_page..=end_page {
        if let Some(&old_page_id) = pages.get(&(p as u32)) {
            // Nesneyi ve bağımlılıklarını yeni belgeye aktar
            let new_id = copy_object_recursive(doc, &mut new_doc, old_page_id)?;
            // Parent'ı yeni pages_id yap
            if let Ok(page_dict) = new_doc.get_object_mut(new_id).and_then(|o| o.as_dict_mut()) {
                page_dict.set("Parent", pages_id);
            }
            new_page_ids.push(new_id);
        }
    }

    let pages_obj = dictionary! {
        "Type" => "Pages",
        "Kids" => new_page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
        "Count" => new_page_ids.len() as i32,
    };
    new_doc.set_object(pages_id, pages_obj);

    let catalog_id = new_doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    new_doc.trailer.set("Root", catalog_id);

    Ok(new_doc)
}

/// Birden fazla PDF belgesini sırasıyla tek bir PDF olarak birleştirir.
pub fn merge_documents(docs: &[LopdfDoc]) -> Result<LopdfDoc> {
    let mut merged = LopdfDoc::with_version("1.7");
    let pages_id = merged.new_object_id();
    let mut merged_page_ids = Vec::new();

    for doc in docs {
        let pages = doc.get_pages();
        for page_num in 1..=(pages.len() as u32) {
            if let Some(&old_page_id) = pages.get(&page_num) {
                let new_id = copy_object_recursive(doc, &mut merged, old_page_id)?;
                if let Ok(page_dict) = merged.get_object_mut(new_id).and_then(|o| o.as_dict_mut()) {
                    page_dict.set("Parent", pages_id);
                }
                merged_page_ids.push(new_id);
            }
        }
    }

    let pages_obj = dictionary! {
        "Type" => "Pages",
        "Kids" => merged_page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
        "Count" => merged_page_ids.len() as i32,
    };
    merged.set_object(pages_id, pages_obj);

    let catalog_id = merged.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    merged.trailer.set("Root", catalog_id);

    Ok(merged)
}

/// Resolve inheritable page attributes through an arbitrarily nested Pages tree.
pub fn resolved_page_dictionary(src: &LopdfDoc, id: ObjectId) -> Result<lopdf::Dictionary> {
    let mut page = src
        .get_dictionary(id)
        .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
        .clone();
    let mut current = id;
    let mut visited = std::collections::HashSet::new();
    loop {
        if !visited.insert(current) {
            return Err(EklerError::InvalidPdf("Döngüsel sayfa ağacı".into()));
        }
        let dictionary = src
            .get_dictionary(current)
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        for key in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
            if !page.has(key) {
                if let Ok(value) = dictionary.get(key) {
                    page.set(key.to_vec(), value.clone());
                }
            }
        }
        match dictionary.get(b"Parent") {
            Ok(parent) => {
                current = parent
                    .as_reference()
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
            }
            Err(_) => break,
        }
    }
    if !page.has(b"MediaBox") {
        return Err(EklerError::InvalidPdf("Sayfa MediaBox içermiyor".into()));
    }
    page.remove(b"Parent");
    Ok(page)
}

fn copy_object_recursive(src: &LopdfDoc, dst: &mut LopdfDoc, id: ObjectId) -> Result<ObjectId> {
    let page = resolved_page_dictionary(src, id)?;
    let mut id_map = BTreeMap::new();
    let new_id = dst.new_object_id();
    id_map.insert(id, new_id);
    let copied = clone_object_remapping(src, dst, &Object::Dictionary(page), &mut id_map)?;
    dst.set_object(new_id, copied);
    Ok(new_id)
}

fn copy_recursive_internal(
    src: &LopdfDoc,
    dst: &mut LopdfDoc,
    id: ObjectId,
    id_map: &mut BTreeMap<ObjectId, ObjectId>,
) -> Result<ObjectId> {
    if let Some(&existing) = id_map.get(&id) {
        return Ok(existing);
    }

    let new_id = dst.new_object_id();
    id_map.insert(id, new_id);

    let obj = src
        .get_object(id)
        .map_err(|e| EklerError::InvalidPdf(format!("Eksik PDF nesnesi {:?}: {}", id, e)))?;
    let cloned_obj = clone_object_remapping(src, dst, obj, id_map)?;
    dst.set_object(new_id, cloned_obj);

    Ok(new_id)
}

/// Kopyalanan sayfanın DIŞINDAKİ bir sayfa ya da sayfa ağacı düğümü mü?
///
/// Bağlantı açıklamalarının `/Dest`i, açıklamaların `/P`si ve sayfa
/// sözlüklerinin `/Parent`ı başka sayfalara işaret eder. Bunlar ham kopyalanınca
/// tek sayfa seçiminde bütün belge yetim nesne olarak çıktıya taşınıyordu
/// (40 sayfalık kaynaktan tek sayfa: kaynağın %100'ü). Böyle bir hedef
/// kopyalanmaz; referans `null` olur — PDF'te geçerli, görüntüleyicide zararsız.
fn is_foreign_page_node(
    src: &LopdfDoc,
    id: ObjectId,
    id_map: &BTreeMap<ObjectId, ObjectId>,
) -> bool {
    if id_map.contains_key(&id) {
        return false;
    }
    src.get_dictionary(id)
        .ok()
        .and_then(|d| d.get(b"Type").ok())
        // /Type dolaylı bir ad olabilir (`/Type 12 0 R`); çözülmeden bakılırsa
        // sayfa düğümü tanınmaz ve ağaç yine sürüklenir.
        .and_then(|t| src.dereference(t).ok())
        .and_then(|(_, t)| t.as_name().ok())
        .is_some_and(|t| t == b"Page" || t == b"Pages")
}

fn clone_object_remapping(
    src: &LopdfDoc,
    dst: &mut LopdfDoc,
    obj: &Object,
    id_map: &mut BTreeMap<ObjectId, ObjectId>,
) -> Result<Object> {
    match obj {
        Object::Reference(old_id) if is_foreign_page_node(src, *old_id, id_map) => Ok(Object::Null),
        Object::Reference(old_id) => {
            let mapped_id = copy_recursive_internal(src, dst, *old_id, id_map)?;
            Ok(Object::Reference(mapped_id))
        }
        Object::Array(arr) => {
            let mut new_arr = Vec::with_capacity(arr.len());
            for item in arr {
                new_arr.push(clone_object_remapping(src, dst, item, id_map)?);
            }
            Ok(Object::Array(new_arr))
        }
        Object::Dictionary(dict) => {
            let mut new_dict = lopdf::Dictionary::new();
            for (k, v) in dict.iter() {
                new_dict.set(k.clone(), clone_object_remapping(src, dst, v, id_map)?);
            }
            Ok(Object::Dictionary(new_dict))
        }
        Object::Stream(stream) => {
            let mut new_dict = lopdf::Dictionary::new();
            for (k, v) in stream.dict.iter() {
                new_dict.set(k.clone(), clone_object_remapping(src, dst, v, id_map)?);
            }
            Ok(Object::Stream(lopdf::Stream::new(
                new_dict,
                stream.content.clone(),
            )))
        }
        other => Ok(other.clone()),
    }
}
