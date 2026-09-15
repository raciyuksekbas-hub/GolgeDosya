//! v0.2.0 hardening — çökme değişmezi ve tohumlu mutation fuzzing.
//!
//! Hiçbir girdi panic / abort / yığın taşması / sonsuz döngü / kilitlenme
//! üretmemeli; kontrollü hata (recover ya da safe reject) kabul edilir. Her
//! çağrı, kabuğun komutları koştuğu 2 MiB'lik iş parçacığında ve bir zaman
//! aşımıyla korunur (bkz. hardening::check::guarded).
//!
//! `HARDENING_FUZZ_SEEDS=0..2000` ile tohum sayısı artırılabilir.

mod hardening;

use ekler_core::toolbox::{run_tool_with_outcome, PageRotation, ToolOperation, ToolOutcome};
use ekler_core::{load_pdf_tolerant, OptimizationLevel};
use hardening::check::{self, guarded, Guarded, Lab};
use hardening::corpus;
use hardening::rng::{self, Rng};
use std::time::Duration;

const LOAD_TIMEOUT: Duration = Duration::from_secs(30);
const TOOL_TIMEOUT: Duration = Duration::from_secs(120);

/// Ayrıştırıcıyı yığından taşıran bilinen girdiler: guard olmadan süreç `abort`
/// ederdi. Guard ile: kontrollü reddediliş, panik/kilitlenme YOK.
#[test]
fn known_stack_overflow_inputs_are_rejected_not_crashing() {
    use hardening::raw::RawPdf;
    let base = |extra: &str| {
        let mut pdf = RawPdf::new("1.7");
        pdf.object(3, b"<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>");
        pdf.object(10, b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]/Resources<</Font<</F1 3 0 R>>>>/Contents 11 0 R>>");
        pdf.stream(11, "", b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET");
        pdf.object(2, b"<</Type/Pages/Count 1/Kids[10 0 R]>>");
        pdf.object(
            1,
            format!("<</Type/Catalog/Pages 2 0 R{extra}>>").as_bytes(),
        );
        pdf.finish_classic("/Root 1 0 R");
        pdf.bytes
    };
    let deep_array = base(&format!(
        "/PieceInfo<</D<</Private {}{}>>>>",
        "[".repeat(20000),
        "]".repeat(20000)
    ));
    let deep_dict = base(&format!(
        "/PieceInfo<</D<</Private {}{}>>>>",
        "<</K ".repeat(20000),
        ">>".repeat(20000)
    ));
    let length_chain = {
        let mut pdf = RawPdf::new("1.7");
        pdf.object(3, b"<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>");
        pdf.object(10, b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]/Resources<</Font<</F1 3 0 R>>>>/Contents 11 0 R>>");
        pdf.stream(11, "", b"BT /F1 18 Tf 72 72 Td (Mk 1) Tj ET");
        pdf.object(2, b"<</Type/Pages/Count 1/Kids[10 0 R]>>");
        for k in 0..10000u32 {
            let id = 1000 + k;
            pdf.object(
                id,
                format!("<</Length {} 0 R>>stream\nabc\nendstream", id + 1).as_bytes(),
            );
        }
        pdf.object(11000, b"3");
        pdf.object(
            1,
            b"<</Type/Catalog/Pages 2 0 R/PieceInfo<</D<</Private 1000 0 R>>>>>>",
        );
        pdf.finish_classic("/Root 1 0 R");
        pdf.bytes
    };

    for (name, bytes) in [
        ("derin-dizi-20000", deep_array),
        ("derin-sozluk-20000", deep_dict),
        ("length-zinciri-10000", length_chain),
    ] {
        let result = guarded(LOAD_TIMEOUT, move || {
            load_pdf_tolerant(&bytes, "x.pdf")
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        match result {
            Guarded::Done(Ok(())) => panic!("{name}: patolojik girdi AÇILDI (reddedilmeliydi)"),
            Guarded::Done(Err(_)) => {}
            Guarded::Panicked(p) => panic!("{name}: PANİK/ABORT: {p}"),
            Guarded::TimedOut => panic!("{name}: KİLİTLENDİ"),
        }
    }
}

/// Geçerli bir PDF'in ham baytlarına yapılan tek bir tohumlu mutation.
fn mutate(bytes: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut out = bytes.to_vec();
    if out.is_empty() {
        return out;
    }
    match rng.below(9) {
        0 => {
            // Rastgele bir baytı değiştir.
            let i = rng.below(out.len());
            out[i] = rng.next_u64() as u8;
        }
        1 => {
            // `/Length N` sayısını boz.
            if let Some(p) = find(&out, b"/Length ") {
                let s = p + 8;
                let mut e = s;
                while e < out.len() && out[e].is_ascii_digit() {
                    e += 1;
                }
                if e > s {
                    out.splice(s..e, format!("{}", rng.range(0, 99999)).bytes());
                }
            }
        }
        2 => {
            // Bir dolaylı başvuruyu (` N 0 R`) olmayan nesneye çevir.
            if let Some(p) = find(&out, b" 0 R") {
                let mut s = p;
                while s > 0 && out[s - 1].is_ascii_digit() {
                    s -= 1;
                }
                out.splice(s..p, b"99999".to_vec());
            }
        }
        3 => {
            // `/Parent`ı kır.
            if let Some(p) = find(&out, b"/Parent ") {
                let s = p + 8;
                let mut e = s;
                while e < out.len() && out[e] != b'/' && out[e] != b'>' {
                    e += 1;
                }
                out.splice(s..e, b"99999 0 R".to_vec());
            }
        }
        4 => {
            // `/Contents`ı kaldır.
            if let Some(p) = find(&out, b"/Contents") {
                for b in out.iter_mut().skip(p).take(9) {
                    *b = b' ';
                }
            }
        }
        5 => {
            // `/Resources` anahtarını boz.
            if let Some(p) = find(&out, b"/Resources") {
                out[p + 1] = b'X';
            }
        }
        6 => {
            // Bir bölgeyi kes.
            let cut = rng.below(out.len());
            out.truncate(cut);
        }
        7 => {
            // `xref` sözcüğünü boz (startxref taraması zorlanır).
            if let Some(p) = find(&out, b"\nxref") {
                out[p + 1] = b'X';
            }
        }
        _ => {
            // `endobj`i kaldır (nesne sınırı belirsizleşir).
            if let Some(p) = find(&out, b"endobj") {
                for b in out.iter_mut().skip(p).take(6) {
                    *b = b' ';
                }
            }
        }
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn tool_ops() -> Vec<(&'static str, ToolOperation)> {
    vec![
        ("select", ToolOperation::Select { pages: vec![1] }),
        ("reorder", ToolOperation::Reorder { pages: vec![1] }),
        ("rotate", ToolOperation::Rotate { degrees: 90 }),
        (
            "rotatepages",
            ToolOperation::RotatePages {
                rotations: vec![PageRotation {
                    page: 1,
                    degrees: 270,
                }],
            },
        ),
        ("crop", ToolOperation::Crop { margin_pt: 5.0 }),
        (
            "watermark",
            ToolOperation::Watermark {
                text: "KOPYA".into(),
            },
        ),
        ("number", ToolOperation::Number { start: 1 }),
        (
            "compress",
            ToolOperation::Compress {
                level: OptimizationLevel::BalancedCompression,
            },
        ),
    ]
}

#[test]
fn mutated_documents_recover_or_reject_without_crash_leak_or_source_change() {
    let seeds = rng::seeds_from_env("HARDENING_FUZZ_SEEDS", &(0..300).collect::<Vec<_>>());
    let seed_pdfs: Vec<Vec<u8>> = corpus::all()
        .into_iter()
        .filter(|f| matches!(f.expect, corpus::Expect::Opens { .. }))
        .map(|f| f.bytes)
        .collect();
    let ops = tool_ops();
    let mut problems = Vec::new();
    let mut opened = 0usize;
    let mut rejected = 0usize;

    for seed in seeds {
        let mut rng = Rng::new(seed);
        let base = rng.pick(&seed_pdfs).clone();
        let bytes = mutate(&base, &mut rng);

        // 1) Yükleme çökmez/kilitlenmez.
        let load_bytes = bytes.clone();
        let load = guarded(LOAD_TIMEOUT, move || {
            load_pdf_tolerant(&load_bytes, "m.pdf")
                .map(|l| l.document.get_pages().len())
                .map_err(|e| e.to_string())
        });
        let loads_ok = match load {
            Guarded::Done(Ok(_)) => true,
            Guarded::Done(Err(_)) => false,
            Guarded::Panicked(p) => {
                problems.push(format!("tohum {seed}: yükleme PANİK: {p}"));
                continue;
            }
            Guarded::TimedOut => {
                problems.push(format!("tohum {seed}: yükleme KİLİTLENDİ"));
                continue;
            }
        };
        if loads_ok {
            opened += 1;
        } else {
            rejected += 1;
        }

        // 2) Her araç çökmez; başarısızlık kaynağı bozmaz, yarım çıktı bırakmaz.
        let op = rng.pick(&ops);
        let lab = Lab::new();
        let src = lab.write("kaynak/m.pdf", &bytes);
        let before = check::sha(&src);
        let out = lab.path("cikti/o.pdf");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        let (src2, out2, operation) = (src.clone(), out.clone(), op.1.clone());
        let run = guarded(TOOL_TIMEOUT, move || {
            run_tool_with_outcome(&[src2], &operation, &out2, true).map_err(|e| e.to_string())
        });
        match run {
            Guarded::Panicked(p) => {
                problems.push(format!("tohum {seed} · {}: araç PANİK: {p}", op.0));
                continue;
            }
            Guarded::TimedOut => {
                problems.push(format!("tohum {seed} · {}: araç KİLİTLENDİ", op.0));
                continue;
            }
            Guarded::Done(outcome) => {
                let published = matches!(
                    outcome,
                    Ok(ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. })
                );
                let files = check::tree(&lab.path("cikti"));
                if published {
                    if let Err(e) = check::reopen_strict(&out) {
                        problems.push(format!(
                            "tohum {seed} · {}: yayınlanan çıktı açılmıyor: {e}",
                            op.0
                        ));
                    }
                } else if !files.is_empty() {
                    problems.push(format!(
                        "tohum {seed} · {}: başarısız işlem dosya bıraktı: {files:?}",
                        op.0
                    ));
                }
            }
        }
        if check::sha(&src) != before {
            problems.push(format!("tohum {seed} · {}: KAYNAK DEĞİŞTİ", op.0));
        }
    }

    println!(
        "fuzz: {opened} açıldı, {rejected} reddedildi, {} sorun",
        problems.len()
    );
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
