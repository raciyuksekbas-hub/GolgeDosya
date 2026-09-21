//! "Bu belge daha önce GölgeDosya'dan çıkmış mı?" — yalnız GölgeDosya'nın
//! kendi marka çizimine bayt bayt bakılarak.
//!
//! Sıkıştırma "kazanç yok" dediğinde kullanıcıya "daha önce işlenmiş" bağlamı
//! verilir. Yanlış pozitif, kullanıcıya belgesi hakkında yalan söylemek olur;
//! sahada 56 yabancı PDF'te sıfır yanlış pozitif ölçüldü, bu testler de
//! kuralın kendisini sabitler.

use lopdf::{dictionary, Document, Stream};
use pdf_core::pdf::stamp::{apply_branding, is_golgedosya_output};

fn plain() -> Document {
    let mut d = Document::with_version("1.7");
    let pages = d.new_object_id();
    let contents = d.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
    let page = d.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,
    "MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()],"Contents"=>contents});
    d.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let root = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer.set("Root", root);
    d
}

/// Belgeye, içeriği verilen çizim olan bir Form XObject ekler — eski
/// sürümlerin marka işaretini taşıdığı biçim.
fn with_form(logo: &[u8]) -> Document {
    let mut d = plain();
    d.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),290.into(),72.into()]},
        logo.to_vec(),
    ));
    d
}

#[test]
fn a_foreign_pdf_is_not_a_golgedosya_output() {
    assert!(!is_golgedosya_output(&plain()));
}

#[test]
fn the_canonical_brand_is_recognised() {
    let mut d = plain();
    apply_branding(&mut d).unwrap();
    assert!(is_golgedosya_output(&d));
    // Kaydedilip yeniden açılmış hâli de: tanıma baytlara bakar, bellekteki
    // nesne kimliğine değil.
    let mut bytes = Vec::new();
    d.save_to(&mut bytes).unwrap();
    assert!(is_golgedosya_output(&Document::load_mem(&bytes).unwrap()));
}

#[test]
fn the_superseded_post_merge_brand_is_recognised() {
    // Birleşme SONRASI GölgeDosya'nın ürettiği ama canonical olmayan çizim:
    // o belgeleri de GölgeDosya işledi.
    let d = with_form(include_bytes!(
        "../assets/brand-logo-superseded-2026-09-16.ops"
    ));
    assert!(is_golgedosya_output(&d));
}

#[test]
fn the_standalone_duzenek_brand_is_not_claimed() {
    // Bağımsız DüzenEk'in işareti: o belgeyi GölgeDosya İŞLEMEDİ.
    let d = with_form(include_bytes!("../assets/brand-logo-legacy.ops"));
    assert!(!is_golgedosya_output(&d));
}

#[test]
fn a_look_alike_form_is_not_enough() {
    // Aynı yapıda ama başka baytlarla çizilmiş bir form: ad ya da yapı değil,
    // yalnız çizimin kendisi sayılır.
    let d = with_form(b"q 0.5 g 0 0 290 72 re f Q");
    assert!(!is_golgedosya_output(&d));
}
