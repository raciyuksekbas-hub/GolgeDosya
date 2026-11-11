//! Metnin satırda kapladığı genişlik (pt).
//!
//! Sekmeli başlık satırında bir etiketin kendi sütununa sığıp sığmadığı, sığmıyorsa
//! nerede alt satıra ineceği buna bakar. UDF'in yazı tipi varsayılan olarak Times
//! New Roman'dır. Onun ASCII genişlikleri, PDF çekirdek yazı tiplerinin serbestçe
//! dağıtılan Adobe AFM ölçüleriyle (Times-Roman, Times-Bold) birebir aynıdır; Türkçe
//! ve aksanlı harfler temel harfin genişliğindedir. Bu ölçüler kesindir; başka yazı
//! tipi ve tabloda olmayan karakter için cömert bir tahmin kullanılır.
//!
//! Kesin genişliğe %4 pay eklenir: UYAP'ın çizimi Word'ünkünden biraz farklı
//! olabilir, sınırdaki bir etiket bir sonraki sekme durağına kaçmamalı.

use crate::model::RunProps;

/// Yazı boyutu bilinmeyen metin için (UDF varsayılanı).
const DEFAULT_SIZE_PT: f32 = 12.0;
/// Kesin genişliğe eklenen pay.
const RENDER_SLACK: f32 = 1.04;

/// Times-Roman, U+0020..U+007E, em'in binde biri.
const ROMAN: [u16; 95] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278, //
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444, //
    921, 722, 667, 667, 722, 611, 556, 722, 722, 333, 389, 722, 611, 889, 722, 722, //
    556, 722, 667, 556, 611, 722, 722, 944, 722, 722, 611, 333, 278, 333, 469, 500, //
    333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500, 278, 778, 500, 500, //
    500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541,
];

/// Times-Bold, U+0020..U+007E, em'in binde biri.
const BOLD: [u16; 95] = [
    250, 333, 555, 500, 500, 1000, 833, 278, 333, 333, 500, 570, 250, 333, 250, 278, //
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500, //
    930, 722, 667, 722, 722, 667, 611, 778, 778, 389, 500, 778, 667, 944, 722, 778, //
    611, 778, 722, 556, 667, 722, 722, 1000, 722, 722, 667, 333, 278, 333, 581, 500, //
    333, 500, 556, 444, 556, 444, 333, 500, 556, 278, 333, 556, 278, 833, 556, 500, //
    556, 556, 444, 389, 333, 556, 500, 722, 500, 500, 444, 394, 220, 394, 520,
];

/// Metnin bir satırdaki genişliği (pt), payıyla.
pub(crate) fn text_width(text: &str, props: &RunProps) -> f32 {
    text.chars().map(|c| char_width(c, props)).sum()
}

/// Bir karakterin genişliği (pt), payıyla.
pub(crate) fn char_width(c: char, props: &RunProps) -> f32 {
    let size = props.font_size_pt.unwrap_or(DEFAULT_SIZE_PT);
    if is_times(props) {
        if let Some(units) = times_units(c, props.bold) {
            return f32::from(units) / 1000.0 * size * RENDER_SLACK;
        }
    }
    let bold = if props.bold { 1.05 } else { 1.0 };
    estimate_em(c) * size * bold
}

/// Yazı tipi belirtilmemiş metin UDF'te varsayılan yazı tipiyle (Times New
/// Roman) çizilir.
fn is_times(props: &RunProps) -> bool {
    props
        .font_family
        .as_deref()
        .map_or(true, |f| f.to_lowercase().contains("times"))
}

fn times_units(c: char, bold: bool) -> Option<u16> {
    let table = if bold { &BOLD } else { &ROMAN };
    let c = base_letter(c);
    Some(match c as u32 {
        0x20..=0x7E => table[(c as u32 - 0x20) as usize],
        0xA0 => 250,
        0x2018 | 0x2019 => 333,
        0x201C | 0x201D if bold => 500,
        0x201C | 0x201D => 444,
        0x2013 | 0xA7 | 0x20AC | 0xAB | 0xBB => 500,
        0x2014 | 0x2026 => 1000,
        0x2022 => 350,
        0xB0 => 400,
        _ => return None,
    })
}

/// Aksanlı harfin genişliği temel harfinkidir.
fn base_letter(c: char) -> char {
    match c {
        'Ç' => 'C',
        'ç' => 'c',
        'Ğ' => 'G',
        'ğ' => 'g',
        'İ' | 'Î' | 'Ï' => 'I',
        'ı' | 'î' | 'ï' => 'i',
        'Ö' | 'Ô' | 'Ó' | 'Ò' => 'O',
        'ö' | 'ô' | 'ó' | 'ò' => 'o',
        'Ş' => 'S',
        'ş' => 's',
        'Ü' | 'Û' | 'Ú' | 'Ù' => 'U',
        'ü' | 'û' | 'ú' | 'ù' => 'u',
        'Â' | 'Á' | 'À' | 'Ä' => 'A',
        'â' | 'á' | 'à' | 'ä' => 'a',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'Ñ' => 'N',
        'ñ' => 'n',
        other => other,
    }
}

/// Bilinmeyen yazı tipinde bir karakterin yaklaşık genişliği (em), cömert:
/// büyük harf 0,7, küçük harf 0,47, rakam 0,5, boşluk 0,25.
fn estimate_em(c: char) -> f32 {
    if c == ' ' {
        0.25
    } else if c.is_uppercase() {
        0.7
    } else if c.is_ascii_digit() {
        0.5
    } else if c.is_alphabetic() {
        0.47
    } else {
        0.33
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(bold: bool, family: Option<&str>) -> RunProps {
        RunProps {
            bold,
            font_family: family.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn times_widths_are_exact_with_the_render_slack() {
        // "DAVACI", kalın 12 pt: 722·4 + 722 + 389 = 3999 binde → 47,988 pt.
        let w = text_width("DAVACI", &props(true, None));
        assert!((w - 47.988 * RENDER_SLACK).abs() < 0.01, "{w}");
        // Türkçe harfler temel harfin genişliğinde.
        assert_eq!(
            text_width("İĞŞÇÖÜığşçöü", &props(false, Some("Times New Roman"))),
            text_width("IGSCOUigscou", &props(false, Some("Times New Roman")))
        );
    }

    #[test]
    fn other_fonts_fall_back_to_the_generous_estimate() {
        let w = text_width("A", &props(false, Some("Arial")));
        assert!((w - 0.7 * 12.0).abs() < 0.001, "{w}");
    }
}
