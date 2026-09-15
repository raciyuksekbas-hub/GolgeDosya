//! Sentetik PDF corpus. Her sayfa kimliğini `(Mk N)` işaretiyle taşır (görsel
//! sayfalarda içerik akışı yorumu olarak); sıra ve seçim bu işaretle ölçülür.
//!
//! Her fixture beklenen sonucunu AÇIKÇA taşır: ya açılır (sayfa sayısı ve onarım
//! notu sabit) ya da güvenle reddedilir. "Kurtar" ile "reddet" arasındaki seçim
//! deterministik olmak zorundadır; beklenti değişirse test düşer.

use super::raw::{zlib, RawPdf};
use lopdf::xref::XrefType;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

#[derive(Debug, Clone, PartialEq)]
pub enum Expect {
    Opens { pages: usize, repaired: bool },
    Rejects,
}

#[derive(Clone)]
pub struct Fixture {
    pub name: String,
    pub bytes: Vec<u8>,
    pub expect: Expect,
    /// İmzalı belge: türetilmiş kopya için onay gerekir.
    pub signed: bool,
}

impl Fixture {
    fn new(name: &str, bytes: Vec<u8>, expect: Expect) -> Self {
        Self {
            name: name.into(),
            bytes,
            expect,
            signed: false,
        }
    }
}

pub const A4: [f64; 4] = [0., 0., 595.28, 841.89];

pub fn rect_obj(r: [f64; 4]) -> Object {
    Object::Array(r.iter().map(|v| Object::Real(*v as f32)).collect())
}

pub fn save(doc: &mut Document) -> Vec<u8> {
    let mut b = Vec::new();
    doc.save_to(&mut b).unwrap();
    b
}

/// Deterministik gürültü (xorshift32): Flate ile küçülmeyen görsel verisi.
pub fn noise(len: usize, seed: u32) -> Vec<u8> {
    let mut s = seed | 1;
    (0..len)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s >> 11) as u8
        })
        .collect()
}

pub fn image(w: u32, h: u32, seed: u32, compress: bool) -> Stream {
    let mut s = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>w as i64,"Height"=>h as i64,
        "ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8},
        noise((w * h * 3) as usize, seed),
    );
    if compress {
        s.compress().unwrap();
    }
    s
}

/// Sayfa ağacı kurucusu.
pub struct Pager {
    pub doc: Document,
    pub pages: ObjectId,
    pub font: ObjectId,
    pub kids: Vec<Object>,
}

impl Pager {
    pub fn new(version: &str) -> Self {
        let mut doc = Document::with_version(version);
        let pages = doc.new_object_id();
        let font =
            doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
        Self {
            doc,
            pages,
            font,
            kids: Vec::new(),
        }
    }

    /// İşaretli metin sayfası; `extra` sayfa sözlüğüne eklenir (üzerine yazar).
    pub fn text_page(&mut self, n: u32, media: [f64; 4], extra: Dictionary) -> ObjectId {
        let contents = self.doc.add_object(Stream::new(
            dictionary! {},
            format!("BT /F1 18 Tf 72 72 Td (Mk {n}) Tj ET").into_bytes(),
        ));
        let mut page = dictionary! {"Type"=>"Page","Parent"=>self.pages,"MediaBox"=>rect_obj(media),
        "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>self.font}},"Contents"=>contents};
        for (k, v) in extra.iter() {
            page.set(k.clone(), v.clone());
        }
        let id = self.doc.add_object(page);
        self.kids.push(id.into());
        id
    }

    /// Görsel ağırlıklı sayfa; kimlik küçük bir altyazı olarak GERÇEK metin
    /// çizilir (yorum değil): lopdf içerik çözücüsü yorumda kesildiği için
    /// işaret gerçek bir `Tj` ile yazılır.
    pub fn image_page(&mut self, n: u32, image: ObjectId) -> ObjectId {
        let contents = self.doc.add_object(Stream::new(
            dictionary! {},
            format!("q 400 0 0 300 72 400 cm /Im0 Do Q BT /F1 8 Tf 72 20 Td (Mk {n}) Tj ET")
                .into_bytes(),
        ));
        let id = self.doc.add_object(dictionary! {"Type"=>"Page","Parent"=>self.pages,
        "MediaBox"=>rect_obj(A4),
        "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>self.font},"XObject"=>dictionary!{"Im0"=>image}},
        "Contents"=>contents});
        self.kids.push(id.into());
        id
    }

    pub fn finish(mut self, pages_extra: Dictionary) -> Document {
        let mut pages =
            dictionary! {"Type"=>"Pages","Kids"=>self.kids.clone(),"Count"=>self.kids.len() as i64};
        for (k, v) in pages_extra.iter() {
            pages.set(k.clone(), v.clone());
        }
        self.doc.objects.insert(self.pages, pages.into());
        let root = self
            .doc
            .add_object(dictionary! {"Type"=>"Catalog","Pages"=>self.pages});
        self.doc.trailer.set("Root", root);
        self.doc
    }
}

pub fn text_doc(n: u32) -> Document {
    let mut p = Pager::new("1.7");
    for i in 1..=n {
        p.text_page(i, A4, dictionary! {});
    }
    p.finish(dictionary! {})
}

fn opens(pages: usize) -> Expect {
    Expect::Opens {
        pages,
        repaired: false,
    }
}

// ------------------------------------------------------------------ basit

pub fn simple() -> Vec<Fixture> {
    let mut out = vec![
        Fixture::new("basit/1-sayfa-metin", save(&mut text_doc(1)), opens(1)),
        Fixture::new("basit/2-sayfa-metin", save(&mut text_doc(2)), opens(2)),
        Fixture::new(
            "basit/100-sayfa-metin",
            save(&mut text_doc(100)),
            opens(100),
        ),
    ];
    // Yalnız görsel: her sayfa kendi görselini çizer.
    let mut p = Pager::new("1.7");
    for i in 1..=3 {
        let img = p.doc.add_object(image(64, 48, i, true));
        p.image_page(i, img);
    }
    out.push(Fixture::new(
        "basit/yalniz-gorsel",
        save(&mut p.finish(dictionary! {})),
        opens(3),
    ));
    // Karma: metin + ortak görsel + sayfaya özel görsel.
    let mut p = Pager::new("1.7");
    let shared = p.doc.add_object(image(96, 64, 99, true));
    for i in 1..=10u32 {
        if i % 3 == 0 {
            p.image_page(i, shared);
        } else {
            let own = p.doc.add_object(image(32, 32, 1000 + i, false));
            let id = p.text_page(i, A4, dictionary! {});
            let page = p.doc.get_dictionary_mut(id).unwrap();
            page.set(
                "Resources",
                dictionary! {"Font"=>dictionary!{"F1"=>p.font},"XObject"=>dictionary!{"Im0"=>own}},
            );
        }
    }
    out.push(Fixture::new(
        "basit/karma-10",
        save(&mut p.finish(dictionary! {})),
        opens(10),
    ));
    out
}

// ------------------------------------------------------------------ geometri

pub fn geometry() -> Vec<Fixture> {
    let mut out = Vec::new();
    let mut p = Pager::new("1.7");
    for (i, media) in [
        A4,
        [0., 0., 841.89, 595.28],
        [0., 0., 612., 792.],
        [0., 0., 72., 72.],
        [0., 0., 3000., 3000.],
    ]
    .iter()
    .enumerate()
    {
        p.text_page(i as u32 + 1, *media, dictionary! {});
    }
    out.push(Fixture::new(
        "geometri/karisik-boyutlar",
        save(&mut p.finish(dictionary! {})),
        opens(5),
    ));

    let single = |name: &str, media: [f64; 4], extra: Dictionary| {
        let mut p = Pager::new("1.7");
        p.text_page(1, media, extra);
        p.text_page(2, A4, dictionary! {});
        Fixture::new(name, save(&mut p.finish(dictionary! {})), opens(2))
    };
    out.push(single(
        "geometri/sifir-disi-orijin",
        [100., 100., 695.28, 941.89],
        dictionary! {},
    ));
    out.push(single(
        "geometri/ters-koseler",
        [595.28, 841.89, 0., 0.],
        dictionary! {},
    ));
    out.push(single(
        "geometri/negatif-orijin",
        [-297.64, -420.95, 297.64, 420.95],
        dictionary! {},
    ));
    out.push(single(
        "geometri/cropbox",
        A4,
        dictionary! {"CropBox"=>rect_obj([36., 36., 559.28, 805.89])},
    ));
    out.push(single(
        "geometri/cropbox-mediabox-disinda",
        A4,
        dictionary! {"CropBox"=>rect_obj([-50., -50., 700., 900.])},
    ));
    out.push(single(
        "geometri/userunit",
        A4,
        dictionary! {"UserUnit"=>2.0f32},
    ));

    let mut p = Pager::new("1.7");
    for (i, r) in [90i64, 180, 270, -90, 450].iter().enumerate() {
        p.text_page(i as u32 + 1, A4, dictionary! {"Rotate"=>*r});
    }
    out.push(Fixture::new(
        "geometri/rotate-90-180-270-eksi90-450",
        save(&mut p.finish(dictionary! {})),
        opens(5),
    ));

    // Miras: Pages düğümünde Rotate 90; 2. sayfa kendi Rotate 0'ını taşır.
    let mut p = Pager::new("1.7");
    p.text_page(1, A4, dictionary! {});
    p.text_page(2, A4, dictionary! {"Rotate"=>0});
    out.push(Fixture::new(
        "geometri/miras-rotate",
        save(&mut p.finish(dictionary! {"Rotate"=>90})),
        opens(2),
    ));

    // Dolaylı ve ondalık /Rotate: geçerli PDF, nadir.
    let mut p = Pager::new("1.7");
    let ninety = p.doc.add_object(Object::Integer(90));
    p.text_page(1, A4, dictionary! {"Rotate"=>ninety});
    p.text_page(2, A4, dictionary! {"Rotate"=>Object::Real(90.0)});
    out.push(Fixture::new(
        "geometri/dolayli-ve-ondalik-rotate",
        save(&mut p.finish(dictionary! {})),
        opens(2),
    ));

    // Geçersiz /Rotate 45 (90'ın katı değil).
    let mut p = Pager::new("1.7");
    p.text_page(1, A4, dictionary! {"Rotate"=>45});
    out.push(Fixture::new(
        "geometri/gecersiz-rotate-45",
        save(&mut p.finish(dictionary! {})),
        opens(1),
    ));

    // MediaBox yalnız Pages düğümünde.
    let mut p = Pager::new("1.7");
    let a = p.text_page(1, A4, dictionary! {});
    let b = p.text_page(2, A4, dictionary! {});
    for id in [a, b] {
        p.doc.get_dictionary_mut(id).unwrap().remove(b"MediaBox");
    }
    out.push(Fixture::new(
        "geometri/miras-mediabox",
        save(&mut p.finish(dictionary! {"MediaBox"=>rect_obj(A4)})),
        opens(2),
    ));
    out
}

// ------------------------------------------------------------------ kaynaklar

pub fn resources() -> Vec<Fixture> {
    let mut out = Vec::new();
    // Paylaşılan font ve görsel.
    let mut p = Pager::new("1.7");
    let shared = p.doc.add_object(image(120, 80, 7, true));
    for i in 1..=5 {
        let id = p.text_page(i, A4, dictionary! {});
        let font = p.font;
        p.doc.get_dictionary_mut(id).unwrap().set(
            "Resources",
            dictionary! {"Font"=>dictionary!{"F1"=>font},"XObject"=>dictionary!{"Im0"=>shared}},
        );
    }
    out.push(Fixture::new(
        "kaynak/paylasilan-font-ve-gorsel",
        save(&mut p.finish(dictionary! {})),
        opens(5),
    ));

    // ICC + SMask + Form XObject (kendi kaynağıyla) + saydamlık grubu + ExtGState.
    let mut p = Pager::new("1.7");
    let icc = p.doc.add_object(Stream::new(
        dictionary! {"N"=>3,"Alternate"=>"DeviceRGB"},
        vec![0u8; 512],
    ));
    let mut mask = Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>64,"Height"=>64,
        "ColorSpace"=>"DeviceGray","BitsPerComponent"=>8},
        noise(64 * 64, 3),
    );
    mask.compress().unwrap();
    let mask = p.doc.add_object(mask);
    let mut img = image(64, 64, 4, true);
    img.dict.set(
        "ColorSpace",
        vec!["ICCBased".into(), Object::Reference(icc)],
    );
    img.dict.set("SMask", mask);
    let img = p.doc.add_object(img);
    let font = p.font;
    let form = p.doc.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),200.into(),50.into()],
        "Group"=>dictionary!{"S"=>"Transparency","I"=>true},
        "Resources"=>dictionary!{"Font"=>dictionary!{"F9"=>font},"ExtGState"=>dictionary!{"G0"=>dictionary!{"Type"=>"ExtGState","ca"=>0.5f32}}}},
        b"q /G0 gs BT /F9 12 Tf 5 5 Td (Form) Tj ET Q".to_vec(),
    ));
    let contents = p.doc.add_object(Stream::new(
        dictionary! {},
        b"q 200 0 0 200 72 500 cm /Im0 Do Q q 1 0 0 1 72 300 cm /Fm0 Do Q BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET"
            .to_vec(),
    ));
    let pages = p.pages;
    let page = p.doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"MediaBox"=>rect_obj(A4),
    "Group"=>dictionary!{"Type"=>"Group","S"=>"Transparency","CS"=>"DeviceRGB"},
    "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font},"XObject"=>dictionary!{"Im0"=>img,"Fm0"=>form}},
    "Contents"=>contents});
    p.kids.push(page.into());
    out.push(Fixture::new(
        "kaynak/icc-smask-form-saydamlik",
        save(&mut p.finish(dictionary! {})),
        opens(1),
    ));

    // Gömülü TrueType (FontFile2) ve çoklu font.
    let mut p = Pager::new("1.7");
    let file = p
        .doc
        .add_object(Stream::new(dictionary! {"Length1"=>256}, noise(256, 11)));
    let descriptor = p
        .doc
        .add_object(dictionary! {"Type"=>"FontDescriptor","FontName"=>"Gomulu",
        "Flags"=>32,"FontBBox"=>vec![0.into(),0.into(),1000.into(),1000.into()],"ItalicAngle"=>0,
        "Ascent"=>800,"Descent"=>-200,"CapHeight"=>700,"StemV"=>80,"FontFile2"=>file});
    let embedded = p.doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"TrueType","BaseFont"=>"Gomulu",
    "FirstChar"=>32,"LastChar"=>126,"Widths"=>vec![Object::Integer(500); 95],"FontDescriptor"=>descriptor});
    let courier = p
        .doc
        .add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Courier"});
    for i in 1..=3 {
        let id = p.text_page(i, A4, dictionary! {});
        let font = p.font;
        p.doc.get_dictionary_mut(id).unwrap().set(
            "Resources",
            dictionary! {"Font"=>dictionary!{"F1"=>font,"F2"=>embedded,"F3"=>courier}},
        );
    }
    out.push(Fixture::new(
        "kaynak/gomulu-ve-coklu-font",
        save(&mut p.finish(dictionary! {})),
        opens(3),
    ));

    // Kaynaklar yalnız Pages düğümünde (miras).
    let mut p = Pager::new("1.7");
    let ids: Vec<_> = (1..=3)
        .map(|i| p.text_page(i, A4, dictionary! {}))
        .collect();
    for id in ids {
        p.doc.get_dictionary_mut(id).unwrap().remove(b"Resources");
    }
    let font = p.font;
    out.push(Fixture::new(
        "kaynak/miras-resources",
        save(
            &mut p.finish(dictionary! {"Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}}}),
        ),
        opens(3),
    ));
    out
}

// ------------------------------------------------------------------ yapısal

/// Klasik düzen: 1 Catalog, 2 Pages, 3 Font, 10+2i sayfa, 11+2i içerik.
fn raw_base(pdf: &mut RawPdf, pages: u32, marker_offset: u32) {
    let kids: String = (0..pages).map(|i| format!("{} 0 R ", 10 + 2 * i)).collect();
    pdf.object(1, b"<</Type/Catalog/Pages 2 0 R>>");
    pdf.object(
        2,
        format!("<</Type/Pages/Count {pages}/Kids[{kids}]>>").as_bytes(),
    );
    pdf.object(3, b"<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>");
    for i in 0..pages {
        pdf.object(
            10 + 2 * i,
            format!("<</Type/Page/Parent 2 0 R/MediaBox[0 0 595.28 841.89]/Resources<</Font<</F1 3 0 R>>>>/Contents {} 0 R>>", 11 + 2 * i).as_bytes(),
        );
        pdf.stream(
            11 + 2 * i,
            "",
            format!("BT /F1 18 Tf 72 72 Td (Mk {}) Tj ET", i + 1 + marker_offset).as_bytes(),
        );
    }
}

pub fn structural() -> Vec<Fixture> {
    let mut out = Vec::new();

    let mut doc = text_doc(3);
    out.push(Fixture::new("yapi/klasik-xref", save(&mut doc), opens(3)));

    let mut doc = text_doc(3);
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceStream;
    doc.version = "1.5".into();
    out.push(Fixture::new("yapi/xref-akisi", save(&mut doc), opens(3)));

    // Nesne akışı + xref akışı.
    let mut pdf = RawPdf::new("1.5");
    pdf.stream(11, "", b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET");
    pdf.stream(13, "", b"BT /F1 18 Tf 72 72 Td (Mk 2) Tj ET");
    let page = |c: u32| {
        format!("<</Type/Page/Parent 2 0 R/MediaBox[0 0 595.28 841.89]/Resources<</Font<</F1 3 0 R>>>>/Contents {c} 0 R>>")
    };
    let members = vec![
        (1, "<</Type/Catalog/Pages 2 0 R>>".to_string()),
        (2, "<</Type/Pages/Count 2/Kids[10 0 R 12 0 R]>>".to_string()),
        (
            3,
            "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_string(),
        ),
        (10, page(11)),
        (12, page(13)),
    ];
    pdf.object_stream(20, &members);
    let at = pdf.bytes.len();
    let rows = vec![
        (0, 0, 0, 255),
        (1, 2, 20, 0),
        (2, 2, 20, 1),
        (3, 2, 20, 2),
        (10, 2, 20, 3),
        (11, 1, pdf.offsets[&11] as u32, 0),
        (12, 2, 20, 4),
        (13, 1, pdf.offsets[&13] as u32, 0),
        (20, 1, pdf.offsets[&20] as u32, 0),
        (21, 1, at as u32, 0),
    ];
    pdf.xref_stream(21, 22, &rows, "/Root 1 0 R");
    pdf.startxref(at);
    out.push(Fixture::new("yapi/nesne-akisi", pdf.bytes, opens(2)));

    // Artımlı güncelleme: 1. sayfanın içeriği yeni revizyonda değişir.
    let mut pdf = RawPdf::new("1.7");
    raw_base(&mut pdf, 2, 0);
    let base = pdf.finish_classic("/Root 1 0 R");
    pdf.stream(
        11,
        "",
        b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET BT 72 120 Td (Rev 2) Tj ET",
    );
    pdf.update_table(&[11], &format!("/Size 14/Root 1 0 R/Prev {base}"));
    out.push(Fixture::new("yapi/artimli-guncelleme", pdf.bytes, opens(2)));

    // Çoklu revizyon: 3 güncelleme, sonuncusu sayfa ekler.
    let mut pdf = RawPdf::new("1.7");
    raw_base(&mut pdf, 2, 0);
    let r1 = pdf.finish_classic("/Root 1 0 R");
    pdf.stream(
        11,
        "",
        b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET BT 72 120 Td (Rev 2) Tj ET",
    );
    let r2 = pdf.update_table(&[11], &format!("/Size 14/Root 1 0 R/Prev {r1}"));
    pdf.stream(
        13,
        "",
        b"BT /F1 18 Tf 72 72 Td (Mk 2) Tj ET BT 72 120 Td (Rev 3) Tj ET",
    );
    let r3 = pdf.update_table(&[13], &format!("/Size 14/Root 1 0 R/Prev {r2}"));
    pdf.object(14, b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 595.28 841.89]/Resources<</Font<</F1 3 0 R>>>>/Contents 15 0 R>>");
    pdf.stream(15, "", b"BT /F1 18 Tf 72 72 Td (Mk 3) Tj ET");
    pdf.object(2, b"<</Type/Pages/Count 3/Kids[10 0 R 12 0 R 14 0 R]>>");
    pdf.update_table(&[2, 14, 15], &format!("/Size 16/Root 1 0 R/Prev {r3}"));
    out.push(Fixture::new("yapi/coklu-revizyon", pdf.bytes, opens(3)));

    // Sarkan, ölümcül olmayan başvurular (yapı ağacı, anahat, popup).
    let mut doc = text_doc(2);
    let missing = |d: &mut Document| d.new_object_id();
    let (m1, m2, m3) = (missing(&mut doc), missing(&mut doc), missing(&mut doc));
    let tree = doc.add_object(dictionary! {"Type"=>"StructTreeRoot","IDTree"=>m1});
    let first = doc.get_pages()[&1];
    let note = doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Text",
    "Rect"=>vec![500.into(),780.into(),520.into(),800.into()],"Popup"=>m3});
    doc.get_dictionary_mut(first)
        .unwrap()
        .set("Annots", vec![Object::Reference(note)]);
    let catalog = doc.catalog_mut().unwrap();
    catalog.set("StructTreeRoot", tree);
    catalog.set("Outlines", m2);
    out.push(Fixture::new(
        "yapi/sarkan-olumcul-olmayan",
        save(&mut doc),
        Expect::Opens {
            pages: 2,
            repaired: true,
        },
    ));

    // Sarkan, ölümcül: sayfa içeriği yok.
    let mut doc = text_doc(2);
    let gone = doc.new_object_id();
    let first = doc.get_pages()[&1];
    doc.get_dictionary_mut(first).unwrap().set("Contents", gone);
    out.push(Fixture::new(
        "yapi/sarkan-olumcul",
        save(&mut doc),
        Expect::Rejects,
    ));

    // 40 halkalı başvuru zinciriyle kaynaklar.
    let mut p = Pager::new("1.7");
    let id = p.text_page(1, A4, dictionary! {});
    let font = p.font;
    let mut link = p
        .doc
        .add_object(dictionary! {"Font"=>dictionary!{"F1"=>font}});
    for _ in 0..40 {
        link = p.doc.add_object(Object::Reference(link));
    }
    p.doc.get_dictionary_mut(id).unwrap().set("Resources", link);
    out.push(Fixture::new(
        "yapi/basvuru-zinciri-40",
        save(&mut p.finish(dictionary! {})),
        opens(1),
    ));

    // Görünüm dışı anahtarda döngüsel başvuru nesneleri (A → B → A).
    let mut doc = text_doc(1);
    let (a, b) = (doc.new_object_id(), doc.new_object_id());
    doc.objects.insert(a, Object::Reference(b));
    doc.objects.insert(b, Object::Reference(a));
    doc.catalog_mut().unwrap().set("Metadata", a);
    // lopdf yükte döngüsel başvuruyu çözerken reddeder: güvenli ret (P2 borç —
    // görünüm dışı bir metadata döngüsü belgeyi açtırmıyor).
    out.push(Fixture::new(
        "yapi/dongusel-basvuru-metadata",
        save(&mut doc),
        Expect::Rejects,
    ));

    // Sayfa ağacında döngü: Pages kendini Kids'te listeler.
    let mut doc = text_doc(2);
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let pages = doc.get_pages();
    doc.get_dictionary_mut(root).unwrap().set(
        "Kids",
        vec![pages[&1].into(), root.into(), pages[&2].into()],
    );
    out.push(Fixture::new(
        "yapi/dongusel-kids",
        save(&mut doc),
        Expect::Rejects,
    ));

    // Kökün üstünde /Parent döngüsü.
    let mut doc = text_doc(1);
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let upper = doc.add_object(dictionary! {"Type"=>"Pages","Parent"=>root});
    doc.get_dictionary_mut(root).unwrap().set("Parent", upper);
    out.push(Fixture::new(
        "yapi/kok-ustu-parent-dongusu",
        save(&mut doc),
        Expect::Rejects,
    ));

    // 500 iç içe Form XObject.
    let mut p = Pager::new("1.7");
    let mut inner = p.doc.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),10.into(),10.into()]},
        b"0 0 10 10 re f".to_vec(),
    ));
    for _ in 0..500 {
        inner = p.doc.add_object(Stream::new(
            dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),10.into(),10.into()],
            "Resources"=>dictionary!{"XObject"=>dictionary!{"X"=>inner}}},
            b"/X Do".to_vec(),
        ));
    }
    let id = p.text_page(1, A4, dictionary! {});
    let font = p.font;
    p.doc.get_dictionary_mut(id).unwrap().set(
        "Resources",
        dictionary! {"Font"=>dictionary!{"F1"=>font},"XObject"=>dictionary!{"X"=>inner}},
    );
    out.push(Fixture::new(
        "yapi/derin-form-zinciri-500",
        save(&mut p.finish(dictionary! {})),
        opens(1),
    ));

    // Görünüm dışı anahtarda derin iç içe dizi: ayrıştırıcı özyinelemesi yığını
    // taşırırdı; yapısal tarama sınırın üstünü reddeder (bkz. guard.rs).
    for depth in [50usize, 300, 4096] {
        let mut pdf = RawPdf::new("1.7");
        raw_base(&mut pdf, 1, 0);
        let nested = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        pdf.object(
            1,
            format!("<</Type/Catalog/Pages 2 0 R/PieceInfo<</Derin<</Private {nested}>>>>>>")
                .as_bytes(),
        );
        pdf.finish_classic("/Root 1 0 R");
        out.push(Fixture::new(
            &format!("yapi/derin-ic-ice-dizi-{depth}"),
            pdf.bytes,
            if depth <= 100 {
                opens(1)
            } else {
                Expect::Rejects
            },
        ));
    }

    // Dolaylı /Length başvuru zinciri: her akışın uzunluğu bir sonraki akışa
    // gider; lopdf bunu özyinelemeli çözerdi.
    let mut pdf = RawPdf::new("1.7");
    raw_base(&mut pdf, 1, 0);
    for k in 0..80u32 {
        let id = 1000 + k;
        pdf.object(
            id,
            format!("<</Length {} 0 R>>stream\nabc\nendstream", id + 1).as_bytes(),
        );
    }
    pdf.object(1080, b"3");
    pdf.object(
        1,
        b"<</Type/Catalog/Pages 2 0 R/PieceInfo<</D<</Private 1000 0 R>>>>>>",
    );
    pdf.finish_classic("/Root 1 0 R");
    out.push(Fixture::new(
        "yapi/length-basvuru-zinciri",
        pdf.bytes,
        Expect::Rejects,
    ));

    // 150 düzeyli sayfa ağacı.
    let mut doc = Document::with_version("1.7");
    let font =
        doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let top = doc.new_object_id();
    let mut parent = top;
    let mut chain = vec![top];
    for _ in 0..150 {
        let node = doc.new_object_id();
        chain.push(node);
        let _ = parent;
        parent = node;
    }
    let contents = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET".to_vec(),
    ));
    let leaf = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>*chain.last().unwrap(),
    "MediaBox"=>rect_obj(A4),"Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},"Contents"=>contents});
    for (i, node) in chain.iter().enumerate() {
        let kid = chain.get(i + 1).copied().unwrap_or(leaf);
        let mut d = dictionary! {"Type"=>"Pages","Kids"=>vec![kid.into()],"Count"=>1};
        if i > 0 {
            d.set("Parent", chain[i - 1]);
        }
        doc.objects.insert(*node, d.into());
    }
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>top});
    doc.trailer.set("Root", root);
    out.push(Fixture::new(
        "yapi/derin-sayfa-agaci-150",
        save(&mut doc),
        Expect::Rejects,
    ));

    // Aynı nesne numarası gövdede iki kez; xref ilkini gösterir.
    let mut pdf = RawPdf::new("1.7");
    raw_base(&mut pdf, 2, 0);
    let first_11 = pdf.offsets[&11];
    pdf.stream(11, "", b"BT /F1 18 Tf 72 72 Td (Mk 99) Tj ET");
    pdf.offsets.insert(11, first_11);
    pdf.finish_classic("/Root 1 0 R");
    out.push(Fixture::new(
        "yapi/cift-nesne-numarasi",
        pdf.bytes,
        opens(2),
    ));

    // /Count yanlış.
    let mut doc = text_doc(3);
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(root).unwrap().set("Count", 7);
    out.push(Fixture::new(
        "yapi/yanlis-count",
        save(&mut doc),
        Expect::Rejects,
    ));
    out
}

// ------------------------------------------------------------------ etkileşimli

pub fn interactive() -> Vec<Fixture> {
    let mut out = Vec::new();
    let rect = |y: i64| vec![72.into(), y.into(), 272.into(), (y + 20).into()];

    // Açıklamalar: not + popup, vurgu + görünüm, serbest metin.
    let mut doc = text_doc(3);
    let pages = doc.get_pages();
    let ap = doc.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),200.into(),20.into()]},
        b"1 1 0 rg 0 0 200 20 re f".to_vec(),
    ));
    let popup = doc.new_object_id();
    let note = doc.add_object(
        dictionary! {"Type"=>"Annot","Subtype"=>"Text","Rect"=>rect(780),
        "Contents"=>Object::string_literal("Not metni"),"Popup"=>popup,"P"=>pages[&1]},
    );
    doc.objects.insert(
        popup,
        dictionary! {"Type"=>"Annot","Subtype"=>"Popup","Rect"=>rect(700),"Parent"=>note}.into(),
    );
    let highlight = doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Highlight","Rect"=>rect(600),
    "QuadPoints"=>vec![72.into(),620.into(),272.into(),620.into(),72.into(),600.into(),272.into(),600.into()],
    "AP"=>dictionary!{"N"=>ap}});
    let free = doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"FreeText","Rect"=>rect(500),
    "DA"=>Object::string_literal("/Helv 10 Tf 0 g"),"Contents"=>Object::string_literal("Serbest")});
    doc.get_dictionary_mut(pages[&1]).unwrap().set(
        "Annots",
        vec![note.into(), popup.into(), highlight.into(), free.into()],
    );
    out.push(Fixture::new(
        "etkilesim/aciklamalar",
        save(&mut doc),
        opens(3),
    ));

    // Bağlantılar ve adlandırılmış hedefler, anahat.
    let mut doc = text_doc(4);
    let pages = doc.get_pages();
    let mut links = Vec::new();
    for (i, (key, value)) in [
        ("Dest", Object::Array(vec![pages[&3].into(), "Fit".into()])),
        (
            "A",
            dictionary! {"S"=>"GoTo","D"=>vec![Object::Reference(pages[&4]),"Fit".into()]}.into(),
        ),
        ("Dest", Object::string_literal("bolum-2")),
        ("Dest", Object::Name(b"ek-4".to_vec())),
        (
            "A",
            dictionary! {"S"=>"URI","URI"=>Object::string_literal("https://ornek.test")}.into(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        links.push(Object::Reference(doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link",
        "Rect"=>rect(700 - 30 * i as i64),"Border"=>vec![0.into(),0.into(),0.into()], key=>value})));
    }
    doc.get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", links);
    let (o1, o2) = (doc.new_object_id(), doc.new_object_id());
    let outlines =
        doc.add_object(dictionary! {"Type"=>"Outlines","First"=>o1,"Last"=>o2,"Count"=>2});
    doc.objects.insert(
        o1,
        dictionary! {"Title"=>Object::string_literal("Giriş"),"Parent"=>outlines,
        "Next"=>o2,"Dest"=>vec![Object::Reference(pages[&1]),"Fit".into()]}
        .into(),
    );
    doc.objects.insert(
        o2,
        dictionary! {"Title"=>Object::string_literal("Ek"),"Parent"=>outlines,
        "Prev"=>o1,"Dest"=>vec![Object::Reference(pages[&4]),"Fit".into()]}
        .into(),
    );
    let catalog = doc.catalog_mut().unwrap();
    catalog.set(
        "Names",
        dictionary! {"Dests"=>dictionary!{"Names"=>vec![Object::string_literal("bolum-2"),
        vec![Object::Reference(pages[&2]),"Fit".into()].into()]}},
    );
    catalog.set(
        "Dests",
        dictionary! {"ek-4"=>vec![Object::Reference(pages[&4]),"Fit".into()]},
    );
    catalog.set("Outlines", outlines);
    out.push(Fixture::new(
        "etkilesim/baglantilar-hedefler-anahat",
        save(&mut doc),
        opens(4),
    ));

    // Form: metin, onay kutusu, radyo grubu; boş imza alanı.
    let mut doc = text_doc(2);
    let pages = doc.get_pages();
    let text = doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Tx",
    "T"=>Object::string_literal("ad"),"V"=>Object::string_literal("Örnek Kişi"),"Rect"=>rect(700),"P"=>pages[&1]});
    let check = doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Btn",
    "T"=>Object::string_literal("onay"),"V"=>"Yes","AS"=>"Yes","Rect"=>rect(660),"P"=>pages[&1]});
    let radio = doc.new_object_id();
    let r1 = doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","Parent"=>radio,"AS"=>"Off","Rect"=>rect(620),"P"=>pages[&1]});
    let r2 = doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget","Parent"=>radio,"AS"=>"B","Rect"=>rect(620),"P"=>pages[&2]});
    doc.objects.insert(radio, dictionary! {"FT"=>"Btn","Ff"=>49152,"T"=>Object::string_literal("secim"),"V"=>"B","Kids"=>vec![r1.into(),r2.into()]}.into());
    let sig = doc.add_object(
        dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Sig",
        "T"=>Object::string_literal("imza"),"Rect"=>rect(500),"P"=>pages[&1]},
    );
    doc.get_dictionary_mut(pages[&1]).unwrap().set(
        "Annots",
        vec![text.into(), check.into(), r1.into(), sig.into()],
    );
    doc.get_dictionary_mut(pages[&2])
        .unwrap()
        .set("Annots", vec![r2.into()]);
    let form = doc
        .add_object(dictionary! {"Fields"=>vec![text.into(),check.into(),radio.into(),sig.into()]});
    doc.catalog_mut().unwrap().set("AcroForm", form);
    out.push(Fixture::new(
        "etkilesim/form-ve-bos-imza-alani",
        save(&mut doc),
        opens(2),
    ));

    // İmzalı belge.
    let mut doc = text_doc(2);
    let pages = doc.get_pages();
    let value = doc.add_object(
        dictionary! {"Type"=>"Sig","Filter"=>"Adobe.PPKLite","SubFilter"=>"adbe.pkcs7.detached",
        "ByteRange"=>vec![0.into(),10.into(),20.into(),30.into()],
        "Contents"=>Object::String(vec![0xAB; 64], lopdf::StringFormat::Hexadecimal)},
    );
    let widget = doc.add_object(
        dictionary! {"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Sig",
        "T"=>Object::string_literal("imza"),"V"=>value,"Rect"=>rect(80),"P"=>pages[&1]},
    );
    doc.get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", vec![widget.into()]);
    let form = doc.add_object(dictionary! {"Fields"=>vec![widget.into()],"SigFlags"=>3});
    doc.catalog_mut().unwrap().set("AcroForm", form);
    let mut signed = Fixture::new("etkilesim/imzali", save(&mut doc), opens(2));
    signed.signed = true;
    out.push(signed);

    // Katmanlar: biri gizli.
    let mut p = Pager::new("1.7");
    let visible = p
        .doc
        .add_object(dictionary! {"Type"=>"OCG","Name"=>Object::string_literal("Görünür")});
    let hidden = p
        .doc
        .add_object(dictionary! {"Type"=>"OCG","Name"=>Object::string_literal("Gizli")});
    let font = p.font;
    let contents = p.doc.add_object(Stream::new(
        dictionary! {},
        b"/OC /L0 BDC BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET EMC /OC /L1 BDC 0 0 100 100 re f EMC"
            .to_vec(),
    ));
    let pages_id = p.pages;
    let page = p.doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>rect_obj(A4),
    "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font},"Properties"=>dictionary!{"L0"=>visible,"L1"=>hidden}},
    "Contents"=>contents});
    p.kids.push(page.into());
    let mut doc = p.finish(dictionary! {});
    doc.catalog_mut().unwrap().set(
        "OCProperties",
        dictionary! {"OCGs"=>vec![visible.into(),hidden.into()],"D"=>dictionary!{"OFF"=>vec![hidden.into()]}},
    );
    out.push(Fixture::new(
        "etkilesim/katmanlar",
        save(&mut doc),
        opens(1),
    ));

    // Döngüler: ad ağacı kendini gösterir, anahat /Next döngüsü, eylem /Next döngüsü.
    let mut doc = text_doc(2);
    let pages = doc.get_pages();
    let tree = doc.new_object_id();
    doc.objects.insert(
        tree,
        dictionary! {"Kids"=>vec![Object::Reference(tree), Object::Reference(tree)]}.into(),
    );
    let (o1, o2) = (doc.new_object_id(), doc.new_object_id());
    let outlines = doc.add_object(dictionary! {"Type"=>"Outlines","First"=>o1,"Last"=>o2});
    doc.objects.insert(
        o1,
        dictionary! {"Title"=>Object::string_literal("A"),"Parent"=>outlines,"Next"=>o2}.into(),
    );
    doc.objects.insert(
        o2,
        dictionary! {"Title"=>Object::string_literal("B"),"Parent"=>outlines,"Next"=>o1}.into(),
    );
    let action = doc.new_object_id();
    doc.objects.insert(action, dictionary! {"S"=>"GoTo","D"=>vec![Object::Reference(pages[&2]),"Fit".into()],"Next"=>action}.into());
    let link = doc.add_object(
        dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>rect(700),"A"=>action,
        "Dest"=>Object::string_literal("olmayan-ad")},
    );
    doc.get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", vec![link.into()]);
    let catalog = doc.catalog_mut().unwrap();
    catalog.set("Names", dictionary! {"Dests"=>tree});
    catalog.set("Outlines", outlines);
    out.push(Fixture::new(
        "etkilesim/donguler-ad-agaci-anahat-eylem",
        save(&mut doc),
        opens(2),
    ));
    out
}

// ------------------------------------------------------------------ hatalı

pub fn malformed() -> Vec<Fixture> {
    let mut out = Vec::new();
    let valid = save(&mut text_doc(3));

    out.push(Fixture::new(
        "hatali/kesik-yarida",
        valid[..valid.len() / 2].to_vec(),
        Expect::Rejects,
    ));
    // Son 12 bayt (startxref ofseti + %%EOF) kesik: normalleştirme kurtaramaz,
    // güvenle reddedilir (P2 borç — nesne tarayan yeniden kurulum kapalı).
    out.push(Fixture::new(
        "hatali/kesik-sonda",
        valid[..valid.len() - 12].to_vec(),
        Expect::Rejects,
    ));

    let mut pdf = RawPdf::new("1.7");
    raw_base(&mut pdf, 2, 0);
    for off in pdf.offsets.values_mut() {
        *off += 7;
    }
    pdf.finish_classic("/Root 1 0 R");
    // Ofsetler kaymış: normalleştirme yalnız satır sonlarını düzeltir, yanlış
    // ofset değerlerini değil. Güvenli ret.
    out.push(Fixture::new(
        "hatali/bozuk-xref-ofsetleri",
        pdf.bytes,
        Expect::Rejects,
    ));

    let mut pdf = RawPdf::new("1.7");
    raw_base(&mut pdf, 2, 0);
    pdf.finish_classic("");
    out.push(Fixture::new(
        "hatali/trailer-root-yok",
        pdf.bytes,
        Expect::Rejects,
    ));

    let mut pdf = RawPdf::new("1.7");
    raw_base(&mut pdf, 1, 0);
    pdf.object(5, b"42");
    pdf.finish_classic("/Root 5 0 R");
    out.push(Fixture::new(
        "hatali/root-sozluk-degil",
        pdf.bytes,
        Expect::Rejects,
    ));

    let mut pdf = RawPdf::new("1.7");
    raw_base(&mut pdf, 1, 0);
    pdf.offsets.remove(&2);
    pdf.finish_classic("/Root 1 0 R");
    out.push(Fixture::new(
        "hatali/pages-eksik",
        pdf.bytes,
        Expect::Rejects,
    ));

    let mut pdf = RawPdf::new("1.7");
    raw_base(&mut pdf, 2, 0);
    pdf.stream(11, "/Filter/FlateDecode", &super::corpus::noise(200, 5));
    pdf.finish_classic("/Root 1 0 R");
    out.push(Fixture::new(
        "hatali/bozuk-flate-icerik",
        pdf.bytes,
        Expect::Rejects,
    ));

    // Yanlış /Length akış sınırını kaydırır; sonraki nesneler ayrıştırılamaz,
    // belge güvenle reddedilir.
    for (name, delta) in [("hatali/length-uzun", 50i64), ("hatali/length-kisa", -9)] {
        let mut pdf = RawPdf::new("1.7");
        raw_base(&mut pdf, 2, 0);
        let data = b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET";
        pdf.stream_with_length(11, "", data, (data.len() as i64 + delta) as usize);
        pdf.finish_classic("/Root 1 0 R");
        out.push(Fixture::new(name, pdf.bytes, Expect::Rejects));
    }

    out.push(Fixture::new(
        "hatali/metin-pdf-adiyla",
        "Merhaba dünya, bu bir PDF değil.\n".repeat(20).into_bytes(),
        Expect::Rejects,
    ));
    out.push(Fixture::new(
        "hatali/bos-dosya",
        Vec::new(),
        Expect::Rejects,
    ));
    out.push(Fixture::new(
        "hatali/yalniz-baslik",
        b"%PDF-1.7\n".to_vec(),
        Expect::Rejects,
    ));

    let mut junk = noise(300, 9);
    junk.extend_from_slice(&valid);
    // Baştaki çöp bütün mutlak ofsetleri kaydırır; nesne tarayan yeniden kurulum
    // kapalı olduğu için güvenle reddedilir (P2 borç).
    out.push(Fixture::new("hatali/basinda-cop", junk, Expect::Rejects));
    let mut tail = valid.clone();
    tail.extend_from_slice(&noise(500, 13));
    out.push(Fixture::new(
        "hatali/eof-sonrasi-cop",
        tail,
        Expect::Opens {
            pages: 3,
            repaired: false,
        },
    ));

    let mut doc = text_doc(1);
    doc.trailer.set(
        "Encrypt",
        dictionary! {"Filter"=>"Standard","V"=>1,"R"=>2,"O"=>Object::String(vec![1;32], lopdf::StringFormat::Hexadecimal),
        "U"=>Object::String(vec![2;32], lopdf::StringFormat::Hexadecimal),"P"=>-4},
    );
    doc.trailer.set(
        "ID",
        vec![
            Object::String(vec![3; 16], lopdf::StringFormat::Hexadecimal),
            Object::String(vec![3; 16], lopdf::StringFormat::Hexadecimal),
        ],
    );
    out.push(Fixture::new(
        "hatali/sifreli",
        save(&mut doc),
        Expect::Rejects,
    ));

    let mut doc = text_doc(2);
    doc.version = "2.0".into();
    out.push(Fixture::new(
        "hatali/pdf-2-0-basligi",
        save(&mut doc),
        opens(2),
    ));
    out
}

pub fn all() -> Vec<Fixture> {
    let mut v = simple();
    v.extend(geometry());
    v.extend(resources());
    v.extend(structural());
    v.extend(interactive());
    v.extend(malformed());
    v
}

/// Boyut ailesi: `megabytes` civarında, sıkıştırılamaz görselli sayfalar.
pub fn sized(megabytes: usize, pages: u32) -> Vec<u8> {
    let mut p = Pager::new("1.7");
    let per_page = megabytes * 1024 * 1024 / pages as usize;
    let side = ((per_page / 3) as f64).sqrt().max(8.0) as u32;
    for i in 1..=pages {
        let img = p.doc.add_object(image(side, side, 7000 + i, false));
        p.image_page(i, img);
    }
    save(&mut p.finish(dictionary! {}))
}

#[allow(unused)]
fn _zlib_used() -> Vec<u8> {
    zlib(b"")
}
