//! Ürün kuralı: GölgeDosya'nın ÜRETTİĞİ ya da DEĞİŞTİRDİĞİ her PDF çıktısı,
//! her sayfada TAM BİR canonical GölgeDosya işareti taşır — görünür alanda ve
//! okunur boyutta. Kaynak hiçbir işlemde değişmez.
//!
//! Saha bulgusu: Sıkıştır çıktısında "işaret yok" bildirildi. Sahadaki üç çıktı
//! ölçüldü: işaret NESNE olarak vardı, her sayfada birer tane. Ama sayfalar
//! telefonla çekilmiş belgelerdi (1414–1851 pt genişlik, A4'ün 2,4–3,1 katı) ve
//! işaret sayfa boyutundan bağımsız sabit 56 pt basılıyordu: sayfanın %3'ü,
//! sayfaya sığdır görünümünde 5–7 px yüksekliğinde bir leke. Varlığa bakan bir
//! test bunu GEÇİRİRDİ. Bu yüzden buradaki sayım boyutu da ölçer.
//!
//! Sayım üretim kodundan BAĞIMSIZDIR. İşaret "BrandAlpha dizesi var mı" diye
//! değil, çizim sözleşmesiyle tanınır: form içeriği, saydamlık grafik durumu,
//! logo baytları, kutu. Çizim, sayfanın içerik akışları PDF anlamında TEK akış
//! olarak (akışlar arası `q`/`Q` ile) yürünerek bulunur.
use ekler_core::{
    calculate_sha256, execute_uyap_preparation, safe_io,
    toolbox::{run_tool_with_outcome, PageRotation, ToolOperation, ToolOutcome},
    ExecutionContext, ExhibitSourceRef, OptimizationLevel as Level, Project, SignedPolicy,
    SourceFile, SourceFormat,
};
use lopdf::{content::Content, dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const CANONICAL_LOGO: &[u8] = include_bytes!("../../pdf-core/assets/brand-logo.ops");
const LEGACY_LOGO: &[u8] = include_bytes!("../../pdf-core/assets/brand-logo-legacy.ops");
const SUPERSEDED_LOGO: &[u8] =
    include_bytes!("../../pdf-core/assets/brand-logo-superseded-2026-09-16.ops");
const BRAND_FORM: &[u8] = b"q /BrandAlpha gs /Mark Do Q";
const BRAND_BBOX_W: f32 = 290.;

/// Kabul edilmiş işaretin sayfaya oranı A4'te 56/595 = %9,4, yatay Letter'da
/// 56/646 = %8,7'dir. %8'in altı, sahadaki okunmaz işaretin sınıfıdır
/// (%3–4). Eşik, kabul edilmiş standart sayfaların HİÇBİRİNİ düşürmez.
const MIN_MARK_RATIO: f32 = 0.08;

// ------------------------------------------------------------------ sayım

/// Form ve içerik akışlarının çözülmüş baytları. Görsel akışları (DCT, JPX)
/// burada ÇÖZÜLMEZ: işaret bir Form XObject'tir, görsel değil.
fn data(s: &Stream) -> Vec<u8> {
    if s.dict.has(b"Filter") {
        s.decompressed_content().expect("akış çözülemedi")
    } else {
        s.content.clone()
    }
}

fn is_form(s: &Stream) -> bool {
    s.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() == Some(b"Form")
}

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).expect("kırık başvuru"),
        other => other,
    }
}

fn number(o: &Object) -> f32 {
    o.as_float().expect("sayı bekleniyordu")
}

/// Kalıtımla (Parent zinciri) sayfa anahtarı.
fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut dict = doc.get_dictionary(page).ok()?;
    loop {
        if let Ok(v) = dict.get(key) {
            return Some(resolve(doc, v));
        }
        let parent = dict.get(b"Parent").ok()?.as_reference().ok()?;
        dict = doc.get_dictionary(parent).ok()?;
    }
}

fn boxed(doc: &Document, page: ObjectId, key: &[u8]) -> Option<[f32; 4]> {
    let a = inherited(doc, page, key)?.as_array().ok()?;
    let v = [0, 1, 2, 3].map(|i| number(resolve(doc, &a[i])));
    Some([
        v[0].min(v[2]),
        v[1].min(v[3]),
        v[0].max(v[2]),
        v[1].max(v[3]),
    ])
}

/// Çizim sözleşmesine uyan marka formları.
fn canonical_forms(doc: &Document) -> HashSet<ObjectId> {
    doc.objects
        .iter()
        .filter_map(|(id, obj)| {
            let form = obj.as_stream().ok()?;
            if !is_form(form) || data(form) != BRAND_FORM {
                return None;
            }
            let bbox = form.dict.get(b"BBox").ok()?.as_array().ok()?;
            let bbox: Vec<f32> = bbox.iter().map(number).collect();
            assert_eq!(bbox, [0., 0., 290., 72.], "marka formunun kutusu değişmiş");
            let res = resolve(doc, form.dict.get(b"Resources").ok()?)
                .as_dict()
                .ok()?;
            let gs = resolve(doc, res.get(b"ExtGState").ok()?).as_dict().ok()?;
            let alpha = resolve(doc, gs.get(b"BrandAlpha").ok()?).as_dict().ok()?;
            for k in [b"ca".as_slice(), b"CA"] {
                let v = number(alpha.get(k).expect("saydamlık yok"));
                assert!((v - 0.24).abs() < 1e-4, "marka saydamlığı değişmiş: {v}");
            }
            let xo = resolve(doc, res.get(b"XObject").ok()?).as_dict().ok()?;
            let mark = resolve(doc, xo.get(b"Mark").ok()?).as_stream().ok()?;
            let group = resolve(doc, mark.dict.get(b"Group").ok()?).as_dict().ok()?;
            assert_eq!(
                group.get(b"S").and_then(|s| s.as_name()).ok(),
                Some(b"Transparency".as_slice())
            );
            (is_form(mark) && data(mark) == CANONICAL_LOGO).then_some(*id)
        })
        .collect()
}

/// Belgede kalmış eski çizim sayısı (DüzenEk / petrol yeşili dönem).
fn outdated_drawings(doc: &Document) -> usize {
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| is_form(s))
        .filter(|s| {
            let d = data(s);
            d == LEGACY_LOGO || d == SUPERSEDED_LOGO
        })
        .count()
}

#[derive(Debug)]
struct Draw {
    inside: bool,
    width: f32,
}

#[derive(Debug)]
struct PageCensus {
    draws: Vec<Draw>,
    short_side: f32,
}

fn census(doc: &Document) -> Vec<PageCensus> {
    let forms = canonical_forms(doc);
    doc.get_pages()
        .into_values()
        .map(|page| {
            let visible = boxed(doc, page, b"CropBox")
                .or_else(|| boxed(doc, page, b"MediaBox"))
                .expect("sayfa kutusu yok");
            let short_side = (visible[2] - visible[0]).min(visible[3] - visible[1]);
            let names: HashSet<Vec<u8>> = inherited(doc, page, b"Resources")
                .and_then(|r| r.as_dict().ok())
                .and_then(|r| r.get(b"XObject").ok())
                .map(|x| resolve(doc, x))
                .and_then(|x| x.as_dict().ok())
                .map(|x: &Dictionary| {
                    x.iter()
                        .filter(|(_, v)| v.as_reference().is_ok_and(|id| forms.contains(&id)))
                        .map(|(k, _)| k.clone())
                        .collect()
                })
                .unwrap_or_default();
            // İçerik akışları PDF'te TEK akıştır: birinin açtığı `q` ötekinde kapanır.
            let mut joined = Vec::new();
            let contents = doc.get_dictionary(page).unwrap().get(b"Contents").unwrap();
            let items = match resolve(doc, contents) {
                Object::Array(a) => a.clone(),
                _ => vec![contents.clone()],
            };
            for item in &items {
                joined.extend(data(resolve(doc, item).as_stream().unwrap()));
                joined.push(b'\n');
            }
            let mut ctm = [1f32, 0., 0., 1., 0., 0.];
            let mut stack = Vec::new();
            let mut draws = Vec::new();
            for op in Content::decode(&joined)
                .expect("içerik çözülemedi")
                .operations
            {
                match op.operator.as_str() {
                    "q" => stack.push(ctm),
                    "Q" => ctm = stack.pop().unwrap_or([1., 0., 0., 1., 0., 0.]),
                    "cm" => {
                        let m: Vec<f32> = op.operands.iter().map(number).collect();
                        ctm = [
                            m[0] * ctm[0] + m[1] * ctm[2],
                            m[0] * ctm[1] + m[1] * ctm[3],
                            m[2] * ctm[0] + m[3] * ctm[2],
                            m[2] * ctm[1] + m[3] * ctm[3],
                            m[4] * ctm[0] + m[5] * ctm[2] + ctm[4],
                            m[4] * ctm[1] + m[5] * ctm[3] + ctm[5],
                        ];
                    }
                    "Do" => {
                        let name = op.operands[0].as_name().unwrap();
                        if !names.contains(name) {
                            continue;
                        }
                        let inside = [(0., 0.), (290., 0.), (0., 72.), (290., 72.)].iter().all(
                            |&(x, y): &(f32, f32)| {
                                let (u, v) = (
                                    ctm[0] * x + ctm[2] * y + ctm[4],
                                    ctm[1] * x + ctm[3] * y + ctm[5],
                                );
                                u >= visible[0] - 0.5
                                    && u <= visible[2] + 0.5
                                    && v >= visible[1] - 0.5
                                    && v <= visible[3] + 0.5
                            },
                        );
                        let width = (ctm[0] * ctm[0] + ctm[1] * ctm[1]).sqrt() * BRAND_BBOX_W;
                        draws.push(Draw { inside, width });
                    }
                    _ => {}
                }
            }
            PageCensus { draws, short_side }
        })
        .collect()
}

/// Kuralın kendisi: her sayfada tam bir, görünür, okunur canonical işaret.
fn assert_exactly_one_legible_mark(path: &Path) {
    let doc = Document::load(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        outdated_drawings(&doc),
        0,
        "{}: eski marka çizimi kalmış",
        path.display()
    );
    let pages = census(&doc);
    assert!(!pages.is_empty());
    for (n, p) in pages.iter().enumerate() {
        let n = n + 1;
        assert_eq!(
            p.draws.len(),
            1,
            "{}: sayfa {n}'de {} işaret çizimi",
            path.display(),
            p.draws.len()
        );
        let d = &p.draws[0];
        assert!(
            d.inside,
            "{}: sayfa {n}'de işaret görünür alanın dışında",
            path.display()
        );
        assert!(
            d.width >= MIN_MARK_RATIO * p.short_side,
            "{}: sayfa {n}'de işaret {:.1} pt, sayfanın kısa kenarı {:.0} pt (%{:.1}) — okunmaz",
            path.display(),
            d.width,
            p.short_side,
            d.width / p.short_side * 100.
        );
    }
}

// ------------------------------------------------------------------ girdiler

#[derive(Clone, Copy)]
struct PageSpec {
    w: f32,
    h: f32,
    rotate: i64,
    crop: Option<[f32; 4]>,
    scan: bool,
}

const fn page(w: f32, h: f32) -> PageSpec {
    PageSpec {
        w,
        h,
        rotate: 0,
        crop: None,
        scan: false,
    }
}
const fn scan(w: f32, h: f32) -> PageSpec {
    PageSpec {
        w,
        h,
        rotate: 0,
        crop: None,
        scan: true,
    }
}

/// Taranmış belge görüntüsü: kâğıt dokusu + satırlar, q100. Yeniden
/// kodlandığında gerçekten küçülür; "kazanç yok" yoluna düşmez.
fn scan_jpeg(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let mut state = seed | 1;
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let noise = (state % 60) as u8;
        let v = if y % 38 < 6 && x > w / 12 && x < w * 3 / 4 {
            30 + noise / 6
        } else {
            190 + noise / 2
        };
        image::Rgb([v, v.saturating_sub(3), v.saturating_sub(9)])
    });
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
        .encode_image(&img)
        .unwrap();
    jpeg
}

fn build_pdf(path: &Path, pages: &[PageSpec]) {
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let mut kids = Vec::new();
    for (i, p) in pages.iter().enumerate() {
        let (resources, content) = if p.scan {
            let (iw, ih) = ((p.w / 2.) as u32, (p.h / 2.) as u32);
            let img = d.add_object(Stream::new(
                dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>iw as i64,"Height"=>ih as i64,
                "ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,"Filter"=>"DCTDecode"},
                scan_jpeg(iw, ih, 0x9E37 + i as u32),
            ));
            (
                dictionary! {"XObject"=>dictionary!{"Scan"=>img}},
                format!("q {} 0 0 {} 0 0 cm /Scan Do Q", p.w, p.h),
            )
        } else {
            (
                dictionary! {},
                format!(
                    "0.15 g 40 {} {} 14 re f 40 {} {} 14 re f",
                    p.h - 60.,
                    p.w * 0.6,
                    p.h - 90.,
                    p.w * 0.4
                ),
            )
        };
        let content = d.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let mut page = dictionary! {"Type"=>"Page","Parent"=>pages_id,
        "MediaBox"=>vec![0.into(),0.into(),p.w.into(),p.h.into()],
        "Resources"=>resources,"Contents"=>content};
        if p.rotate != 0 {
            page.set("Rotate", p.rotate);
        }
        if let Some(c) = p.crop {
            page.set(
                "CropBox",
                c.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
            );
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

/// v0.3.0'ın (bu düzeltmeden önce) büyük bir sayfaya bastığı işaretin BİREBİR
/// yapısı. Sahadaki üç Sıkıştır çıktısında ölçüldü: içerik orijinal kutuya
/// kırpılır (`q x y w h re W n` … `Q`), kutu alttan 34 pt genişler, işaret
/// 56 pt (ölçek 56/290) ayrı bir `q cm cm Do Q` akışıyla çizilir. `logo`
/// eski çizimlerden biri de olabilir: ürün her dönemin işaretini tanımalı.
fn old_style_branded(path: &Path, w: f32, h: f32, logo: &[u8]) {
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let body = d.add_object(Stream::new(
        dictionary! {},
        format!("0.15 g 40 {} {} 14 re f", h - 60., w * 0.6).into_bytes(),
    ));
    let prefix = d.add_object(Stream::new(
        dictionary! {},
        format!("q\n0 0 {w} {h} re W n\n").into_bytes(),
    ));
    let suffix = d.add_object(Stream::new(dictionary! {}, b"\nQ\n".to_vec()));
    let font = d.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1",
    "BaseFont"=>"Helvetica-Bold","Encoding"=>"WinAnsiEncoding"});
    let bbox = || vec![0.into(), 0.into(), 290.into(), 72.into()];
    let vector = d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>bbox(),
        "Group"=>dictionary!{"S"=>"Transparency","I"=>true,"CS"=>"DeviceRGB"},
        "Resources"=>dictionary!{"Font"=>dictionary!{"BrandFont"=>font}}},
        logo.to_vec(),
    ));
    let form = d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>bbox(),
        "Resources"=>dictionary!{"XObject"=>dictionary!{"Mark"=>vector},
        "ExtGState"=>dictionary!{"BrandAlpha"=>dictionary!{"Type"=>"ExtGState","ca"=>0.24f32,"CA"=>0.24f32}}}},
        BRAND_FORM.to_vec(),
    ));
    let s = 56. / 290.;
    let stamp = d.add_object(Stream::new(
        dictionary! {},
        format!(
            "q 1 0 0 1 0 -34 cm {s} 0 0 {s} {} 8 cm /GolgeDosyaBrand Do Q",
            w - 8. - 56.
        )
        .into_bytes(),
    ));
    let page = d.add_object(dictionary! {"Type"=>"Page","Parent"=>pages_id,
    "MediaBox"=>vec![0.into(),(-34).into(),w.into(),h.into()],
    "CropBox"=>vec![0.into(),(-34).into(),w.into(),h.into()],
    "Resources"=>dictionary!{"XObject"=>dictionary!{"GolgeDosyaBrand"=>form}},
    "Contents"=>vec![prefix.into(), body.into(), suffix.into(), stamp.into()]});
    d.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    d.trailer.set("Root", root);
    d.save(path).unwrap();
}

// ------------------------------------------------------------------ yürütme

/// Arayüzün kullandığı YOL: `run_tool_with_outcome`. Kaynak baytları her
/// koşuda karşılaştırılır.
fn tool(sources: &[PathBuf], op: ToolOperation, out: &Path) -> ToolOutcome {
    let before: Vec<_> = sources
        .iter()
        .map(|s| calculate_sha256(s).unwrap())
        .collect();
    let outcome =
        run_tool_with_outcome(sources, &op, out, true).unwrap_or_else(|e| panic!("{op:?}: {e}"));
    let after: Vec<_> = sources
        .iter()
        .map(|s| calculate_sha256(s).unwrap())
        .collect();
    assert_eq!(before, after, "{op:?}: kaynak değişti");
    outcome
}

fn compress(level: Level) -> ToolOperation {
    ToolOperation::Compress { level }
}

fn identity(pages: usize) -> ToolOperation {
    ToolOperation::Reorder {
        pages: (1..=pages).collect(),
    }
}

// ------------------------------------------------------------------ testler

/// SAHA REGRESYONU. Telefonla çekilmiş bir belgenin boyutları (1 px = 1 pt).
#[test]
fn a_phone_scan_compresses_with_one_legible_mark() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("telefon.pdf");
    build_pdf(
        &src,
        &[scan(1414., 2068.), scan(1626., 2367.), scan(1851., 2783.)],
    );
    let out = dir.path().join("sikistirilmis.pdf");
    let outcome = tool(&[src], compress(Level::BalancedCompression), &out);
    assert!(
        matches!(outcome, ToolOutcome::Compressed { .. }),
        "{outcome:?}"
    );
    assert_exactly_one_legible_mark(&out);
}

#[test]
fn every_compression_level_brands_a4_landscape_and_rotated_scans() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("tarama.pdf");
    let rotated = PageSpec {
        rotate: 90,
        ..scan(595., 842.)
    };
    let upside = PageSpec {
        rotate: 270,
        ..scan(595., 842.)
    };
    build_pdf(&src, &[scan(595., 842.), scan(842., 595.), rotated, upside]);
    for level in [
        Level::GentleCompression,
        Level::BalancedCompression,
        Level::AggressiveCompression,
    ] {
        let out = dir.path().join(format!("{level:?}.pdf"));
        let outcome = tool(std::slice::from_ref(&src), compress(level), &out);
        assert!(
            matches!(outcome, ToolOutcome::Compressed { .. }),
            "{level:?}: {outcome:?}"
        );
        assert_exactly_one_legible_mark(&out);
    }
}

#[test]
fn an_already_branded_pdf_compresses_without_a_second_mark() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("kaynak.pdf");
    build_pdf(&src, &[scan(595., 842.), scan(1414., 2068.)]);
    let branded = dir.path().join("markali.pdf");
    tool(&[src], identity(2), &branded);
    assert_exactly_one_legible_mark(&branded);
    let out = dir.path().join("markali-sikistirilmis.pdf");
    let outcome = tool(&[branded], compress(Level::AggressiveCompression), &out);
    assert!(
        matches!(outcome, ToolOutcome::Compressed { .. }),
        "{outcome:?}"
    );
    assert_exactly_one_legible_mark(&out);
}

/// Her sayfa aracı, işaretsiz VE işaretli girdide, dikey/yatay/dönük/kırpık
/// sayfalarda tam bir okunur işaret bırakır.
#[test]
fn every_page_tool_leaves_exactly_one_legible_mark() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("duz.pdf");
    build_pdf(
        &plain,
        &[
            page(595., 842.),
            page(842., 595.),
            PageSpec {
                rotate: 90,
                ..page(595., 842.)
            },
            PageSpec {
                crop: Some([40., 60., 555., 800.]),
                ..page(595., 842.)
            },
            page(1414., 2068.),
        ],
    );
    let branded = dir.path().join("markali.pdf");
    tool(std::slice::from_ref(&plain), identity(5), &branded);
    let ops = [
        ("rotate", ToolOperation::Rotate { degrees: 90 }),
        (
            "rotate_selected",
            ToolOperation::RotateSelected {
                degrees: 180,
                pages: vec![1, 3],
            },
        ),
        (
            "rotate_pages",
            ToolOperation::RotatePages {
                rotations: vec![PageRotation {
                    page: 2,
                    degrees: 270,
                }],
            },
        ),
        (
            "reorder",
            ToolOperation::Reorder {
                pages: vec![5, 4, 3, 2, 1],
            },
        ),
        ("select", ToolOperation::Select { pages: vec![2, 5] }),
        ("delete", ToolOperation::Delete { pages: vec![1] }),
        ("crop", ToolOperation::Crop { margin_pt: 10. }),
        (
            "watermark",
            ToolOperation::Watermark {
                text: "GİZLİ".into(),
            },
        ),
        ("number", ToolOperation::Number { start: 1 }),
    ];
    for (input, label) in [(&plain, "isaretsiz"), (&branded, "isaretli")] {
        for (name, op) in &ops {
            let out = dir.path().join(format!("{label}-{name}.pdf"));
            tool(std::slice::from_ref(input), op.clone(), &out);
            assert_exactly_one_legible_mark(&out);
        }
    }
}

#[test]
fn merge_brands_every_page_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    build_pdf(&a, &[page(595., 842.), page(1414., 2068.)]);
    build_pdf(&b, &[page(842., 595.)]);
    let branded_b = dir.path().join("b-markali.pdf");
    tool(std::slice::from_ref(&b), identity(1), &branded_b);
    let out = dir.path().join("birlesik.pdf");
    tool(&[a, branded_b], ToolOperation::Merge, &out);
    assert_eq!(Document::load(&out).unwrap().get_pages().len(), 3);
    assert_exactly_one_legible_mark(&out);
}

#[test]
fn images_to_pdf_brands_every_page_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut images = Vec::new();
    for (i, (w, h)) in [(1200u32, 1700u32), (1700, 1200)].into_iter().enumerate() {
        let p = dir.path().join(format!("foto-{i}.jpg"));
        std::fs::write(&p, scan_jpeg(w, h, 7 + i as u32)).unwrap();
        images.push(p);
    }
    let out = dir.path().join("gorseller.pdf");
    tool(&images, ToolOperation::Images, &out);
    assert_eq!(Document::load(&out).unwrap().get_pages().len(), 2);
    assert_exactly_one_legible_mark(&out);
}

fn source(id: &str, path: &Path, signed: bool, policy: SignedPolicy) -> SourceFile {
    SourceFile {
        id: id.into(),
        path: path.to_path_buf(),
        file_name: path.file_name().unwrap().to_string_lossy().into_owned(),
        format: SourceFormat::Pdf,
        size_bytes: std::fs::metadata(path).unwrap().len(),
        sha256_before: calculate_sha256(path).unwrap(),
        mtime: 1000,
        page_count: Document::load(path).unwrap().get_pages().len(),
        is_signed: signed,
        signature_note: None,
        signed_policy: policy,
        is_approved_for_conversion: true,
        is_repaired: false,
        repair_note: None,
    }
}

/// Ekleri Hazırla: türetilen her PDF işaretli. İmzalı orijinal ("olduğu gibi
/// kullan") BAYT BAYT aynı kalır: işaret eklemek e-imzayı bozar ve kural
/// yalnız GölgeDosya'nın ürettiği/değiştirdiği PDF'i kapsar.
#[test]
fn ekler_brands_every_derived_pdf_and_keeps_the_signed_original_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let scan_src = dir.path().join("tarama.pdf");
    build_pdf(&scan_src, &[scan(1414., 2068.), scan(595., 842.)]);
    let plain_src = dir.path().join("dilekce.pdf");
    build_pdf(&plain_src, &[page(595., 842.)]);
    let signed_src = dir.path().join("imzali.pdf");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/signed-marker.pdf"),
        &signed_src,
    )
    .unwrap();
    let hashes: Vec<_> = [&scan_src, &plain_src, &signed_src]
        .iter()
        .map(|p| calculate_sha256(p).unwrap())
        .collect();

    let mut project = Project::new("Marka değişmezi");
    for (id, p, signed) in [
        ("s1", &scan_src, false),
        ("s2", &plain_src, false),
        ("s3", &signed_src, true),
    ] {
        project.add_source(source(id, p, signed, SignedPolicy::UseOriginalAsIs));
    }
    for (name, id) in [("Tarama", "s1"), ("Dilekçe", "s2"), ("İmzalı", "s3")] {
        project.create_exhibit(name).sources.push(ExhibitSourceRef {
            source_id: id.into(),
            page_range: None,
        });
    }
    project.optimization = Level::BalancedCompression;
    let out_dir = dir.path().join("paket");
    std::fs::create_dir_all(&out_dir).unwrap();
    let result = execute_uyap_preparation(
        &project,
        &ExecutionContext {
            output_dir: out_dir,
        },
    )
    .unwrap();

    let signed_bytes = std::fs::read(&signed_src).unwrap();
    let (mut derived, mut originals) = (0, 0);
    for o in &result.outputs {
        let path = result.package_dir.join(&o.file_name);
        if std::fs::read(&path).unwrap() == signed_bytes {
            originals += 1;
            continue;
        }
        derived += 1;
        assert_exactly_one_legible_mark(&path);
    }
    assert!(derived >= 2, "türetilmiş Ek sayısı: {derived}");
    assert_eq!(originals, 1, "imzalı orijinal bayt bayt korunmalı");
    let after: Vec<_> = [&scan_src, &plain_src, &signed_src]
        .iter()
        .map(|p| calculate_sha256(p).unwrap())
        .collect();
    assert_eq!(hashes, after, "Ekler bir kaynağı değiştirdi");
}

#[test]
fn a_no_benefit_compression_leaves_no_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("vektor.pdf");
    build_pdf(&src, &[page(595., 842.)]);
    let out = dir.path().join("sikistirilmis.pdf");
    let outcome = tool(&[src], compress(Level::BalancedCompression), &out);
    assert!(
        matches!(outcome, ToolOutcome::NoBenefit { .. }),
        "{outcome:?}"
    );
    assert!(!out.exists(), "kazanç yokken çıktı bırakıldı");
    let stray: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "vektor.pdf")
        .collect();
    assert!(stray.is_empty(), "geride kalan dosya: {stray:?}");
}

/// Bir PDF birden fazla işlemden geçer: Ekler → sıkıştır → döndür → sırala →
/// yeniden sıkıştır. Üç işlem, üç işaret DEĞİL: tek işaret.
#[test]
fn a_chain_of_operations_ends_with_exactly_one_mark() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("kaynak.pdf");
    build_pdf(
        &src,
        &[scan(1414., 2068.), scan(595., 842.), scan(842., 595.)],
    );
    let mut project = Project::new("Zincir");
    project.add_source(source("s1", &src, false, SignedPolicy::UseOriginalAsIs));
    project
        .create_exhibit("Tarama")
        .sources
        .push(ExhibitSourceRef {
            source_id: "s1".into(),
            page_range: None,
        });
    let out_dir = dir.path().join("paket");
    std::fs::create_dir_all(&out_dir).unwrap();
    let result = execute_uyap_preparation(
        &project,
        &ExecutionContext {
            output_dir: out_dir,
        },
    )
    .unwrap();
    let mut current = result.package_dir.join(&result.outputs[0].file_name);
    let steps = [
        ("sikistir", compress(Level::AggressiveCompression)),
        ("dondur", ToolOperation::Rotate { degrees: 90 }),
        (
            "sirala",
            ToolOperation::Reorder {
                pages: vec![3, 1, 2],
            },
        ),
        ("yeniden-sikistir", compress(Level::BalancedCompression)),
        ("dondur-geri", ToolOperation::Rotate { degrees: 270 }),
    ];
    for (name, op) in steps {
        let out = dir.path().join(format!("{name}.pdf"));
        match tool(std::slice::from_ref(&current), op, &out) {
            ToolOutcome::NoBenefit { .. } => continue,
            _ => {
                assert_exactly_one_legible_mark(&out);
                current = out;
            }
        }
    }
}

/// Düzeltmeden önce üretilmiş, büyük sayfada okunmaz işaret taşıyan belgeler
/// (sahadaki üç dosya dahil) yeniden işlendiğinde işaret yükseltilir —
/// yanına ikinci bir işaret eklenmez. Her dönemin çizimi için.
#[test]
fn an_old_undersized_mark_is_upgraded_not_duplicated() {
    let dir = tempfile::tempdir().unwrap();
    for (name, logo) in [
        ("canonical", CANONICAL_LOGO),
        ("superseded", SUPERSEDED_LOGO),
        ("legacy", LEGACY_LOGO),
    ] {
        let src = dir.path().join(format!("eski-{name}.pdf"));
        old_style_branded(&src, 1414., 2068., logo);
        for (op_name, op) in [
            ("sirala", identity(1)),
            ("dondur", ToolOperation::Rotate { degrees: 90 }),
        ] {
            let out = dir.path().join(format!("eski-{name}-{op_name}.pdf"));
            tool(std::slice::from_ref(&src), op, &out);
            assert_exactly_one_legible_mark(&out);
        }
    }
}

/// Kabul edilmiş işaret standart sayfalarda DEĞİŞMEZ: A4 ve Letter'da işaret
/// 56 pt, pay 34 pt kalır (dikey ve yatay).
#[test]
fn the_accepted_mark_is_unchanged_on_standard_pages() {
    let dir = tempfile::tempdir().unwrap();
    for (w, h) in [
        (595., 842.),
        (842., 595.),
        (612., 792.),
        (792., 612.),
        (612., 1008.),
    ] {
        let src = dir.path().join(format!("std-{w}x{h}.pdf"));
        build_pdf(&src, &[page(w, h)]);
        let out = dir.path().join(format!("std-{w}x{h}-out.pdf"));
        tool(&[src], identity(1), &out);
        let doc = Document::load(&out).unwrap();
        let p = &census(&doc)[0];
        assert_eq!(p.draws.len(), 1);
        assert!(
            (p.draws[0].width - 56.).abs() < 0.01,
            "{w}x{h}: işaret {} pt",
            p.draws[0].width
        );
        let id = *doc.get_pages().values().next().unwrap();
        let crop = boxed(&doc, id, b"CropBox").unwrap();
        assert!(
            ((crop[3] - crop[1]) - (h + 34.)).abs() < 0.01,
            "{w}x{h}: pay değişmiş"
        );
    }
}

/// İşaret artık isteğe bağlı değil: işareti taşıyamayacak kadar küçük sayfa
/// sessizce işaretsiz bırakılmaz, işaret sığacak kadar küçültülür.
#[test]
fn a_tiny_page_is_branded_not_silently_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("minik.pdf");
    build_pdf(&src, &[page(60., 60.), page(595., 842.)]);
    let out = dir.path().join("minik-out.pdf");
    tool(&[src], identity(2), &out);
    assert_exactly_one_legible_mark(&out);
}

/// Çıkış sınırı yapısal: ham PDF baytı yayın yazıcısından geçemez. İşaretli
/// PDF yalnız markalama kapısından geçmiş bir değer olarak yayımlanabilir.
#[test]
fn raw_pdf_bytes_cannot_be_published_through_the_generic_writer() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("x.pdf");
    build_pdf(&src, &[page(595., 842.)]);
    let raw = std::fs::read(&src).unwrap();
    let dest = dir.path().join("yayim.pdf");
    let result = safe_io::write_new_bytes(&dest, &[], &raw);
    assert!(result.is_err(), "işaretsiz PDF genel yazıcıdan yayımlandı");
    assert!(!dest.exists());
    // PDF olmayan içerik (manifest, liste) etkilenmez.
    let text = dir.path().join("liste.txt");
    safe_io::write_new_bytes(&text, &[], b"Ek-1 Dilekce").unwrap();
    assert!(text.exists());
}
