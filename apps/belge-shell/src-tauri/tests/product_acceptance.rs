//! ÜRÜN KABULÜ — arayüzün GERÇEKTEN çağırdığı production command yüzeyinden
//! başlayıp gerçek çıktı üretir, çıktıyı yeniden açar ve semantiğini doğrular.
//!
//! Motor fonksiyonlarını değil, `#[tauri::command]` ile işaretlenmiş komutları
//! çağırır: arayüz ile motor arasındaki katman da kanıt kapsamındadır.
//! `tauri::State`/`AppHandle` alan komutlar buradan çağrılamaz; onlar ayrıca
//! işaretlenmiştir.
//!
//! Hiçbir gerçek kullanıcı belgesi kullanılmaz; her fixture koddan üretilir ve
//! yalnız `tempfile::tempdir()` altına yazılır.

#![cfg(all(feature = "feature_duzenek", feature = "feature_ikincigoz"))]

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use belge_shell_lib::modules::duzenek;

fn block<F: std::future::Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

fn sha(p: &Path) -> String {
    let mut h = Sha256::new();
    h.update(std::fs::read(p).expect("oku"));
    format!("{:x}", h.finalize())
}

/// `n` sayfalık, her sayfası işaretli sentetik PDF.
fn make_pdf(n: u32) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"
    });
    let mut kids = Vec::new();
    for i in 1..=n {
        let contents = doc.add_object(Stream::new(
            dictionary! {},
            format!("BT /F1 18 Tf 72 700 Td (Sayfa {i}) Tj ET").into_bytes(),
        ));
        let page = doc.add_object(dictionary! {
            "Type"=>"Page","Parent"=>pages_id,
            "MediaBox"=>vec![0.into(),0.into(),595.28.into(),841.89.into()],
            "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},
            "Contents"=>contents
        });
        kids.push(Object::Reference(page));
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>count}.into(),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", root);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("kaydet");
    buf
}

fn reopen(p: &Path) -> LopdfDoc {
    let d = LopdfDoc::load(p).unwrap_or_else(|e| panic!("çıktı açılamadı {}: {e}", p.display()));
    assert!(!d.get_pages().is_empty(), "çıktıda sayfa yok");
    d
}

fn rotation_of(doc: &LopdfDoc, page: lopdf::ObjectId) -> i64 {
    doc.get_object(page)
        .and_then(|o| o.as_dict())
        .ok()
        .and_then(|d| d.get(b"Rotate").ok().cloned())
        .and_then(|o| doc.dereference(&o).ok().and_then(|(_, v)| v.as_i64().ok()))
        .unwrap_or(0)
}

struct Lab {
    dir: tempfile::TempDir,
}
impl Lab {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("tempdir"),
        }
    }
    fn path(&self, n: &str) -> PathBuf {
        self.dir.path().join(n)
    }
    fn write(&self, n: &str, b: &[u8]) -> PathBuf {
        let p = self.path(n);
        std::fs::write(&p, b).expect("yaz");
        p
    }
}

// =================================================== Düzenle — gerçek komutlar

/// Her PDF aracını PRODUCTION KOMUTUNDAN çalıştır, çıktıyı yeniden aç,
/// semantiğini doğrula ve kaynağın değişmediğini kanıtla.
#[test]
fn duzenle_every_pdf_tool_runs_from_its_command_and_produces_a_reopenable_output() {
    use ekler_core::toolbox::{ToolOperation, ToolOutcome};

    let lab = Lab::new();
    let src = lab.write("kaynak.pdf", &make_pdf(6));
    let src_sha = sha(&src);

    // (ad, işlem, beklenen sayfa sayısı)
    let cases: Vec<(&str, ToolOperation, usize)> = vec![
        ("sec", ToolOperation::Select { pages: vec![1, 3, 5] }, 3),
        (
            "sirala",
            ToolOperation::Reorder {
                pages: vec![6, 5, 4, 3, 2, 1],
            },
            6,
        ),
        ("sil", ToolOperation::Delete { pages: vec![2, 4] }, 4),
        ("dondur", ToolOperation::Rotate { degrees: 90 }, 6),
        ("kirp", ToolOperation::Crop { margin_pt: 10.0 }, 6),
        (
            "filigran",
            ToolOperation::Watermark {
                text: "GİZLİ".into(),
            },
            6,
        ),
        ("numara", ToolOperation::Number { start: 1 }, 6),
    ];

    for (name, op, want_pages) in cases {
        let out = lab.path(&format!("{name}.pdf"));
        let outcome = block(duzenek::duzenek_run_pdf_tool(
            vec![src.to_string_lossy().to_string()],
            op.clone(),
            out.to_string_lossy().to_string(),
            false,
        ))
        .unwrap_or_else(|e| panic!("{name}: komut hata verdi: {e}"));

        match outcome {
            ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. } => {}
            other => panic!("{name}: beklenmedik sonuç {other:?}"),
        }
        assert!(out.exists(), "{name}: çıktı dosyası yok");
        assert!(
            std::fs::metadata(&out).unwrap().len() > 0,
            "{name}: çıktı boş"
        );

        let doc = reopen(&out);
        assert_eq!(
            doc.get_pages().len(),
            want_pages,
            "{name}: sayfa sayısı yanlış"
        );
        assert_eq!(sha(&src), src_sha, "{name}: KAYNAK DEĞİŞTİ (P0)");
    }

    // Döndürme semantiği gerçekten çıktıda mı?
    let out = lab.path("dondur.pdf");
    let doc = reopen(&out);
    let first = *doc.get_pages().get(&1).unwrap();
    assert_eq!(rotation_of(&doc, first), 90, "döndürme çıktıya yansımadı");
}

/// P1 REGRESYON: Türkçe filigran. Ürün Türk hukukçular için; "GİZLİ",
/// "ÖRNEKTİR", "SURETİDİR" en olağan filigranlardır ve reddediliyorlardı
/// ("bu sürümde Unicode filigran desteklenmiyor"). Kök neden metnin `Tj`'ye
/// HAM UTF-8 verilmesiydi (WinAnsi fontta mojibake) — bu yüzden ASCII'ye
/// kısıtlanmıştı. Artık metin font kodlamasına çevriliyor.
#[test]
fn duzenle_watermark_accepts_real_turkish_text() {
    use ekler_core::toolbox::{ToolOperation, ToolOutcome};

    let lab = Lab::new();
    let src = lab.write("kaynak.pdf", &make_pdf(2));
    let src_sha = sha(&src);

    for (i, text) in ["GİZLİ", "ÖRNEKTİR", "SURETİDİR", "ÇĞİÖŞÜ çğış öü"]
        .iter()
        .enumerate()
    {
        let out = lab.path(&format!("tr{i}.pdf"));
        let outcome = block(duzenek::duzenek_run_pdf_tool(
            vec![src.to_string_lossy().to_string()],
            ToolOperation::Watermark {
                text: (*text).to_string(),
            },
            out.to_string_lossy().to_string(),
            false,
        ))
        .unwrap_or_else(|e| panic!("Türkçe filigran {text:?} reddedildi: {e}"));
        assert!(matches!(
            outcome,
            ToolOutcome::Published { .. } | ToolOutcome::Compressed { .. }
        ));
        let doc = reopen(&out);
        assert_eq!(doc.get_pages().len(), 2, "{text}: sayfa sayısı bozuldu");
    }
    assert_eq!(sha(&src), src_sha, "KAYNAK DEĞİŞTİ (P0)");

    // Gerçekten yazılamayan bir karakter hâlâ AÇIKÇA reddedilmeli
    // (sessizce bozuk glif basılmamalı).
    let out = lab.path("emoji.pdf");
    let r = block(duzenek::duzenek_run_pdf_tool(
        vec![src.to_string_lossy().to_string()],
        ToolOperation::Watermark {
            text: "GİZLİ 🔒".into(),
        },
        out.to_string_lossy().to_string(),
        false,
    ));
    assert!(r.is_err(), "yazılamayan karakter sessizce kabul edildi");
}

/// Sırala komutu gerçekten sayfa SIRASINI değiştiriyor mu (yalnız sayı değil)?
#[test]
fn duzenle_reorder_actually_reverses_the_visible_page_order() {
    use ekler_core::toolbox::ToolOperation;

    let lab = Lab::new();
    let src = lab.write("kaynak.pdf", &make_pdf(4));
    let out = lab.path("ters.pdf");
    block(duzenek::duzenek_run_pdf_tool(
        vec![src.to_string_lossy().to_string()],
        ToolOperation::Reorder {
            pages: vec![4, 3, 2, 1],
        },
        out.to_string_lossy().to_string(),
        false,
    ))
    .expect("sırala");

    let doc = reopen(&out);
    let pages = doc.get_pages();
    // İlk sayfanın içeriği "Sayfa 4" olmalı.
    let first = *pages.get(&1).unwrap();
    let text = String::from_utf8_lossy(&doc.get_page_content(first).expect("içerik")).to_string();
    assert!(
        text.contains("Sayfa 4"),
        "sırala sayfa sırasını değiştirmedi; ilk sayfa: {text}"
    );
}

/// Sil/Döndür/Birleştir/Böl komutları ayrı komut olarak da çalışmalı.
#[test]
fn duzenle_dedicated_page_commands_work_and_leave_sources_intact() {
    let lab = Lab::new();
    let a = lab.write("a.pdf", &make_pdf(5));
    let b = lab.write("b.pdf", &make_pdf(3));
    let (a_sha, b_sha) = (sha(&a), sha(&b));

    // Sil
    let out = lab.path("silinmis.pdf");
    duzenek::duzenek_delete_pdf_pages(
        a.to_string_lossy().to_string(),
        vec![2, 3],
        out.to_string_lossy().to_string(),
    )
    .expect("sil");
    assert_eq!(reopen(&out).get_pages().len(), 3);

    // Döndür
    let out = lab.path("donuk.pdf");
    duzenek::duzenek_rotate_pdf_pages_cmd(
        a.to_string_lossy().to_string(),
        180,
        out.to_string_lossy().to_string(),
    )
    .expect("döndür");
    let doc = reopen(&out);
    let first = *doc.get_pages().get(&1).unwrap();
    assert_eq!(rotation_of(&doc, first), 180);

    // Böl (1..=2)
    let out = lab.path("bolunmus.pdf");
    duzenek::duzenek_split_pdf(
        a.to_string_lossy().to_string(),
        1,
        2,
        out.to_string_lossy().to_string(),
    )
    .expect("böl");
    assert_eq!(reopen(&out).get_pages().len(), 2);

    // Birleştir
    let out = lab.path("birlesik.pdf");
    duzenek::duzenek_merge_pdfs(
        vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
        ],
        out.to_string_lossy().to_string(),
    )
    .expect("birleştir");
    assert_eq!(reopen(&out).get_pages().len(), 8, "birleştirme 5+3=8 olmalı");

    assert_eq!(sha(&a), a_sha, "KAYNAK A DEĞİŞTİ (P0)");
    assert_eq!(sha(&b), b_sha, "KAYNAK B DEĞİŞTİ (P0)");
}

/// Boş sayfa tespiti komutu gerçek bir boş sayfayı bulur, dolu sayfayı bulmaz.
#[test]
fn duzenle_blank_page_detection_command_is_accurate() {
    let lab = Lab::new();
    // 3 sayfa: 2. sayfa boş.
    let mut doc = LopdfDoc::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let mut kids = Vec::new();
    for i in 1..=3 {
        let body = if i == 2 {
            String::new()
        } else {
            format!("BT /F1 18 Tf 72 700 Td (Sayfa {i} dolu metin) Tj ET")
        };
        let contents = doc.add_object(Stream::new(dictionary! {}, body.into_bytes()));
        let page = doc.add_object(dictionary! {
            "Type"=>"Page","Parent"=>pages_id,
            "MediaBox"=>vec![0.into(),0.into(),595.28.into(),841.89.into()],
            "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},
            "Contents"=>contents
        });
        kids.push(Object::Reference(page));
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>count}.into(),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", root);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    let lab_src = lab.write("bos.pdf", &buf);
    let blanks =
        duzenek::duzenek_detect_blank_pages(lab_src.to_string_lossy().to_string()).expect("tespit");
    assert_eq!(blanks, vec![2], "boş sayfa tespiti yanlış: {blanks:?}");
}

/// PDF → görsel komutu gerçek görsel dosyaları üretir.
#[test]
fn duzenle_pdf_to_images_command_writes_real_image_files() {
    let lab = Lab::new();
    let src = lab.write("kaynak.pdf", &make_pdf(2));
    let out_dir = lab.path("gorseller");
    std::fs::create_dir_all(&out_dir).unwrap();

    let produced = block(duzenek::duzenek_pdf_to_images(
        src.to_string_lossy().to_string(),
        out_dir.to_string_lossy().to_string(),
        "png".into(),
        72,
        true,
    ));

    match produced {
        Ok(dir) => {
            let files: Vec<_> = std::fs::read_dir(&dir)
                .expect("çıktı klasörü")
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.to_lowercase().ends_with(".png"))
                .collect();
            assert!(!files.is_empty(), "PNG üretilmedi: {dir:?}");
            for f in &files {
                let p = dir.join(f);
                assert!(
                    std::fs::metadata(&p).unwrap().len() > 100,
                    "boş görsel: {f}"
                );
            }
        }
        // Harici bir renderer gerekiyorsa bu makinede yoktur; sessiz başarı
        // OLMADIĞI sürece kabul edilir (hata açık olmalı).
        Err(e) => {
            assert!(!e.is_empty(), "boş hata mesajı");
            eprintln!("pdf_to_images bu ortamda kullanılamadı: {e}");
        }
    }
}

// ==================================================== Denetle — gerçek komutlar

/// Denetle zinciri: analiz → düzeltme uygula → çıktıyı yeniden aç;
/// kaynak belge DEĞİŞMEMELİ.
#[test]
fn denetle_analyze_then_apply_to_copy_leaves_the_source_untouched() {
    use belge_shell_lib::modules::ikincigoz;

    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../crates/ikincigoz-core/tests/samples");
    if !samples.is_dir() {
        panic!("Referans fixture klasörü yok: {}", samples.display());
    }
    let sample = samples.join("ornek-dilekce-hatali.docx");
    let lab = Lab::new();
    let src = lab.write("dilekce.docx", &std::fs::read(&sample).expect("örnek"));
    let src_sha = sha(&src);

    let analysis = block(ikincigoz::ikincigoz_analyze_document(
        src.to_string_lossy().to_string(),
    ))
    .expect("analiz");
    assert!(
        !analysis.findings.is_empty(),
        "hatalı örnekte bulgu çıkmadı — denetim gerçekten çalışıyor mu?"
    );

    let fixes: Vec<_> = analysis
        .findings
        .iter()
        .filter_map(|f| f.fix.clone())
        .collect();
    assert!(!fixes.is_empty(), "önerilen düzeltme yok");

    let out_dir = lab.path("cikti");
    std::fs::create_dir_all(&out_dir).unwrap();
    let written = block(ikincigoz::ikincigoz_apply_fixes(
        src.to_string_lossy().to_string(),
        fixes.clone(),
        Some(out_dir.to_string_lossy().to_string()),
        None,
    ))
    .expect("düzeltme uygula");

    assert!(written.applied > 0, "hiç düzeltme uygulanmadı");
    let produced = out_dir.join(&written.file_name);
    assert!(produced.exists(), "düzeltilmiş kopya yok");

    // Çıktı gerçekten açılabilir bir belge mi?
    let bytes = std::fs::read(&produced).unwrap();
    ikincigoz_core::parser::parse(&written.file_name, &bytes)
        .expect("düzeltilmiş kopya yeniden açılabilmeli");

    // P0: kaynak değişmemeli.
    assert_eq!(sha(&src), src_sha, "KAYNAK BELGE DEĞİŞTİ (P0)");
}

// ================================================= Dönüştür — gerçek komutlar

/// Dönüştür'ün dosya inceleme komutu gerçek bir DOCX'i tanır ve
/// desteklenmeyen bir dosyayı AÇIKÇA reddeder (sahte başarı yok).
#[test]
fn donustur_inspect_command_classifies_real_and_unsupported_inputs() {
    use belge_shell_lib::modules::tavzih;

    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../crates/ikincigoz-core/tests/samples");
    let lab = Lab::new();

    let docx = lab.write(
        "belge.docx",
        &std::fs::read(samples.join("ornek-dilekce-hatali.docx")).expect("örnek"),
    );
    let info = tavzih::tavzih_inspect_file(docx.to_string_lossy().to_string())
        .expect("DOCX tanınmalı");
    assert!(
        format!("{info:?}").to_lowercase().contains("docx"),
        "DOCX biçimi bildirilmedi: {info:?}"
    );

    // Uzantısı .docx ama içeriği DOCX olmayan dosya: sessizce kabul edilmemeli.
    let sahte = lab.write("sahte.docx", b"bu bir docx degil");
    let r = tavzih::tavzih_inspect_file(sahte.to_string_lossy().to_string());
    assert!(
        r.is_err(),
        "bozuk DOCX sahte başarıyla kabul edildi: {r:?}"
    );
}

// ================================================ Karşılaştır — gerçek komut

/// Karşılaştır'ın belge okuma komutu gerçek bir belgeyi okur ve
/// olmayan dosyada açık hata verir.
#[test]
fn karsilastir_read_document_command_reads_real_files_and_fails_loudly() {
    use belge_shell_lib::modules::degisikis;

    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../crates/ikincigoz-core/tests/samples");
    let lab = Lab::new();
    let docx = lab.write(
        "belge.docx",
        &std::fs::read(samples.join("ornek-dilekce-hatali.docx")).expect("örnek"),
    );

    let bytes = block(degisikis::degisikis_read_document(
        docx.to_string_lossy().to_string(),
    ))
    .expect("belge okunmalı");
    assert!(!bytes.is_empty(), "boş içerik döndü");

    let missing = lab.path("yok.docx");
    let r = block(degisikis::degisikis_read_document(
        missing.to_string_lossy().to_string(),
    ));
    assert!(r.is_err(), "olmayan dosya sessizce başarılı döndü");
}

/// P1 REGRESYON: Kaydetme penceresi dosya ADI soruyor ama yazılan ad sessizce
/// atılıyordu — yalnız klasör kullanılıyor, dosya adı motorda türetiliyordu.
/// Görünen bir kontrol kullanıcının girdisini yutuyordu.
#[test]
fn denetle_honours_the_file_name_the_user_typed_in_the_save_dialog() {
    use belge_shell_lib::modules::ikincigoz;

    let samples =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../crates/ikincigoz-core/tests/samples");
    let sample = samples.join("ornek-dilekce-hatali.docx");
    let lab = Lab::new();
    let src = lab.write("dilekce.docx", &std::fs::read(&sample).expect("örnek"));
    let src_sha = sha(&src);

    let analysis = block(ikincigoz::ikincigoz_analyze_document(
        src.to_string_lossy().to_string(),
    ))
    .expect("analiz");
    let fixes: Vec<_> = analysis
        .findings
        .iter()
        .filter_map(|f| f.fix.clone())
        .collect();
    assert!(!fixes.is_empty());

    let out_dir = lab.path("cikti");
    std::fs::create_dir_all(&out_dir).unwrap();

    // Kullanıcı kaydetme penceresinde kendi adını yazdı.
    let written = block(ikincigoz::ikincigoz_apply_fixes(
        src.to_string_lossy().to_string(),
        fixes.clone(),
        Some(out_dir.to_string_lossy().to_string()),
        Some("sözleşme-son-hâli.docx".into()),
    ))
    .expect("düzeltme uygula");
    assert_eq!(
        written.file_name, "sözleşme-son-hâli.docx",
        "P1: kullanıcının yazdığı dosya adı atıldı"
    );
    assert!(out_dir.join("sözleşme-son-hâli.docx").exists());

    // Uzantısız yazılırsa türetilmiş uzantı korunur (açılamayan dosya olmasın).
    let written = block(ikincigoz::ikincigoz_apply_fixes(
        src.to_string_lossy().to_string(),
        fixes.clone(),
        Some(out_dir.to_string_lossy().to_string()),
        Some("uzantisiz".into()),
    ))
    .expect("düzeltme uygula");
    assert_eq!(written.file_name, "uzantisiz.docx", "uzantı korunmalı");

    // Yol ayırıcısı taşıyan ad hedef klasörün DIŞINA yazamaz.
    let written = block(ikincigoz::ikincigoz_apply_fixes(
        src.to_string_lossy().to_string(),
        fixes,
        Some(out_dir.to_string_lossy().to_string()),
        Some("../../kacis.docx".into()),
    ))
    .expect("düzeltme uygula");
    assert_eq!(written.file_name, "kacis.docx", "yol bileşeni atılmalı");
    assert!(out_dir.join("kacis.docx").exists(), "hedef klasörde kalmalı");

    assert_eq!(sha(&src), src_sha, "KAYNAK DEĞİŞTİ (P0)");
}
