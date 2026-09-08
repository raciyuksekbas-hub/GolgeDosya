use crate::error::{EklerError, Result};
use lopdf::{Document as LopdfDoc, Object};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationLevel {
    /// Yalnızca metadata temizliği ve kullanılmayan nesnelerin budanması (sıfır kalite kaybı)
    #[default]
    LowRiskCleanup,
    /// Düşük sıkıştırma (JPEG kalite 80)
    GentleCompression,
    BalancedCompression,
    /// Orta sıkıştırma (JPEG kalite 65, hafif downsample)
    AggressiveCompression,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OptimizationResult {
    pub initial_size_bytes: u64,
    pub optimized_size_bytes: u64,
    pub images_found: usize,
    pub images_supported: usize,
    pub images_recompressed_count: usize,
}

/// Decode only color spaces and filters whose sample interpretation we can preserve.
/// Unsupported images are reported separately; malformed supported images are errors.
pub(crate) fn decode_image(stream: &lopdf::Stream) -> Result<Option<image::DynamicImage>> {
    let fail = |e: String| EklerError::InvalidPdf(format!("Görsel çözümlenemedi: {e}"));
    if stream.dict.has(b"Decode") || stream.dict.has(b"SMask") || stream.dict.has(b"Mask") {
        return Ok(None);
    }
    let space = stream
        .dict
        .get(b"ColorSpace")
        .ok()
        .and_then(|v| v.as_name().ok());
    let channels = match space {
        Some(b"DeviceRGB") => 3,
        Some(b"DeviceGray") => 1,
        _ => return Ok(None),
    };
    let filters: Vec<&[u8]> = match stream.dict.get(b"Filter") {
        Ok(Object::Name(n)) => vec![n],
        Ok(Object::Array(a)) => a
            .iter()
            .map(|v| v.as_name())
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| fail(e.to_string()))?,
        Err(_) => vec![],
        _ => return Ok(None),
    };
    if filters == [b"DCTDecode".as_slice()] {
        return image::load_from_memory(&stream.content)
            .map(Some)
            .map_err(|e| fail(e.to_string()));
    }
    if !filters.iter().all(|f| *f == b"FlateDecode") {
        return Ok(None);
    }
    if stream
        .dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|v| v.as_i64().ok())
        != Some(8)
    {
        return Ok(None);
    }
    let dimension = |key: &[u8]| -> Result<u32> {
        let v = stream
            .dict
            .get(key)
            .and_then(|v| v.as_i64())
            .map_err(|e| fail(e.to_string()))?;
        u32::try_from(v)
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| fail("Geçersiz boyut".into()))
    };
    let (w, h) = (dimension(b"Width")?, dimension(b"Height")?);
    let raw = if filters.is_empty() {
        stream.content.clone()
    } else {
        // lopdf refuses image streams generically. Samples/color depth were checked above;
        // decode an otherwise identical stream through its Flate/predictor implementation.
        let mut encoded = stream.clone();
        encoded.dict.remove(b"Subtype");
        encoded
            .decompressed_content()
            .map_err(|e| fail(e.to_string()))?
    };
    if channels == 3 {
        image::RgbImage::from_raw(w, h, raw)
            .map(image::DynamicImage::ImageRgb8)
            .map(Some)
            .ok_or_else(|| fail("RGB örnek uzunluğu uyuşmuyor".into()))
    } else {
        image::GrayImage::from_raw(w, h, raw)
            .map(image::DynamicImage::ImageLuma8)
            .map(Some)
            .ok_or_else(|| fail("Gri örnek uzunluğu uyuşmuyor".into()))
    }
}

/// No page rasterization. Presets differ in JPEG quality and pixel ceiling.
/// The ceiling is not described as effective DPI: placement may vary per page.
pub fn optimize_pdf(doc: &mut LopdfDoc, level: OptimizationLevel) -> Result<OptimizationResult> {
    let mut initial_buf = Vec::new();
    doc.save_to(&mut initial_buf)
        .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
    doc.prune_objects();
    if let Ok(root) = doc.catalog_mut() {
        root.remove(b"Metadata");
        root.remove(b"PieceInfo");
    }
    doc.prune_objects();
    let (quality, max_dimension) = match level {
        OptimizationLevel::LowRiskCleanup => (100, u32::MAX),
        OptimizationLevel::GentleCompression => (80, 2400),
        OptimizationLevel::BalancedCompression => (72, 2000),
        OptimizationLevel::AggressiveCompression => (65, 1600),
    };
    let mut found = 0;
    let mut supported = 0;
    let mut recompressed = 0;
    // Resolve indirect colors/filter dictionaries before decoding without altering source samples.
    let mask_ids: std::collections::HashSet<_> = doc
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .flat_map(|s| {
            [b"SMask".as_slice(), b"Mask"]
                .into_iter()
                .filter_map(|k| s.dict.get(k).ok().and_then(|v| v.as_reference().ok()))
        })
        .collect();
    let ids: Vec<_> = doc.objects.keys().copied().collect();
    for id in ids {
        let Ok(Object::Stream(original)) = doc.get_object(id) else {
            continue;
        };
        if original
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|v| v.as_name().ok())
            != Some(b"Image")
        {
            continue;
        }
        found += 1;
        if level == OptimizationLevel::LowRiskCleanup || mask_ids.contains(&id) {
            continue;
        }
        let mut stream = original.clone();
        for key in [b"ColorSpace".as_slice(), b"Filter", b"DecodeParms"] {
            if let Ok(value) = stream.dict.get(key).cloned() {
                let (_, resolved) = doc
                    .dereference(&value)
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
                stream.dict.set(key, resolved.clone());
            }
        }
        let Some(img) = decode_image(&stream)? else {
            continue;
        };
        supported += 1;
        let resized = if img.width() > max_dimension || img.height() > max_dimension {
            img.resize(
                max_dimension,
                max_dimension,
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            img
        };
        let gray = stream
            .dict
            .get(b"ColorSpace")
            .ok()
            .and_then(|v| v.as_name().ok())
            == Some(b"DeviceGray");
        let mut jpeg = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality);
        let encoded = if gray {
            encoder.encode_image(&resized.to_luma8())
        } else {
            encoder.encode_image(&resized.to_rgb8())
        };
        encoded.map_err(|e| EklerError::InvalidPdf(format!("JPEG kodlanamadı: {e}")))?;
        if jpeg.len() < original.content.len() {
            stream.dict.set("Width", resized.width() as i64);
            stream.dict.set("Height", resized.height() as i64);
            stream.dict.set("Filter", "DCTDecode");
            stream.dict.remove(b"DecodeParms");
            stream.set_content(jpeg);
            doc.objects.insert(id, Object::Stream(stream));
            recompressed += 1;
        }
    }
    if level != OptimizationLevel::LowRiskCleanup && found > 0 && supported == 0 {
        return Err(EklerError::ValidationFailed(format!("{found} görsel bulundu; renk uzayı, maske veya filtreleri güvenli sıkıştırma kapsamında değil.")));
    }
    doc.compress();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
    Ok(OptimizationResult {
        initial_size_bytes: initial_buf.len() as u64,
        optimized_size_bytes: bytes.len() as u64,
        images_found: found,
        images_supported: supported,
        images_recompressed_count: recompressed,
    })
}
