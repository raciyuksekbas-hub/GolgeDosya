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
