//! P1 REGRESSION KİLİDİ — türetilen her PDF'in sağ alt işareti canonical
//! GölgeDosya kimliğini taşır; eski DüzenEk kimliği hiçbir koşulda görünmez.
//!
//! Bu bug GÖRSELDİ, metinsel değil: eski marka kelime işareti VEKTÖR YOLLARI
//! olarak çiziliyordu, yani çıktı baytlarında "DüzenEk" DİZESİ HİÇ GEÇMİYORDU.
//! Bu yüzden burada dizeye değil ÇİZİME (logo akışının baytlarına) bakılır.
//! Yalnız string arayan bir test bu bug'ı kaçırırdı.

mod hardening;

use hardening::check::{self, Lab};
use hardening::corpus::{self, Pager};
use lopdf::{dictionary, Dictionary, Document as LopdfDoc, Object, Stream};

use ekler_core::toolbox::{run_tool_with_outcome, ToolOperation, ToolOutcome};

/// Üretimdeki canonical marka çizimi.
const BRAND_LOGO: &[u8] = include_bytes!("../../pdf-core/assets/brand-logo.ops");
/// Birleşme öncesi DüzenEk çizimi. Çıktıda ASLA bulunmamalı.
const LEGACY_BRAND_LOGO: &[u8] = include_bytes!("../../pdf-core/assets/brand-logo-legacy.ops");
const BRAND_FORM_CONTENT: &[u8] = b"q /BrandAlpha gs /Mark Do Q";

const A4: [f64; 4] = [0., 0., 595.28, 841.89];

fn stream_bytes(s: &Stream) -> Option<Vec<u8>> {
    if s.dict.has(b"Filter") {
        s.decompressed_content().ok()
    } else {
        Some(s.content.clone())
    }
}

/// Belgedeki bütün akışların çözülmüş içerikleri.
fn all_streams(doc: &LopdfDoc) -> Vec<Vec<u8>> {
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter_map(stream_bytes)
        .collect()
}

fn count_logo(doc: &LopdfDoc, logo: &[u8]) -> usize {
    all_streams(doc).iter().filter(|s| s.as_slice() == logo).count()
}

/// Bir sayfanın marka formunu kaç kez çizdiği (birikme denetimi).
fn brand_draw_count(doc: &LopdfDoc) -> usize {
    all_streams(doc)
        .iter()
        .filter(|s| s.as_slice() == BRAND_FORM_CONTENT)
        .count()
}

fn open(path: &std::path::Path) -> LopdfDoc {
    check::reopen_strict(path).expect("çıktı katı biçimde açılmalı")
}

/// Bir aracı gerçekten çalıştır ve markalı çıktıyı aç.
fn run(lab: &Lab, src: &std::path::Path, op: ToolOperation, name: &str) -> (std::path::PathBuf, LopdfDoc) {
    let out = lab.path(name);
    match run_tool_with_outcome(std::slice::from_ref(&src.to_path_buf()), &op, &out, false) {
        Ok(ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. }) => {}
        other => panic!("{op:?} başarısız: {other:?}"),
    }
    let doc = open(&out);
    (out, doc)
}

// ---------------------------------------------------------------- 1) kimlik

#[test]
fn derived_output_carries_golgedosya_and_never_the_legacy_duzenek_mark() {
    let lab = Lab::new();
    let fx = corpus::simple()
        .into_iter()
        .find(|f| f.name == "basit/1-sayfa-metin")
        .expect("fixture");
    let src = lab.write("kaynak.pdf", &fx.bytes);
    let (_, doc) = run(&lab, &src, ToolOperation::Rotate { degrees: 0 }, "cikti.pdf");

    // Eski ÇİZİM hiçbir biçimde bulunmamalı (asıl regresyon).
    assert_eq!(
        count_logo(&doc, LEGACY_BRAND_LOGO),
        0,
        "P1: çıktıda eski DüzenEk marka çizimi var"
    );
    // Canonical çizim tam bir kez.
    assert_eq!(
        count_logo(&doc, BRAND_LOGO),
        1,
        "canonical GölgeDosya çizimi tam bir kez olmalı"
    );
    // Kelime işareti metinsel olarak da canonical olmalı (kopyalanabilir/aranabilir).
    let logo = String::from_utf8_lossy(BRAND_LOGO).to_string();
    assert!(
        logo.contains("GölgeDosya") || logo.contains(r"G\366lgeDosya"),
        "marka çizimi GölgeDosya kelime işaretini taşımalı: {logo}"
    );
    // Eski ürün adı hiçbir akışta DİZE olarak da geçmemeli.
    for s in all_streams(&doc) {
        let t = String::from_utf8_lossy(&s);
        for bad in ["DüzenEk", "DuzenEk"] {
            assert!(!t.contains(bad), "çıktı akışında eski ürün adı: {bad}");
        }
    }
}

// ------------------------------------------------- 2) geometri × sayfa sayısı

#[test]
fn the_mark_is_applied_across_sizes_orientations_rotations_and_page_counts() {
    let lab = Lab::new();

    // (ad, belge) — dikey, yatay, döndürülmüş, karışık boyut, çok sayfa.
    let mut cases: Vec<(String, LopdfDoc)> = Vec::new();

    let mut p = Pager::new("1.7");
    p.text_page(1, A4, dictionary! {});
    cases.push(("dikey-1".into(), p.finish(dictionary! {})));

    let mut p = Pager::new("1.7");
    p.text_page(1, [0., 0., 841.89, 595.28], dictionary! {});
    cases.push(("yatay-1".into(), p.finish(dictionary! {})));

    for deg in [90i64, 180, 270] {
        let mut p = Pager::new("1.7");
        p.text_page(1, A4, dictionary! {"Rotate"=>deg});
        cases.push((format!("donuk-{deg}"), p.finish(dictionary! {})));
    }

    // Karışık boyut: A4 + yatay + küçük.
    let mut p = Pager::new("1.7");
    p.text_page(1, A4, dictionary! {});
    p.text_page(2, [0., 0., 841.89, 595.28], dictionary! {});
    p.text_page(3, [0., 0., 300., 400.], dictionary! {});
    cases.push(("karisik".into(), p.finish(dictionary! {})));

    cases.push(("sayfa-10".into(), corpus::text_doc(10)));
    cases.push(("sayfa-100".into(), corpus::text_doc(100)));

    for (name, mut doc) in cases {
        let bytes = corpus::save(&mut doc);
        let src = lab.write(&format!("{name}-kaynak.pdf"), &bytes);
        let before = check::sha(&src);
        let (_, out_doc) = run(
            &lab,
            &src,
            ToolOperation::Rotate { degrees: 0 },
            &format!("{name}-cikti.pdf"),
        );

        assert_eq!(
            count_logo(&out_doc, LEGACY_BRAND_LOGO),
            0,
            "{name}: eski DüzenEk çizimi çıktıda"
        );
        assert_eq!(
            count_logo(&out_doc, BRAND_LOGO),
            1,
            "{name}: canonical çizim tam bir kez paylaşılmalı"
        );
        // Kaynak değişmemeli.
        assert_eq!(check::sha(&src), before, "{name}: KAYNAK DEĞİŞTİ");
    }
}

// ---------------------------------------------------------- 3) idempotentlik

#[test]
fn chained_operations_never_accumulate_a_second_brand_layer() {
    let lab = Lab::new();
    let fx = corpus::simple()
        .into_iter()
        .find(|f| f.name == "basit/karma-10")
        .expect("fixture");
    let src = lab.write("kaynak.pdf", &fx.bytes);

    // filigran → döndür → sıkıştır → döndür: kullanıcının gerçek zinciri.
    let chain: Vec<(ToolOperation, &str)> = vec![
        (
            ToolOperation::Watermark {
                text: "KOPYA".into(),
            },
            "a.pdf",
        ),
        (ToolOperation::Rotate { degrees: 90 }, "b.pdf"),
        (
            ToolOperation::Compress {
                level: ekler_core::OptimizationLevel::GentleCompression,
            },
            "c.pdf",
        ),
        (ToolOperation::Rotate { degrees: 180 }, "d.pdf"),
    ];

    let mut current = src.clone();
    for (op, name) in chain {
        let label = format!("{op:?}");
        let out = lab.path(name);
        match run_tool_with_outcome(std::slice::from_ref(&current), &op, &out, false) {
            Ok(ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. }) => {}
            // Sıkıştırma kazanç sağlamazsa çıktı YAZILMAZ; zincir bir önceki
            // dosyayla sürer. Bu bir hata değil, meşru bir sonuçtur.
            Ok(ToolOutcome::NoBenefit { .. }) => continue,
            other => panic!("{label} başarısız: {other:?}"),
        }
        let doc = open(&out);
        assert_eq!(
            count_logo(&doc, BRAND_LOGO),
            1,
            "{label}: marka çizimi birikti (tek paylaşılan form olmalı)"
        );
        assert_eq!(
            count_logo(&doc, LEGACY_BRAND_LOGO),
            0,
            "{label}: eski çizim ortaya çıktı"
        );
        // Sayfa başına en fazla bir marka formu çizimi; katman birikmemeli.
        let pages = doc.get_pages().len();
        assert!(
            brand_draw_count(&doc) <= pages,
            "{label}: sayfa sayısından ({pages}) fazla marka çizimi: {}",
            brand_draw_count(&doc)
        );
        current = out;
    }
}

// ------------------------------------------- 4) eski markalı belgenin yükseltimi

/// Eski sürümün ürettiği gibi DüzenEk markalı bir belge kur.
fn legacy_branded_document() -> Vec<u8> {
    let mut p = Pager::new("1.7");
    let page = p.text_page(1, A4, dictionary! {});
    let mut doc = p.finish(dictionary! {});

    // Eski iç vektör formu: legacy çizim, BOŞ Resources (font yoktu).
    let mut vector = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form",
            "BBox"=>vec![Object::Integer(0),Object::Integer(0),Object::Integer(290),Object::Integer(72)],
            "Group"=>dictionary! {"S"=>"Transparency","I"=>true,"CS"=>"DeviceRGB"},
            "Resources"=>Dictionary::new()},
        LEGACY_BRAND_LOGO.to_vec(),
    );
    vector.compress().unwrap();
    let vector_id = doc.add_object(vector);

    let mut form = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form",
            "BBox"=>vec![Object::Integer(0),Object::Integer(0),Object::Integer(290),Object::Integer(72)],
            "Resources"=>dictionary! {"XObject"=>dictionary! {"Mark"=>vector_id},
                "ExtGState"=>dictionary! {"BrandAlpha"=>dictionary! {"Type"=>"ExtGState","ca"=>0.24f32,"CA"=>0.24f32}}}},
        BRAND_FORM_CONTENT.to_vec(),
    );
    form.compress().unwrap();
    let form_id = doc.add_object(form);

    // Sayfaya formu çizen bir içerik akışı ekle (sağ altta).
    let draw = doc.add_object(Stream::new(
        dictionary! {},
        b"q 0.193 0 0 0.193 531 8 cm /GolgeDosyaBrand Do Q".to_vec(),
    ));
    let page_dict = doc.get_dictionary_mut(page).unwrap();
    let existing = page_dict.get(b"Contents").unwrap().clone();
    page_dict.set(
        "Contents",
        Object::Array(vec![existing, Object::Reference(draw)]),
    );
    let resources = page_dict.get_mut(b"Resources").unwrap().as_dict_mut().unwrap();
    resources.set(
        "XObject",
        dictionary! {"GolgeDosyaBrand"=>form_id},
    );

    corpus::save(&mut doc)
}

/// Eski sürümle markalanmış bir belge yeniden işlendiğinde kullanıcı
/// "DüzenEk + GölgeDosya" çift filigranı GÖRMEMELİ; eski işaret canonical
/// işarete yükseltilmeli.
#[test]
fn a_legacy_duzenek_branded_document_is_upgraded_not_double_marked() {
    let lab = Lab::new();
    let bytes = legacy_branded_document();
    let src = lab.write("eski-markali.pdf", &bytes);

    // Girdi gerçekten eski çizimi taşıyor (fixture'ın kendisi doğru mu?).
    let input_doc = open(&src);
    assert_eq!(
        count_logo(&input_doc, LEGACY_BRAND_LOGO),
        1,
        "fixture eski çizimi taşımalı"
    );

    let (_, doc) = run(&lab, &src, ToolOperation::Rotate { degrees: 0 }, "cikti.pdf");

    assert_eq!(
        count_logo(&doc, LEGACY_BRAND_LOGO),
        0,
        "P1: eski DüzenEk çizimi çıktıda KALDI (kullanıcı çift filigran görür)"
    );
    assert_eq!(
        count_logo(&doc, BRAND_LOGO),
        1,
        "P1: tam bir canonical GölgeDosya çizimi olmalı (çift işaret yok)"
    );
    // Sayfa tek bir marka katmanı çizmeli.
    assert_eq!(
        brand_draw_count(&doc),
        1,
        "P1: sayfada birden çok marka katmanı"
    );
}

// ------------------------------------------------------------- 5) boyut etkisi

#[test]
fn the_mark_does_not_materially_bloat_the_document() {
    let lab = Lab::new();
    let fx = corpus::simple()
        .into_iter()
        .find(|f| f.name == "basit/karma-10")
        .expect("fixture");
    let src = lab.write("kaynak.pdf", &fx.bytes);
    let before = std::fs::metadata(&src).unwrap().len();
    let (out, _) = run(&lab, &src, ToolOperation::Rotate { degrees: 0 }, "cikti.pdf");
    let after = std::fs::metadata(&out).unwrap().len();

    // Marka TEK paylaşılan formdur; 10 sayfalık belgeye eklediği yük küçük olmalı.
    assert!(
        after < before + 8 * 1024,
        "marka belgeyi anlamsız büyüttü: {before} → {after}"
    );
}
