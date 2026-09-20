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
    ] {
        assert!(dir.join(name).exists(), "{name} tanıma için tutulmalı");
    }
    let stamp = std::fs::read_to_string(repo_root().join("crates/pdf-core/src/pdf/stamp.rs"))
        .expect("stamp.rs");
    assert!(stamp.contains("OUTDATED_BRAND_LOGOS: [&[u8]; 2]"));
}
