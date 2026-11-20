use std::io::Cursor;
fn decoded_pixels(bytes: &[u8]) -> Vec<::image::RgbImage> {
    let doc = ekler_core::image::convert_tiff_bytes_to_pdf(bytes).unwrap();
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| s.dict.get(b"Subtype").and_then(|v| v.as_name()).ok() == Some(b"Image"))
        .map(|s| ::image::load_from_memory(&s.content).unwrap().to_rgb8())
        .collect()
}
#[test]
fn gray8_gray16_rgb_rgba_and_multipage_preserve_pixels() {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut enc = tiff::encoder::TiffEncoder::new(&mut bytes).unwrap();
        enc.new_image::<tiff::encoder::colortype::Gray8>(8, 8)
            .unwrap()
            .write_data(&[255; 64])
            .unwrap();
        enc.new_image::<tiff::encoder::colortype::Gray16>(8, 8)
            .unwrap()
            .write_data(&[32768; 64])
            .unwrap();
        enc.new_image::<tiff::encoder::colortype::RGB8>(8, 8)
            .unwrap()
            .write_data(&[255, 0, 0].repeat(64))
            .unwrap();
        enc.new_image::<tiff::encoder::colortype::RGBA8>(8, 8)
            .unwrap()
            .write_data(&[0, 0, 0, 0].repeat(64))
            .unwrap();
    }
    let images = decoded_pixels(bytes.get_ref());
    assert_eq!(images.len(), 4);
    for (i, expected) in [
        [255, 255, 255],
        [128, 128, 128],
        [255, 0, 0],
        [255, 255, 255],
    ]
    .iter()
    .enumerate()
    {
        for pixel in images[i].pixels() {
            for channel in 0..3 {
                assert!(
                    pixel[channel].abs_diff(expected[channel]) <= 3,
                    "frame {i}: {pixel:?}"
                );
            }
        }
    }
}
#[test]
fn invalid_tiff_is_error_not_black_fallback() {
    assert!(ekler_core::image::convert_tiff_bytes_to_pdf(b"not a tiff").is_err());
}

/// Tek IFD'li, sıkıştırılmamış 8 bit gri TIFF; bildirilen boyut gerçek veriden
/// bağımsız (şerit yalnız 16 bayt).
fn tiff_declaring(width: u32, height: u32) -> Vec<u8> {
    let entries: [(u16, u16, u32); 8] = [
        (256, 4, width),
        (257, 4, height),
        (258, 3, 8),
        (259, 3, 1),
        (262, 3, 1),
        (273, 4, 8 + 2 + 8 * 12 + 4),
        (278, 4, height),
        (279, 4, 16),
    ];
    let mut t = b"II*\0".to_vec();
    t.extend_from_slice(&8u32.to_le_bytes());
    t.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (tag, kind, value) in entries {
        t.extend_from_slice(&tag.to_le_bytes());
        t.extend_from_slice(&kind.to_le_bytes());
        t.extend_from_slice(&1u32.to_le_bytes());
        if kind == 3 {
            t.extend_from_slice(&(value as u16).to_le_bytes());
            t.extend_from_slice(&[0, 0]);
        } else {
            t.extend_from_slice(&value.to_le_bytes());
        }
    }
    t.extend_from_slice(&0u32.to_le_bytes());
    t.extend_from_slice(&[0u8; 16]);
    t
}

#[test]
fn frame_declaring_an_oversized_canvas_is_refused_before_decoding() {
    let started = std::time::Instant::now();
    let e = ekler_core::image::convert_tiff_bytes_to_pdf(&tiff_declaring(20_000, 20_000))
        .unwrap_err()
        .to_string();
    assert!(e.contains("çok büyük"), "{e}");
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    // Sınırın altındaki bildirim kare okumasına geçer (veri eksik olduğu için
    // orada düşer); yani sınır gerçek taramaları etkilemez.
    let e = ekler_core::image::convert_tiff_bytes_to_pdf(&tiff_declaring(4_960, 7_016))
        .unwrap_err()
        .to_string();
    assert!(!e.contains("çok büyük"), "{e}");
}

#[test]
fn tiff_with_too_many_frames_is_refused() {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut enc = tiff::encoder::TiffEncoder::new(&mut bytes).unwrap();
        for _ in 0..2_001 {
            enc.write_image::<tiff::encoder::colortype::Gray8>(1, 1, &[255])
                .unwrap();
        }
    }
    let e = ekler_core::image::convert_tiff_bytes_to_pdf(bytes.get_ref())
        .unwrap_err()
        .to_string();
    assert!(e.contains("2000 sayfadan fazla"), "{e}");
}
