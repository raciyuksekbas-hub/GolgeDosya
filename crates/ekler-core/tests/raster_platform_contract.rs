//! PDF önizleme/görselleştirmenin PLATFORM sözleşmesi.
//!
//! `raster` yerel bir işletim sistemi renderer'ına dayanır ve bugün yalnız
//! macOS'ta uygulanmıştır. Diğer platformlarda doğru davranış "çökmek" ya da
//! "sessizce boş dönmek" değil, kullanıcıya gösterilebilir bir redde
//! bulunmaktır. Bu dosya o sözleşmeyi HER platformda koşarak sabitler:
//!
//!   * macOS'ta iki giriş noktası da gerçekten çalışır,
//!   * macOS dışında ikisi de `UnsupportedFormat` ile, boş olmayan ve
//!     kullanıcıya gösterilebilir bir Türkçe cümleyle reddeder,
//!   * her iki durumda da KAYNAK DOSYA bayt bayt aynı kalır,
//!   * giriş doğrulaması platformdan ÖNCE gelir: geçersiz sayfa/DPI her
//!     platformda aynı doğrulama hatasını verir.
//!
//! `workspace_raster_tests.rs` görüntünün doğruluğunu ölçer ve bu yüzden
//! macOS'a kilitlidir; burada ölçülen, Windows'ta paketlenen ürünün bu
//! yüzeye dokununca ne yaptığıdır.
use ekler_core::{calculate_sha256, raster, EklerError};
use lopdf::{dictionary, Document, Stream};
use std::path::{Path, PathBuf};

fn one_page_pdf(dir: &Path) -> PathBuf {
    let (w, h) = (400, 600);
    let mut d = Document::with_version("1.5");
    let pages = d.new_object_id();
    let content = d.add_object(Stream::new(
        dictionary! {},
        format!("0.2 0.5 0.7 rg 0 0 {w} {h} re f").into_bytes(),
    ));
    let page = d.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
        "Resources" => dictionary! {}, "Contents" => content,
    });
    d.objects.insert(
        pages,
        dictionary! {"Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1}.into(),
    );
    let root = d.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
    d.trailer.set("Root", root);
    let path = dir.join("sozlesme.pdf");
    d.save(&path).unwrap();
    path
}

/// Reddetme kullanıcıya gösterilebilir olmalı: doğru varyant, boş olmayan
/// cümle, ve geliştirici jargonu değil. Yalnız macOS dışında çağrılır; macOS
/// derlemesinde `-D warnings` altında ölü kod sayılmaması için kapatılır.
#[cfg(not(target_os = "macos"))]
fn assert_user_facing_refusal(err: EklerError, what: &str) {
    match err {
        EklerError::UnsupportedFormat(message) => {
            assert!(
                !message.trim().is_empty(),
                "{what}: reddetme cümlesi boş; kullanıcı neden olmadığını göremez"
            );
            for jargon in ["panic", "unwrap", "None", "Err(", "cfg(", "target_os"] {
                assert!(
                    !message.contains(jargon),
                    "{what}: kullanıcı cümlesinde geliştirici jargonu var: {message}"
                );
            }
        }
        other => panic!("{what}: UnsupportedFormat bekleniyordu, {other:?} geldi"),
    }
}

#[test]
fn preview_and_export_honour_the_platform_contract() {
    let dir = tempfile::tempdir().unwrap();
    let source = one_page_pdf(dir.path());
    let before = calculate_sha256(&source).unwrap();

    let preview = raster::preview_page(&source, 1, 150, 0);
    let export = raster::pdf_to_images(&source, dir.path(), "png", 150, false);

    #[cfg(target_os = "macos")]
    {
        let bytes = preview.expect("macOS'ta önizleme çalışmalı");
        assert!(!bytes.is_empty(), "önizleme boş PNG döndürdü");
        image::load_from_memory(&bytes).expect("önizleme geçerli bir görsel olmalı");
        let folder = export.expect("macOS'ta PDF → görsel çalışmalı");
        assert!(folder.join("sayfa-0001.png").is_file(), "sayfa yazılmadı");
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert_user_facing_refusal(preview.unwrap_err(), "önizleme");
        assert_user_facing_refusal(export.unwrap_err(), "PDF → görsel");
    }

    assert_eq!(
        before,
        calculate_sha256(&source).unwrap(),
        "kaynak dosya değişti — hangi platformda olursa olsun dokunulmamalı"
    );
}

/// Giriş doğrulaması platform kapısından ÖNCE gelir. Aksi hâlde Windows'ta
/// kullanıcı geçersiz bir DPI için "bu platformda yok" cümlesini görürdü;
/// bu, hatayı yanlış yere yazmak olurdu.
#[test]
fn invalid_input_is_rejected_the_same_way_on_every_platform() {
    let dir = tempfile::tempdir().unwrap();
    let source = one_page_pdf(dir.path());

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
