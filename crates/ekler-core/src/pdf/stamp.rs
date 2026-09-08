use crate::error::{EklerError, Result};
use crate::model::{StampConfig, StampPosition};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document as LopdfDoc, Object, Stream};

pub fn apply_stamp_to_document(
    doc: &mut LopdfDoc,
    exhibit_order: usize,
    start_page_num: usize,
    config: &StampConfig,
) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let pages = doc.get_pages();
    let total_pages = pages.len();

    // Damga için ortak Helvetica-Bold font nesnesi ekle
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
        "Encoding" => "WinAnsiEncoding",
    });

    for i in 1..=(total_pages as u32) {
        if let Some(&page_id) = pages.get(&i) {
            let page_num = start_page_num + (i as usize - 1);
            let stamp_text = format!("Ek-{} / {}. Sayfa", exhibit_order, page_num);
            apply_stamp_to_page(doc, page_id, &stamp_text, font_id, config, None)?;
        }
    }

    Ok(())
}

struct BrandResources {
    form: lopdf::ObjectId,
    streams: std::collections::HashMap<Vec<u8>, lopdf::ObjectId>,
    raster_dpi: Option<u32>,
}
fn add_mark_stream(
    doc: &mut LopdfDoc,
    content: Vec<u8>,
    brand: &mut Option<&mut BrandResources>,
) -> lopdf::ObjectId {
    if let Some(resources) = brand.as_deref_mut() {
        if let Some(id) = resources.streams.get(&content) {
            return *id;
        }
        let id = doc.add_object(Stream::new(Dictionary::new(), content.clone()));
        resources.streams.insert(content, id);
        id
    } else {
        doc.add_object(Stream::new(Dictionary::new(), content))
    }
}

fn apply_stamp_to_page(
    doc: &mut LopdfDoc,
    page_id: lopdf::ObjectId,
    stamp_text: &str,
    font_id: lopdf::ObjectId,
    config: &StampConfig,
    mut brand: Option<&mut BrandResources>,
) -> Result<()> {
    let brand_form = brand.as_ref().map(|b| b.form);
    let resolved = super::resolved_page_dictionary(doc, page_id)?;
    let bounds = resolved
        .get(b"CropBox")
        .or_else(|_| resolved.get(b"MediaBox"))
        .map_err(pdf_error)?;
    let (_, bounds) = doc.dereference(bounds).map_err(pdf_error)?;
    let bounds = bounds.as_array().map_err(pdf_error)?;
    if bounds.len() != 4 {
        return Err(EklerError::InvalidPdf("Geçersiz sayfa kutusu".into()));
    }
    let number = |i: usize| bounds[i].as_float().map_err(pdf_error);
    let (mut x0, mut y0, mut x1, mut y1) = (number(0)?, number(1)?, number(2)?, number(3)?);
    let rotation = resolved
        .get(b"Rotate")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    let original_box = (x0, y0, x1, y1);
    if let Ok(annots) = resolved.get(b"Annots") {
        let (_, annots) = doc.dereference(annots).map_err(pdf_error)?;
        for item in annots.as_array().map_err(pdf_error)? {
            let (_, item) = doc.dereference(item).map_err(pdf_error)?;
            let item = item.as_dict().map_err(pdf_error)?;
            let rect = item
                .get(b"Rect")
                .and_then(|o| o.as_array())
                .map_err(pdf_error)?;
            if rect.len() != 4
                || rect[0].as_float().map_err(pdf_error)? < x0
                || rect[1].as_float().map_err(pdf_error)? < y0
                || rect[2].as_float().map_err(pdf_error)? > x1
                || rect[3].as_float().map_err(pdf_error)? > y1
            {
                return Err(EklerError::InvalidPdf("Görünür alan dışına taşan açıklama/form alanı var; damga payı eklenirken gizli görünümü açmamak için işlem durduruldu".into()));
            }
        }
    }

    let gutter = config.margin_pt * 2.0 + config.font_size + 10.0;
    let top = matches!(
        config.position,
        StampPosition::TopRight | StampPosition::TopLeft
    );
    match (rotation, top) {
        (0, true) | (180, false) => y1 += gutter,
        (0, false) | (180, true) => y0 -= gutter,
        (90, true) | (270, false) => x0 -= gutter,
        (90, false) | (270, true) => x1 += gutter,
        _ => return Err(EklerError::InvalidPdf("Geçersiz sayfa dönüşü".into())),
    }
    let media = resolved.get(b"MediaBox").map_err(pdf_error)?;
    let (_, media) = doc.dereference(media).map_err(pdf_error)?;
    let media = media.as_array().map_err(pdf_error)?;
    if media.len() != 4 {
        return Err(EklerError::InvalidPdf("Geçersiz MediaBox".into()));
    }
    let expanded_media = vec![
        media[0].as_float().map_err(pdf_error)?.min(x0).into(),
        media[1].as_float().map_err(pdf_error)?.min(y0).into(),
        media[2].as_float().map_err(pdf_error)?.max(x1).into(),
        media[3].as_float().map_err(pdf_error)?.max(y1).into(),
    ];
    // The extra strip is outside the old visible page; no source text is covered.
    let page = doc
        .get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(pdf_error)?;
    page.set(
        "CropBox",
        vec![
            Object::Real(x0),
            Object::Real(y0),
            Object::Real(x1),
            Object::Real(y1),
        ],
    );
    page.set("MediaBox", Object::Array(expanded_media));
    let (page_w, page_h) = if rotation == 90 || rotation == 270 {
        (y1 - y0, x1 - x0)
    } else {
        (x1 - x0, y1 - y0)
    };
    let matrix: [f32; 6] = match rotation {
        0 => [1., 0., 0., 1., x0, y0],
        90 => [0., 1., -1., 0., x1, y0],
        180 => [-1., 0., 0., -1., x1, y1],
        270 => [0., -1., 1., 0., x0, y1],
        _ => return Err(EklerError::InvalidPdf("Geçersiz sayfa dönüşü".into())),
    };
    // Copy resource dictionaries per page; never overwrite a source font binding.
    let mut resources = resolved
        .get(b"Resources")
        .ok()
        .map(|o| doc.dereference(o).and_then(|(_, o)| o.as_dict()).cloned())
        .transpose()
        .map_err(pdf_error)?
        .unwrap_or_default();
    let mut fonts = resources
        .get(b"Font")
        .ok()
        .map(|o| doc.dereference(o).and_then(|(_, o)| o.as_dict()).cloned())
        .transpose()
        .map_err(pdf_error)?
        .unwrap_or_default();
    let mut font_name = "DuzenEkStamp".to_string();
    while fonts.has(font_name.as_bytes()) {
        font_name.push('_');
    }
    if brand_form.is_none() {
        fonts.set(font_name.clone(), font_id);
        resources.set("Font", fonts);
    }
    doc.get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(pdf_error)?
        .set("Resources", resources);

    let badge_w = if brand_form.is_some() {
        56.0
    } else {
        (stamp_text.len() as f32 * config.font_size * 0.65 + 16.0).max(40.0)
    };
    let badge_h = config.font_size + 10.0;
    if page_w < badge_w + config.margin_pt * 2.0 || page_h < badge_h + config.margin_pt * 2.0 {
        return Err(EklerError::InvalidPdf(
            "Damga bu sayfaya sığmıyor; boyut/kenar boşluğunu azaltın".into(),
        ));
    }
    let margin = config.margin_pt;

    let (badge_x, badge_y, text_x, text_y) = match config.position {
        StampPosition::TopRight => {
            let x = page_w - margin - badge_w;
            let y = page_h - margin - badge_h;
            (x, y, x + 8.0, y + 6.0)
        }
        StampPosition::TopLeft => {
            let x = margin;
            let y = page_h - margin - badge_h;
            (x, y, x + 8.0, y + 6.0)
        }
        StampPosition::BottomRight => {
            let x = page_w - margin - badge_w;
            let y = margin;
            (x, y, x + 8.0, y + 6.0)
        }
        StampPosition::BottomLeft => {
            let x = margin;
            let y = margin;
            (x, y, x + 8.0, y + 6.0)
        }
    };

    let mut ops = vec![
        Operation::new("q", vec![]),
        Operation::new("cm", matrix.into_iter().map(Object::Real).collect()),
    ];

    if config.show_badge {
        ops.extend(vec![
            // Rozet arka planı (açık gri, okunabilirlik için)
            Operation::new("rg", vec![0.94.into(), 0.94.into(), 0.96.into()]),
            Operation::new(
                "re",
                vec![
                    badge_x.into(),
                    badge_y.into(),
                    badge_w.into(),
                    badge_h.into(),
                ],
            ),
            Operation::new("f", vec![]),
            // İnce kenarlık
            Operation::new("RG", vec![0.4.into(), 0.4.into(), 0.4.into()]),
            Operation::new("w", vec![0.6.into()]),
            Operation::new(
                "re",
                vec![
                    badge_x.into(),
                    badge_y.into(),
                    badge_w.into(),
                    badge_h.into(),
                ],
            ),
            Operation::new("S", vec![]),
        ]);
    }

    ops.extend(vec![
        // Metin
        Operation::new("BT", vec![]),
        Operation::new("rg", vec![0.1.into(), 0.1.into(), 0.1.into()]),
        Operation::new(
            "Tf",
            vec![
                Object::Name(font_name.into_bytes()),
                config.font_size.into(),
            ],
        ),
        Operation::new("Td", vec![text_x.into(), text_y.into()]),
        Operation::new("Tj", vec![Object::string_literal(stamp_text)]),
        Operation::new("ET", vec![]),
        Operation::new("Q", vec![]),
    ]);

    if let Some(form_id) = brand_form {
        let mark_width = if let Some(dpi) = brand.as_ref().and_then(|b| b.raster_dpi) {
            (page_w * dpi as f32 / 72. * 0.094).clamp(32., 240.) * 72. / dpi as f32
        } else {
            56.
        };
        let mark_width = mark_width
            .min((page_w - margin * 2.).max(1.))
            .min((gutter - margin * 2.).max(1.) * 290. / 72.);
        let page = doc.get_dictionary_mut(page_id).map_err(pdf_error)?;
        let resources = page
            .get_mut(b"Resources")
            .and_then(|o| o.as_dict_mut())
            .map_err(pdf_error)?;
        let mut objects = Dictionary::new();
        // Existing XObjects may be indirect. Add a unique top-level binding by resolving below.
        if let Ok(Object::Dictionary(existing)) = resources.get(b"XObject") {
            objects = existing.clone();
        }
        // The resolved resource graph was copied above; preserve indirect dictionaries too.
        let existing = resources.get(b"XObject").ok().cloned();
        if let Some(Object::Reference(id)) = existing {
            objects = doc.get_dictionary(id).map_err(pdf_error)?.clone();
        }
        let mut name = "DuzenEkBrand".to_string();
        while objects.has(name.as_bytes()) {
            name.push('_');
        }
        objects.set(name.clone(), form_id);
        doc.get_dictionary_mut(page_id)
            .map_err(pdf_error)?
            .get_mut(b"Resources")
            .and_then(|o| o.as_dict_mut())
            .map_err(pdf_error)?
            .set("XObject", objects);
        ops = vec![
            Operation::new("q", vec![]),
            Operation::new("cm", matrix.into_iter().map(Object::Real).collect()),
            Operation::new(
                "cm",
                vec![
                    (mark_width / 290.0).into(),
                    0.into(),
                    0.into(),
                    (mark_width / 290.0).into(),
                    (page_w - margin - mark_width).into(),
                    margin.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(name.into_bytes())]),
            Operation::new("Q", vec![]),
        ];
    }
    let stamp_content = Content { operations: ops };
    let stamp_stream_id = add_mark_stream(
        doc,
        stamp_content
            .encode()
            .map_err(|e| EklerError::InvalidPdf(e.to_string()))?,
        &mut brand,
    );
    // Reuse identical wrappers as well as the vector asset; source streams stay intact.
    let (ox0, oy0, ox1, oy1) = original_box;
    let prefix = add_mark_stream(
        doc,
        format!("q\n{ox0} {oy0} {} {} re W n\n", ox1 - ox0, oy1 - oy0).into_bytes(),
        &mut brand,
    );
    let suffix = add_mark_stream(doc, b"\nQ\n".to_vec(), &mut brand);
    let old = resolved.get(b"Contents").ok().cloned();
    let mut contents = vec![Object::Reference(prefix)];
    if let Some(old) = old {
        match old {
            Object::Array(items) => contents.extend(items),
            other => contents.push(other),
        }
    }
    contents.extend([
        Object::Reference(suffix),
        Object::Reference(stamp_stream_id),
    ]);
    doc.get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(pdf_error)?
        .set("Contents", contents);
    Ok(())
}
fn pdf_error(e: lopdf::Error) -> EklerError {
    EklerError::InvalidPdf(e.to_string())
}

pub fn apply_text_marks(doc: &mut LopdfDoc, text: Option<&str>, start: usize) -> Result<()> {
    let font=doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica-Bold","Encoding"=>"WinAnsiEncoding"});
    let config = StampConfig {
        enabled: true,
        position: StampPosition::BottomRight,
        font_size: 10.,
        margin_pt: 20.,
        show_badge: false,
    };
    for (i, id) in doc.get_pages().into_values().enumerate() {
        let mark = text
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}", start + i));
        apply_stamp_to_page(doc, id, &mark, font, &config, None)?;
    }
    Ok(())
}

/// Required mark for derived documents only. Call after transforms and before size validation.
pub fn apply_branding(doc: &mut LopdfDoc) -> Result<()> {
    apply_branding_for_output(doc, None)
}
/// Same shared vector/alpha as PDF output, with deterministic pixel bounds for raster export.
pub fn apply_raster_branding(doc: &mut LopdfDoc, dpi: u32) -> Result<()> {
    apply_branding_for_output(doc, Some(dpi))
}
fn apply_branding_for_output(doc: &mut LopdfDoc, raster_dpi: Option<u32>) -> Result<()> {
    let config = StampConfig {
        enabled: true,
        position: StampPosition::BottomRight,
        font_size: 8.0,
        margin_pt: 8.0,
        show_badge: false,
    };
    // One shared, translucent vector form; no raster or unused font per page.
    // Isolate the vector as a transparency group: opacity applies once to the
    // complete mark, not repeatedly where the logo's paths overlap.
    let mut vector = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),290.into(),72.into()],
        "Group"=>dictionary! {"S"=>"Transparency","I"=>true,"CS"=>"DeviceRGB"}, "Resources"=>Dictionary::new()},
        include_bytes!("../../assets/brand-logo.ops").to_vec(),
    );
    vector.compress().map_err(pdf_error)?;
    let vector_id = doc.add_object(vector);
    let mut form = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),290.into(),72.into()],
        "Resources"=>dictionary! {"XObject"=>dictionary! {"Mark"=>vector_id},"ExtGState"=>dictionary! {"BrandAlpha"=>dictionary! {"Type"=>"ExtGState","ca"=>0.24f32,"CA"=>0.24f32}}}},
        b"q /BrandAlpha gs /Mark Do Q".to_vec(),
    );
    form.compress().map_err(pdf_error)?;
    let form_id = doc.add_object(form);
    let mut resources = BrandResources {
        form: form_id,
        streams: Default::default(),
        raster_dpi,
    };
    for id in doc.get_pages().into_values() {
        apply_stamp_to_page(doc, id, "DuzenEk", (0, 0), &config, Some(&mut resources))?;
    }
    Ok(())
}
