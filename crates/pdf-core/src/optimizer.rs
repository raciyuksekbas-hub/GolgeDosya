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
///
/// `pub` çünkü crate sınırını geçiyor: `ekler-core::toolbox` sıkıştırma kalitesini
/// ölçerken önceki ve sonraki görseli bununla çözüyor. Bir PDF görsel akışını
/// çözmek zaten bu crate'in işidir; genişletme değil, sınırın doğru tarafı.
pub fn decode_image(stream: &lopdf::Stream) -> Result<Option<image::DynamicImage>> {
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
    // /DecodeParms: lopdf yalnız SÖZLÜK biçimini okur ve yalnız PNG öngörücülerini
    // (10–15) geri alır. Dizi biçimli parametre ya da TIFF öngörücüsü (2) sessizce
    // atlanınca çözülmemiş DELTALAR piksel sanılıp JPEG'e kodlanıyordu — görsel
    // bozuluyordu. Bu yüzden: dizi biçimi filtreyle eşleştirilip sözlüğe indirgenir;
    // geri alamadığımız öngörücü taşıyan görsel dokunulmadan ATLANIR.
    let params: Option<lopdf::Dictionary> = match stream.dict.get(b"DecodeParms") {
        Err(_) | Ok(Object::Null) => None,
        Ok(Object::Dictionary(d)) => Some(d.clone()),
        Ok(Object::Array(a)) if a.len() == filters.len() => match a.first() {
            Some(Object::Dictionary(d)) => Some(d.clone()),
            Some(Object::Null) | None => None,
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    if let Some(d) = &params {
        let predictor = d
            .get(b"Predictor")
            .ok()
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(1);
        if predictor != 1 && !(10..=15).contains(&predictor) {
            return Ok(None);
        }
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
            .dict
            .set("Filter", Object::Name(b"FlateDecode".to_vec()));
        match &params {
            Some(d) => encoded
                .dict
                .set("DecodeParms", Object::Dictionary(d.clone())),
            None => {
                encoded.dict.remove(b"DecodeParms");
            }
        }
        encoded
            .decompressed_content()
            .map_err(|e| fail(e.to_string()))?
    };
    // Tam uzunluk: `from_raw` fazla baytı kabul eder; geri alınmamış satır
    // başlığı ya da öngörücü artığı piksel gibi geçemez.
    if raw.len() != (w as usize) * (h as usize) * channels {
        return Err(fail("örnek uzunluğu boyutla uyuşmuyor".into()));
    }
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

/// Görselin renk uzayını, örnekleri koruyarak çözebileceğimiz DEVICE adına
/// indirger; indirgenemiyorsa `None`.
///
/// Gerçek dünyada `/ColorSpace` nadiren çıplak `/DeviceRGB`dir: ofis
/// uygulamaları, Preview ve tarayıcılar `[/ICCBased n 0 R]` yazar. Profil
/// akışının `/N` alanı bileşen sayısını verir (1 gri, 3 RGB); örnek düzeni
/// Device eşdeğeriyle aynıdır, yalnız yorumu profile bağlıdır. Çözerken
/// Device adı kullanılır; yazılırken ÖZGÜN nesne korunur ki profil kaybolmasın.
/// CMYK (`/N 4`), Indexed, Separation vb. kapsam dışıdır.
pub fn decodable_colorspace(doc: &LopdfDoc, space: &Object) -> Option<Object> {
    let (_, space) = doc.dereference(space).ok()?;
    match space {
        Object::Name(n) if n == b"DeviceRGB" || n == b"DeviceGray" => Some(space.clone()),
        Object::Array(a) if a.len() == 2 && a[0].as_name().ok() == Some(b"ICCBased") => {
            let (_, profile) = doc.dereference(&a[1]).ok()?;
            let n = profile
                .as_stream()
                .ok()?
                .dict
                .get(b"N")
                .ok()?
                .as_i64()
                .ok()?;
            match n {
                1 => Some(Object::Name(b"DeviceGray".to_vec())),
                3 => Some(Object::Name(b"DeviceRGB".to_vec())),
                _ => None,
            }
        }
        _ => None,
    }
}

/// No page rasterization. Presets differ in JPEG quality and pixel ceiling.
/// The ceiling is not described as effective DPI: placement may vary per page.
///
/// Kapsam dışı görsel (CMYK, Indexed, maske…) bir HATA değildir: atlanır ve
/// `images_supported` sayısında görünür. Çağıran, hiçbir görsel yeniden
/// kodlanmamışsa bunu dürüstçe "kazanç yok" olarak raporlar. Eskiden burada
/// hata döndürülüyor ve tek bir ICCBased görsel bütün sıkıştırmayı — temizlik
/// adayı dâhil — "başarısız" yapıyordu.
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
        for key in [b"Filter".as_slice(), b"DecodeParms"] {
            if let Ok(value) = stream.dict.get(key).cloned() {
                let (_, resolved) = doc
                    .dereference(&value)
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
                stream.dict.set(key, resolved.clone());
            }
        }
        // Renk uzayı yalnız ÇÖZMEK için Device adına indirgenir; yazılan akış
        // özgün `/ColorSpace` nesnesini (ör. ICC profili) korur.
        let Some(space) = stream
            .dict
            .get(b"ColorSpace")
            .ok()
            .and_then(|v| decodable_colorspace(doc, v))
        else {
            continue;
        };
        stream.dict.set("ColorSpace", space);
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
            let mut out = original.clone();
            out.dict.set("Width", resized.width() as i64);
            out.dict.set("Height", resized.height() as i64);
            out.dict.set("Filter", "DCTDecode");
            out.dict.remove(b"DecodeParms");
            out.set_content(jpeg);
            doc.objects.insert(id, Object::Stream(out));
            recompressed += 1;
        }
    }
    // Yalnız GÖRSEL OLMAYAN akışlar Flate'lenir. Görseller yukarıda açıkça ele
    // alındı; ham (filtresiz) bir görseli burada sarmak, kalite kapısının
    // "bayt bayt aynı" kaçışını bozuyor ve temizlik adayını düşürüyordu.
    for obj in doc.objects.values_mut() {
        if let Object::Stream(s) = obj {
            let is_image =
                s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image");
            if !is_image && !s.dict.has(b"Filter") {
                let _ = s.compress();
            }
        }
    }
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
