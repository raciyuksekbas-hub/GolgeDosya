use ekler_core::load_pdf_tolerant;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document as LopdfDoc, Stream};

fn create_clean_sample_pdf(pages_cnt: usize) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.4");
    doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();

    for _ in 0..pages_cnt {
        let content = Content {
            operations: vec![Operation::new("q", vec![]), Operation::new("Q", vec![])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }

    doc.set_object(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => pages_cnt as i64,
        },
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

#[test]
fn test_synthetic_reproduction_of_irregular_xref_eol() {
    // 1. Düzgün bir 2 sayfalık sentetik PDF üret
    let clean_bytes = create_clean_sample_pdf(2);
    assert!(
        lopdf::Document::load_mem(&clean_bytes).is_ok(),
        "Temiz PDF lopdf tarafından açılabilmeli"
    );

    // 2. Marvell Semiconductor benzeri 'f\n' / 'n\n' (boşluksuz 19 bayt) hatasını sentetik olarak üret
    // Xref tablosundaki " \n" veya "\r\n" yerine tek "\n" koyarak lopdf nom_parser'ın reddetmesini sağla
    let xref_pos = clean_bytes.windows(5).position(|w| w == b"\nxref").unwrap() + 1;

    let mut irregular_xref = String::new();
    let xref_slice_str = std::str::from_utf8(&clean_bytes[xref_pos..]).unwrap();
    for line in xref_slice_str.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with(" f") || trimmed.ends_with(" n") {
            // Boşluksuz tek newline (18 bayt + \n = 19 bayt): nom_parser bunu kabul etmez!
            irregular_xref.push_str(trimmed);
            irregular_xref.push('\n');
        } else {
            irregular_xref.push_str(line);
            irregular_xref.push('\n');
        }
    }

    let mut synthetic_corrupt = clean_bytes[..xref_pos].to_vec();
    synthetic_corrupt.extend_from_slice(irregular_xref.as_bytes());

    // Doğrulama: Standart lopdf load_mem bu sentetik dosyayı Err(Trailer) ile reddetmelidir!
    let strict_result = lopdf::Document::load_mem(&synthetic_corrupt);
    assert!(
        strict_result.is_err(),
        "Standart lopdf parser bu sentetik 19-baytlık xref'i reddetmelidir"
    );

    // Doğrulama: Çok katmanlı toleranslı yükleyicimiz (DüzenEk Tier 2) bunu kurtarmalıdır!
    let norm_res =
        ekler_core::pdf::tolerant::load_pdf_tolerant(&synthetic_corrupt, "sentetik_hatali.pdf");
    println!(
        "Norm res: {:?}",
        norm_res.as_ref().map(|r| (&r.is_repaired, &r.repair_note))
    );
    let tolerant_res =
        norm_res.expect("Toleranslı motor sentetik xref eol hatasını düzeltip yüklemelidir");

    assert_eq!(
        tolerant_res.strategy,
        ekler_core::pdf::tolerant::RepairStrategy::XrefNormalization
    );
    assert!(
        tolerant_res.is_repaired,
        "Dosya onarılmış olarak işaretlenmelidir"
    );
    assert_eq!(
        tolerant_res.document.get_pages().len(),
        2,
        "Sayfa sayısı tam olarak 2 olmalıdır"
    );
    assert!(tolerant_res.repair_note.is_some());
}

#[test]
fn test_synthetic_trailing_garbage_and_broken_startxref() {
    let clean_bytes = create_clean_sample_pdf(1);

    // Dosyanın sonuna 500 baytlık sunucu/ağ çöpü ekle
    let mut garbage_bytes = clean_bytes.clone();
    garbage_bytes
        .extend_from_slice(b"\n<!-- HTML Error or Web Server Diagnostic Trailing Garbage -->\n");
    for i in 0..300 {
        garbage_bytes.push((i % 256) as u8);
    }

    // Toleranslı motor %%EOF sonrasındaki çöpü temizleyip açmalıdır
    let tolerant_res = load_pdf_tolerant(&garbage_bytes, "cop_ekli.pdf")
        .expect("Toleranslı motor %%EOF arkasındaki çöp baytları temizlemelidir");

    assert_eq!(tolerant_res.document.get_pages().len(), 1);
}

#[test]
fn test_missing_xref_is_rejected() {
    let clean_bytes = create_clean_sample_pdf(3);

    // xref tablosunu tamamen uçur
    let xref_pos = clean_bytes.windows(5).position(|w| w == b"\nxref").unwrap() + 1;
    let mut no_xref_bytes = clean_bytes[..xref_pos].to_vec();
    no_xref_bytes.extend_from_slice(b"\n%%EOF\n");

    // Standart parser kesinlikle açamaz
    assert!(lopdf::Document::load_mem(&no_xref_bytes).is_err());

    assert!(
        load_pdf_tolerant(&no_xref_bytes, "eksik_xref.pdf").is_err(),
        "Unknown missing indexes must not be reconstructed heuristically"
    );
}

#[test]
#[ignore = "External private fixture; run explicitly on the fixture owner machine"]
fn test_real_user_file_read_only_immutability_and_success() {
    let path =
        std::path::Path::new("/Users/racicetinyuksekbas/Downloads/hakan almalı ihtarname.pdf");
    assert!(path.exists(), "Required external fixture is missing");

    let original_bytes = std::fs::read(path).unwrap();
    let original_hash = ekler_core::calculate_sha256(path).unwrap();

    // Tolerant yükleme yap
    let tolerant_res = load_pdf_tolerant(&original_bytes, "hakan almalı ihtarname.pdf")
        .expect("Gerçek kullanıcı PDF'i başarıyla yüklenmelidir");

    assert_eq!(
        tolerant_res.document.get_pages().len(),
        1,
        "Gerçek dosya 1 sayfa olmalıdır"
    );
    assert!(
        tolerant_res.is_repaired,
        "Standart dışı xref nedeniyle onarılmış olmalıdır"
    );

    // KRİTİK İNVARYANT: Orijinal kaynak dosya diskte 1 bayt dahi değişmemiş olmalıdır!
    let post_read_bytes = std::fs::read(path).unwrap();
    let post_read_hash = ekler_core::calculate_sha256(path).unwrap();

    assert_eq!(
        original_bytes.len(),
        post_read_bytes.len(),
        "Dosya boyutu değişmemelidir"
    );
    assert_eq!(
        original_hash, post_read_hash,
        "Dosya SHA-256 hash'i 100% korunmalıdır"
    );
}

#[test]
fn missing_content_reference_is_rejected_even_with_valid_xref() {
    let bytes = create_clean_sample_pdf(1);
    let mut doc = lopdf::Document::load_mem(&bytes).unwrap();
    let id = *doc.get_pages().values().next().unwrap();
    doc.get_dictionary_mut(id)
        .unwrap()
        .set("Contents", lopdf::Object::Reference((999, 0)));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert!(load_pdf_tolerant(&bytes, "missing-content.pdf").is_err());
}
#[test]
fn broken_page_count_is_not_repaired() {
    let mut doc = lopdf::Document::load_mem(&create_clean_sample_pdf(1)).unwrap();
    let id = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(id).unwrap().set("Count", 2);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert!(load_pdf_tolerant(&bytes, "wrong-count.pdf").is_err());
}
#[test]
fn corrupt_flate_content_cannot_be_accepted_as_blank_repair() {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let content = doc.add_object(Stream::new(
        dictionary! {"Filter"=>"FlateDecode"},
        b"broken compressed bytes".to_vec(),
    ));
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()],"Contents"=>content});
    doc.set_object(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![Object::Reference(page)],"Count"=>1},
    );
    let cat = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", cat);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert!(ekler_core::load_pdf_tolerant(&bytes, "bad-flate.pdf").is_err());
}
