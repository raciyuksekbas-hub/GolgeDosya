//! Unit conversions. The canonical unit is the PostScript point (1/72 inch) — see EXP-005.
//!
//! Every scale here is exact in rational arithmetic, which is why the canonical unit was
//! chosen: no conversion in the pipeline introduces representational drift.

/// DOCX indents/spacing are twentieths of a point ("dxa"/twips). 1440 twip/in ÷ 72 pt/in = 20.
pub const TWIPS_PER_POINT: f32 = 20.0;
/// DOCX font sizes are half-points ("w:sz").
pub const HALFPOINTS_PER_POINT: f32 = 2.0;
/// DOCX drawing extents are EMU. 914400 EMU/in ÷ 72 pt/in = 12700.
pub const EMU_PER_POINT: f32 = 12_700.0;
/// 72 pt/in ÷ 2.54 cm/in.
pub const POINTS_PER_CM: f32 = 28.346_457;

#[inline]
pub fn twips_to_pt(t: f32) -> f32 {
    t / TWIPS_PER_POINT
}

#[inline]
pub fn pt_to_twips(p: f32) -> i64 {
    (p * TWIPS_PER_POINT).round() as i64
}

#[inline]
pub fn halfpoints_to_pt(h: f32) -> f32 {
    h / HALFPOINTS_PER_POINT
}

#[inline]
pub fn pt_to_halfpoints(p: f32) -> i64 {
    (p * HALFPOINTS_PER_POINT).round() as i64
}

#[inline]
pub fn emu_to_pt(e: f32) -> f32 {
    e / EMU_PER_POINT
}

#[inline]
pub fn pt_to_emu(p: f32) -> i64 {
    (p * EMU_PER_POINT).round() as i64
}

#[inline]
pub fn cm_to_pt(c: f32) -> f32 {
    c * POINTS_PER_CM
}

/// Round emitted UDF geometry to the precision range observed in genuine specimens (EXP-005).
pub fn udf_round(v: f32) -> f32 {
    (v * 100_000.0).round() / 100_000.0
}

/// Format a float the way the observed UDF writer does: always at least one decimal place.
pub fn udf_float(v: f32) -> String {
    let v = udf_round(v);
    if v == v.trunc() && v.abs() < 1e9 {
        format!("{:.1}", v)
    } else {
        let s = format!("{}", v);
        if s.contains('.') {
            s
        } else {
            format!("{}.0", s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_scales() {
        // Chosen so that every DOCX <-> canonical conversion is exact.
        assert_eq!(twips_to_pt(1440.0), 72.0);
        assert_eq!(pt_to_twips(72.0), 1440);
        assert_eq!(halfpoints_to_pt(24.0), 12.0);
        assert_eq!(pt_to_halfpoints(12.0), 24);
        assert_eq!(emu_to_pt(914_400.0), 72.0);
        assert_eq!(pt_to_emu(72.0), 914_400);
    }

    #[test]
    fn roundtrip_is_lossless_for_realistic_values() {
        for twips in [0i64, 20, 567, 1440, 2835, 5670, 14400] {
            let pt = twips_to_pt(twips as f32);
            assert_eq!(pt_to_twips(pt), twips, "twips {twips} did not survive");
        }
    }

    #[test]
    fn uyap_margin_reproduces_the_observed_constant() {
        // The measurement that fixed the unit: 2.5 cm == the 70.875 in every specimen.
        let m = cm_to_pt(2.5);
        assert!((m - 70.875).abs() < 0.01, "got {m}");
    }

    #[test]
    fn float_formatting_matches_observed_style() {
        assert_eq!(udf_float(70.875), "70.875");
        assert_eq!(udf_float(20.0), "20.0");
        assert_eq!(udf_float(0.0), "0.0");
        assert_eq!(udf_float(1.0), "1.0");
    }
}
