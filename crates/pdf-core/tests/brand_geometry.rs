//! PDF marka işaretinin GEOMETRİSİ, ürünün kabul edilmiş işaretiyle aynı mı?
//!
//! SAHA BULGUSU: üretilen PDF'in sağ alt köşesinde "GölgeDosya" yazısı doğru
//! ama İKON yanlıştı. `5a399ea` eski DüzenEk kelime işaretini kaldırırken
//! yerine yeni bir şekil uydurmuş ("üst üste iki yaprak") ve eski DüzenEk
//! paletini korumuştu.
//!
//! Bu test TAUTOLOJİK DEĞİLDİR: mevcut `brand_identity.rs` çıktıyı asset'in
//! kendisiyle karşılaştırıyor, yani asset yanlışsa test de yanlış yeşil verir.
//! Burada asset, ürünün TEK DOĞRU KAYNAĞI olan `apps/belge-shell/brand/mark.svg`
//! ile karşılaştırılır.
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace kökü")
        .to_path_buf()
}

fn canonical_mark_svg() -> String {
    std::fs::read_to_string(repo_root().join("apps/belge-shell/brand/mark.svg"))
        .expect("canonical mark.svg okunmalı")
}

fn pdf_mark_ops() -> String {
    include_str!("../assets/brand-logo.ops").to_string()
}

/// `#RRGGBB` -> PDF `r g b` ondalıkları (4 hane).
fn pdf_rgb(hex: &str) -> [String; 3] {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).expect("hex");
    [(v >> 16) & 255, (v >> 8) & 255, v & 255].map(|c| format!("{:.5}", c as f64 / 255.0))
}

#[test]
fn the_pdf_mark_uses_the_brand_palette_not_the_legacy_one() {
    let svg = canonical_mark_svg();
    let ops = pdf_mark_ops();
    for hex in ["#8A6A3E", "#1C1C1E"] {
        assert!(
            svg.contains(hex),
            "canonical mark {hex} taşımıyor; test güncellenmeli"
        );
        let [r, g, b] = pdf_rgb(hex);
        assert!(
            ops.contains(&format!("{r} {g} {b} rg")),
            "PDF işareti marka rengi {hex} kullanmıyor ({r} {g} {b})"
        );
    }
}

#[test]
fn the_banned_legacy_teal_is_gone() {
    // BRAND.md: "Mavi-SaaS tonu, petrol yeşili ve sert turkuvaz kullanılmaz."
    // #176D73 tam olarak o tondur ve eski DüzenEk işaretinden mirastır.
    let ops = pdf_mark_ops();
    let [r, g, b] = pdf_rgb("#176D73");
    assert!(
        !ops.contains(&format!("{r} {g} {b} rg")) && !ops.contains("0.0902 0.42745 0.45098"),
        "yasaklı petrol yeşili PDF işaretinde duruyor"
    );
    let brand = std::fs::read_to_string(repo_root().join("apps/belge-shell/brand/BRAND.md"))
        .expect("BRAND.md");
    assert!(
        brand.contains("petrol yeşili"),
        "renk kuralı BRAND.md'den kalkmış"
    );
}

#[test]
fn the_pdf_mark_reproduces_the_single_diagonal() {
    // Canonical fikir: TEK siluet, sağ alt köşe tek bir diyagonalle kalkık.
    // Eski çizim iki ayrı dikdörtgendi — diyagonal hiç yoktu.
    let svg = canonical_mark_svg();
    assert!(
        svg.contains("M12 8 H52 V32 L28 56 H12 Z"),
        "canonical yol değişmiş; PDF çizimi de güncellenmeli"
    );
    let ops = pdf_mark_ops();
    // Aynı yol, PDF y-yukarı uzayında (+4 kaydırma): 60 / 36 / 12.
    assert!(
        ops.contains("12 60 m 52 60 l 52 36 l 28 12 l 12 12 l h"),
        "PDF işaretinde canonical diyagonal yok"
    );
}

#[test]
fn the_pdf_mark_is_one_clipped_silhouette() {
    let ops = pdf_mark_ops();
    assert!(
        ops.contains("W n"),
        "kırpma yolu yok: siluet tek parça değil"
    );
    // rx=7 yuvarlatma Bézier ile kurulur.
    assert!(ops.contains(" c\n"), "yuvarlatılmış köşe (Bézier) yok");
    assert!(
        canonical_mark_svg().contains("rx=\"7\""),
        "canonical yuvarlatma değişmiş"
    );
}

#[test]
fn superseded_drawings_are_still_recognised_for_upgrade() {
    // Sahada bu iki çizimle damgalanmış belgeler var; tanınmazsa üzerlerine
    // ikinci bir işaret eklenir ve kullanıcı çift filigran görür.
    let dir = repo_root().join("crates/pdf-core/assets");
    for name in [
        "brand-logo-legacy.ops",
        "brand-logo-superseded-2026-09-16.ops",
        "brand-logo-2026-09-20.ops",
    ] {
        assert!(dir.join(name).exists(), "{name} tanıma için tutulmalı");
    }
    let stamp = std::fs::read_to_string(repo_root().join("crates/pdf-core/src/pdf/stamp.rs"))
        .expect("stamp.rs");
    assert!(stamp.contains("OUTDATED_BRAND_LOGOS: [&[u8]; 3]"));
}

/// Simgenin dikey ortası (yuvarlatılmış sayfanın kutusu: `12 12 40 48 re`).
fn icon_centre(ops: &str) -> f64 {
    let line = ops
        .lines()
        .find(|l| l.trim_end().ends_with(" re"))
        .expect("simge kutusu");
    let v: Vec<f64> = line
        .split_whitespace()
        .take(4)
        .map(|n| n.parse().unwrap())
        .collect();
    v[1] + v[3] / 2.0
}

/// Kelime işaretinin büyük harf bandının ortası: taban çizgisi + yarım büyük
/// harf yüksekliği. Helvetica-Bold AFM: CapHeight 718/1000.
fn cap_band_centre(ops: &str) -> f64 {
    let size: f64 = ops
        .lines()
        .find(|l| l.trim_end().ends_with(" Tf"))
        .and_then(|l| l.split_whitespace().nth(1))
        .expect("Tf")
        .parse()
        .unwrap();
    let baseline: f64 = ops
        .lines()
        .find(|l| l.trim_end().ends_with(" Td"))
        .and_then(|l| l.split_whitespace().nth(1))
        .expect("Td")
        .parse()
        .unwrap();
    baseline + 0.718 * size / 2.0
}

/// SAHA: "Bu filigranda simge ile yazı ortada değil." Kanonik yatay logo
/// (`apps/belge-shell/brand/horizontal.svg`) büyük harf bandını simgenin
/// ortasına oturtur. PDF çiziminde taban çizgisi 27'deydi: bant ortası 39,6,
/// simge ortası 36; yazı simgeye göre 3,6 birim (%7,5) yukarıdaydı.
#[test]
fn the_word_mark_cap_band_is_centred_on_the_icon() {
    let ops = pdf_mark_ops();
    let (icon, cap) = (icon_centre(&ops), cap_band_centre(&ops));
    assert!(
        (cap - icon).abs() <= 0.3,
        "büyük harf bandı ortası {cap:.2}, simge ortası {icon:.2}: yazı simgeyle hizasız"
    );
    // Kanonik SVG'de de kural aynı (y aşağı): simge 8..56, taban 41,5, 27 px.
    let svg = std::fs::read_to_string(repo_root().join("apps/belge-shell/brand/horizontal.svg"))
        .expect("horizontal.svg");
    assert!(svg.contains(r#"y="8" width="40" height="48""#) && svg.contains(r#"y="41.5""#));
    assert!(((41.5 - 0.718 * 27.0 / 2.0) - 32.0f64).abs() <= 0.3);
    // Hizalama yalnız taban çizgisini değiştirir: simge, renk, punto aynı.
    let old = include_str!("../assets/brand-logo-2026-09-20.ops");
    assert_eq!(icon_centre(old), icon);
    assert_eq!(
        old.replace("76 27 Td", "76 23.4 Td"),
        ops,
        "hizalama dışında bir değişiklik var"
    );
}
