//! Ek damgasının metni kutusunun ortasında (GölgeDosya saha maddesi 5).
//!
//! "Kutunun ortasında olsa daha iyi olabilir." Kutu genişliği harf sayısıyla
//! tahmin ediliyor, metin sol kenardan 8 pt'ye sabitleniyordu: "Ek-1 / 1. Sayfa"
//! için solda 8 pt, sağda 38,2 pt boşluk. Test, sayfa içerik akışındaki GERÇEK
//! kutuyu (`re`) ve metin konumunu (`Td`) okur; metin genişliğini uygulamadan
//! BAĞIMSIZ bir tabloyla hesaplar.
use lopdf::content::Content;
use lopdf::{dictionary, Document, Object, Stream};

fn one_page() -> Document {
    let mut d = Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let content = d.add_object(Stream::new(dictionary! {}, b"BT ET".to_vec()));
    let page = d.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content,
        "Resources" => dictionary! {},
    });
    d.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = d.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    d.trailer.set("Root", catalog);
    d
}

/// Adobe'nin Helvetica-Bold AFM'inden, yalnız damganın kullandığı glifler.
fn afm_width(text: &str, size: f32) -> f32 {
    let units: u32 = text
        .chars()
        .map(|c| match c {
            ' ' | '.' | '/' => 278,
            '-' | 'f' => 333,
            '0'..='9' | 'a' | 'k' | 'y' => 556,
            'E' | 'S' => 667,
            other => panic!("tabloda yok: {other}"),
        })
        .sum();
    units as f32 * size / 1000.0
}

fn gaps(exhibit: usize, page: usize) -> (f32, f32) {
    let mut d = one_page();
    let config = ekler_core::StampConfig::default();
    ekler_core::pdf::stamp::apply_stamp_to_document(&mut d, exhibit, page, &config).unwrap();
    let page_id = *d.get_pages().values().next().unwrap();
    let ops = Content::decode(&d.get_page_content(page_id).unwrap())
        .unwrap()
        .operations;
    // Rozet: doldurulan dikdörtgen ("re" hemen ardından "f"). İlk "re" sayfa
    // kırpma bölgesidir, rozet değil.
    let rect = ops
        .windows(2)
        .find(|w| w[0].operator == "re" && w[1].operator == "f")
        .map(|w| &w[0])
        .expect("rozet kutusu");
    let td = ops
        .iter()
        .find(|o| o.operator == "Td")
        .expect("metin konumu");
    let f = |o: &Object| o.as_float().unwrap();
    let (x, w) = (f(&rect.operands[0]), f(&rect.operands[2]));
    let tx = f(&td.operands[0]);
    let text_w = afm_width(&format!("Ek-{exhibit} / {page}. Sayfa"), config.font_size);
    (tx - x, (x + w) - (tx + text_w))
}

#[test]
fn the_stamp_text_sits_in_the_middle_of_its_box() {
    for (exhibit, page) in [(1, 1), (3, 12), (12, 125)] {
        let (left, right) = gaps(exhibit, page);
        assert!(
            (left - right).abs() <= 1.0,
            "Ek-{exhibit} / {page}. Sayfa: sol {left:.1} pt, sağ {right:.1} pt"
        );
        assert!(left >= 6.0, "metin kutunun kenarına yapışmamalı: {left:.1}");
    }
}
