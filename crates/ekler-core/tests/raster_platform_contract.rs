//! PDF önizleme / görselleştirmenin GERÇEK render sözleşmesi.
//!
//! Saha bulgusu (v0.3.0, Windows): her PDF'de, GölgeDosya'nın kendi ürettiği
//! dahil, önizleme yerine "Yerel PDF önizlemesi bu platformda henüz mevcut
//! değil" çıkıyordu; Düzenle fiilen kullanılamıyordu. Bu dosyanın önceki
//! sürümü tam olarak bu reddi "doğru davranış" diye sabitliyordu. Artık kabul
//! kriteri reddin temizliği değil, GERÇEK PİKSELDİR.
//!
//! macOS (CoreGraphics) ve Windows (Windows.Data.Pdf) aynı dış sözleşmeyi
//! sağlar ve bu dosya ikisinde de aynen koşar:
//!
//! * görünür kutu (CropBox, yoksa MediaBox) `ceil(pt · dpi / 72)` piksele basılır,
//! * sayfanın `/Rotate` değeri ve kullanıcı dönüşü BİR KEZ, saat yönünde uygulanır,
//! * doğru sayfa döner (sıra değişse de),
//! * kaynak dosya bayt bayt aynı kalır.
//!
//! Altın görüntü ANALİTİKTİR: düz renk dikdörtgenlerden oluşan bir sayfanın
//! beklenen bitmap'i PDF geometrisinden hesaplanır ve ortalama mutlak hatayla
//! karşılaştırılır. Platforma özgü PNG saklanmaz; kenar yumuşatması farkı
//! eşiğe sığar, yanlış sayfa / yanlış dönüş / yanlış kırpma sığmaz.
use ekler_core::{calculate_sha256, raster, EklerError};
use lopdf::{dictionary, Document, Object, Stream};
use std::path::{Path, PathBuf};

/// Kullanıcı cümlesi teknik iz taşımamalı (arayüzdeki `safeMessage` süzgeci).
fn assert_user_safe(message: &str, what: &str) {
    assert!(!message.trim().is_empty(), "{what}: boş hata cümlesi");
    for jargon in [
        "panic",
        "unwrap",
        "HRESULT",
        "0x8",
        "::",
        "Err(",
        "cfg(",
        "target_os",
    ] {
        assert!(
            !message.contains(jargon),
            "{what}: kullanıcı cümlesinde teknik iz ({jargon}): {message}"
        );
    }
}

/// Giriş doğrulaması platform kapısından ÖNCE gelir: geçersiz sayfa/DPI her
/// platformda aynı doğrulama hatasını verir.
#[test]
fn invalid_input_is_rejected_the_same_way_on_every_platform() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("x.pdf");
    let mut d = Document::with_version("1.5");
    let pages = d.new_object_id();
    let content = d.add_object(Stream::new(
        dictionary! {},
        b"0 0 0 rg 10 10 50 50 re f".to_vec(),
    ));
    let page = d.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,
    "MediaBox"=>vec![0.into(),0.into(),200.into(),200.into()],"Contents"=>content});
    d.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer.set("Root", root);
    d.save(&source).unwrap();
    for (page, dpi, rotation, why) in [
        (0usize, 150u32, 0i32, "sayfa 0"),
        (1, 10, 0, "DPI çok düşük"),
        (1, 4000, 0, "DPI çok yüksek"),
        (1, 150, 45, "90'ın katı olmayan dönüş"),
    ] {
        match raster::preview_page(&source, page, dpi, rotation) {
            Err(EklerError::InvalidPdf(_)) => {}
            other => panic!("{why}: doğrulama hatası bekleniyordu, {other:?} geldi"),
        }
    }
    for (format, dpi, why) in [
        ("gif", 150u32, "desteklenmeyen biçim"),
        ("png", 10, "DPI düşük"),
    ] {
        match raster::pdf_to_images(&source, dir.path(), format, dpi, false) {
            Err(EklerError::ValidationFailed(_)) => {}
            other => panic!("{why}: doğrulama hatası bekleniyordu, {other:?} geldi"),
        }
    }
}

/// Yerel renderer'ı olmayan platform (ör. Linux): reddi temiz ve dürüst.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[test]
fn a_platform_without_a_native_renderer_refuses_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("x.pdf");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/vector.pdf"),
        &source,
    )
    .unwrap();
    for err in [
        raster::preview_page(&source, 1, 72, 0).unwrap_err(),
        raster::pdf_to_images(&source, dir.path(), "png", 72, false).unwrap_err(),
    ] {
        match err {
            EklerError::UnsupportedFormat(m) => assert_user_safe(&m, "ret"),
            other => panic!("UnsupportedFormat bekleniyordu: {other:?}"),
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod render {
    use super::*;
    use ekler_core::toolbox::{run_tool_with_outcome, ToolOperation, ToolOutcome};
    use ekler_core::{
        execute_uyap_preparation, ExecutionContext, ExhibitSourceRef, OptimizationLevel, Project,
        SignedPolicy, SourceFile, SourceFormat,
    };
    use image::{Rgb, RgbImage};

    const BLACK: [u8; 3] = [0, 0, 0];
    const RED: [u8; 3] = [255, 0, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    /// PDF koordinatlarında (sol-alt köken) düz renk dikdörtgen.
    #[derive(Clone, Copy)]
    struct Fill {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rgb: [u8; 3],
    }

    const fn fill(x: f32, y: f32, w: f32, h: f32, rgb: [u8; 3]) -> Fill {
        Fill { x, y, w, h, rgb }
    }

    #[derive(Clone)]
    struct VPage {
        media: [f32; 4],
        crop: Option<[f32; 4]>,
        rotate: i64,
        fills: Vec<Fill>,
        /// Altın karşılaştırmanın dışında tutulan metin (yazı tipi yumuşatması
        /// platforma göre değişir): (metin, x, y, punto).
        label: Option<(&'static str, f32, f32, f32)>,
    }

    fn vpage(w: f32, h: f32, fills: &[Fill]) -> VPage {
        VPage {
            media: [0., 0., w, h],
            crop: None,
            rotate: 0,
            fills: fills.to_vec(),
            label: None,
        }
    }

    fn vector_pdf(path: &Path, pages: &[VPage]) {
        let mut d = Document::with_version("1.7");
        let pages_id = d.new_object_id();
        let font = d.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1",
        "BaseFont"=>"Helvetica-Bold","Encoding"=>"WinAnsiEncoding"});
        let mut kids = Vec::new();
        for p in pages {
            let mut ops = String::new();
            for f in &p.fills {
                let c = f.rgb.map(|v| f32::from(v) / 255.);
                ops.push_str(&format!(
                    "{} {} {} rg {} {} {} {} re f\n",
                    c[0], c[1], c[2], f.x, f.y, f.w, f.h
                ));
            }
            if let Some((text, x, y, size)) = p.label {
                ops.push_str(&format!(
                    "0 0 0 rg BT /F1 {size} Tf {x} {y} Td ({text}) Tj ET\n"
                ));
            }
            let content = d.add_object(Stream::new(dictionary! {}, ops.into_bytes()));
            let boxed = |b: [f32; 4]| b.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>();
            let mut page = dictionary! {"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>boxed(p.media),
            "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},"Contents"=>content};
            if let Some(c) = p.crop {
                page.set("CropBox", boxed(c));
            }
            if p.rotate != 0 {
                page.set("Rotate", p.rotate);
            }
            kids.push(d.add_object(page).into());
        }
        let count = kids.len() as i64;
        d.objects.insert(
            pages_id,
            dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>count}.into(),
        );
        let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
        d.trailer.set("Root", root);
        d.save(path).unwrap();
    }

    /// Beklenen bitmap: görünür kutu `ceil(pt·dpi/72)` piksel, piksel merkezi
    /// hangi dolgunun içindeyse o renk, değilse beyaz; sonra toplam dönüş saat
    /// yönünde. `mask` (varsa) karşılaştırma dışı bölgedir (PDF koordinatı).
    fn golden(p: &VPage, dpi: u32, extra_rotation: i64) -> (RgbImage, Option<RgbImage>) {
        // Tuval CropBox boyutunda; CropBox ∩ MediaBox tuvalin ortasına oturur ve
        // içerik MediaBox'a kırpılır (macOS'ta ölçülen davranış; Windows yolu
        // birebir izler). Olağan sayfada kesişim = CropBox, kayma sıfırdır.
        let m = p.media;
        let c = p.crop.unwrap_or(m);
        let d = [
            c[0].max(m[0]),
            c[1].max(m[1]),
            c[2].min(m[2]),
            c[3].min(m[3]),
        ];
        let (ox, oy) = (
            ((c[2] - c[0]) - (d[2] - d[0])) / 2.,
            ((c[3] - c[1]) - (d[3] - d[1])) / 2.,
        );
        let v = [d[0] - ox, d[1] - oy, d[2] + ox, d[3] + oy];
        let drawn = |x: f64, y: f64| {
            x >= m[0] as f64 && x < m[2] as f64 && y >= m[1] as f64 && y < m[3] as f64
        };
        let k = 72. / dpi as f64;
        let (w, h) = (
            ((v[2] - v[0]) as f64 / k).ceil() as u32,
            ((v[3] - v[1]) as f64 / k).ceil() as u32,
        );
        let at = |px: u32, py: u32| -> (f64, f64) {
            (
                v[0] as f64 + (px as f64 + 0.5) * k,
                v[3] as f64 - (py as f64 + 0.5) * k,
            )
        };
        let img = RgbImage::from_fn(w, h, |px, py| {
            let (x, y) = at(px, py);
            if !drawn(x, y) {
                return Rgb([255, 255, 255]);
            }
            let hit = p.fills.iter().rev().find(|f| {
                x >= f.x as f64
                    && x < (f.x + f.w) as f64
                    && y >= f.y as f64
                    && y < (f.y + f.h) as f64
            });
            Rgb(hit.map(|f| f.rgb).unwrap_or([255, 255, 255]))
        });
        let mask = p.label.map(|(text, tx, ty, size)| {
            let (mx0, my0) = (tx as f64 - 4., ty as f64 - size as f64 * 0.4);
            let (mx1, my1) = (
                tx as f64 + text.len() as f64 * size as f64 * 0.8,
                ty as f64 + size as f64 * 1.1,
            );
            RgbImage::from_fn(w, h, |px, py| {
                let (x, y) = at(px, py);
                Rgb(if x >= mx0 && x <= mx1 && y >= my0 && y <= my1 {
                    [1, 1, 1]
                } else {
                    [0, 0, 0]
                })
            })
        });
        let rotate = |i: RgbImage| match (p.rotate + extra_rotation).rem_euclid(360) {
            90 => image::imageops::rotate90(&i),
            180 => image::imageops::rotate180(&i),
            270 => image::imageops::rotate270(&i),
            _ => i,
        };
        (rotate(img), mask.map(rotate))
    }

    /// Ortalama mutlak hata (0–255), maskelenen pikseller hariç. Boyut eşit olmalı.
    fn mae(expected: &RgbImage, got: &RgbImage, mask: Option<&RgbImage>) -> f64 {
        assert_eq!(expected.dimensions(), got.dimensions(), "görüntü boyutu");
        let (mut sum, mut n) = (0u64, 0u64);
        for (x, y, e) in expected.enumerate_pixels() {
            if mask.is_some_and(|m| m.get_pixel(x, y)[0] == 1) {
                continue;
            }
            let g = got.get_pixel(x, y);
            sum += (0..3).map(|c| u64::from(e[c].abs_diff(g[c]))).sum::<u64>();
            n += 3;
        }
        sum as f64 / n.max(1) as f64
    }

    /// Kaynak baytları değişmeden, gerçek bir PNG olarak önizleme.
    fn preview(src: &Path, page: usize, dpi: u32, rotation: i32) -> RgbImage {
        let before = calculate_sha256(src).unwrap();
        let png = raster::preview_page(src, page, dpi, rotation)
            .unwrap_or_else(|e| panic!("{} s.{page} @{dpi}: {e}", src.display()));
        assert_eq!(
            before,
            calculate_sha256(src).unwrap(),
            "önizleme kaynağı değiştirdi"
        );
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "önizleme PNG değil");
        let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .expect("PNG çözülemedi")
            .to_rgb8();
        assert!(img.width() > 0 && img.height() > 0);
        img
    }

    fn assert_golden(src: &Path, page: usize, p: &VPage, dpi: u32, rotation: i32, what: &str) {
        let got = preview(src, page, dpi, rotation);
        let (want, mask) = golden(p, dpi, i64::from(rotation));
        let err = mae(&want, &got, mask.as_ref());
        assert!(
            err < GOLDEN_LIMIT,
            "{what}: altın görüntüden sapma MAE {err:.2} (sınır {GOLDEN_LIMIT})"
        );
    }

    /// Altın görüntü eşiği. Ölçüldü (macOS, CoreGraphics): doğru render MAE
    /// 0,00; yanlış yönde dönüş (90° yerine 270°, 0° yerine 180°) — boyut aynı
    /// olduğu için yalnız piksel yakalayabilir — en az 3,46. Eşik ikisinin
    /// ARASINDADIR ve tam sayı olmayan DPI'daki kenar yumuşatmasına yer bırakır.
    /// İlk taslakta 6'ydı: yanlış yönde döndüren bir renderer GEÇERDİ.
    const GOLDEN_LIMIT: f64 = 1.5;

    /// Sol-üst işaretin (siyah) görüntülenen sayfada düştüğü köşe. Saat
    /// yönünde: 0 → sol üst, 90 → sağ üst, 180 → sağ alt, 270 → sol alt.
    /// Boyutla ayırt edilemeyen yön hatasını doğrudan yakalar.
    fn assert_marker_corner(img: &RgbImage, total_rotation: i64, what: &str) {
        let (w, h, d) = (img.width(), img.height(), 60u32);
        let corners = [(d, d), (w - d, d), (w - d, h - d), (d, h - d)];
        let want = match total_rotation.rem_euclid(360) {
            0 => 0,
            90 => 1,
            180 => 2,
            _ => 3,
        };
        for (i, (x, y)) in corners.into_iter().enumerate() {
            let px = img.get_pixel(x, y).0;
            let dark = px.iter().all(|c| *c < 60);
            assert_eq!(
                dark,
                i == want,
                "{what}: köşe {i} {} (işaret {want}. köşede olmalı); piksel {px:?}",
                if dark { "koyu" } else { "açık" }
            );
        }
    }

    /// Koyu piksellerin kütle merkezi (0–1) ve koyu oran: içerik var mı, nerede?
    fn dark_mass(img: &RgbImage) -> (f64, f64, f64) {
        let (mut n, mut sx, mut sy) = (0u64, 0u64, 0u64);
        for (x, y, p) in img.enumerate_pixels() {
            if (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) < 240 {
                n += 1;
                sx += u64::from(x);
                sy += u64::from(y);
            }
        }
        let total = f64::from(img.width()) * f64::from(img.height());
        if n == 0 {
            return (0., -1., -1.);
        }
        (
            n as f64 / total,
            sx as f64 / n as f64 / f64::from(img.width()),
            sy as f64 / n as f64 / f64::from(img.height()),
        )
    }

    // --------------------------------------------------------------- girdiler

    /// Sol yarısı siyah, sağ yarısı açık bir "tarama" görüntüsü.
    fn half_dark(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                Rgb([20, 20, 20])
            } else {
                Rgb([235, 232, 225])
            }
        })
    }

    fn image_page_pdf(path: &Path, image: Stream, pw: f32, ph: f32) {
        let mut d = Document::with_version("1.7");
        let pages = d.new_object_id();
        let img = d.add_object(image);
        let content = d.add_object(Stream::new(
            dictionary! {},
            format!("q {pw} 0 0 {ph} 0 0 cm /Im0 Do Q").into_bytes(),
        ));
        let page = d.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,
        "MediaBox"=>vec![0.into(),0.into(),pw.into(),ph.into()],
        "Resources"=>dictionary!{"XObject"=>dictionary!{"Im0"=>img}},"Contents"=>content});
        d.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
        );
        let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        d.trailer.set("Root", root);
        d.save(path).unwrap();
    }

    fn jpeg_stream(img: &RgbImage, quality: u8) -> Stream {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
            .encode_image(img)
            .unwrap();
        Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>img.width() as i64,
            "Height"=>img.height() as i64,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,
            "Filter"=>"DCTDecode"},
            jpeg,
        )
    }

    fn flate_stream(img: &RgbImage) -> Stream {
        let mut s = Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>img.width() as i64,
            "Height"=>img.height() as i64,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8},
            img.as_raw().clone(),
        );
        s.compress().unwrap();
        s
    }

    /// iLovePDF benzeri araçların yaptığı gibi: PNG'nin IDAT verisi olduğu gibi,
    /// PNG öngörücüsüyle (`/Predictor 15`) Flate akışı olarak gömülür.
    fn png_passthrough_stream(img: &RgbImage) -> Stream {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let mut idat = Vec::new();
        let mut at = 8;
        while at + 8 <= png.len() {
            let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            if &png[at + 4..at + 8] == b"IDAT" {
                idat.extend_from_slice(&png[at + 8..at + 8 + len]);
            }
            at += 12 + len;
        }
        Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>img.width() as i64,
            "Height"=>img.height() as i64,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,
            "Filter"=>"FlateDecode","DecodeParms"=>dictionary!{"Predictor"=>15,"Colors"=>3,
            "BitsPerComponent"=>8,"Columns"=>img.width() as i64}},
            idat,
        )
    }

    use image::ImageEncoder;

    fn tool(src: &[PathBuf], op: ToolOperation, out: &Path) -> ToolOutcome {
        run_tool_with_outcome(src, &op, out, true).unwrap_or_else(|e| panic!("{op:?}: {e}"))
    }

    fn a4_markers() -> VPage {
        vpage(
            595.,
            842.,
            &[
                fill(20., 662., 160., 160., BLACK), // sol üst (büyük, ayırt edici)
                fill(247., 371., 100., 100., RED),  // orta
                fill(455., 40., 100., 100., BLUE),  // sağ alt
            ],
        )
    }

    // ----------------------------------------------------------------- testler

    #[test]
    fn a_vector_page_matches_its_analytic_golden_image() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("vektor.pdf");
        let p = a4_markers();
        vector_pdf(&src, std::slice::from_ref(&p));
        let got = preview(&src, 1, 72, 0);
        assert_eq!(got.dimensions(), (595, 842), "72 DPI'da 1 pt = 1 piksel");
        // Dolguların iç noktaları tam renginde (kenar yumuşatmasından uzak).
        for (x, y, want) in [
            (100u32, 100u32, BLACK),
            (297, 421, RED),
            (505, 752, BLUE),
            (300, 250, [255; 3]),
        ] {
            let g = got.get_pixel(x, y).0;
            let diff = (0..3).map(|c| g[c].abs_diff(want[c])).max().unwrap();
            assert!(diff <= 40, "({x},{y}) {g:?}, beklenen {want:?}");
        }
        assert_golden(&src, 1, &p, 72, 0, "A4 vektör");
    }

    /// Her sayfa KENDİ içeriğini döndürür; sıra karışık istenince de. Önbellek
    /// ya da yarış bir sayfanın resmini ötekine vermemeli.
    #[test]
    fn each_page_renders_its_own_content_in_any_order() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("cok-sayfa.pdf");
        let mut pages = Vec::new();
        for (i, (x, y)) in [(40., 702.), (455., 40.), (247., 371.), (40., 40.)]
            .into_iter()
            .enumerate()
        {
            let mut p = vpage(595., 842., &[fill(x, y, 100., 100., BLACK)]);
            p.label = Some((["PAGE 1", "PAGE 2", "PAGE 3", "PAGE 4"][i], 200., 790., 24.));
            pages.push(p);
        }
        vector_pdf(&src, &pages);
        for n in [1usize, 2, 3, 4, 1, 3, 2, 4] {
            assert_golden(&src, n, &pages[n - 1], 72, 0, &format!("sayfa {n}"));
            // Aynı çağrı BAŞKA bir sayfanın altın görüntüsüne uymamalı.
            let got = preview(&src, n, 72, 0);
            let other = &pages[n % pages.len()];
            let (want, mask) = golden(other, 72, 0);
            let other = mae(&want, &got, mask.as_ref());
            assert!(
                other > 2.0 * GOLDEN_LIMIT,
                "sayfa {n} başka sayfaya benziyor (MAE {other:.2})"
            );
        }
    }

    /// `/Rotate` ve kullanıcı dönüşü BİR KEZ ve saat yönünde uygulanır; en-boy
    /// 90/270'te yer değiştirir. Çift dönüş sol-üst işaretini yanlış köşeye koyar.
    #[test]
    fn rotation_is_applied_once_clockwise_in_display_orientation() {
        let dir = tempfile::tempdir().unwrap();
        for page_rotate in [0i64, 90, 180, 270] {
            let src = dir.path().join(format!("donuk-{page_rotate}.pdf"));
            let mut p = a4_markers();
            p.rotate = page_rotate;
            vector_pdf(&src, std::slice::from_ref(&p));
            for user in [0i32, 90, 180, 270] {
                let total = (page_rotate + i64::from(user)).rem_euclid(360);
                let got = preview(&src, 1, 72, user);
                let want_dims = if total % 180 == 0 {
                    (595, 842)
                } else {
                    (842, 595)
                };
                assert_eq!(
                    got.dimensions(),
                    want_dims,
                    "/Rotate {page_rotate} + {user}"
                );
                assert_marker_corner(&got, total, &format!("/Rotate {page_rotate} + {user}"));
                assert_golden(
                    &src,
                    1,
                    &p,
                    72,
                    user,
                    &format!("/Rotate {page_rotate} + kullanıcı {user}"),
                );
            }
        }
    }

    /// Görünür alan CropBox'tır (MediaBox'a kırpılmış): dışındaki içerik
    /// görünmez, boyut kırpılmış kutudan gelir.
    #[test]
    fn the_cropbox_is_the_visible_area() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("kirpik.pdf");
        let mut p = vpage(
            600.,
            800.,
            &[
                fill(0., 0., 90., 800., BLACK),    // kırpılan şerit
                fill(150., 500., 120., 100., RED), // görünür
                fill(380., 180., 100., 80., BLUE), // görünür
            ],
        );
        p.crop = Some([100., 150., 500., 650.]);
        vector_pdf(&src, std::slice::from_ref(&p));
        let got = preview(&src, 1, 72, 0);
        assert_eq!(got.dimensions(), (400, 500));
        assert_golden(&src, 1, &p, 72, 0, "CropBox");
        // MediaBox'tan TAŞAN (dejenere) CropBox: tuval CropBox boyutunda, içerik
        // kesişim ve tuvalin ortasında — macOS'ta ölçülen davranış. Windows yolu
        // aynısını üretir; platformlar bu uç durumda da AYRIŞMAZ.
        let src2 = dir.path().join("tasan.pdf");
        let mut q = p.clone();
        q.crop = Some([-50., 100., 650., 900.]);
        vector_pdf(&src2, std::slice::from_ref(&q));
        assert_eq!(preview(&src2, 1, 72, 0).dimensions(), (700, 800));
        assert_golden(&src2, 1, &q, 72, 0, "taşan CropBox");
    }

    /// Çözünürlük istenen DPI'dır (Windows %125/150/200 ölçekleme dahil,
    /// arayüz DPI'ı `96 · zoom · devicePixelRatio` olarak ister).
    #[test]
    fn resolution_follows_the_requested_dpi() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("dpi.pdf");
        let p = a4_markers();
        vector_pdf(&src, std::slice::from_ref(&p));
        for dpi in [40u32, 72, 96, 120, 144, 192, 300] {
            let got = preview(&src, 1, dpi, 0);
            let want = (
                (595. * f64::from(dpi) / 72.).ceil() as u32,
                (842. * f64::from(dpi) / 72.).ceil() as u32,
            );
            assert_eq!(got.dimensions(), want, "{dpi} DPI");
        }
        assert_golden(&src, 1, &p, 150, 0, "150 DPI");
    }

    /// Tam piksel sınırındaki sayfa bir piksel FAZLA yuvarlanmaz. Gerçek A4
    /// ondalıklı (595,28 × 841,89 pt); görsel dışa aktarma 34 pt pay ekler ve
    /// yatay sayfada yükseklik 629,28 pt × 150/72 = tam 1311,0 olur. Windows'un
    /// ilk native koşusu burada 1312 verdi (f32 yuvarlama); macOS 1311.
    #[test]
    fn a_page_on_an_exact_pixel_boundary_is_not_rounded_up() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a4-yatay.pdf");
        vector_pdf(
            &src,
            &[vpage(841.89, 595.28, &[fill(40., 40., 100., 100., BLACK)])],
        );
        let out = dir.path().join("cikti");
        std::fs::create_dir_all(&out).unwrap();
        let folder = raster::pdf_to_images(&src, &out, "jpg", 150, false).unwrap();
        let img = image::open(folder.join("sayfa-0001.jpg")).unwrap();
        // Beklenen değer, dosyadaki ONDALIK metinden (f64) hesaplanır.
        let (w, h) = (841.89f64, 595.28f64 + 34.);
        assert_eq!(img.width(), (w * 150. / 72.).ceil() as u32, "genişlik");
        assert_eq!(
            img.height(),
            (h * 150. / 72.).ceil() as u32,
            "yükseklik (1311, 1312 değil)"
        );
        assert_eq!(img.height(), 1311);
    }

    /// Yatay sayfa ve yatay + dönük sayfa.
    #[test]
    fn landscape_pages_render_in_their_own_orientation() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("yatay.pdf");
        let a = vpage(
            842.,
            595.,
            &[
                fill(40., 455., 120., 100., BLACK),
                fill(682., 40., 120., 100., RED),
            ],
        );
        let mut b = a.clone();
        b.rotate = 90;
        vector_pdf(&src, &[a.clone(), b.clone()]);
        assert_eq!(preview(&src, 1, 72, 0).dimensions(), (842, 595));
        assert_golden(&src, 1, &a, 72, 0, "yatay");
        assert_eq!(preview(&src, 2, 72, 0).dimensions(), (595, 842));
        assert_golden(&src, 2, &b, 72, 0, "yatay + /Rotate 90");
    }

    /// Taranmış (JPEG), Flate görüntülü ve karışık (metin + görüntü) sayfalar.
    #[test]
    fn scanned_flate_and_mixed_pages_render_their_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let img = half_dark(400, 560);
        for (name, stream) in [
            ("jpeg", jpeg_stream(&img, 90)),
            ("flate", flate_stream(&img)),
            ("png-oncukulu", png_passthrough_stream(&img)),
        ] {
            let src = dir.path().join(format!("{name}.pdf"));
            image_page_pdf(&src, stream, 595., 842.);
            let got = preview(&src, 1, 72, 0);
            let (dark, cx, _) = dark_mass(&got);
            assert!((0.35..0.65).contains(&dark), "{name}: koyu oran {dark:.2}");
            assert!(cx < 0.3, "{name}: koyu kütle solda olmalı (x={cx:.2})");
        }
        // Karışık: görüntünün üstüne metin ve dolgu.
        let src = dir.path().join("karisik.pdf");
        let mut d = Document::load(dir.path().join("jpeg.pdf")).unwrap();
        let page = *d.get_pages().values().next().unwrap();
        let extra = d.add_object(Stream::new(
            dictionary! {},
            b"1 0 0 rg 400 60 150 80 re f".to_vec(),
        ));
        let contents = d
            .get_dictionary(page)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .clone();
        d.get_dictionary_mut(page)
            .unwrap()
            .set("Contents", vec![contents, extra.into()]);
        d.save(&src).unwrap();
        let got = preview(&src, 1, 72, 0);
        let p = got.get_pixel(470, 842 - 100).0;
        assert!(
            p[0] > 200 && p[1] < 60 && p[2] < 60,
            "karışık sayfada kırmızı dolgu yok: {p:?}"
        );
    }

    /// Kullanıcının saha senaryosu: görüntüler → GölgeDosya PDF'i → kapat/aç →
    /// Düzenle önizlemesi. Ürünün KENDİ PDF'ini görüntüleyememesi kabul edilemez.
    #[test]
    fn golgedosya_images_to_pdf_output_previews_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut images = Vec::new();
        for (i, (w, h)) in [(1200u32, 1600u32), (1600, 1200)].into_iter().enumerate() {
            let p = dir.path().join(format!("foto-{i}.png"));
            half_dark(w, h).save(&p).unwrap();
            images.push(p);
        }
        let out = dir.path().join("gorseller.pdf");
        tool(&images, ToolOperation::Images, &out);
        let reopened = Document::load(&out).unwrap();
        assert_eq!(reopened.get_pages().len(), 2);
        for n in 1..=2 {
            let got = preview(&out, n, 72, 0);
            let (dark, cx, _) = dark_mass(&got);
            assert!(
                dark > 0.15,
                "s.{n}: sayfada görüntü yok (koyu oran {dark:.2})"
            );
            assert!(
                cx < 0.5,
                "s.{n}: görüntünün koyu yarısı solda olmalı (x={cx:.2})"
            );
        }
    }

    /// GölgeDosya'nın işaretli ve sıkıştırılmış çıktıları, Ekler çıktısı.
    #[test]
    fn golgedosya_outputs_preview_with_their_content_and_mark() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("tarama.pdf");
        image_page_pdf(&src, jpeg_stream(&half_dark(1190, 1684), 100), 595., 842.);

        let reordered = dir.path().join("isaretli.pdf");
        tool(
            std::slice::from_ref(&src),
            ToolOperation::Reorder { pages: vec![1] },
            &reordered,
        );
        let compressed = dir.path().join("sikistirilmis.pdf");
        let outcome = tool(
            std::slice::from_ref(&src),
            ToolOperation::Compress {
                level: OptimizationLevel::BalancedCompression,
            },
            &compressed,
        );
        assert!(
            matches!(outcome, ToolOutcome::Compressed { .. }),
            "{outcome:?}"
        );

        let mut project = Project::new("Önizleme");
        project.add_source(SourceFile {
            id: "s1".into(),
            path: src.clone(),
            file_name: "tarama.pdf".into(),
            format: SourceFormat::Pdf,
            size_bytes: std::fs::metadata(&src).unwrap().len(),
            sha256_before: calculate_sha256(&src).unwrap(),
            mtime: 1000,
            page_count: 1,
            is_signed: false,
            signature_note: None,
            signed_policy: SignedPolicy::UseOriginalAsIs,
            is_approved_for_conversion: false,
            is_repaired: false,
            repair_note: None,
        });
        project
            .create_exhibit("Tarama")
            .sources
            .push(ExhibitSourceRef {
                source_id: "s1".into(),
                page_range: None,
            });
        let pkg = dir.path().join("paket");
        std::fs::create_dir_all(&pkg).unwrap();
        let result =
            execute_uyap_preparation(&project, &ExecutionContext { output_dir: pkg }).unwrap();
        let ekler = result.package_dir.join(&result.outputs[0].file_name);

        for (name, path) in [
            ("işaretli", &reordered),
            ("sıkıştırılmış", &compressed),
            ("Ekler", &ekler),
        ] {
            let got = preview(path, 1, 72, 0);
            let (dark, cx, _) = dark_mass(&got);
            assert!(
                dark > 0.3,
                "{name}: sayfa içeriği yok (koyu oran {dark:.2})"
            );
            assert!(cx < 0.5, "{name}: içerik yanlış yönde (x={cx:.2})");
            // İşaret payı (sayfanın altı) boş değil: GölgeDosya işareti görünür.
            let band_y = got.height() - 16;
            let marked = (got.width() * 2 / 3..got.width() - 8)
                .filter(|x| got.get_pixel(*x, band_y).0.iter().any(|c| *c < 235))
                .count();
            assert!(
                marked > 0,
                "{name}: işaret payında GölgeDosya işareti görünmüyor"
            );
        }
    }

    /// PDF → görsel: seçilen her sayfa, sırayla, doğru içerikle.
    #[test]
    fn pdf_to_images_writes_every_page_in_order_with_its_own_content() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("uc-sayfa.pdf");
        let pages: Vec<VPage> = [(40., 702.), (455., 40.), (247., 371.)]
            .into_iter()
            .map(|(x, y)| vpage(595., 842., &[fill(x, y, 100., 100., BLACK)]))
            .collect();
        vector_pdf(&src, &pages);
        let before = calculate_sha256(&src).unwrap();
        let out = dir.path().join("cikti");
        std::fs::create_dir_all(&out).unwrap();
        let folder = raster::pdf_to_images(&src, &out, "png", 72, false).unwrap();
        assert_eq!(before, calculate_sha256(&src).unwrap(), "kaynak değişti");
        for (i, p) in pages.iter().enumerate() {
            let file = folder.join(format!("sayfa-{:04}.png", i + 1));
            let got = image::open(&file).unwrap().to_rgb8();
            // Görsel dışa aktarma alta işaret payı ekler; sayfa içeriği üstte.
            assert_eq!(got.width(), 595);
            assert_eq!(got.height(), 842 + 34, "s.{}: pay dahil yükseklik", i + 1);
            let top = image::imageops::crop_imm(&got, 0, 0, 595, 842).to_image();
            let (want, _) = golden(p, 72, 0);
            let err = mae(&want, &top, None);
            assert!(err < GOLDEN_LIMIT, "s.{}: MAE {err:.2}", i + 1);
        }
    }

    /// Bozuk PDF temiz bir hatayla reddedilir; kaynak değişmez, çökme yok.
    #[test]
    fn a_malformed_pdf_fails_cleanly_without_touching_the_source() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in [
            ("cop.pdf", b"%PDF-1.7\nbu bir pdf degil\n%%EOF".to_vec()),
            ("kesik.pdf", {
                let src = dir.path().join("tam.pdf");
                vector_pdf(&src, &[a4_markers()]);
                let b = std::fs::read(&src).unwrap();
                b[..b.len() / 2].to_vec()
            }),
        ] {
            let p = dir.path().join(name);
            std::fs::write(&p, &bytes).unwrap();
            let before = calculate_sha256(&p).unwrap();
            match raster::preview_page(&p, 1, 72, 0) {
                Ok(_) => {} // onarılabilen kesik PDF açılabilir; önemli olan çökmemesi
                Err(e) => assert_user_safe(&e.to_string(), name),
            }
            assert_eq!(
                before,
                calculate_sha256(&p).unwrap(),
                "{name}: kaynak değişti"
            );
        }
    }

    /// Parola isteyen PDF'te mevcut açma politikası korunur: renderer şifreyi
    /// atlatamaz, çünkü renderer'a yalnız `load_pdf_tolerant`'ın açabildiği
    /// belgeden türetilen kopya gider.
    #[test]
    fn a_password_protected_pdf_keeps_its_existing_refusal() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/encrypted.pdf");
        let before = calculate_sha256(&src).unwrap();
        let err = raster::preview_page(&src, 1, 72, 0).expect_err("parolalı PDF önizlenemez");
        assert_user_safe(&err.to_string(), "parolalı PDF");
        assert_eq!(before, calculate_sha256(&src).unwrap());
    }

    /// Bir sayfanın hatası sonraki isteği bozmaz (arayüz her küçük resmi ayrı ister).
    #[test]
    fn a_failing_page_does_not_poison_the_next_request() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("iki.pdf");
        let p = a4_markers();
        vector_pdf(&src, &[p.clone(), p.clone()]);
        assert!(
            raster::preview_page(&src, 99, 72, 0).is_err(),
            "sınır dışı sayfa"
        );
        assert_golden(&src, 2, &p, 72, 0, "hatadan sonra s.2");
        assert_golden(&src, 1, &p, 72, 0, "hatadan sonra s.1");
    }
}
