//! v0.2.0 hardening — şüpheci turu (S1): toleranslı yükleyicinin AŞIRI REDDİ.
//!
//! Yükleyici, lopdf ve her gerçek görüntüleyicinin (Acrobat/Preview/Chrome)
//! sorunsuz açıp render ettiği iki PDF sınıfını reddediyordu. İkisi de
//! deterministik, minimal ve mevcut corpus/fuzz/soak takımının kör noktasında.
//! Bir avukat, herhangi bir görüntüleyicide açılan belgeyi Düzenle'nin hiçbir
//! aracıyla (döndür/filigran/numara/kırp/sıkıştır) işleyemiyordu.
//!
//! Bu dosya iki bulguyu da SENTETİK, elle kurulmuş baytlarla yeniden üretir.

mod hardening;

use hardening::check::{self, guarded, Guarded, Lab};
use hardening::raw::{zlib, RawPdf};
use std::time::Duration;

use ekler_core::toolbox::{run_tool_with_outcome, ToolOperation, ToolOutcome};

/// Yükleyiciyi çökme değişmezini koruyarak (2 MiB yığın + zaman aşımı) çağır ve
/// açılan sayfa sayısını ya da red mesajını döndür.
fn open_pages(bytes: Vec<u8>) -> Result<usize, String> {
    match guarded(Duration::from_secs(20), move || {
        ekler_core::load_pdf_tolerant(&bytes, "sentetik.pdf")
            .map(|r| r.document.get_pages().len())
            .map_err(|e| e.to_string())
    }) {
        Guarded::Done(r) => r,
        Guarded::Panicked(p) => panic!("yükleyici panikledi: {p}"),
        Guarded::TimedOut => panic!("yükleyici zaman aşımına uğradı"),
    }
}

/// 1 sayfalık iskelet: 1 Catalog, 2 Pages, 3 Font, 10 Page. İçerik nesnesini ve
/// sayfanın `/Contents` değerini çağıran belirler.
fn skeleton(contents_value: &str) -> RawPdf {
    let mut pdf = RawPdf::new("1.7");
    pdf.object(1, b"<</Type/Catalog/Pages 2 0 R>>");
    pdf.object(2, b"<</Type/Pages/Count 1/Kids[10 0 R]>>");
    pdf.object(3, b"<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>");
    pdf.object(
        10,
        format!(
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 595.28 841.89]\
             /Resources<</Font<</F1 3 0 R>>>>/Contents {contents_value}>>"
        )
        .as_bytes(),
    );
    pdf
}

/// S1-a (P1): içerik akışı geçerli, TAM bir zlib akışıdır; ama `/Length`'e
/// sayılan tek bir satır sonu baytı (üreticilerin çok yaygın alışkanlığı)
/// akışın SONUNDADIR. lopdf zlib EOD'unda durup baytı yok sayar, sayfayı bulur;
/// gerçek görüntüleyici de öyle. Yükleyici ise `total_in != content.len()`
/// eşitliğinde takılıp "uzunluk tutarsız" diye reddediyordu — kendi görsel
/// yolu (optimizer.rs) aynı deseni ≤2 bayt tolere ederken.
#[test]
fn a_trailing_eol_byte_inside_a_flate_content_stream_still_opens() {
    let mut data = zlib(b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET");
    data.push(b'\n'); // üreticinin endstream'den önce yazdığı satır sonu
    let mut pdf = skeleton("11 0 R");
    // stream(): /Length = data.len(), yani sondaki baytı sayar (bulgunun tam koşulu).
    pdf.stream(11, "/Filter/FlateDecode", &data);
    pdf.finish_classic("/Root 1 0 R");

    let pages = open_pages(pdf.bytes.clone()).unwrap_or_else(|e| {
        panic!("görüntüleyicinin açtığı belge reddedildi (S1-a): {e}")
    });
    assert_eq!(pages, 1, "sayfa bulunmalı");

    // Uçtan uca: avukat bu belgeyi gerçekten işleyebilmeli.
    let lab = Lab::new();
    let src = lab.write("sondaki-bayt.pdf", &pdf.bytes);
    let out = lab.path("dondurulmus.pdf");
    match run_tool_with_outcome(std::slice::from_ref(&src), &ToolOperation::Rotate { degrees: 90 }, &out, false) {
        Ok(ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. }) => {
            let doc = check::reopen_strict(&out).expect("çıktı katı biçimde açılmalı");
            assert_eq!(doc.get_pages().len(), 1);
        }
        other => panic!("döndürme başarısız (S1-a): {other:?}"),
    }
}

/// S1-a devamı: birden çok satır sonu (CRLF) da tolere edilmeli; artık bayt
/// sayısı sabit ≤2 sınırına kilitli değildir. Aynı zamanda gerçek bozulma
/// (yarıda kesilmiş zlib) hâlâ reddedilmelidir.
#[test]
fn multiple_trailing_bytes_open_but_a_truncated_stream_is_still_rejected() {
    // İki artık bayt (CRLF).
    let mut data = zlib(b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET");
    data.extend_from_slice(b"\r\n");
    let mut pdf = skeleton("11 0 R");
    pdf.stream(11, "/Filter/FlateDecode", &data);
    pdf.finish_classic("/Root 1 0 R");
    assert_eq!(open_pages(pdf.bytes.clone()).unwrap(), 1, "CRLF sonu açılmalı");

    // Gerçekten bozuk: zlib akışı ortadan kesik → çözme HATASI → hâlâ reddedilir.
    let full = zlib(b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET (uzun icerik) Tj ET");
    let truncated = &full[..full.len() / 2];
    let mut pdf = skeleton("11 0 R");
    pdf.stream(11, "/Filter/FlateDecode", truncated);
    pdf.finish_classic("/Root 1 0 R");
    assert!(
        open_pages(pdf.bytes).is_err(),
        "yarıda kesilmiş zlib akışı yine de reddedilmeli (kısmi veri kabul edilmez)"
    );
}

/// S1-b (P1): `/Contents` DOLAYLI bir başvurudur ve bir DİZİYE çözülür
/// (`/Contents 5 0 R`, nesne 5 = `[6 0 R]`). ISO 32000-1 buna izin verir:
/// `/Contents` değeri dolaylı olabilir, bir akışa ya da diziye çözülür. lopdf
/// açar, `get_pages()==1`, içeriği çözer. Yükleyici ise `/Contents` değerini
/// dereference ETMEDEN Array/stream dallanması yapıp, dolaylı başvuruyu tek
/// stream sanıp `.as_stream()` başarısız olunca "Contents stream eksik" diyordu.
#[test]
fn an_indirect_reference_to_a_contents_array_still_opens() {
    let mut pdf = skeleton("5 0 R");
    pdf.object(5, b"[6 0 R]"); // dolaylı başvurunun çözüldüğü dizi
    pdf.stream(6, "", b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET");
    pdf.finish_classic("/Root 1 0 R");

    let pages = open_pages(pdf.bytes.clone())
        .unwrap_or_else(|e| panic!("görüntüleyicinin açtığı belge reddedildi (S1-b): {e}"));
    assert_eq!(pages, 1, "dolaylı dizi /Contents ile sayfa bulunmalı");

    // Uçtan uca: filigran da eklenebilmeli.
    let lab = Lab::new();
    let src = lab.write("dolayli-dizi.pdf", &pdf.bytes);
    let out = lab.path("filigranli.pdf");
    match run_tool_with_outcome(
        std::slice::from_ref(&src),
        &ToolOperation::Watermark { text: "KOPYA".into() },
        &out,
        false,
    ) {
        Ok(ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. }) => {
            let doc = check::reopen_strict(&out).expect("çıktı katı biçimde açılmalı");
            assert_eq!(doc.get_pages().len(), 1);
        }
        other => panic!("filigran başarısız (S1-b): {other:?}"),
    }
}
