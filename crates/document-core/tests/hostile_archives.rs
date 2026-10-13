//! Düşmanca arşivler: boyutu hakkında yalan söyleyen girdiler ve aynı metni
//! yüzlerce kez okutan UDF başvuruları.
//!
//! Denetim bulgusu (OR-02, OR-03): merkezi dizindeki boyut saldırganın
//! elindedir. 100 bayt beyan edip 120 MiB'a açılan 16 üst bilgi parçası
//! 1,9 MB'lık bir DOCX'ten 2,3 GB bellek tüketti ve dönüştürme "başarılı"
//! bitti; 1,5 KB'lık bir UDF, 1 milyon karakterlik tamponu 300 paragrafta
//! yeniden okutarak 1,5 GB'a çıktı. Beklenen: sınır GERÇEK baytlara uygulanır,
//! yalan söyleyen girdi okunmaya devam edilmeden reddedilir (UNSAFE_ARCHIVE),
//! başvurulmayan parçalar hiç açılmaz.
//!
//! Bütün arşivler bu dosyada sentetik olarak kurulur; gerçek dava dosyası yok.

use std::io::Write;

use tavzih_core::convert::{convert_bytes, Direction};
use tavzih_core::warnings::WarningSink;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_HEADER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";

fn le16(b: &[u8], at: usize) -> usize {
    u16::from_le_bytes([b[at], b[at + 1]]) as usize
}

fn le32(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
}

/// Arşivi `entries` ile kur (deflate). Gerçek boyutlar doğru yazılır.
fn zip_of(entries: &[(&str, Vec<u8>)], method: zip::CompressionMethod) -> Vec<u8> {
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let o: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(method);
        for (name, bytes) in entries {
            z.start_file(*name, o).unwrap();
            z.write_all(bytes).unwrap();
        }
        z.finish().unwrap();
    }
    cur.into_inner()
}

/// Bir girdinin açılmış boyutunu hem yerel başlıkta hem merkezi dizinde
/// `declared` olarak yeniden yaz: saldırganın yaptığı tam olarak budur.
/// Sıkıştırılmış akış ve CRC olduğu gibi kalır.
fn lie_about_size(mut zip: Vec<u8>, entry: &str, declared: u32) -> Vec<u8> {
    let eocd = zip
        .windows(4)
        .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
        .unwrap();
    let count = le16(&zip, eocd + 10);
    let mut p = le32(&zip, eocd + 16);
    let mut found = false;
    for _ in 0..count {
        assert_eq!(&zip[p..p + 4], &[0x50, 0x4b, 0x01, 0x02]);
        let (nlen, elen, clen) = (le16(&zip, p + 28), le16(&zip, p + 30), le16(&zip, p + 32));
        let local = le32(&zip, p + 42);
        if &zip[p + 46..p + 46 + nlen] == entry.as_bytes() {
            zip[p + 24..p + 28].copy_from_slice(&declared.to_le_bytes());
            zip[local + 22..local + 26].copy_from_slice(&declared.to_le_bytes());
            found = true;
        }
        p += 46 + nlen + elen + clen;
    }
    assert!(found, "{entry} arşivde yok");
    zip
}

/// Saklanan (sıkıştırılmamış) bir girdinin verisinde bir baytı boz: CRC tutmaz.
fn corrupt_stored(mut zip: Vec<u8>, entry: &str) -> Vec<u8> {
    let at = zip
        .windows(entry.len())
        .position(|w| w == entry.as_bytes())
        .unwrap();
    // Yerel başlık: ad 30. bayttan başlar; veri adın ve ek alanın hemen ardındadır.
    let header = at - 30;
    let data = at + entry.len() + le16(&zip, header + 28);
    zip[data] ^= 0x20;
    zip
}

fn document_with_header() -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}" xmlns:r="{R}"><w:body><w:p><w:r><w:t>Gövde</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH1"/></w:sectPr></w:body></w:document>"#
    )
    .into_bytes()
}

fn rels_with_header() -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH1" Type="{REL_HEADER}" Target="header1.xml"/></Relationships>"#
    )
    .into_bytes()
}

/// Geçerli bir üst bilgi; kökün içinde `padding` bayt boşluk taşır.
fn header_xml(padding: usize) -> Vec<u8> {
    let mut v =
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><w:hdr xmlns:w="{W}">"#).into_bytes();
    v.extend(std::iter::repeat(b' ').take(padding));
    v.extend_from_slice(b"<w:p><w:r><w:t>\xC3\x9Cst bilgi</w:t></w:r></w:p></w:hdr>");
    v
}

fn to_udf(docx: &[u8]) -> Result<Vec<u8>, String> {
    let mut w = WarningSink::new();
    convert_bytes(docx, Direction::DocxToUdf, &mut w).map_err(|e| e.code.as_str().to_string())
}

fn to_docx(udf: &[u8]) -> Result<Vec<u8>, String> {
    let mut w = WarningSink::new();
    convert_bytes(udf, Direction::UdfToDocx, &mut w).map_err(|e| e.code.as_str().to_string())
}

/// Sonucun kodu: başarıda "OK" (çıktı baytlarını test çıktısına dökmemek için).
fn code(r: Result<Vec<u8>, String>) -> String {
    r.map(|_| "OK".to_string()).unwrap_or_else(|c| c)
}

const UDF_TAIL: &str = r#"<properties><pageFormat mediaSizeName="1" leftMargin="70.0" rightMargin="70.0" topMargin="70.0" bottomMargin="70.0" paperOrientation="1" headerFOffset="20.0" footerFOffset="20.0" /></properties>"#;
const UDF_STYLES: &str =
    r#"<styles><style name="hvl-default" family="Dialog" size="12" /></styles></template>"#;

fn udf_xml(buffer: &str, elements: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?><template format_id="1.8"><content><![CDATA[{buffer}]]></content>{UDF_TAIL}<elements resolver="hvl-default">{elements}</elements>{UDF_STYLES}"#
    )
}

// ------------------------------------------------------------ DOCX: yalan boyut

#[test]
fn a_header_that_inflates_past_its_declared_size_is_refused() {
    // 100 bayt beyan, 8 MiB gerçek: okuma beyan edilen boyutun bir bayt
    // ötesinde kesilir ve arşiv reddedilir. Eskiden 8 MiB'ın tamamı açılıyor,
    // dönüştürme uyarısız "başarılı" bitiyordu.
    let zip = zip_of(
        &[
            ("word/document.xml", document_with_header()),
            ("word/_rels/document.xml.rels", rels_with_header()),
            ("word/header1.xml", header_xml(8 * 1024 * 1024)),
        ],
        zip::CompressionMethod::Deflated,
    );
    let bomb = lie_about_size(zip, "word/header1.xml", 100);
    assert!(bomb.len() < 100_000, "bomba küçük olmalı: {}", bomb.len());
    assert_eq!(code(to_udf(&bomb)), "UNSAFE_ARCHIVE");
}

#[test]
fn an_honest_header_still_converts() {
    // Aynı paket, boyut yalanı olmadan: dönüştürme başarılı ve üst bilgi
    // UDF'e taşınmış olmalı (sınır meşru belgeyi düşürmez).
    let zip = zip_of(
        &[
            ("word/document.xml", document_with_header()),
            ("word/_rels/document.xml.rels", rels_with_header()),
            ("word/header1.xml", header_xml(64)),
        ],
        zip::CompressionMethod::Deflated,
    );
    let udf = to_udf(&zip).expect("dürüst paket dönüşmeli");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(udf)).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut z.by_name("content.xml").unwrap(), &mut s).unwrap();
    assert!(s.contains("Üst bilgi"), "üst bilgi metni kayboldu");
    assert!(s.contains("<header"), "üst bilgi öğesi yok");
}

#[test]
fn header_parts_the_document_never_references_are_not_opened() {
    // Altı başvurusuz "üst bilgi" parçası: hiçbiri belgenin ilişkilerinde yok.
    // Biri bozuk (CRC tutmuyor). Okuyucu onları açmaya bile kalkmamalı;
    // eskiden hepsi belleğe açılıyor, bozuk olan da bütün belgeyi
    // "geçerli bir Word belgesi değil" diye düşürüyordu.
    let mut entries = vec![
        ("word/document.xml", document_with_header()),
        ("word/_rels/document.xml.rels", rels_with_header()),
        ("word/header1.xml", header_xml(16)),
    ];
    let names: Vec<String> = (10..16).map(|i| format!("word/header{i}.xml")).collect();
    for n in &names {
        entries.push((n.as_str(), header_xml(16)));
    }
    let zip = zip_of(&entries, zip::CompressionMethod::Stored);
    let zip = corrupt_stored(zip, "word/header13.xml");
    assert_eq!(
        code(to_udf(&zip)),
        "OK",
        "başvurusuz parçalar dönüştürmeyi etkilememeli"
    );
}

// ------------------------------------------------------------ UDF: yalan boyut

#[test]
fn a_udf_content_that_inflates_past_its_declared_size_is_refused() {
    let mut xml = udf_xml(
        "Dilekçe\n",
        r#"<paragraph><content startOffset="0" length="8"/></paragraph>"#,
    );
    // Kök kapandıktan sonra 4 MiB boşluk: XML hâlâ geçerli, ama beyan 200 bayt.
    xml.push_str(&" ".repeat(4 * 1024 * 1024));
    let zip = zip_of(
        &[("content.xml", xml.into_bytes())],
        zip::CompressionMethod::Deflated,
    );
    let bomb = lie_about_size(zip, "content.xml", 200);
    assert_eq!(code(to_docx(&bomb)), "UNSAFE_ARCHIVE");
}

#[test]
fn an_entry_that_only_slightly_overruns_its_declaration_is_damaged_not_hostile() {
    // Denetimin crc.udf'i: deflate akışındaki tek bir bayt çıktıyı beyandan
    // biraz uzun yapıyor. Bu bir bomba değil, bozuk dosyadır: kullanıcıya
    // "güvenlik sınırını aşıyor" değil, biçimin "bozulmuş" cümlesi söylenir.
    let xml = udf_xml(
        "Dilekçe\n",
        r#"<paragraph><content startOffset="0" length="8"/></paragraph>"#,
    );
    let n = xml.len() as u32;
    let zip = zip_of(
        &[("content.xml", xml.into_bytes())],
        zip::CompressionMethod::Deflated,
    );
    assert_eq!(
        code(to_docx(&lie_about_size(zip, "content.xml", n - 10))),
        "INVALID_UDF"
    );

    let doc = document_with_header();
    let n = doc.len() as u32;
    let zip = zip_of(
        &[("word/document.xml", doc)],
        zip::CompressionMethod::Deflated,
    );
    assert_eq!(
        code(to_udf(&lie_about_size(zip, "word/document.xml", n - 10))),
        "INVALID_DOCX"
    );
}

// ------------------------------------------------------------ UDF: başvuru çoğaltma

#[test]
fn reading_one_buffer_many_times_over_is_refused() {
    // 300 000 karakterlik tek tampon, 20 paragrafta baştan sona yeniden
    // okutuluyor: 6 milyon karakter, birkaç KB'lık bir dosyadan. Gerçek
    // UYAP belgeleri tamponun her parçasına bir kez başvurur (gerçek
    // derlemde en yüksek oran 1,0).
    let n = 300_000;
    let mut elements = String::new();
    for _ in 0..20 {
        elements.push_str(&format!(
            r#"<paragraph><content startOffset="0" length="{}"/></paragraph>"#,
            n + 1
        ));
    }
    let xml = udf_xml(&format!("{}\n", "a".repeat(n)), &elements);
    let zip = zip_of(
        &[("content.xml", xml.into_bytes())],
        zip::CompressionMethod::Deflated,
    );
    assert!(zip.len() < 8 * 1024, "girdi küçük olmalı: {}", zip.len());
    assert_eq!(code(to_docx(&zip)), "UNSAFE_ARCHIVE");
}

#[test]
fn a_large_buffer_read_once_is_not_mistaken_for_amplification() {
    // Sınır meşru büyük belgeyi düşürmemeli: 2 milyon karakter, 2000
    // paragrafa bölünmüş, her parça bir kez okunuyor. Metin gerçek bir
    // dilekçe gibi az sıkışır (sabit bir harf dizisi sıkıştırma oranı
    // denetimine takılırdı; o ayrı bir koruma).
    let letters: Vec<char> = "abcçdefgğhıijklmnoöprsştuüvyz ".chars().collect();
    let mut seed: u32 = 0x2026_1013;
    let mut buffer = String::new();
    let mut elements = String::new();
    for i in 0..2000 {
        for _ in 0..999 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            buffer.push(letters[(seed >> 24) as usize % letters.len()]);
        }
        buffer.push('\n');
        elements.push_str(&format!(
            r#"<paragraph><content startOffset="{}" length="1000"/></paragraph>"#,
            i * 1000
        ));
    }
    let xml = udf_xml(&buffer, &elements);
    let zip = zip_of(
        &[("content.xml", xml.into_bytes())],
        zip::CompressionMethod::Deflated,
    );
    assert_eq!(
        code(to_docx(&zip)),
        "OK",
        "tek okunan büyük tampon dönüşmeli"
    );
}
