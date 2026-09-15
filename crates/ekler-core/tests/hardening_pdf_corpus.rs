//! v0.2.0 hardening — C katmanı: sentetik PDF corpus'u.
//!
//! Her aile için üç soru:
//!   1. Bu girdi açılışı bozabilir mi?  → açılır ya da güvenle reddedilir,
//!      seçim deterministiktir, panik/sonsuz döngü yoktur.
//!   2. Bu girdi bir aracı bozabilir mi? → her araç ya geçerli bir çıktı yayınlar
//!      ya da kontrollü hata verir; çıktı katı biçimde yeniden açılır.
//!   3. Başarısızlık veri kaybı ya da yarım çıktı bırakır mı? → kaynak özeti
//!      değişmez, hata sonrası hedef klasörde tek bayt kalmaz.
//!
//! `HARDENING_DISCOVER=1` beklentileri denetlemeden gerçek sonuçları basar.

mod hardening;

use ekler_core::toolbox::{run_tool_with_outcome, PageRotation, ToolOperation, ToolOutcome};
use ekler_core::{load_pdf_tolerant, OptimizationLevel};
use hardening::check::{self, guarded, Guarded, Lab};
use hardening::corpus::{self, Expect, Fixture};
use lopdf::Document;
use std::time::Duration;

const OPEN_TIMEOUT: Duration = Duration::from_secs(60);
const TOOL_TIMEOUT: Duration = Duration::from_secs(180);

fn classify(bytes: Vec<u8>) -> Guarded<Expect> {
    guarded(OPEN_TIMEOUT, move || {
        match load_pdf_tolerant(&bytes, "girdi.pdf") {
            Ok(loaded) => Expect::Opens {
                pages: loaded.document.get_pages().len(),
                repaired: loaded.is_repaired,
            },
            Err(_) => Expect::Rejects,
        }
    })
}

#[test]
fn every_corpus_fixture_opens_or_rejects_deterministically_without_panic_or_hang() {
    let discover = std::env::var("HARDENING_DISCOVER").is_ok();
    let mut problems = Vec::new();
    for fx in corpus::all() {
        let first = classify(fx.bytes.clone());
        let second = classify(fx.bytes.clone());
        let (first, second) = match (first, second) {
            (Guarded::Done(a), Guarded::Done(b)) => (a, b),
            (a, b) => {
                problems.push(format!(
                    "{}: açılış çöktü/kilitlendi: {a:?} / {b:?}",
                    fx.name
                ));
                continue;
            }
        };
        if first != second {
            problems.push(format!(
                "{}: deterministik değil: {first:?} ≠ {second:?}",
                fx.name
            ));
        }
        if discover {
            println!("{:<48} {:?}", fx.name, first);
        } else if first != fx.expect {
            problems.push(format!(
                "{}: beklenen {:?}, gerçekleşen {first:?}",
                fx.name, fx.expect
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[derive(Debug, Clone, Copy)]
enum Op {
    SelectFirst,
    SelectLastFirst,
    ReorderReverse,
    DeleteFirst,
    RotateAllRight,
    RotateFirstLeft,
    RotatePageHalf,
    Crop,
    Watermark,
    Number,
    Compress,
}

const WATERMARK: &str = "Taslak (Gizli) \\ %100 [1]";

impl Op {
    const ALL: [Op; 11] = [
        Op::SelectFirst,
        Op::SelectLastFirst,
        Op::ReorderReverse,
        Op::DeleteFirst,
        Op::RotateAllRight,
        Op::RotateFirstLeft,
        Op::RotatePageHalf,
        Op::Crop,
        Op::Watermark,
        Op::Number,
        Op::Compress,
    ];

    fn operation(self, n: usize) -> Option<ToolOperation> {
        Some(match self {
            Op::SelectFirst => ToolOperation::Select { pages: vec![1] },
            Op::SelectLastFirst if n > 1 => ToolOperation::Select { pages: vec![n, 1] },
            Op::ReorderReverse => ToolOperation::Reorder {
                pages: (1..=n).rev().collect(),
            },
            Op::DeleteFirst if n > 1 => ToolOperation::Delete { pages: vec![1] },
            Op::RotateAllRight => ToolOperation::Rotate { degrees: 90 },
            Op::RotateFirstLeft => ToolOperation::RotateSelected {
                degrees: -90,
                pages: vec![1],
            },
            Op::RotatePageHalf => ToolOperation::RotatePages {
                rotations: vec![PageRotation {
                    page: n,
                    degrees: 180,
                }],
            },
            Op::Crop => ToolOperation::Crop { margin_pt: 5.0 },
            Op::Watermark => ToolOperation::Watermark {
                text: WATERMARK.into(),
            },
            Op::Number => ToolOperation::Number { start: 1 },
            Op::Compress => ToolOperation::Compress {
                level: OptimizationLevel::BalancedCompression,
            },
            _ => return None,
        })
    }

    /// Çıktıdaki sayfa sırası, kaynak sayfa indeksleriyle (0 tabanlı).
    fn expected_order(self, n: usize) -> Vec<usize> {
        match self {
            Op::SelectFirst => vec![0],
            Op::SelectLastFirst => vec![n - 1, 0],
            Op::ReorderReverse => (0..n).rev().collect(),
            Op::DeleteFirst => (1..n).collect(),
            _ => (0..n).collect(),
        }
    }

    /// Kaynak indeksindeki sayfaya uygulanan dönüş.
    fn added_rotation(self, source_index: usize, n: usize) -> i64 {
        match self {
            Op::RotateAllRight => 90,
            Op::RotateFirstLeft if source_index == 0 => -90,
            Op::RotatePageHalf if source_index == n - 1 => 180,
            _ => 0,
        }
    }
}

/// Görüntüleyici semantiği: 90'ın katı olmayan /Rotate yok sayılır (pdf.js).
fn effective(rotate: i64) -> i64 {
    if rotate % 90 == 0 {
        rotate.rem_euclid(360)
    } else {
        0
    }
}

fn verify_output(fx: &Fixture, op: Op, source: &Document, out: &Document) -> Vec<String> {
    let mut errors = Vec::new();
    let n = source.get_pages().len();
    let order = op.expected_order(n);
    let src_ids: Vec<_> = source.get_pages().into_values().collect();
    let out_ids: Vec<_> = out.get_pages().into_values().collect();
    if out_ids.len() != order.len() {
        errors.push(format!(
            "{} · {op:?}: sayfa sayısı {} (beklenen {})",
            fx.name,
            out_ids.len(),
            order.len()
        ));
        return errors;
    }
    for (pos, (&src_index, &out_id)) in order.iter().zip(&out_ids).enumerate() {
        let src_id = src_ids[src_index];
        let want_marker = check::page_marker(source, src_id);
        let got_marker = check::page_marker(out, out_id);
        if want_marker.is_some() && want_marker != got_marker {
            errors.push(format!(
                "{} · {op:?}: çıktı s.{} işaret {got_marker:?} (beklenen {want_marker:?})",
                fx.name,
                pos + 1
            ));
        }
        // Görüntüleyici semantiği: geçiş araçları kaynağın ham /Rotate'ını
        // (450, -90…) olduğu gibi taşır; döndürme araçları normalize eder.
        // İkisini de effective() ile karşılaştır: 450 ve 90 aynı, -90 ve 270 aynı.
        let want_rot = (effective(check::rotation(source, src_id))
            + op.added_rotation(src_index, n))
        .rem_euclid(360);
        let got_rot = effective(check::rotation(out, out_id));
        if got_rot != want_rot {
            errors.push(format!(
                "{} · {op:?}: çıktı s.{} dönüş {got_rot} (beklenen {want_rot})",
                fx.name,
                pos + 1
            ));
        }
        let (src_media, src_crop) = check::boxes(source, src_id);
        let (out_media, out_crop) = check::boxes(out, out_id);
        match (src_media, out_media) {
            (Some(s), Some(o)) => {
                if !check::contains(&o, &s, 0.5) {
                    errors.push(format!(
                        "{} · {op:?}: çıktı s.{} MediaBox {o:?} kaynağınkini {s:?} kapsamıyor",
                        fx.name,
                        pos + 1
                    ));
                }
                if let Some(c) = out_crop {
                    if c[2] - c[0] <= 0.0 || c[3] - c[1] <= 0.0 {
                        errors.push(format!(
                            "{} · {op:?}: çıktı s.{} CropBox boş {c:?}",
                            fx.name,
                            pos + 1
                        ));
                    }
                }
                if matches!(op, Op::Crop) {
                    // Kırpma her kenardan 5 pt içeri alır; sonra marka payı (34 pt)
                    // görsel alt kenarı DIŞARI genişletir. Alt kenar dönüşe göre
                    // değişir, o yüzden dört kenardan üçü tam 5 pt içeride olmalı,
                    // dördüncüsü (pay kenarı) ya 5 pt içeride ya 34 pt payla dışarıda.
                    // Kırpma aracı var olan kutuyu (CropBox, yoksa MediaBox)
                    // olduğu gibi 5 pt içeri alır; MediaBox ile kesişim ALMAZ.
                    let visible = src_crop.unwrap_or(s);
                    const GUTTER: f64 = 34.0;
                    // Kenar başına içe alma: sol/alt +5, sağ/üst −5.
                    let inset = [5.0, 5.0, -5.0, -5.0];
                    match out_crop {
                        Some(c) => {
                            let mut cropped = 0;
                            let mut guttered = 0;
                            for k in 0..4 {
                                let want = visible[k] + inset[k];
                                let sign = if k < 2 { -1.0 } else { 1.0 }; // pay dışarı
                                if (c[k] - want).abs() < 0.6 {
                                    cropped += 1;
                                } else if (c[k] - (want + sign * GUTTER)).abs() < 0.6 {
                                    guttered += 1;
                                }
                            }
                            if cropped < 3 || cropped + guttered != 4 {
                                errors.push(format!(
                                    "{} · Crop: çıktı s.{} CropBox {c:?} kırpma 5 pt + pay 34 pt modeline uymuyor (görünür {visible:?})",
                                    fx.name,
                                    pos + 1
                                ));
                            }
                        }
                        None => errors.push(format!(
                            "{} · Crop: çıktı s.{} CropBox yok",
                            fx.name,
                            pos + 1
                        )),
                    }
                }
            }
            (_, None) => errors.push(format!(
                "{} · {op:?}: çıktı s.{} MediaBox yok",
                fx.name,
                pos + 1
            )),
            _ => {}
        }
    }
    if matches!(op, Op::Watermark) {
        let mut found = false;
        for id in &out_ids {
            let Ok(content) = out.get_page_content(*id) else {
                continue;
            };
            let Ok(ops) = lopdf::content::Content::decode(&content) else {
                errors.push(format!("{} · Watermark: içerik akışı çözülemedi", fx.name));
                break;
            };
            found |= ops.operations.iter().any(|o| {
                o.operator == "Tj"
                    && o.operands
                        .first()
                        .and_then(|s| s.as_str().ok())
                        .is_some_and(|s| s == WATERMARK.as_bytes())
            });
        }
        if !found {
            errors.push(format!(
                "{} · Watermark: filigran metni içerikte birebir bulunamadı",
                fx.name
            ));
        }
    }
    errors
}

#[test]
fn every_tool_on_every_opening_fixture_publishes_a_valid_copy_or_fails_cleanly() {
    let only = std::env::var("HARDENING_ONLY").unwrap_or_default();
    let mut problems = Vec::new();
    for fx in corpus::all() {
        if !only.is_empty() && !fx.name.contains(&only) {
            continue;
        }
        let Expect::Opens { .. } = fx.expect else {
            continue;
        };
        let lab = Lab::new();
        let src = lab.write("kaynak/girdi.pdf", &fx.bytes);
        let before = check::sha(&src);
        let source = match load_pdf_tolerant(&fx.bytes, "girdi.pdf") {
            Ok(l) => l.document,
            Err(e) => {
                problems.push(format!("{}: kaynak açılamadı: {e}", fx.name));
                continue;
            }
        };
        let n = source.get_pages().len();
        for op in Op::ALL {
            let Some(operation) = op.operation(n) else {
                continue;
            };
            let out = lab.path(&format!("cikti/{op:?}.pdf"));
            std::fs::create_dir_all(out.parent().unwrap()).unwrap();
            let before_dir = check::tree(&lab.path("cikti"));
            let (src2, out2, signed) = (src.clone(), out.clone(), fx.signed);
            let result = guarded(TOOL_TIMEOUT, move || {
                run_tool_with_outcome(&[src2], &operation, &out2, signed).map_err(|e| e.to_string())
            });
            let outcome = match result {
                Guarded::Done(r) => r,
                other => {
                    problems.push(format!("{} · {op:?}: {other:?}", fx.name));
                    continue;
                }
            };
            if check::sha(&src) != before {
                problems.push(format!("{} · {op:?}: KAYNAK DEĞİŞTİ", fx.name));
            }
            let after_dir = check::tree(&lab.path("cikti"));
            let new_files: Vec<_> = after_dir.difference(&before_dir).cloned().collect();
            match &outcome {
                Ok(ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. }) => {
                    let expected_name = format!("{op:?}.pdf");
                    if new_files != vec![expected_name.clone()] {
                        problems.push(format!(
                            "{} · {op:?}: beklenmeyen dosyalar {new_files:?}",
                            fx.name
                        ));
                    }
                    match check::reopen_strict(&out) {
                        Ok(doc) => problems.extend(verify_output(&fx, op, &source, &doc)),
                        Err(e) => problems.push(format!(
                            "{} · {op:?}: yayınlanan çıktı açılmıyor: {e}",
                            fx.name
                        )),
                    }
                }
                Ok(ToolOutcome::NoBenefit { .. }) | Ok(ToolOutcome::Failed { .. }) | Err(_) => {
                    if !new_files.is_empty() {
                        problems.push(format!(
                            "{} · {op:?}: başarısız işlem dosya bıraktı {new_files:?} ({outcome:?})",
                            fx.name
                        ));
                    }
                    if std::env::var("HARDENING_DISCOVER").is_ok() {
                        println!("{:<48} {op:?}: {outcome:?}", fx.name);
                    }
                    // Kurtarılabilir bir belgede sıkıştırma dışı araç başarısız olmamalı.
                    if !matches!(op, Op::Compress) {
                        problems.push(format!(
                            "{} · {op:?}: açılabilen belgede araç başarısız: {outcome:?}",
                            fx.name
                        ));
                    } else if let Ok(ToolOutcome::Failed { reason }) = &outcome {
                        problems.push(format!(
                            "{} · Compress: Failed (NoBenefit değil): {reason}",
                            fx.name
                        ));
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn rejected_fixtures_never_produce_an_output_and_never_touch_the_source() {
    let mut problems = Vec::new();
    for fx in corpus::all() {
        if fx.expect != Expect::Rejects {
            continue;
        }
        let lab = Lab::new();
        let src = lab.write("kaynak/girdi.pdf", &fx.bytes);
        let before = check::sha(&src);
        for (label, operation) in [
            ("select", ToolOperation::Select { pages: vec![1] }),
            ("rotate", ToolOperation::Rotate { degrees: 90 }),
            (
                "compress",
                ToolOperation::Compress {
                    level: OptimizationLevel::BalancedCompression,
                },
            ),
        ] {
            let out = lab.path(&format!("cikti/{label}.pdf"));
            std::fs::create_dir_all(out.parent().unwrap()).unwrap();
            let (src2, out2) = (src.clone(), out.clone());
            let result = guarded(TOOL_TIMEOUT, move || {
                run_tool_with_outcome(&[src2], &operation, &out2, true).map_err(|e| e.to_string())
            });
            match result {
                Guarded::Done(Ok(
                    ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. },
                )) => problems.push(format!(
                    "{} · {label}: reddedilmesi gereken belge yayınlandı",
                    fx.name
                )),
                Guarded::Done(_) => {}
                other => problems.push(format!("{} · {label}: {other:?}", fx.name)),
            }
            if !check::tree(&lab.path("cikti")).is_empty() {
                problems.push(format!("{} · {label}: dosya bırakıldı", fx.name));
            }
            if check::sha(&src) != before {
                problems.push(format!("{} · {label}: KAYNAK DEĞİŞTİ", fx.name));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
