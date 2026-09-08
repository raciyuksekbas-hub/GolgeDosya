use crate::error::{EklerError, Result};
use crate::model::SourceFormat;
use image::{DynamicImage, GenericImageView, ImageFormat};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document as LopdfDoc, Object, Stream};
use std::io::Cursor;
use std::path::Path;

pub const A4_WIDTH_PT: f32 = 595.28;
pub const A4_HEIGHT_PT: f32 = 841.89;
pub const DEFAULT_MARGIN_PT: f32 = 36.0; // 0.5 inç

/// Herhangi bir desteklenen görseli (JPG, PNG, Tekli/Çoklu TIFF, HEIC)
/// standart A4 boyutunda, en-boy oranını koruyan bir PDF belgesine dönüştürür.
pub fn image_file_to_pdf(path: &Path) -> Result<LopdfDoc> {
    let format = SourceFormat::from_path(path);
    let bytes = std::fs::read(path).map_err(|e| EklerError::IoError {
        path: path.to_path_buf(),
        source: e,
    })?;

    if format.is_tiff() {
        // Çok sayfalı TIFF denetimi ve ayrıştırması
        convert_tiff_bytes_to_pdf(&bytes)
    } else if format == SourceFormat::Heic {
        // macOS sips köprüsü veya dinamik çözümleme
        convert_heic_to_pdf(path)
    } else {
        let img =
            image::load_from_memory(&bytes).map_err(|e| EklerError::InvalidImage(e.to_string()))?;
        convert_single_image_to_pdf(img)
    }
}

pub fn convert_single_image_to_pdf(img: DynamicImage) -> Result<LopdfDoc> {
    let mut doc = LopdfDoc::with_version("1.7");
    let pages_id = doc.new_object_id();

    let page_id = add_image_page(&mut doc, pages_id, &img)?;

    let pages_obj = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
    };
    doc.set_object(pages_id, pages_obj);

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    Ok(doc)
}

pub fn convert_tiff_bytes_to_pdf(bytes: &[u8]) -> Result<LopdfDoc> {
    let mut decoder = tiff::decoder::Decoder::new(Cursor::new(bytes))
        .map_err(|e| EklerError::InvalidImage(format!("TIFF çözümlenemedi: {}", e)))?;

    let mut doc = LopdfDoc::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut page_ids = Vec::new();

    loop {
        let (w, h) = decoder
            .dimensions()
            .map_err(|e| EklerError::InvalidImage(format!("TIFF boyut hatası: {}", e)))?;

        let img_result = decoder
            .read_image()
            .map_err(|e| EklerError::InvalidImage(format!("TIFF kare okuma hatası: {}", e)))?;

        let color = decoder
            .colortype()
            .map_err(|e| EklerError::InvalidImage(e.to_string()))?;
        use tiff::{decoder::DecodingResult as D, ColorType as C};
        let dyn_img = match (color, img_result) {
            (C::Gray(8), D::U8(v)) => {
                image::GrayImage::from_raw(w, h, v).map(DynamicImage::ImageLuma8)
            }
            (C::Gray(16), D::U16(v)) => {
                image::ImageBuffer::<image::Luma<u16>, _>::from_raw(w, h, v)
                    .map(DynamicImage::ImageLuma16)
            }
            (C::RGB(8), D::U8(v)) => {
                image::RgbImage::from_raw(w, h, v).map(DynamicImage::ImageRgb8)
            }
            (C::RGBA(8), D::U8(v)) => {
                image::RgbaImage::from_raw(w, h, v).map(DynamicImage::ImageRgba8)
            }
            (C::RGB(16), D::U16(v)) => image::ImageBuffer::<image::Rgb<u16>, _>::from_raw(w, h, v)
                .map(DynamicImage::ImageRgb16),
            (C::RGBA(16), D::U16(v)) => {
                image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(w, h, v)
                    .map(DynamicImage::ImageRgba16)
            }
            (kind, _) => {
                return Err(EklerError::InvalidImage(format!(
                    "Desteklenmeyen TIFF renk modeli: {:?}",
                    kind
                )))
            }
        }
        .ok_or_else(|| {
            EklerError::InvalidImage("TIFF piksel verisi boyutlarla uyuşmuyor".into())
        })?;

        let page_id = add_image_page(&mut doc, pages_id, &dyn_img)?;
        page_ids.push(page_id);

        if !decoder.more_images() {
            break;
        }
        decoder.next_image().map_err(|e| {
            EklerError::InvalidImage(format!("Sonraki TIFF karesi açılamadı: {}", e))
        })?;
    }

    if page_ids.is_empty() {
        return Err(EklerError::InvalidImage(
            "TIFF dosyası hiç sayfa içermiyor".to_string(),
        ));
    }

    let count = page_ids.len() as i32;
    let pages_obj = dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
        "Count" => count,
    };
    doc.set_object(pages_id, pages_obj);

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    Ok(doc)
}

fn convert_heic_to_pdf(path: &Path) -> Result<LopdfDoc> {
    #[cfg(target_os = "macos")]
    {
        let tmp_jpg = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .map_err(|e| EklerError::IoError {
                path: path.to_path_buf(),
                source: e,
            })?;

        // Süreç başlatma `process-bridge` üzerinden. Bu çağrının önceden
        // zaman aşımı da ağ politikası da YOKTU; ikisi de eklendi. Ağ reddinin
        // `sips`'i bozmadığı ölçüldü, varsayılmadı.
        process_bridge::run(
            process_bridge::Spawn::new(std::path::Path::new("/usr/bin/sips"))
                .args(["-s", "format", "png"])
                .arg(path)
                .arg("--out")
                .arg(tmp_jpg.path())
                .timeout(std::time::Duration::from_secs(60))
                .network(process_bridge::NetworkPolicy::Deny),
        )
        .map_err(|e| match e {
            process_bridge::BridgeError::Launch { source, .. } => {
                EklerError::InvalidImage(format!("sips çağrılamadı: {}", source))
            }
            _ => EklerError::InvalidImage("sips HEIC dönüşümünde hata verdi".to_string()),
        })?;

        let img =
            image::open(tmp_jpg.path()).map_err(|e| EklerError::InvalidImage(e.to_string()))?;
        convert_single_image_to_pdf(img)
    }

    #[cfg(target_os = "windows")]
    {
        let temp = tempfile::tempdir().map_err(|e| EklerError::InvalidImage(e.to_string()))?;
        let output = temp.path().join("image.png");
        let script = r#"$ErrorActionPreference='Stop'; Add-Type -AssemblyName PresentationCore; $inputStream=[IO.File]::OpenRead($env:DUZENEK_IMAGE_SOURCE); try {$decoder=[Windows.Media.Imaging.BitmapDecoder]::Create($inputStream,[Windows.Media.Imaging.BitmapCreateOptions]::PreservePixelFormat,[Windows.Media.Imaging.BitmapCacheOption]::OnLoad); if($decoder.Frames.Count -ne 1){throw 'Expected one HEIC frame'}; $encoder=New-Object Windows.Media.Imaging.PngBitmapEncoder; $encoder.Frames.Add($decoder.Frames[0]); $outputStream=[IO.File]::Open($env:DUZENEK_IMAGE_OUTPUT,[IO.FileMode]::CreateNew); try {$encoder.Save($outputStream)} finally {$outputStream.Dispose()}} finally {$inputStream.Dispose()}"#;
        // Süreç başlatma `process-bridge` üzerinden; zaman aşımı eklendi.
        // Ağ politikası Windows'ta UYGULANAMAZ: `sandbox-exec` macOS'a özgüdür
        // ve bu platformda karşılığı yoktur. Köprü bunu sessizce geçmez,
        // `NetworkPolicy::applied_on_this_platform()` false döner.
        process_bridge::run(
            process_bridge::Spawn::new(std::path::Path::new("powershell.exe"))
                .os_resolved()
                .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
                .env("DUZENEK_IMAGE_SOURCE", path)
                .env("DUZENEK_IMAGE_OUTPUT", &output)
                .timeout(std::time::Duration::from_secs(60))
                .network(process_bridge::NetworkPolicy::Deny),
        )
        .map_err(|_| {
            EklerError::UnsupportedFormat("Windows HEIF/WIC codec bulunamadı veya görsel çözümlenemedi. Yerel PNG/JPEG kopyası kullanın.".into())
        })?;
        let image = image::open(&output).map_err(|e| EklerError::InvalidImage(e.to_string()))?;
        convert_single_image_to_pdf(image)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(EklerError::UnsupportedFormat(
            "HEIC formatı bu işletim sisteminde henüz doğrudan desteklenmiyor".to_string(),
        ))
    }
}

fn add_image_page(
    doc: &mut LopdfDoc,
    pages_id: lopdf::ObjectId,
    img: &DynamicImage,
) -> Result<lopdf::ObjectId> {
    let (img_w, img_h) = img.dimensions();
    // Flatten transparency against paper white, not black; preserve tonal data
    // until the explicit 8-bit PDF image encoding boundary.
    let rgba = img.to_rgba8();
    let mut rgb = image::RgbImage::new(img_w, img_h);
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = u32::from(p[3]);
        rgb.put_pixel(
            x,
            y,
            image::Rgb(
                [0, 1, 2].map(|i| ((u32::from(p[i]) * a + 255 * (255 - a) + 127) / 255) as u8),
            ),
        );
    }

    let mut jpeg_bytes = Vec::new();
    let mut cursor = Cursor::new(&mut jpeg_bytes);
    rgb.write_to(&mut cursor, ImageFormat::Jpeg)
        .map_err(|e| EklerError::InvalidImage(e.to_string()))?;

    let image_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => img_w as i32,
            "Height" => img_h as i32,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg_bytes,
    );
    let image_id = doc.add_object(image_stream);

    let avail_w = A4_WIDTH_PT - 2.0 * DEFAULT_MARGIN_PT;
    let avail_h = A4_HEIGHT_PT - 2.0 * DEFAULT_MARGIN_PT;

    let img_aspect = (img_w as f32) / (img_h as f32);
    let box_aspect = avail_w / avail_h;

    let (draw_w, draw_h) = if img_aspect > box_aspect {
        (avail_w, avail_w / img_aspect)
    } else {
        (avail_h * img_aspect, avail_h)
    };

    let draw_x = DEFAULT_MARGIN_PT + (avail_w - draw_w) / 2.0;
    let draw_y = DEFAULT_MARGIN_PT + (avail_h - draw_h) / 2.0;

    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    draw_w.into(),
                    0.into(),
                    0.into(),
                    draw_h.into(),
                    draw_x.into(),
                    draw_y.into(),
                ],
            ),
            Operation::new("Do", vec!["Img1".into()]),
            Operation::new("Q", vec![]),
        ],
    };

    let content_id = doc.add_object(Stream::new(
        Dictionary::new(),
        content
            .encode()
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?,
    ));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), A4_WIDTH_PT.into(), A4_HEIGHT_PT.into()],
        "Resources" => dictionary! {
            "XObject" => dictionary! {
                "Img1" => image_id,
            },
        },
    });

    Ok(page_id)
}
