//! Sahip parolalı (owner-password-only) PDF regresyonu.
//!
//! SAHA BULGUSU: Gerçek bir Findeks kredi/risk raporu Düzenle'de açılamadı;
//! kullanıcıya "Dosya bozulmuş olabilir" dendi. Dosya sağlamdı — yalnız
//! ŞİFRELİYDİ: sahip parolası konmuş, kullanıcı parolası BOŞ bırakılmıştı.
//! Banka, Findeks, icra ve mahkeme çıktılarının çoğu böyle üretilir ve her
//! görüntüleyici (Preview dahil) bunları sormadan açar. `validate_document`
//! ise `/Encrypt` gören her belgeyi koşulsuz reddediyordu.
//!
//! Fixture SENTETİKTİR: gerçek belge repoya girmez. Burada standart güvenlik
//! handler'ı (RC4, V=2, R=3, 128 bit) elle kurulur; gerçek dosyadaki
//! yapının aynısı.
use lopdf::{dictionary, Document, Object, Stream};
use md5::{Digest, Md5};

/// PDF spec, Algorithm 2 — 32 baytlık standart dolgu dizesi.
const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut j = 0u8;
    for i in 0..256 {
        j = j
            .wrapping_add(s[i])
            .wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter()
        .map(|b| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            b ^ s[(s[i as usize].wrapping_add(s[j as usize])) as usize]
        })
        .collect()
}

/// Algorithm 2: boş kullanıcı parolasından şifreleme anahtarı.
fn encryption_key(owner: &[u8; 32], permissions: i32, id0: &[u8], key_len: usize) -> Vec<u8> {
    let mut h = Md5::new();
    h.update(PAD); // boş kullanıcı parolası -> yalnız dolgu
    h.update(owner);
    h.update(permissions.to_le_bytes());
    h.update(id0);
    let mut digest = h.finalize().to_vec();
    // R >= 3: 50 kez yeniden özetle.
    for _ in 0..50 {
        digest = Md5::digest(&digest[..key_len]).to_vec();
    }
    digest[..key_len].to_vec()
}

/// Algorithm 5: R >= 3 için `/U` değeri.
fn user_entry(key: &[u8], id0: &[u8]) -> Vec<u8> {
    let mut h = Md5::new();
    h.update(PAD);
    h.update(id0);
    let mut x = rc4(key, &h.finalize());
    for i in 1u8..=19 {
        let k: Vec<u8> = key.iter().map(|b| b ^ i).collect();
        x = rc4(&k, &x);
    }
    x.extend_from_slice(&[0u8; 16]); // 32 bayta tamamla
    x
}

/// Nesne başına anahtar: MD5(anahtar + nesne no + üretim no).
fn object_key(key: &[u8], id: (u32, u16)) -> Vec<u8> {
    let mut h = Md5::new();
    h.update(key);
    h.update(&id.0.to_le_bytes()[..3]);
    h.update(&id.1.to_le_bytes()[..2]);
    let n = (key.len() + 5).min(16);
    h.finalize()[..n].to_vec()
}

/// Sahip parolası konmuş, kullanıcı parolası BOŞ bir PDF üretir.
fn owner_protected_pdf(pages: usize) -> Vec<u8> {
    build(pages, true)
}

/// `user_password_empty = false` ise `/U` kasten tutarsız yazılır: belge
/// GERÇEK bir kullanıcı parolası istiyor demektir.
fn build(pages: usize, user_password_empty: bool) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for i in 0..pages {
        let text = format!("BT /F1 18 Tf 72 700 Td (Sentetik sayfa {}) Tj ET", i + 1);
        let content_id = doc.add_object(Stream::new(dictionary! {}, text.into_bytes()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Count" => pages as i64, "Kids" => kids,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", Object::Reference(catalog_id));

    // Gerçek dosyadakiyle aynı profil: V=2, R=3, 128 bit, kopyalama kısıtlı.
    let id0: Vec<u8> = (0u8..16).collect();
    let owner: [u8; 32] = std::array::from_fn(|i| (200 - i) as u8);
    let permissions: i32 = -3900;
    let key = encryption_key(&owner, permissions, &id0, 16);
    let mut u = user_entry(&key, &id0);
    if !user_password_empty {
        // `/U` artık boş parolayla türetilen değeri tutmuyor.
        u[0] ^= 0xFF;
        u[1] ^= 0xFF;
    }

    let encrypt_id = doc.add_object(dictionary! {
        "Filter" => "Standard",
        "V" => 2i64, "R" => 3i64, "Length" => 128i64,
        "P" => permissions as i64,
        "O" => Object::String(owner.to_vec(), lopdf::StringFormat::Literal),
        "U" => Object::String(u, lopdf::StringFormat::Literal),
    });

    // Şifreleme sözlüğü DIŞINDAKİ bütün akış ve dizeler şifrelenir.
    let ids: Vec<_> = doc.objects.keys().copied().filter(|i| *i != encrypt_id).collect();
    for id in ids {
        let k = object_key(&key, id);
        match doc.objects.get_mut(&id) {
            Some(Object::Stream(s)) => {
                let enc = rc4(&k, &s.content);
                s.set_content(enc);
            }
            Some(Object::String(c, _)) => *c = rc4(&k, c),
            _ => {}
        }
    }

    doc.trailer.set("Encrypt", Object::Reference(encrypt_id));
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(id0.clone(), lopdf::StringFormat::Hexadecimal),
            Object::String(id0, lopdf::StringFormat::Hexadecimal),
        ]),
    );

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("kaydet");
    out
}

#[test]
fn owner_protected_pdf_opens_like_every_other_viewer() {
    let bytes = owner_protected_pdf(3);
    assert!(
        bytes.windows(8).any(|w| w == b"/Encrypt"),
        "fixture şifreli değil; test bir şey kanıtlamıyor"
    );
    let loaded = pdf_core::pdf::load_pdf_tolerant(&bytes, "findeks.pdf")
        .expect("sahip parolalı PDF açılmalı: kullanıcı parolası boş");
    assert_eq!(loaded.document.get_pages().len(), 3);
}

#[test]
fn decrypted_content_is_really_readable() {
    // Açılmak yetmez: içerik gerçekten ÇÖZÜLMÜŞ olmalı, yoksa sayfalar
    // görünür ama çizim akışı çöp olur.
    let bytes = owner_protected_pdf(1);
    let loaded = pdf_core::pdf::load_pdf_tolerant(&bytes, "findeks.pdf").expect("açılmalı");
    let page = *loaded.document.get_pages().values().next().unwrap();
    let content = loaded.document.get_page_content(page).expect("içerik");
    assert!(
        String::from_utf8_lossy(&content).contains("Sentetik sayfa 1"),
        "içerik çözülmemiş: şifreli baytlar okunuyor"
    );
}

#[test]
fn a_real_user_password_is_still_refused() {
    // Sahip parolasını açmak ile GERÇEK bir kullanıcı parolasını kırmak ayrı
    // şeylerdir. İkincisi yapılmaz: `/U` bozulunca boş parola tutmaz.
    let bytes = build(1, false);
    let err = pdf_core::pdf::load_pdf_tolerant(&bytes, "parolali.pdf")
        .expect_err("parola isteyen belge açılmamalı");
    let message = err.to_string();
    assert!(
        message.contains("parola"),
        "sebep parola olarak söylenmeli, oldu: {message}"
    );
    assert!(
        !message.contains("bozul"),
        "parola korumasına 'bozuk dosya' denmemeli: {message}"
    );
}
