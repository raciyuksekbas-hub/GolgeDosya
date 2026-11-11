use crate::error::{EklerError, Result};
use lopdf::{Document as LopdfDoc, Object, ObjectId};
use rayon::prelude::*;

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

/// Aynı anda işlenen görsellerin tutabileceği toplam bellek.
const IMAGE_MEMORY_BUDGET: u64 = 2 * 1024 * 1024 * 1024;
/// İşlenen bir görselin piksel başına tuttuğu bellek: çözülmüş örnekler,
/// küçültülmüş kopya ve kodlama kopyası, her biri 3 bayt.
const IN_FLIGHT_BYTES_PER_PIXEL: u64 = 9;

fn is_image(stream: &lopdf::Stream) -> bool {
    stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|v| v.as_name().ok())
        == Some(b"Image")
}

fn image_pixels(doc: &LopdfDoc, stream: &lopdf::Stream) -> u64 {
    let side = |key: &[u8]| {
        stream
            .dict
            .get(key)
            .ok()
            .and_then(|v| doc.dereference(v).ok())
            .and_then(|(_, v)| v.as_i64().ok())
            .and_then(|n| u64::try_from(n).ok())
            .unwrap_or(0)
    };
    side(b"Width").saturating_mul(side(b"Height"))
}

/// Görsel işini iş parçacıklarına yayar. Sayı belgenin en büyük görseline göre
/// sınırlanır: 8 MP'lik taramada bütün çekirdekler, 100 MP'lik görselde iki
/// iş parçacığı. Bir çekirdek arayüze kalır.
pub fn with_image_workers<R: Send>(doc: &LopdfDoc, work: impl FnOnce() -> R + Send) -> R {
    let largest = doc
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| is_image(s))
        .map(|s| image_pixels(doc, s))
        .max()
        .unwrap_or(0)
        .max(1);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    match rayon::ThreadPoolBuilder::new()
        .num_threads(image_worker_count(largest, cores))
        .build()
    {
        Ok(pool) => pool.install(work),
        Err(_) => work(),
    }
}

fn image_worker_count(largest_pixels: u64, cores: usize) -> usize {
    let by_memory = IMAGE_MEMORY_BUDGET
        / largest_pixels
            .max(1)
            .saturating_mul(IN_FLIGHT_BYTES_PER_PIXEL);
    (by_memory as usize).clamp(1, cores.saturating_sub(1).max(1))
}

/// Bir görselin yeniden kodlama sonucu.
enum Recoded {
    /// Kapsam dışı: dokunulmaz, `images_supported` sayısına girmez.
    Unsupported,
    /// Çözüldü ama yeni JPEG küçülmedi.
    Kept,
    Replaced {
        id: ObjectId,
        width: u32,
        height: u32,
        jpeg: Vec<u8>,
    },
}

fn recode_image(
    id: ObjectId,
    stream: &lopdf::Stream,
    quality: u8,
    max_dimension: u32,
) -> Result<Recoded> {
    let Some(img) = decode_image(stream)? else {
        return Ok(Recoded::Unsupported);
    };
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
    if jpeg.len() >= stream.content.len() {
        return Ok(Recoded::Kept);
    }
    Ok(Recoded::Replaced {
        id,
        width: resized.width(),
        height: resized.height(),
        jpeg,
    })
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
    let Some(chain) = filter_chain(&stream.dict) else {
        return Ok(None);
    };
    // Zincirin sonundaki görsel kodeki (bugün yalnız DCTDecode) kalan baytları
    // çözer; ondan önceki her süzgeç bir TAŞIMA katmanıdır ve sırayla geri
    // alınır. Kodek yoksa sonuç ham örneklerdir.
    let (jpeg, transport) = match chain.split_last() {
        Some(((last, _), rest)) if last == b"DCTDecode" => (true, rest),
        _ => (false, chain.as_slice()),
    };
    // Taşıma katmanı olarak yalnız FlateDecode geri alınır. Başka bir süzgeç —
    // zincirin ortasında kalmış bir DCTDecode dâhil — görseli kapsam dışı
    // bırakır: dokunulmaz, ATLANIR.
    //
    // /DecodeParms: lopdf yalnız PNG öngörücülerini (10–15) geri alır. TIFF
    // öngörücüsü (2) sessizce atlanınca çözülmemiş DELTALAR piksel sanılıp
    // JPEG'e kodlanıyordu — görsel bozuluyordu. Geri alamadığımız öngörücü
    // taşıyan görsel dokunulmadan ATLANIR.
    for (name, params) in transport {
        if name != b"FlateDecode" {
            return Ok(None);
        }
        let predictor = params
            .as_ref()
            .and_then(|d| d.get(b"Predictor").ok())
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(1);
        if predictor != 1 && !(10..=15).contains(&predictor) {
            return Ok(None);
        }
    }
    let unwrap_transport = || -> Result<Vec<u8>> {
        let mut bytes = stream.content.clone();
        for (_, params) in transport {
            // lopdf görsel akışlarını toptan reddeder; her katman, yalnız o
            // katmanın süzgecini ve parametresini taşıyan bir akış olarak açılır.
            let mut layer =
                lopdf::Stream::new(lopdf::dictionary! {"Filter" => "FlateDecode"}, bytes);
            if let Some(d) = params {
                layer.dict.set("DecodeParms", Object::Dictionary(d.clone()));
            }
            bytes = layer
                .decompressed_content()
                .map_err(|e| fail(e.to_string()))?;
        }
        Ok(bytes)
    };
    if jpeg {
        return image::load_from_memory(&unwrap_transport()?)
            .map(Some)
            .map_err(|e| fail(e.to_string()));
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
    let raw = unwrap_transport()?;
    // Örnek uzunluğu. Tam eşleşme çözülür. Sonda en çok iki bayt fazlalık —
    // bazı üreticilerin /Length'e kattığı satır sonu (LF/CRLF) — kırpılır.
    // Başka her uyuşmazlık "bu örnekleri güvenle yorumlayamıyoruz" demektir:
    // görsel ATLANIR, baytları olduğu gibi kalır. Geri alınmamış satır başlığı
    // ya da öngörücü artığı piksel gibi kodlanamaz; ama görüntüleyicinin
    // sorunsuz gösterdiği bir görsel yüzünden bütün belge de reddedilmez.
    let expected = (w as usize) * (h as usize) * channels;
    let mut raw = raw;
    match raw.len().checked_sub(expected) {
        Some(0) => {}
        Some(extra) if extra <= 2 => raw.truncate(expected),
        _ => return Ok(None),
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

/// Akışın süzgeçlerini ÇÖZME sırasında, her birini KENDİ parametresiyle
/// eşleştirerek döndürür (ISO 32000-1 §7.4): `/Filter` dizisinin ilk öğesi en
/// dıştaki kodlamadır; `/DecodeParms` dizisi ona paraleldir, `null` öğe "bu
/// süzgecin parametresi yok" demektir.
///
/// Yapısı yorumlanamayan zincir `None` döner ve görsel ATLANIR: ad olmayan
/// süzgeç öğesi, uzunluğu tutmayan ya da sözlük/null dışı öğe taşıyan
/// parametre dizisi, birden çok süzgece verilmiş tek sözlük (hangi katmana ait
/// olduğu belirsizdir). Eskiden ad olmayan öğe bütün sıkıştırmayı hata ile
/// düşürüyordu; oysa dokunamayacağımız bir görsel, belgenin geri kalanının
/// sıkıştırılmasına engel değildir.
fn filter_chain(dict: &lopdf::Dictionary) -> Option<Vec<(Vec<u8>, Option<lopdf::Dictionary>)>> {
    let names: Vec<Vec<u8>> = match dict.get(b"Filter") {
        Err(_) | Ok(Object::Null) => return Some(Vec::new()),
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(a)) => a
            .iter()
            .map(|v| v.as_name().ok().map(<[u8]>::to_vec))
            .collect::<Option<_>>()?,
        _ => return None,
    };
    let params: Vec<Option<lopdf::Dictionary>> = match dict.get(b"DecodeParms") {
        Err(_) | Ok(Object::Null) => vec![None; names.len()],
        Ok(Object::Dictionary(d)) if names.len() == 1 => vec![Some(d.clone())],
        Ok(Object::Array(a)) if a.len() == names.len() => a
            .iter()
            .map(|v| match v {
                Object::Dictionary(d) => Some(Some(d.clone())),
                Object::Null => Some(None),
                _ => None,
            })
            .collect::<Option<_>>()?,
        _ => return None,
    };
    Some(names.into_iter().zip(params).collect())
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
    let mut jobs = Vec::new();
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
        if !is_image(original) {
            continue;
        }
        found += 1;
        if level == OptimizationLevel::LowRiskCleanup || mask_ids.contains(&id) {
            continue;
        }
        let mut stream = original.clone();
        // Görsel sözlüğünün her sayısal/ad girdisi dolaylı olabilir; çözücü
        // belgeyi görmez, bu yüzden hepsi burada çözülür. Dolaylı /Width
        // yüzünden görselin — ve belgenin — reddedilmesi kabul edilemez.
        // Çözülemeyen referans görseli atlatır, belgeyi düşürmez.
        let mut unresolved = false;
        for key in [
            b"Filter".as_slice(),
            b"DecodeParms",
            b"Width",
            b"Height",
            b"BitsPerComponent",
        ] {
            if let Ok(value) = stream.dict.get(key).cloned() {
                match doc.dereference(&value) {
                    Ok((_, resolved)) => stream.dict.set(key, resolved.clone()),
                    Err(_) => unresolved = true,
                }
            }
        }
        if unresolved {
            continue;
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
        jobs.push((id, stream));
    }
    let results = with_image_workers(doc, || {
        jobs.par_iter()
            .map(|(id, stream)| recode_image(*id, stream, quality, max_dimension))
            .collect::<Result<Vec<_>>>()
    })?;
    drop(jobs);
    let mut supported = 0;
    let mut recompressed = 0;
    for result in results {
        let (id, width, height, jpeg) = match result {
            Recoded::Unsupported => continue,
            Recoded::Kept => {
                supported += 1;
                continue;
            }
            Recoded::Replaced {
                id,
                width,
                height,
                jpeg,
            } => (id, width, height, jpeg),
        };
        supported += 1;
        if let Some(Object::Stream(out)) = doc.objects.get_mut(&id) {
            out.dict.set("Width", width as i64);
            out.dict.set("Height", height as i64);
            out.dict.set("Filter", "DCTDecode");
            out.dict.remove(b"DecodeParms");
            out.set_content(jpeg);
            recompressed += 1;
        }
    }
    // Yalnız GÖRSEL OLMAYAN akışlar Flate'lenir. Görseller yukarıda açıkça ele
    // alındı; ham (filtresiz) bir görseli burada sarmak, kalite kapısının
    // "bayt bayt aynı" kaçışını bozuyor ve temizlik adayını düşürüyordu.
    for obj in doc.objects.values_mut() {
        if let Object::Stream(s) = obj {
            if !is_image(s) && !s.dict.has(b"Filter") {
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

#[cfg(test)]
mod tests {
    use super::image_worker_count;

    #[test]
    fn workers_leave_a_core_free_and_shrink_for_huge_images() {
        // A4, 300 dpi: bellek sınır değil, çekirdekler sınır.
        assert_eq!(image_worker_count(8_700_000, 10), 9);
        // 100 MP: işlenen her görsel ~900 MB tutar.
        assert_eq!(image_worker_count(100_000_000, 10), 2);
        // Bütçeyi tek başına aşan görsel yine işlenir, tek tek.
        assert_eq!(image_worker_count(u64::MAX, 10), 1);
        assert_eq!(image_worker_count(8_700_000, 1), 1);
        assert_eq!(image_worker_count(0, 4), 3);
    }
}
