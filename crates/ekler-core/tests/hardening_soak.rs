//! v0.2.0 hardening — D katmanı: uzun soluklu soak, kaynak tüketimi ve sızıntı.
//!
//! "Bir hukukçu yüzlerce belgeyle aylarca kullanırsa nerede patlar?" — burada
//! sentetik olarak: aynı belge üzerinde 100+ işlem zinciri, tekrarlı aç/işle,
//! dosya tanıtıcı (fd) ve geçici dosya sızıntısı.
//!
//! `HARDENING_SOAK_SEEDS` ve `HARDENING_SOAK_STEPS` ile ölçek artırılır.

mod hardening;

use ekler_core::toolbox::{run_tool_with_outcome, PageRotation, ToolOperation, ToolOutcome};
use ekler_core::{load_pdf_tolerant, OptimizationLevel};
use hardening::check::{self, Lab};
use hardening::corpus;
use hardening::rng::{self, Rng};

/// Zincirin her adımında sayfa sayısını koruyan ya da bilinen biçimde değiştiren
/// bir işlem seç. Çıktı bir sonraki adımın girdisidir (kullanıcı kaydedip açar).
fn pick_operation(rng: &mut Rng, pages: usize) -> (ToolOperation, i64) {
    // (işlem, çıktı sayfa sayısı deltası)
    loop {
        return match rng.below(9) {
            0 if pages > 1 => (
                ToolOperation::Select {
                    pages: rng.subset(pages),
                },
                0,
            ),
            1 => (
                ToolOperation::Reorder {
                    pages: {
                        let mut p: Vec<usize> = (1..=pages).collect();
                        rng.shuffle(&mut p);
                        p
                    },
                },
                0,
            ),
            2 if pages > 1 => {
                let victim = 1 + rng.below(pages);
                (
                    ToolOperation::Delete {
                        pages: vec![victim],
                    },
                    0,
                )
            }
            3 => (
                ToolOperation::Rotate {
                    degrees: *rng.pick(&[90, 180, 270, -90]),
                },
                0,
            ),
            4 => (
                ToolOperation::RotateSelected {
                    degrees: *rng.pick(&[90, -90, 180]),
                    pages: vec![1 + rng.below(pages)],
                },
                0,
            ),
            5 => (
                ToolOperation::RotatePages {
                    rotations: vec![PageRotation {
                        page: 1 + rng.below(pages),
                        degrees: *rng.pick(&[90, 180, 270]),
                    }],
                },
                0,
            ),
            6 => (ToolOperation::Crop { margin_pt: 3.0 }, 0),
            7 => (
                ToolOperation::Watermark {
                    text: format!("KOPYA {}", rng.below(1000)),
                },
                0,
            ),
            8 => (ToolOperation::Number { start: 1 }, 0),
            _ => continue,
        };
    }
}

/// Beklenen çıktı sayfa sayısı (Select/Delete için sayfa sayısını değiştirir).
fn expected_pages(op: &ToolOperation, pages: usize) -> usize {
    match op {
        ToolOperation::Select { pages: p } => p.len(),
        ToolOperation::Delete { pages: p } => pages - p.len(),
        _ => pages,
    }
}

#[test]
fn a_hundred_chained_operations_keep_a_valid_pdf_and_never_touch_the_source() {
    let seeds = rng::seeds_from_env("HARDENING_SOAK_SEEDS", &[1, 2, 3, 7, 42]);
    let steps = rng::usize_from_env("HARDENING_SOAK_STEPS", 120);
    let bases: Vec<Vec<u8>> = corpus::simple()
        .into_iter()
        .filter(|f| matches!(f.expect, corpus::Expect::Opens { pages, .. } if pages >= 6))
        .map(|f| f.bytes)
        .collect();
    assert!(!bases.is_empty());

    for seed in seeds {
        let mut rng = Rng::new(seed);
        let lab = Lab::new();
        // İlk kaynak; SHA'sı bütün zincir boyunca değişmemeli.
        let genesis = lab.write("genesis.pdf", rng.pick(&bases));
        let genesis_sha = check::sha(&genesis);

        let mut current = genesis.clone();
        let mut current_pages = load_pdf_tolerant(&std::fs::read(&current).unwrap(), "x.pdf")
            .unwrap()
            .document
            .get_pages()
            .len();

        for step in 0..steps {
            let (op, _) = pick_operation(&mut rng, current_pages);
            let want_pages = expected_pages(&op, current_pages);
            let out = lab.path(&format!("adim-{step}.pdf"));
            let before = check::sha(&current);
            let outcome = run_tool_with_outcome(std::slice::from_ref(&current), &op, &out, false);
            // Her adımda İLK kaynak değişmemeli.
            assert_eq!(
                check::sha(&genesis),
                genesis_sha,
                "seed {seed} adım {step}: İLK KAYNAK DEĞİŞTİ"
            );
            // Bir önceki adımın çıktısı (bu adımın girdisi) da değişmemeli.
            assert_eq!(
                check::sha(&current),
                before,
                "seed {seed} adım {step}: GİRDİ DEĞİŞTİ ({op:?})"
            );
            match outcome {
                Ok(ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. }) => {
                    let doc = check::reopen_strict(&out).unwrap_or_else(|e| {
                        panic!("seed {seed} adım {step} · {op:?}: çıktı açılmıyor: {e}")
                    });
                    let got = doc.get_pages().len();
                    assert_eq!(
                        got, want_pages,
                        "seed {seed} adım {step} · {op:?}: sayfa {got} (beklenen {want_pages})"
                    );
                    current = out;
                    current_pages = got;
                }
                other => panic!("seed {seed} adım {step} · {op:?}: {other:?}"),
            }
        }
        println!("seed {seed}: {steps} adım, son {current_pages} sayfa, kaynak değişmedi");
    }
}

/// macOS/Linux'ta bu sürecin açık dosya tanıtıcısı sayısı.
fn open_fd_count() -> usize {
    std::fs::read_dir("/dev/fd").map(|d| d.count()).unwrap_or(0)
}

#[test]
fn repeated_open_and_tool_do_not_leak_file_descriptors_or_temp_files() {
    let lab = Lab::new();
    let fx = corpus::simple()
        .into_iter()
        .find(|f| f.name == "basit/karma-10")
        .unwrap();
    let src = lab.write("kaynak.pdf", &fx.bytes);
    let rounds = rng::usize_from_env("HARDENING_LEAK_ROUNDS", 200);

    // Isınma: ilk birkaç tur tembel/statik tahsisleri kurar; ondan sonra ölç.
    let mut baseline = 0usize;
    for round in 0..rounds {
        // Aç → incele → kapat.
        {
            let bytes = std::fs::read(&src).unwrap();
            let loaded = load_pdf_tolerant(&bytes, "k.pdf").unwrap();
            let _ = loaded.document.get_pages().len();
        }
        // Bir araç çalıştır, çıktıyı sil (kullanıcı kaydedip siler).
        let out = lab.path(&format!("o-{round}.pdf"));
        let op = if round % 2 == 0 {
            ToolOperation::Rotate { degrees: 90 }
        } else {
            ToolOperation::Compress {
                level: OptimizationLevel::GentleCompression,
            }
        };
        let _ = run_tool_with_outcome(std::slice::from_ref(&src), &op, &out, false);
        let _ = std::fs::remove_file(&out);

        if round == 20 {
            baseline = open_fd_count();
        }
    }
    let after = open_fd_count();
    // Küçük bir tolerans (raporlama/önbellek dalgalanması); sürekli büyüme olmamalı.
    assert!(
        after <= baseline + 8,
        "fd sızıntısı: temel {baseline}, {rounds} tur sonrası {after}"
    );

    // Geçici dosya sızıntısı: çıktı dizininde bizim `.tmp*` artığımız kalmamalı.
    let leftovers: Vec<_> = check::tree(lab.dir.path())
        .into_iter()
        .filter(|n| {
            let base = n.rsplit('/').next().unwrap_or(n);
            base.starts_with('.') && base.contains("tmp")
        })
        .collect();
    assert!(leftovers.is_empty(), "geçici dosya sızdı: {leftovers:?}");
    println!("fd: temel {baseline} → {after} ({rounds} tur); geçici artık yok");
}

/// Aynı belgeyi 500 kez aç-incele-kapat: fd sabit kalmalı.
#[test]
fn five_hundred_open_inspect_close_cycles_do_not_leak() {
    let lab = Lab::new();
    let fx = corpus::simple()
        .into_iter()
        .find(|f| f.name == "basit/100-sayfa-metin")
        .unwrap();
    let src = lab.write("k.pdf", &fx.bytes);
    let cycles = rng::usize_from_env("HARDENING_OPEN_CYCLES", 500);
    let mut baseline = 0usize;
    for i in 0..cycles {
        let bytes = std::fs::read(&src).unwrap();
        let loaded = load_pdf_tolerant(&bytes, "k.pdf").unwrap();
        let n = loaded.document.get_pages().len();
        assert_eq!(n, 100);
        // Önizleme yolu da bir kez: kabuğun boş sayfa taraması.
        let _ = ekler_core::pdf::inspect_pdf(&src);
        if i == 30 {
            baseline = open_fd_count();
        }
    }
    let after = open_fd_count();
    assert!(
        after <= baseline + 8,
        "fd sızıntısı: temel {baseline}, {cycles} döngü sonrası {after}"
    );
}
