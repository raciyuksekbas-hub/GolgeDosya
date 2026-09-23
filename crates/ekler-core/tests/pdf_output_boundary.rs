//! PDF çıkış sınırının KAYNAK düzeyinde korunması.
//!
//! Ürün kuralı: GölgeDosya'nın ürettiği ya da değiştirdiği her PDF canonical
//! GölgeDosya işaretini taşır. Çalışma zamanında bunu iki şey garanti eder:
//! `finalize_pdf_output` (işaret + doğrulama) ve genel yazıcının PDF baytını
//! reddetmesi (`safe_io`). Bu dosya üçüncü kilidi takar: yeni bir kod yolunun
//! PDF'i bu kapıların YANINDAN diske yazmasını derleme kapısında yakalar.
//!
//! Saha hatası tam olarak böyle bir sınıftı: işaret her aracın kendi başına
//! çağırdığı bir adımdı ve Ekler dilimleri ortak yazıcıyı hiç kullanmadan
//! `File::create` ile yazılıyordu. Bir sayı değişirse bu test düşer ve
//! mesajı nereye bakılacağını söyler.
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Satır yorumları atılmış kaynak: açıklamalarda geçen adlar sayılmaz.
fn code(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `pattern`'in geçtiği her dosya ve sayısı, depo köküne göre.
fn occurrences(roots: &[&str], pattern: &str) -> Vec<(String, usize)> {
    let mut files = Vec::new();
    for root in roots {
        rust_files(&repo().join(root), &mut files);
    }
    let mut found: Vec<(String, usize)> = files
        .iter()
        .filter_map(|f| {
            let n = code(f).matches(pattern).count();
            (n > 0).then(|| {
                let rel = f
                    .strip_prefix(repo())
                    .unwrap_or(f)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel.trim_start_matches("./").to_string(), n)
            })
        })
        .collect();
    found.sort();
    found
}

fn expect(pattern: &str, roots: &[&str], allowed: &[(&str, usize)], why: &str) {
    let found = occurrences(roots, pattern);
    let mut allowed: Vec<(String, usize)> =
        allowed.iter().map(|(f, n)| (f.to_string(), *n)).collect();
    allowed.sort();
    assert_eq!(
        found, allowed,
        "`{pattern}` beklenmeyen yerde ya da sayıda.\n{why}\nBulunan: {found:#?}"
    );
}

const PDF_CRATES: &[&str] = &[
    "crates/pdf-core/src",
    "crates/ekler-core/src",
    "apps/belge-shell/src-tauri/src",
];

#[test]
fn a_pdf_is_serialised_only_at_the_boundary_or_for_measurement() {
    let why = "PDF serileştirmesi (lopdf `save_to`/`save`) yalnız şu yerlerde olabilir:\n\
               * `finalize_pdf_output` — yayına giden her PDF'in tek kapısı\n\
               * bellekte BOYUT ÖLÇÜMÜ (optimizer, sıkıştırma gövdesi, Ekler hedef boyutu)\n\
               * yerel renderer'ın `TempDir` içindeki çalışma kopyası\n\
               Kullanıcıya teslim edilecek bir PDF üretiyorsanız `finalize_pdf_output` + \
               `safe_io::publish_pdf` kullanın; kendi başınıza serileştirip yazmayın.";
    let allowed = [
        ("crates/pdf-core/src/pdf/stamp.rs", 1),
        ("crates/pdf-core/src/optimizer.rs", 2),
        ("crates/ekler-core/src/toolbox.rs", 1),
        ("crates/ekler-core/src/pipeline.rs", 1),
        // macOS/Windows çalışma kopyası (TempDir) ve Windows renderer'ının
        // BELLEKTEKİ normalize kopyası (diske yazılmaz, yayımlanmaz).
        ("crates/ekler-core/src/raster.rs", 3),
    ];
    expect(".save_to(", PDF_CRATES, &allowed, why);
    expect(".save(", PDF_CRATES, &[], why);
}

#[test]
fn ekler_core_creates_files_only_through_safe_io() {
    let why = "ekler-core dosyayı yalnız `safe_io` üzerinden yazar: genel yazıcı PDF baytını \
               reddeder, PDF `publish_pdf` (işaretli) ya da `publish_signed_original` \
               (imzalı orijinal) ile çıkar. Doğrudan dosya oluşturmak bu sınırı atlar — \
               Ekler dilimleri eskiden tam olarak böyle yazılıyordu.";
    let root = ["crates/ekler-core/src"];
    expect("File::create(", &root, &[], why);
    expect("fs::write(", &root, &[], why);
    expect("OpenOptions", &root, &[], why);
    expect(
        ".persist",
        &root,
        &[("crates/ekler-core/src/safe_io.rs", 1)],
        why,
    );
}

#[test]
fn only_the_boundary_brands_a_published_pdf() {
    let why = "İşaret, yayından önce `finalize_pdf_output` içinde basılır ve DOĞRULANIR. \
               Bir yayıcının `apply_branding`'i kendisi çağırması doğrulamayı atlar. Tek \
               istisna Ekler'in hedef-boyut ÖLÇÜMÜDÜR: ölçülen bayt, finalize'ın yayımladığıyla \
               birebir aynı olmalı.";
    expect(
        "apply_branding(",
        PDF_CRATES,
        &[
            // tanım + finalize içindeki çağrı
            ("crates/pdf-core/src/pdf/stamp.rs", 2),
            ("crates/ekler-core/src/pipeline.rs", 1),
        ],
        why,
    );
}

#[test]
fn every_publisher_goes_through_finalize() {
    // Yayıcı sayısı artarsa bu test düşer ve yeni yolun envantere (ve
    // `pdf_branding_invariant.rs` kapsamına) eklenmesini ister.
    let why = "PDF yayımlayan her yol burada sayılır: Düzenle araçları (yayım + sıkıştırma), \
               Ofis → PDF ve Ekler dilimleri. Yeni bir yol ekliyorsanız işaret testlerine de ekleyin.";
    expect(
        "finalize_pdf_output(",
        PDF_CRATES,
        &[
            ("crates/pdf-core/src/pdf/stamp.rs", 1),
            ("crates/ekler-core/src/toolbox.rs", 2),
            ("crates/ekler-core/src/office.rs", 1),
            ("crates/ekler-core/src/pipeline.rs", 1),
        ],
        why,
    );
    expect(
        "publish_signed_original(",
        PDF_CRATES,
        &[
            ("crates/ekler-core/src/safe_io.rs", 1),
            ("crates/ekler-core/src/pipeline.rs", 1),
        ],
        "İşaretsiz çıkabilen tek PDF, imzalı orijinalin bayt bayt kopyasıdır ve yalnız \
         Ekler'in \"olduğu gibi kullan\" politikasından gelir.",
    );
}
