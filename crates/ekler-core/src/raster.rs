//! Native OS rasterization. No GPL renderer is shipped or invoked.
use crate::*;
use std::path::{Path, PathBuf};
pub fn pdf_to_images(
    source: &Path,
    output_dir: &Path,
    format: &str,
    dpi: u32,
    approved: bool,
) -> Result<PathBuf> {
    if !["png", "jpg"].contains(&format) || !(72..=300).contains(&dpi) {
        return Err(EklerError::ValidationFailed(
            "PNG/JPG ve 72–300 DPI seçin".into(),
        ));
    }
    let before = calculate_sha256(source).map_err(|source_error| EklerError::IoError {
        path: source.into(),
        source: source_error,
    })?;
    let info = crate::pdf::inspect_pdf(source)?;
    if info.is_signed && !approved {
        return Err(EklerError::ValidationFailed(
            "İmzalı PDF için türetilmiş görsel onayı gerekli".into(),
        ));
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (output_dir, before);
        Err(EklerError::UnsupportedFormat(
            "PDF → görsel bu platformda henüz doğrulanmış bir yerel renderer içermiyor".into(),
        ))
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let staging = tempfile::Builder::new()
            .prefix(".duzenek-images-")
            .tempdir_in(output_dir)
            .map_err(|source| EklerError::IoError {
                path: output_dir.into(),
                source,
            })?;
        let bytes = std::fs::read(source).map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let checked = load_pdf_tolerant(&bytes, "source.pdf")?.document;
        for id in checked.get_pages().values() {
            let page = checked
                .get_dictionary(*id)
                .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
            if page.has(b"Annots") {
                return Err(EklerError::UnsupportedFormat("Açıklama veya form içeren PDF'nin görsel dönüşümü bu sürümde desteklenmiyor; görünüm kaybını önlemek için işlem durduruldu".into()));
            }
        }
        let mut working = checked;
        crate::pdf::stamp::apply_raster_branding(&mut working, dpi)?;
        let mut bytes = Vec::new();
        working
            .save_to(&mut bytes)
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let temp = tempfile::tempdir().map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let copy = safe_io::write_render_scratch(&temp, "source.pdf", &bytes)?;
        let images = native::render(&copy, dpi)?;
        if images.len() != info.page_count {
            return Err(EklerError::InvalidPdf(
                "Renderer sayfa sayısı uyuşmuyor".into(),
            ));
        }
        for (i, img) in images.into_iter().enumerate() {
            let mut data = std::io::Cursor::new(Vec::new());
            img.write_to(
                &mut data,
                if format == "png" {
                    ::image::ImageFormat::Png
                } else {
                    ::image::ImageFormat::Jpeg
                },
            )
            .map_err(|e| EklerError::InvalidImage(e.to_string()))?;
            ::image::load_from_memory(data.get_ref())
                .map_err(|e| EklerError::InvalidImage(e.to_string()))?;
            safe_io::write_new_bytes(
                &staging.path().join(format!("sayfa-{:04}.{format}", i + 1)),
                &[],
                data.get_ref(),
            )?;
        }
        if before != calculate_sha256(source).map_err(|e| EklerError::InvalidPdf(e.to_string()))? {
            return Err(EklerError::SourceIntegrityCompromised {
                path: source.into(),
            });
        }
        let name = staging
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".duzenek-images-", "");
        let destination = output_dir.join(format!("GolgeDosya-Gorseller-{name}"));
        safe_io::publish_directory(staging.path(), &destination)?;
        Ok(destination)
    }
}
/// Windows yerel renderer'ı: Windows.Data.Pdf (WinRT, işletim sisteminin kendi
/// PDF motoru). Kurulacak bileşen, paketlenecek DLL, ağ erişimi yok.
///
/// Dış sözleşme macOS'unkiyle AYNIDIR: `render(yol, dpi)` her sayfa için görünür
/// kutuyu (CropBox, yoksa MediaBox), sayfanın kendi `/Rotate` değeriyle,
/// `ceil(pt · dpi / 72)` piksele, beyaz zemine basar; güvenlik sınırları aynı.
///
/// Geometri Windows'un yorumuna BIRAKILMAZ. Renderer'a verilen, bellekte
/// normalize edilmiş özel bir kopyadır: her sayfada MediaBox = CropBox =
/// görünür kutu ve `/Rotate` = 0. Windows yalnız bu kutuyu verilen piksel
/// boyutuna boyar; saat yönündeki dönüş (ISO 32000-1 §7.7.3.3) bitmap üzerinde
/// uygulanır. Kırpma ve dönüş böylece macOS ile birebir aynıdır ve Windows'un
/// kutu/dönüş davranışına bağlı değildir. Kaynak dosya açılmaz bile: girdi,
/// çağıranın `TempDir` içine yazdığı çalışma kopyasıdır ve renderer onu
/// BELLEKTEN okur, diske yazmaz.
#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
    use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};

    fn fail() -> EklerError {
        EklerError::InvalidPdf("Yerel PDF renderer başarısız".into())
    }

    /// WinRT hata kodu kullanıcıya çıkmaz; cümle macOS yoluyla aynıdır.
    fn winrt<T>(result: windows::core::Result<T>) -> Result<T> {
        result.map_err(|_| fail())
    }

    /// WinRT için süreç geneli çok iş parçacıklı daire (MTA).
    ///
    /// Tauri ana iş parçacığını WebView2 için STA başlatır; render
    /// `spawn_blocking` iş parçacıklarında koşar. `CoIncrementMTAUsage` bir kez
    /// çağrılır ve bırakılmaz: COM'u hiç başlatmamış iş parçacıkları örtük
    /// MTA'ya katılır ve WinRT işleminin engelleyici `get()` beklemesi güvenlidir.
    fn ensure_mta() -> Result<()> {
        static MTA: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        // SAFETY: argümansız, süreç geneli sayaç artırımı; çerez bilerek
        // bırakılmaz (MTA süreç boyunca yaşar).
        let ready = *MTA
            .get_or_init(|| unsafe { windows::Win32::System::Com::CoIncrementMTAUsage() }.is_ok());
        ready.then_some(()).ok_or_else(fail)
    }

    /// Her sayfanın görünür kutusu (pt) ve saat yönündeki dönüşü; sayfa kopyada
    /// normalize edilir (MediaBox = CropBox = görünür kutu, `/Rotate` 0).
    fn normalize(doc: &mut lopdf::Document) -> Result<Vec<([f32; 4], i64)>> {
        let ids: Vec<lopdf::ObjectId> = doc.get_pages().into_values().collect();
        let mut pages = Vec::with_capacity(ids.len());
        for id in ids {
            let resolved = crate::pdf::resolved_page_dictionary(doc, id)?;
            let rect = |key: &[u8]| -> Option<[f32; 4]> {
                let (_, obj) = doc.dereference(resolved.get(key).ok()?).ok()?;
                let v: Vec<f32> = obj
                    .as_array()
                    .ok()?
                    .iter()
                    .filter_map(|o| o.as_float().ok())
                    .collect();
                (v.len() == 4).then(|| {
                    [
                        v[0].min(v[2]),
                        v[1].min(v[3]),
                        v[0].max(v[2]),
                        v[1].max(v[3]),
                    ]
                })
            };
            let media = rect(b"MediaBox")
                .ok_or_else(|| EklerError::InvalidPdf("Geçersiz MediaBox".into()))?;
            // Görünür alan — macOS renderer'ının (CoreGraphics) ölçülmüş davranışıyla
            // AYNI, platformlar ayrışmasın diye:
            //  * tuval CropBox'ın boyutundadır (kesişim ALINMAZ; GölgeDosya'nın kutu
            //    işlemleri de CropBox'ı olduğu gibi kullanır);
            //  * CropBox ∩ MediaBox tuvalin ORTASINA oturur; içerik MediaBox'a
            //    kırpılır (kesişime değil).
            // Olağan sayfada CropBox MediaBox'ın içindedir: kesişim = CropBox, kayma
            // sıfır, bu kural hiçbir şeyi değiştirmez. Yalnız MediaBox'tan TAŞAN
            // CropBox'ta (dejenere kutu) macOS'un ortalamasını birebir izler.
            let crop = rect(b"CropBox")
                .filter(|c| c[2] > c[0] && c[3] > c[1])
                .unwrap_or(media);
            let drawn = [
                crop[0].max(media[0]),
                crop[1].max(media[1]),
                crop[2].min(media[2]),
                crop[3].min(media[3]),
            ];
            let drawn = if drawn[2] > drawn[0] && drawn[3] > drawn[1] {
                drawn
            } else {
                media
            };
            let (ox, oy) = (
                ((crop[2] - crop[0]) - (drawn[2] - drawn[0])) / 2.,
                ((crop[3] - crop[1]) - (drawn[3] - drawn[1])) / 2.,
            );
            let visible = [drawn[0] - ox, drawn[1] - oy, drawn[2] + ox, drawn[3] + oy];
            if drawn != visible {
                // Pencere MediaBox'ı aşıyor: taşan kısım boş kalmalı. İçerik
                // MediaBox'a kırpılır — kesişime DEĞİL. macOS'ta ölçüldü: MediaBox
                // içinde olup CropBox dışında kalan içerik (pencereye düşüyorsa)
                // GÖRÜNÜR; kesişime kırpmak onu yanlışlıkla silerdi.
                let items = match doc
                    .get_dictionary(id)
                    .ok()
                    .and_then(|d| d.get(b"Contents").ok())
                {
                    Some(lopdf::Object::Array(a)) => a.clone(),
                    Some(lopdf::Object::Reference(r)) => match doc.get_object(*r) {
                        Ok(lopdf::Object::Array(a)) => a.clone(),
                        _ => vec![lopdf::Object::Reference(*r)],
                    },
                    _ => Vec::new(),
                };
                let clip = doc.add_object(lopdf::Stream::new(
                    lopdf::Dictionary::new(),
                    format!(
                        "q {} {} {} {} re W n\n",
                        media[0],
                        media[1],
                        media[2] - media[0],
                        media[3] - media[1]
                    )
                    .into_bytes(),
                ));
                let close = doc.add_object(lopdf::Stream::new(
                    lopdf::Dictionary::new(),
                    b"\nQ\n".to_vec(),
                ));
                let mut wrapped = vec![lopdf::Object::Reference(clip)];
                wrapped.extend(items);
                wrapped.push(lopdf::Object::Reference(close));
                doc.get_dictionary_mut(id)
                    .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
                    .set("Contents", wrapped);
            }
            let rotation = crate::toolbox::existing_rotation(doc, id)?;
            let boxed: Vec<lopdf::Object> =
                visible.iter().map(|v| lopdf::Object::Real(*v)).collect();
            let page = doc
                .get_dictionary_mut(id)
                .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
            page.set("MediaBox", boxed.clone());
            page.set("CropBox", boxed);
            page.set("Rotate", 0);
            pages.push((visible, rotation));
        }
        Ok(pages)
    }

    /// PNG baytlarını okur; saydam piksel beyaz zemine oturtulur (macOS yolu da
    /// beyaz doldurulmuş bir tuvale çizer).
    fn opaque_rgb(png: &[u8]) -> Result<::image::RgbImage> {
        let rgba = ::image::load_from_memory_with_format(png, ::image::ImageFormat::Png)
            .map_err(|e| EklerError::InvalidImage(e.to_string()))?
            .to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(::image::RgbImage::from_fn(w, h, |x, y| {
            let [r, g, b, a] = rgba.get_pixel(x, y).0;
            let blend =
                |c: u8| ((u16::from(c) * u16::from(a) + 255 * (255 - u16::from(a))) / 255) as u8;
            ::image::Rgb([blend(r), blend(g), blend(b)])
        }))
    }

    pub fn render(path: &Path, dpi: u32) -> Result<Vec<::image::DynamicImage>> {
        let raw = std::fs::read(path).map_err(|source| EklerError::IoError {
            path: path.into(),
            source,
        })?;
        let mut doc =
            lopdf::Document::load_mem(&raw).map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let pages = normalize(&mut doc)?;
        let mut normalized = Vec::new();
        doc.save_to(&mut normalized)
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        drop(doc);

        ensure_mta()?;
        let stream = winrt(InMemoryRandomAccessStream::new())?;
        let writer = winrt(DataWriter::CreateDataWriter(&stream))?;
        winrt(writer.WriteBytes(&normalized))?;
        winrt(winrt(writer.StoreAsync())?.get())?;
        winrt(winrt(writer.FlushAsync())?.get())?;
        winrt(writer.DetachStream())?;
        winrt(stream.Seek(0))?;
        let pdf = winrt(winrt(PdfDocument::LoadFromStreamAsync(&stream))?.get())?;
        if winrt(pdf.PageCount())? as usize != pages.len() {
            return Err(fail());
        }

        let mut images = Vec::with_capacity(pages.len());
        let mut budget = 0usize;
        for (index, (visible, rotation)) in pages.into_iter().enumerate() {
            let (w, h) = (
                f64::from(visible[2] - visible[0]),
                f64::from(visible[3] - visible[1]),
            );
            let width = (w * f64::from(dpi) / 72.).ceil();
            let height = (h * f64::from(dpi) / 72.).ceil();
            if !width.is_finite()
                || !height.is_finite()
                || width < 1.
                || height < 1.
                || width * height > 40_000_000.
            {
                return Err(EklerError::InvalidPdf(
                    "Görsel boyutu güvenli sınırı aşıyor".into(),
                ));
            }
            let (width, height) = (width as u32, height as u32);
            budget += width as usize * height as usize * 4;
            if budget > 512 * 1024 * 1024 {
                return Err(EklerError::InvalidPdf(
                    "Görsel paketi bellek sınırını aşıyor; daha az sayfa veya DPI seçin".into(),
                ));
            }
            let page = winrt(pdf.GetPage(index as u32))?;
            let options = winrt(PdfPageRenderOptions::new())?;
            winrt(options.SetDestinationWidth(width))?;
            winrt(options.SetDestinationHeight(height))?;
            winrt(options.SetBackgroundColor(windows::UI::Color {
                A: 255,
                R: 255,
                G: 255,
                B: 255,
            }))?;
            // Yüksek karşıtlık teması belgenin kendi renklerini DEĞİŞTİRMEMELİ.
            winrt(options.SetIsIgnoringHighContrast(true))?;
            let out = winrt(InMemoryRandomAccessStream::new())?;
            winrt(winrt(page.RenderWithOptionsToStreamAsync(&out, &options))?.get())?;
            let _ = page.Close();
            let size = u32::try_from(winrt(out.Size())?).map_err(|_| fail())?;
            let reader = winrt(DataReader::CreateDataReader(&winrt(
                out.GetInputStreamAt(0),
            )?))?;
            winrt(winrt(reader.LoadAsync(size))?.get())?;
            let mut png = vec![0u8; size as usize];
            winrt(reader.ReadBytes(&mut png))?;
            let mut rgb = opaque_rgb(&png)?;
            // Renderer istenen tuvali döndürmeli; birkaç piksellik yuvarlama
            // farkı tuvale oturtulur, daha büyüğü gerçek bir hatadır.
            if rgb.dimensions() != (width, height) {
                let (gw, gh) = rgb.dimensions();
                if gw.abs_diff(width) > 2 || gh.abs_diff(height) > 2 {
                    return Err(fail());
                }
                rgb = ::image::imageops::resize(
                    &rgb,
                    width,
                    height,
                    ::image::imageops::FilterType::Triangle,
                );
            }
            let img = ::image::DynamicImage::ImageRgb8(rgb);
            images.push(match rotation {
                90 => img.rotate90(),
                180 => img.rotate180(),
                270 => img.rotate270(),
                _ => img,
            });
        }
        Ok(images)
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::ffi::c_void;
    type Ptr = *mut c_void;
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Size {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        origin: Point,
        size: Size,
    }
    #[repr(C)]
    struct Transform {
        a: f64,
        b: f64,
        c: f64,
        d: f64,
        tx: f64,
        ty: f64,
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: Ptr,
            buffer: *const u8,
            len: isize,
            is_directory: bool,
        ) -> Ptr;
        fn CFRelease(value: Ptr);
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPDFDocumentCreateWithURL(url: Ptr) -> Ptr;
        fn CGPDFDocumentRelease(doc: Ptr);
        fn CGPDFDocumentGetNumberOfPages(doc: Ptr) -> usize;
        fn CGPDFDocumentGetPage(doc: Ptr, page: usize) -> Ptr;
        fn CGPDFPageGetBoxRect(page: Ptr, box_id: i32) -> Rect;
        fn CGPDFPageGetRotationAngle(page: Ptr) -> i32;
        fn CGPDFPageGetDrawingTransform(
            page: Ptr,
            box_id: i32,
            rect: Rect,
            rotate: i32,
            preserve: bool,
        ) -> Transform;
        fn CGColorSpaceCreateDeviceRGB() -> Ptr;
        fn CGColorSpaceRelease(space: Ptr);
        fn CGBitmapContextCreate(
            data: Ptr,
            width: usize,
            height: usize,
            bits: usize,
            row_bytes: usize,
            space: Ptr,
            info: u32,
        ) -> Ptr;
        fn CGContextRelease(ctx: Ptr);
        fn CGContextSetRGBFillColor(ctx: Ptr, r: f64, g: f64, b: f64, a: f64);
        fn CGContextFillRect(ctx: Ptr, rect: Rect);
        fn CGContextConcatCTM(ctx: Ptr, transform: Transform);
        fn CGContextScaleCTM(ctx: Ptr, sx: f64, sy: f64);
        fn CGContextDrawPDFPage(ctx: Ptr, page: Ptr);
    }
    struct Owned(Ptr, unsafe extern "C" fn(Ptr));
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { (self.1)(self.0) }
        }
    }
    pub fn render(path: &Path, dpi: u32) -> Result<Vec<::image::DynamicImage>> {
        use std::os::unix::ffi::OsStrExt;
        let fail = || EklerError::InvalidPdf("Yerel PDF renderer başarısız".into());
        unsafe {
            let path = path.as_os_str().as_bytes();
            let url = CFURLCreateFromFileSystemRepresentation(
                std::ptr::null_mut(),
                path.as_ptr(),
                path.len() as isize,
                false,
            );
            if url.is_null() {
                return Err(fail());
            }
            let url = Owned(url, CFRelease);
            let doc = CGPDFDocumentCreateWithURL(url.0);
            if doc.is_null() {
                return Err(fail());
            }
            let doc = Owned(doc, CGPDFDocumentRelease);
            let space = CGColorSpaceCreateDeviceRGB();
            if space.is_null() {
                return Err(fail());
            }
            let space = Owned(space, CGColorSpaceRelease);
            let mut images = Vec::new();
            let mut budget = 0usize;
            for i in 1..=CGPDFDocumentGetNumberOfPages(doc.0) {
                let page = CGPDFDocumentGetPage(doc.0, i);
                if page.is_null() {
                    return Err(fail());
                }
                let bounds = CGPDFPageGetBoxRect(page, 1);
                let rotated = CGPDFPageGetRotationAngle(page).rem_euclid(180) != 0;
                let (w, h) = if rotated {
                    (bounds.size.height, bounds.size.width)
                } else {
                    (bounds.size.width, bounds.size.height)
                };
                let width = (w * dpi as f64 / 72.).ceil();
                let height = (h * dpi as f64 / 72.).ceil();
                if !width.is_finite()
                    || !height.is_finite()
                    || width < 1.
                    || height < 1.
                    || width * height > 40_000_000.
                {
                    return Err(EklerError::InvalidPdf(
                        "Görsel boyutu güvenli sınırı aşıyor".into(),
                    ));
                }
                let (width, height) = (width as usize, height as usize);
                budget += width * height * 4;
                if budget > 512 * 1024 * 1024 {
                    return Err(EklerError::InvalidPdf(
                        "Görsel paketi bellek sınırını aşıyor; daha az sayfa veya DPI seçin".into(),
                    ));
                }
                let mut data = vec![255u8; width * height * 4];
                let ctx = CGBitmapContextCreate(
                    data.as_mut_ptr().cast(),
                    width,
                    height,
                    8,
                    width * 4,
                    space.0,
                    1 | (4 << 12),
                );
                if ctx.is_null() {
                    return Err(fail());
                }
                let ctx = Owned(ctx, CGContextRelease);
                let rect = Rect {
                    origin: Point { x: 0., y: 0. },
                    size: Size {
                        width: width as f64,
                        height: height as f64,
                    },
                };
                CGContextSetRGBFillColor(ctx.0, 1., 1., 1., 1.);
                CGContextFillRect(ctx.0, rect);
                // CGPDFPageGetDrawingTransform scales down only (per CGPDFPage.h).
                // Apply DPI explicitly, then compute page/rotation placement in PDF points.
                CGContextScaleCTM(ctx.0, dpi as f64 / 72., dpi as f64 / 72.);
                let page_rect = Rect {
                    origin: Point { x: 0., y: 0. },
                    size: Size {
                        width: w,
                        height: h,
                    },
                };
                CGContextConcatCTM(
                    ctx.0,
                    CGPDFPageGetDrawingTransform(page, 1, page_rect, 0, true),
                );
                CGContextDrawPDFPage(ctx.0, page);
                drop(ctx);
                let img = ::image::RgbaImage::from_raw(width as u32, height as u32, data)
                    .ok_or_else(fail)?;
                images.push(::image::DynamicImage::ImageRgba8(img).to_rgb8().into());
            }
            Ok(images)
        }
    }
}

/// Read-only, bounded preview. A private one-page copy avoids rendering a whole book.
pub fn preview_page(source: &Path, page: usize, dpi: u32, rotation: i32) -> Result<Vec<u8>> {
    if page == 0 || !(36..=300).contains(&dpi) || rotation % 90 != 0 {
        return Err(EklerError::InvalidPdf(
            "Geçersiz önizleme sayfası/ölçeği".into(),
        ));
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = source;
        Err(EklerError::UnsupportedFormat(
            "Yerel PDF önizlemesi bu platformda henüz mevcut değil".into(),
        ))
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let before = calculate_sha256(source).map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let raw = std::fs::read(source).map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let doc = load_pdf_tolerant(&raw, "önizleme.pdf")?.document;
        let mut selected = crate::pdf::extract_page_range(&doc, page, page)?;
        for id in selected.get_pages().into_values() {
            // Dönüşü ARAÇLARLA AYNI biçimde oku. Burada ham `as_i64` vardı:
            // `/Rotate` dolaylı bir başvuru (`12 0 R`) ya da ondalık (`90.0`)
            // olduğunda 0 sayılıyor, önizleme sayfayı DÖNDÜRÜLMEMİŞ gösteriyordu
            // — oysa kaydedilen çıktı dönüşü koruyordu. Ekranda görülen ile
            // diske yazılan ayrışıyordu; tek kaynaktan okuyunca ayrışamaz.
            let old = crate::toolbox::existing_rotation(&selected, id)?;
            selected
                .get_dictionary_mut(id)
                .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
                .set("Rotate", (old + i64::from(rotation)).rem_euclid(360));
        }
        let dir = tempfile::tempdir().map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let mut working = Vec::new();
        selected
            .save_to(&mut working)
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let path = safe_io::write_render_scratch(&dir, "preview.pdf", &working)?;
        let mut images = native::render(&path, dpi)?;
        let img = images
            .pop()
            .ok_or_else(|| EklerError::InvalidPdf("Önizleme üretilemedi".into()))?;
        let mut bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut bytes, ::image::ImageFormat::Png)
            .map_err(|e| EklerError::InvalidImage(e.to_string()))?;
        if before != calculate_sha256(source).map_err(|e| EklerError::InvalidPdf(e.to_string()))? {
            return Err(EklerError::SourceIntegrityCompromised {
                path: source.into(),
            });
        }
        Ok(bytes.into_inner())
    }
}
