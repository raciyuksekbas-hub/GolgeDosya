use crate::error::{EklerError, Result};
use lopdf::Document as LopdfDoc;

#[derive(Debug, Clone)]
pub struct TolerantLoadResult {
    pub strategy: RepairStrategy,
    pub document: LopdfDoc,
    pub is_repaired: bool,
    pub repair_note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairStrategy {
    Strict,
    XrefNormalization,
    /// Sayfa görünüm grafiği eksiksiz; yalnız onun dışındaki eksik nesne
    /// başvuruları null sayıldı.
    DanglingReferences,
}

/// Çok katmanlı toleranslı PDF yükleyici:
/// Tier 1: Standart katı parse (lopdf::Document::load_mem)
/// Tier 2: Bellek içi normalizasyon ve yapısal onarım:
///   - Xref satır sonu normalizasyonu (19-baytlık veya boşluksuz `f\n` / `n\n` girişleri 20-bayt standardına getirme)
///   - Dosya sonu (%%EOF) sonrasındaki artık/çöp baytların temizlenmesi
///   - startxref offset taraması ve düzeltilmesi
///
/// Her iki katmanda da okunan belge normalize edilir ve katı kurallarla YENİDEN
/// doğrulanır (`canonicalize`): toleranslı oku → normalize et → doğrula.
///
/// Unknown damage is rejected; object-scanning reconstruction is intentionally disabled.
pub fn load_pdf_tolerant(bytes: &[u8], file_name: &str) -> Result<TolerantLoadResult> {
    // Ayrıştırma öncesi yapısal güvenlik: lopdf'in özyinelemeli ayrıştırıcısı
    // yeterince derin bir girdide yığını taşırır ve süreç `abort` eder. Bu
    // tarama patolojik yuvalanmayı ve `/Length` başvuru zincirini ayrıştırıcı
    // çağrılmadan önce reddeder (bkz. guard.rs).
    super::guard::check_structure(bytes)?;

    // Tier 1: Doğrudan ve müdahalesiz hızlı yükleme
    if let Ok(mut doc) = LopdfDoc::load_mem(bytes) {
        let detached = canonicalize(&mut doc)?;
        return Ok(TolerantLoadResult {
            strategy: if detached == 0 {
                RepairStrategy::Strict
            } else {
                RepairStrategy::DanglingReferences
            },
            document: doc,
            is_repaired: detached > 0,
            repair_note: (detached > 0).then(|| dangling_note(detached)),
        });
    }

    // Dosyada asgari PDF başlığı kontrolü
    if !bytes.windows(5).any(|w| w == b"%PDF-") {
        return Err(EklerError::InvalidPdf(format!(
            "'{}' standart PDF yapısında okunamadı: Geçerli bir PDF başlığı (%PDF-) bulunamadı.",
            file_name
        )));
    }

    // Tier 2: Toleranslı normalizasyon ve onarım denemeleri
    // Adım 2.1: Xref ve startxref normalizasyonu
    if let Ok(normalized_bytes) = normalize_xref_and_startxref(bytes) {
        if let Ok(mut doc) = LopdfDoc::load_mem(&normalized_bytes) {
            let detached = canonicalize(&mut doc)?;
            let mut note =
                "Standart dışı xref tablosu ve satır sonları normalize edildi.".to_string();
            if detached > 0 {
                note = format!("{note} {}", dangling_note(detached));
            }
            return Ok(TolerantLoadResult {
                strategy: RepairStrategy::XrefNormalization,
                document: doc,
                is_repaired: true,
                repair_note: Some(note),
            });
        }
    }

    Err(EklerError::InvalidPdf(format!(
        "'{}' standart PDF yapısında okunamadı. Dosya şifreli, eksik veya ağır hasarlı olabilir.",
        file_name
    )))
}

/// Okunan belgeyi normalize eder ve katı kurallarla doğrular. Dönen sayı, null
/// sayılan eksik nesne başvurularıdır.
fn canonicalize(doc: &mut LopdfDoc) -> Result<usize> {
    unlock_if_owner_protected(doc)?;
    detach_from_source_layout(doc);
    // Şifreli belge ayrıştırılmış sayılmaz; reddi validate_document kendi
    // sözüyle verir.
    let detached = if doc.trailer.has(b"Encrypt") {
        0
    } else {
        super::validate::detach_dangling_references(doc)?
    };
    super::validate_document(doc)?;
    Ok(detached)
}


/// Standart güvenlik handler'ının alanları spec'e uygun mu?
///
/// Amaç güvenlik denetimi değil, ÇÖZÜCÜYE SAĞLAM GİRDİ vermek: eksik ya da
/// kısa alanlar aşağıdaki katmanda dilim taşmasına yol açıyor.
fn standard_handler_is_well_formed(doc: &LopdfDoc) -> std::result::Result<(), String> {
    const MALFORMED: &str =
        "Bu PDF'in şifreleme bilgisi eksik ya da hasarlı. Kaynak uygulamada şifresiz bir kopya oluşturun.";
    let dict = doc
        .get_encrypted()
        .map_err(|_| MALFORMED.to_string())?;
    // Yalnız standart handler çözülebilir; diğerleri aşağıda zaten reddedilir.
    if dict.get(b"Filter").and_then(|f| f.as_name()).ok() != Some(b"Standard") {
        return Ok(());
    }
    for key in [b"O".as_slice(), b"U".as_slice()] {
        let value = dict
            .get(key)
            .and_then(|v| v.as_str())
            .map_err(|_| MALFORMED.to_string())?;
        // PDF 32000-1, Tablo 21: R2–R4 için 32 baytlık dize.
        if value.len() < 32 {
            return Err(MALFORMED.to_string());
        }
    }
    if dict.get(b"P").and_then(|v| v.as_i64()).is_err() {
        return Err(MALFORMED.to_string());
    }
    Ok(())
}

/// Yalnız **sahip parolası** taşıyan belgeleri açar.
///
/// Banka, Findeks, icra ve mahkeme çıktılarının çoğu şifreli üretilir: sahip
/// parolası konur, kullanıcı parolası BOŞ bırakılır. Bu bir erişim kilidi
/// değildir — Preview, Acrobat ve her görüntüleyici bu belgeyi sormadan açar,
/// çünkü boş kullanıcı parolası belgeyi çözmeye yeter. `/P` alanındaki
/// kısıtlar (kopyalama, düzenleme) bu katmanda tavsiye niteliğindedir.
///
/// Eskiden `validate_document` `/Encrypt` gören her belgeyi koşulsuz
/// reddediyordu: sahada gerçek bir Findeks kredi raporu "Dosya bozulmuş
/// olabilir" denilerek geri çevrildi — oysa dosya sağlamdı ve 22 sayfası
/// Preview'de açılıyordu.
///
/// GERÇEKTEN parola isteyen belge hâlâ reddedilir; parola tahmini yapılmaz,
/// sahip parolası kırılmaz. Desteklenmeyen şema (AES, V≥4) da reddedilir —
/// kör tolerans eklenmez, sebep doğru söylenir.
fn unlock_if_owner_protected(doc: &mut LopdfDoc) -> Result<()> {
    if !doc.trailer.has(b"Encrypt") {
        return Ok(());
    }
    // Şifreleme sözlüğü lopdf'e verilmeden ÖNCE doğrulanır.
    //
    // lopdf'in parola denetimi `/U` değerini uzunluk denetimi olmadan
    // `expected[..16]` ile dilimler (encryption.rs:160). Kısaltılmış bir `/U`
    // süreci PANİKLETİR: Tauri komut iş parçacığı çözülür, `invoke` sözü hiç
    // tamamlanmaz ve kullanıcı için uygulama donar. Bozuk ya da kötü niyetli
    // tek bir dosya bunu tetiklemeye yeter.
    //
    // Standart güvenlik handler'ında (R2–R4) `/O` ve `/U` TAM 32 bayttır;
    // olmayan belge zaten spec dışıdır ve çözülemez. Panik yerine dürüst bir
    // ret verilir.
    if let Err(reason) = standard_handler_is_well_formed(doc) {
        return Err(EklerError::InvalidPdf(reason));
    }
    match doc.decrypt("") {
        // Başarılı çözümde lopdf `/Encrypt`'i trailer'dan kaldırır; belge
        // bundan sonrası için sıradan bir PDF'tir.
        Ok(()) => Ok(()),
        Err(lopdf::Error::Decryption(lopdf::encryption::DecryptionError::IncorrectPassword)) => {
            Err(EklerError::InvalidPdf(
                "Bu PDF bir parola ile korunuyor. Açmak için parolasız bir kopya oluşturun."
                    .to_string(),
            ))
        }
        Err(_) => Err(EklerError::InvalidPdf(
            "Bu PDF'in şifreleme yöntemi desteklenmiyor. Kaynak uygulamada şifresiz bir kopya oluşturun."
                .to_string(),
        )),
    }
}

fn dangling_note(detached: usize) -> String {
    format!(
        "Sayfa içeriğine ve kaynaklarına ait olmayan {detached} eksik nesne başvurusu PDF standardına göre boş sayıldı; kaynak dosya değiştirilmedi."
    )
}

/// Yükleyicinin verdiği belge BÜTÜN bir belgedir; kaynak dosyanın fiziksel
/// yerleşimine ait trailer girdilerini taşımaz.
///
/// lopdf 0.34, artımlı güncellenmiş bir dosyanın — Word'ün hibrit başvurulu
/// "PDF olarak kaydet" çıktısı, Acrobat'ta "Kaydet", imza, form doldurma —
/// SON bölümündeki trailer'ı olduğu gibi tutar. Oradaki `/Prev` eski dosyada
/// bir bayt ofsetidir. Belge baştan yazıldığında (sıkıştır, döndür, kırp,
/// filigran, sayfa numarası) bu ofset yeni dosyaya kopyalanıyor ve anlamsız
/// bir yeri gösteriyordu: çıktı kendi içinde tutarsızdı, yeniden açma
/// doğrulaması onu haklı olarak reddediyor, kullanıcı açılan ve önizlenen
/// belgesi için "Belge okunamadı" görüyordu. `/XRefStm` (hibrit akışın eski
/// ofseti) ve eski xref akışının kendi akış/kodlama girdileri aynı sınıftandır;
/// yazıcı yeni dosya için gerekenleri kendisi üretir. `Root`, `Info`, `ID`
/// belgeye aittir ve korunur.
fn detach_from_source_layout(doc: &mut LopdfDoc) {
    for key in [
        b"Prev".as_slice(),
        b"XRefStm",
        b"Type",
        b"W",
        b"Index",
        b"Length",
        b"Filter",
        b"DecodeParms",
        b"F",
        b"FFilter",
        b"FDecodeParms",
        b"DL",
    ] {
        doc.trailer.remove(key);
    }
}

/// Xref tablosundaki satır sonlarını ve startxref konumunu standart 20-baytlık PDF biçimine normalize eder.
fn normalize_xref_and_startxref(bytes: &[u8]) -> Result<Vec<u8>> {
    // 1. Son %%EOF konumunu tespit et (trailing garbage kırpma)
    let eof_pos = bytes
        .windows(5)
        .rposition(|w| w == b"%%EOF")
        .ok_or_else(|| EklerError::InvalidPdf("%%EOF sonlandırıcısı bulunamadı".to_string()))?;
    let content_to_eof = &bytes[..eof_pos + 5];

    // 2. startxref anahtar kelimesini geriye doğru ara
    let startxref_pos = content_to_eof
        .windows(9)
        .rposition(|w| w == b"startxref")
        .ok_or_else(|| EklerError::InvalidPdf("startxref bulunamadı".to_string()))?;

    // startxref sonrasındaki offset değerini oku
    let startxref_str = String::from_utf8_lossy(&content_to_eof[startxref_pos + 9..eof_pos]);
    let declared_offset: Option<usize> = startxref_str
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok());

    // Gerçek 'xref' anahtar kelimesini bul (önce bildirilen offset civarına bak, yoksa geriye doğru ara)
    let mut xref_pos: Option<usize> = None;
    if let Some(decl) = declared_offset {
        if decl < content_to_eof.len() && content_to_eof[decl..].starts_with(b"xref") {
            xref_pos = Some(decl);
        }
    }
    if xref_pos.is_none() {
        xref_pos = content_to_eof[..startxref_pos]
            .windows(5)
            .rposition(|w| w == b"\nxref")
            .map(|p| p + 1);
    }

    let xref_pos =
        xref_pos.ok_or_else(|| EklerError::InvalidPdf("xref tablosu bulunamadı".to_string()))?;
    if xref_pos >= startxref_pos {
        return Err(EklerError::InvalidPdf("Geçersiz xref konumu".to_string()));
    }

    // xref öncesi veri (tüm nesneler ve stream'ler) dokunulmadan korunur
    let mut result = content_to_eof[..xref_pos].to_vec();
    let new_xref_offset = result.len();

    // xref ile startxref arasındaki bölümü (xref tablosu + trailer) satır satır ayrıştır
    let trailer_relative = content_to_eof[xref_pos..startxref_pos]
        .windows(7)
        .position(|w| w == b"trailer")
        .ok_or_else(|| EklerError::InvalidPdf("Klasik trailer bulunamadı".into()))?;
    let trailer_pos = xref_pos + trailer_relative;
    let xref_section = &content_to_eof[xref_pos..trailer_pos];
    let xref_text = String::from_utf8_lossy(xref_section);

    let mut normalized_xref = String::with_capacity(xref_section.len() + 256);
    let mut in_trailer = false;

    for line in xref_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("trailer") {
            in_trailer = true;
        }

        if in_trailer {
            normalized_xref.push_str(line);
            normalized_xref.push('\n');
        } else if trimmed == "xref" {
            normalized_xref.push_str("xref\n");
        } else if trimmed.split_whitespace().count() == 2
            && trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_whitespace())
        {
            // "0 10" gibi alt bölüm başlığı
            normalized_xref.push_str(trimmed);
            normalized_xref.push('\n');
        } else {
            // Xref girişi: "0000000000 65535 f" veya "0000000031 00000 n"
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() == 3 && (parts[2] == "f" || parts[2] == "n") {
                if let (Ok(offset), Ok(gen)) = (parts[0].parse::<u64>(), parts[1].parse::<u32>()) {
                    // Standarda ve nom_parser'a tam uyumlu 20 bayt: "0000000031 00000 n \n"
                    let entry = format!("{:010} {:05} {} \n", offset, gen, parts[2]);
                    normalized_xref.push_str(&entry);
                    continue;
                }
            }
            normalized_xref.push_str(line);
            normalized_xref.push('\n');
        }
    }

    result.extend_from_slice(normalized_xref.as_bytes());
    result.extend_from_slice(&content_to_eof[trailer_pos..startxref_pos]);

    // Yeni startxref ve %%EOF ekle
    let footer = format!("startxref\n{}\n%%EOF\n", new_xref_offset);
    result.extend_from_slice(footer.as_bytes());

    Ok(result)
}
