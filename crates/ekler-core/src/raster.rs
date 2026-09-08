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
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (output_dir, before);
        Err(EklerError::UnsupportedFormat(
            "PDF → görsel bu platformda henüz doğrulanmış bir yerel renderer içermiyor".into(),
        ))
    }
    #[cfg(target_os = "macos")]
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
        let copy = temp.path().join("source.pdf");
        safe_io::write_new_bytes(&copy, &[], &bytes)?;
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
        let destination = output_dir.join(format!("DuzenEk-Gorseller-{name}"));
        safe_io::publish_directory(staging.path(), &destination)?;
        Ok(destination)
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
    #[cfg(not(target_os = "macos"))]
    {
        let _ = source;
        Err(EklerError::UnsupportedFormat(
            "Yerel PDF önizlemesi bu platformda henüz mevcut değil".into(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        let before = calculate_sha256(source).map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let raw = std::fs::read(source).map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let doc = load_pdf_tolerant(&raw, "önizleme.pdf")?.document;
        let mut selected = crate::pdf::extract_page_range(&doc, page, page)?;
        for id in selected.get_pages().into_values() {
            let old = crate::pdf::resolved_page_dictionary(&selected, id)?
                .get(b"Rotate")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            selected
                .get_dictionary_mut(id)
                .map_err(|e| EklerError::InvalidPdf(e.to_string()))?
                .set("Rotate", (old + i64::from(rotation)).rem_euclid(360));
        }
        let dir = tempfile::tempdir().map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
        let path = dir.path().join("preview.pdf");
        selected
            .save(&path)
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?;
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
