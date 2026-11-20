//! Yapısal güvenlik taraması ve lopdf'in tek giriş kapısı: ayrıştırmadan önce
//! ham baytlar, yüklerken nesne akışları, çizimden önce içerik akışları.
//!
//! lopdf 0.42'den önceki ayrıştırıcı iç içe dizileri/sözlükleri SINIRSIZ
//! özyinelemeyle çözüyordu (RUSTSEC-2026-0187). Yeterince derin bir girdi,
//! komutları taşıyan iş parçacığının yığınını taşırıyor; Rust yığın taşmasını
//! `abort` ile karşılar — yakalanamaz, süreç ölür. lopdf 0.42'den beri her
//! ayrıştırma yolu (dolaylı nesne, nesne akışı üyesi, içerik akışı işleneni)
//! derinliği kendisi sınırlar (`reader.rs`, `MAX_NESTING_DEPTH` = 100) ve aşan
//! nesneyi hata sayar. Yuvalanmadan gelen çökme artık lopdf içinde önlenir.
//!
//! Bu modül derinlemesine savunma olarak kalır ve kararı lopdf'in SESSİZ
//! davranışına bırakmaz: lopdf sınırı aşan nesne akışı üyesini atlayıp belgeyi
//! eksik nesneyle açar; burada ise patolojik derinlik taşıyan belge BÜTÜNÜYLE ve
//! açık bir sebeple reddedilir. Kurtarma değil, rettir: karar deterministiktir
//! ve lopdf sürümüne bağlı değildir. Sınırlar gerçek belgelerin çok üstündedir
//! (gerçekte iç içelik ~30, `/Length` zinciri 1).
//!
//! Dört denetim:
//!
//! 1. [`check_structure`] ayrıştırıcı çağrılmadan ÖNCE baytların üzerinden bir
//!    kez geçer: yuvalanma derinliği ve dolaylı `/Length` başvuru zinciri.
//!    Zincir denetiminin lopdf'te karşılığı YOKTUR: lopdf 0.47 de `/Length M G R`
//!    başvurusunu özyinelemeli çözer ve yalnız döngüyü yakalar, zincirin
//!    uzunluğunu sınırlamaz (parser `stream` → `Reader::get_object`). 2 MiB'lık
//!    bir iş parçacığı ~200 halkada taşar; buradaki sınır 50'dir.
//! 2. Ham tarama akış gövdelerini atlar; nesne akışlarının (`/Type/ObjStm`)
//!    içini [`load_mem_guarded`]'ın yükleme kancası ölçer. 71 baytlık
//!    sıkıştırılmış bir nesne akışı 20 000 derinlikte bir dizi taşıyabilir ve
//!    lopdf 0.34'te ham taramadan geçip süreci düşürüyordu.
//! 3. Sayfa içerik akışları yüklemede ayrıştırılmaz; onları çözen kod önce
//!    [`exceeds_nesting`] sorar.
//! 4. Kabul sınırı: [`load_mem_guarded`] lopdf'i KATI kipte çağırır. Aşağıda.
//!
//! ## Kabul sınırı (lopdf 0.47 katı kip)
//!
//! lopdf 0.47'nin varsayılan (hoşgörülü) kipi hasarlı dosyayı SESSİZCE onarır:
//! bozuk xref'i nesne tarayarak yeniden kurar, kaymış `startxref`/`/Prev`
//! ofsetini yakındaki xref'e düzeltir, yanlış `/Length`'i `endstream`
//! konumundan yeniden hesaplar, 19 baytlık xref girişlerini kabul eder,
//! `%PDF-` öncesindeki çöpü atlar. Toleranslı yükleyicinin ilkesi bunun tersidir
//! (bkz. `tolerant.rs`): bilinmeyen hasar reddedilir, nesne taramalı onarım
//! kasten kapalıdır, bilinen iki onarım (xref satır sonu, `startxref`) ayrı
//! katmanda yapılır ve kullanıcıya not olarak söylenir. `/Length` onarımı
//! yüklemeden sonra görülemez bile: lopdf akış sözlüğüne yeni uzunluğu yazar.
//!
//! Bu yüzden lopdf katı kipte çağrılır. Katı kip 0.34'ün ayrıştırma kurallarıyla
//! aynıdır (xref girişi yalnız `SP CR`/`SP LF`/`CR LF` ile biter, `/Length`
//! onarılmaz, ofset düzeltilmez, xref yeniden kurulmaz) ve `%PDF-`'in ilk baytta
//! olması şartı 0.34'teki gibi burada ayrıca aranır. Tek fark: 0.34 tek tek
//! ayrıştırılamayan nesneleri ATLARDI (sayfa grafiği dışındaysa belge "sarkan
//! başvuru" notuyla açılırdı); katı kip böyle bir belgeyi bütünüyle reddeder.

use crate::error::{EklerError, Result};
use lopdf::{
    Document as LopdfDoc, LoadOptions, Object, ObjectId, ParseError, DEFAULT_MAX_DECOMPRESSED_SIZE,
};

/// İç içe dizi/sözlük derinliği üst sınırı. lopdf'in kendi sınırıyla
/// (`MAX_NESTING_DEPTH`) aynı büyüklükte: lopdf'in okuyabildiği hiçbir belge
/// burada derinlik yüzünden reddedilmez.
const MAX_NESTING: usize = 100;
/// Dolaylı `/Length` başvuru zincirinin derinlik üst sınırı.
const MAX_LENGTH_CHAIN: usize = 50;
/// Derinliği sınırı aşan nesne akışına yükleme sırasında verilen tür. lopdf
/// bu akışı nesne akışı saymaz, içini hiç ayrıştırmaz; yükleme bitince bu tür
/// aranır ve belge reddedilir.
const REJECTED_OBJECT_STREAM: &[u8] = b"GolgeDosyaReddedilenNesneAkisi";

/// Ham tarama + nesne akışı taramasıyla, katı kipte lopdf yüklemesi.
///
/// Dış `Result` güvenlik reddidir (belge açılmaz, onarım da denenmez); iç
/// `Result` lopdf'in ayrıştırma sonucudur (çağıran onarım katmanına geçebilir).
/// Kabul sınırı için modül belgesine bakın.
pub fn load_mem_guarded(bytes: &[u8]) -> Result<std::result::Result<LopdfDoc, lopdf::Error>> {
    check_structure(bytes)?;
    // lopdf 0.47 başlıktan önceki çöpü katı kipte de atlar; 0.34 atlamazdı.
    if !bytes.starts_with(b"%PDF-") {
        return Ok(Err(ParseError::InvalidFileHeader.into()));
    }
    let options = LoadOptions {
        strict: true,
        ..LoadOptions::with_filter(flag_deep_object_stream)
    };
    let parsed = LopdfDoc::load_mem_with_options(bytes, options);
    if let Ok(doc) = &parsed {
        let rejected = doc
            .objects
            .values()
            .any(|o| matches!(o, Object::Stream(s) if s.dict.has_type(REJECTED_OBJECT_STREAM)));
        if rejected {
            return Err(EklerError::InvalidPdf(format!(
                "PDF nesne akışındaki yuvalanma güvenli sınırı aşıyor (> {MAX_NESTING}); belge açılmadı"
            )));
        }
    }
    Ok(parsed)
}

/// Çözülmüş bir içerik akışı (`Content::decode` girdisi) güvenli sınırı aşıyor mu?
pub fn exceeds_nesting(bytes: &[u8]) -> bool {
    max_nesting_depth(bytes) > MAX_NESTING
}

/// Yükleme kancası (`lopdf::FilterFunc`): derin nesne akışını işaretler.
///
/// lopdf 0.34'te kanca, akışı lopdf açmadan BOŞALTMAK zorundaydı; yoksa
/// özyinelemeli ayrıştırıcı yığını taşırırdı. 0.47 derinliği kendisi sınırladığı
/// için kancanın tek işi artık akışı işaretlemektir: tür değişince lopdf onu
/// nesne akışı saymaz, içini açmaz ve akış belgede kalır; [`load_mem_guarded`]
/// onu bulup reddeder. (Kanca `Result` döndüremez, `fn` işaretçisi olduğu için
/// durum da taşıyamaz; ret kararını belgenin kendisi taşır.)
///
/// lopdf kancayı iki yerde çağırır (`reader.rs`, `load_objects_raw`): (1) xref'teki
/// her nesne için — dönüş değerinden yalnız `None` olup olmadığına bakar,
/// kullandığı `&mut` nesnedir; (2) nesne akışından çıkan her üye için — burada
/// DÖNÜŞ değerini kullanır. Nesne akışının üyeleri akış olamaz (PDF 7.5.7),
/// dolayısıyla akış olmayan her nesne aynen döndürülür; akışlar yalnız (1)'de
/// gelir ve orada dönüş değeri atıldığı için büyük akış gövdesi kopyalanmaz.
/// lopdf bu sözleşmeyi değiştirirse `object_stream_members_survive_the_guarded_load`
/// düşer. Şifreli belgelerde lopdf 0.47 kancayı hiç çağırmaz: onların nesne
/// akışlarını, çözdükten sonra, yalnız lopdf'in kendi derinlik sınırı korur.
/// Kancalı yüklemenin bir yan etkisi de lopdf'in "sayfa ağacı yüklenmedi"
/// durumunda xref'i yeniden kurmamasıdır; katı kip bunu zaten kapatır.
///
/// Akış, lopdf'in yüklemede uyguladığı açma sınırıyla
/// (`DEFAULT_MAX_DECOMPRESSED_SIZE`) açılır; sınırsız açma, lopdf'in
/// sıkıştırma bombasına karşı koyduğu sınırı kancada delerdi. Sınırı aşan akışı
/// lopdf zaten reddeder ve yükleme başarısız olur. Süzgeci çözülemeyen akış ham
/// içeriğiyle ölçülür: lopdf 0.34 böyle bir nesne akışını ham hâliyle
/// ayrıştırırdı, 0.47 onu hiç açmadan atar. İkisinde de ham içerik ölçülür:
/// fazla ret güvenli yöndür ve karar lopdf sürümüne bağlı kalmaz.
fn flag_deep_object_stream(id: ObjectId, object: &mut Object) -> Option<(ObjectId, Object)> {
    let Object::Stream(stream) = object else {
        return Some((id, object.clone()));
    };
    if stream.dict.has_type(b"ObjStm") {
        let deep = match stream.decompressed_content_with_limit(DEFAULT_MAX_DECOMPRESSED_SIZE) {
            Ok(content) => exceeds_nesting(&content),
            Err(_) => exceeds_nesting(&stream.content),
        };
        if deep {
            stream
                .dict
                .set("Type", Object::Name(REJECTED_OBJECT_STREAM.to_vec()));
        }
    }
    Some((id, Object::Null))
}

pub fn check_structure(bytes: &[u8]) -> Result<()> {
    let depth = max_nesting_depth(bytes);
    if depth > MAX_NESTING {
        return Err(EklerError::InvalidPdf(format!(
            "PDF nesne yuvalanması güvenli sınırı aşıyor ({depth} > {MAX_NESTING}); belge açılmadı"
        )));
    }
    let chain = max_length_chain(bytes);
    if chain > MAX_LENGTH_CHAIN {
        return Err(EklerError::InvalidPdf(format!(
            "PDF akış uzunluğu başvuru zinciri güvenli sınırı aşıyor ({chain} > {MAX_LENGTH_CHAIN}); belge açılmadı"
        )));
    }
    Ok(())
}

/// `[`…`]` ve `<<`…`>>` yuvalanmasının en büyük derinliği. Dizeler, onaltılık
/// dizeler, yorumlar ve akış gövdeleri atlanır; sınır aşılınca erken döner.
///
/// Bu bir üst sınır tahminidir, tam ayrıştırıcı değil: amacı ayrıştırıcıya
/// verilecek derinliği güvenli bir eşikle sınırlamaktır. Fazla sayması (yanlış
/// ret) güvenli yöndür ve sınırın gerçek belgelerin çok üstünde olması bunu
/// pratikte olanaksız kılar.
fn max_nesting_depth(bytes: &[u8]) -> usize {
    let n = bytes.len();
    let mut i = 0;
    let mut depth = 0usize;
    let mut max = 0usize;
    let is_delim = |b: u8| b.is_ascii_whitespace() || b == b'>' || b == b'\0' || b == 0x0c;
    while i < n {
        match bytes[i] {
            b'%' => {
                while i < n && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    i += 1;
                }
            }
            b'(' => i = skip_literal_string(bytes, i),
            b'<' if i + 1 < n && bytes[i + 1] == b'<' => {
                depth += 1;
                max = max.max(depth);
                if depth > MAX_NESTING {
                    return depth;
                }
                i += 2;
            }
            b'<' => {
                // Onaltılık dize: `>`e kadar.
                i += 1;
                while i < n && bytes[i] != b'>' {
                    i += 1;
                }
                i += 1;
            }
            b'>' if i + 1 < n && bytes[i + 1] == b'>' => {
                depth = depth.saturating_sub(1);
                i += 2;
            }
            b'[' => {
                depth += 1;
                max = max.max(depth);
                if depth > MAX_NESTING {
                    return depth;
                }
                i += 1;
            }
            b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b's' if bytes[i..].starts_with(b"stream")
                && (i == 0 || is_delim(bytes[i - 1]))
                && !(i >= 3 && &bytes[i - 3..i] == b"end") =>
            {
                // Akış gövdesi ikili olabilir; `endstream`e kadar atlanır.
                match find(bytes, i + 6, b"endstream") {
                    Some(end) => i = end + b"endstream".len(),
                    None => return max,
                }
            }
            _ => i += 1,
        }
    }
    max
}

fn skip_literal_string(bytes: &[u8], start: usize) -> usize {
    let n = bytes.len();
    let mut i = start + 1;
    let mut nesting = 1usize;
    while i < n {
        match bytes[i] {
            b'\\' => i += 2,
            b'(' => {
                nesting += 1;
                i += 1;
            }
            b')' => {
                nesting -= 1;
                i += 1;
                if nesting == 0 {
                    break;
                }
            }
            _ => i += 1,
        }
    }
    i
}

fn find(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Dolaylı `/Length` başvuru zincirinin en uzun yolu.
///
/// Her `N G obj` gövdesinde `stream` anahtar sözcüğü ve `/Length M G R` varsa
/// N→M kenarı kaydedilir; sonra döngü korumalı özyinelemeyle en uzun yol
/// bulunur. Gerçek belgelerde `/Length` en çok bir tamsayı nesnesine gider
/// (derinlik 1); zincir yalnız kasıtlı kurulmuş girdilerde büyür.
fn max_length_chain(bytes: &[u8]) -> usize {
    use std::collections::HashMap;
    let mut edges: HashMap<u32, u32> = HashMap::new();
    let mut i = 0;
    let n = bytes.len();
    while let Some(pos) = find(bytes, i, b" obj") {
        // Nesne numarasını `obj`'dan geriye doğru oku: `<num> <gen> obj`.
        let Some((id, _)) = read_object_header(bytes, pos) else {
            i = pos + 4;
            continue;
        };
        let end = find(bytes, pos, b"endobj").unwrap_or(n);
        let body = &bytes[pos..end.min(n)];
        if find(body, 0, b"stream").is_some() {
            if let Some(target) = length_reference(body) {
                edges.insert(id, target);
            }
        }
        i = end + 6;
        if edges.len() > 1_000_000 {
            break;
        }
    }
    // En uzun yol: her düğümden döngü korumalı iniş.
    let mut memo: HashMap<u32, usize> = HashMap::new();
    let mut max = 0;
    for &start in edges.keys() {
        max = max.max(chain_depth(start, &edges, &mut memo));
        if max > MAX_LENGTH_CHAIN {
            return max;
        }
    }
    max
}

fn chain_depth(
    node: u32,
    edges: &std::collections::HashMap<u32, u32>,
    memo: &mut std::collections::HashMap<u32, usize>,
) -> usize {
    // Döngü ve derinlik koruması: özyineleme kendisi patolojik girdide taşmasın
    // diye yinelemeli yürünür.
    let mut depth = 0usize;
    let mut current = node;
    let mut visited = std::collections::HashSet::new();
    loop {
        if let Some(&cached) = memo.get(&current) {
            depth += cached;
            break;
        }
        if !visited.insert(current) {
            // Döngü: sonlu ama sınırın üstünde say.
            return MAX_LENGTH_CHAIN + 1;
        }
        match edges.get(&current) {
            Some(&next) => {
                depth += 1;
                current = next;
                if depth > MAX_LENGTH_CHAIN {
                    break;
                }
            }
            None => break,
        }
    }
    memo.insert(node, depth);
    depth
}

/// ` obj`'ın hemen öncesindeki `<num> <gen>`'i okur.
fn read_object_header(bytes: &[u8], obj_pos: usize) -> Option<(u32, u32)> {
    let mut j = obj_pos;
    let read_back_uint = |end: usize| -> Option<(u32, usize)> {
        let mut k = end;
        while k > 0 && bytes[k - 1].is_ascii_whitespace() {
            k -= 1;
        }
        let hi = k;
        while k > 0 && bytes[k - 1].is_ascii_digit() {
            k -= 1;
        }
        if k == hi {
            return None;
        }
        let value: u32 = std::str::from_utf8(&bytes[k..hi]).ok()?.parse().ok()?;
        Some((value, k))
    };
    let (gen, before_gen) = read_back_uint(j)?;
    j = before_gen;
    let (id, _) = read_back_uint(j)?;
    Some((id, gen))
}

/// Gövdede `/Length M G R` varsa M'i döndürür (doğrudan tamsayı Length değil).
fn length_reference(body: &[u8]) -> Option<u32> {
    let pos = find(body, 0, b"/Length")?;
    let mut i = pos + b"/Length".len();
    let n = body.len();
    let skip_ws = |mut i: usize| {
        while i < n && body[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    };
    let read_uint = |mut i: usize| -> Option<(u32, usize)> {
        let start = i;
        while i < n && body[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
        Some((std::str::from_utf8(&body[start..i]).ok()?.parse().ok()?, i))
    };
    i = skip_ws(i);
    let (target, after) = read_uint(i)?;
    i = skip_ws(after);
    let (_gen, after) = read_uint(i)?;
    i = skip_ws(after);
    if i < n && body[i] == b'R' {
        Some(target)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(nested: &str) -> Vec<u8> {
        format!("%PDF-1.7\n1 0 obj<</A {nested}>>endobj\n").into_bytes()
    }

    #[test]
    fn shallow_nesting_passes() {
        assert!(check_structure(&wrap(&format!("{}{}", "[".repeat(30), "]".repeat(30)))).is_ok());
    }

    #[test]
    fn deep_nesting_is_rejected() {
        let deep = format!("{}{}", "[".repeat(400), "]".repeat(400));
        assert!(check_structure(&wrap(&deep)).is_err());
    }

    #[test]
    fn deep_dict_nesting_is_rejected() {
        let deep = format!("{}{}", "<</K ".repeat(400), ">>".repeat(400));
        assert!(check_structure(&wrap(&deep)).is_err());
    }

    #[test]
    fn brackets_inside_strings_and_streams_do_not_count() {
        let s = format!("%PDF-1.7\n1 0 obj({})endobj\n", "[".repeat(400));
        assert!(check_structure(s.as_bytes()).is_ok());
        let mut bytes = b"%PDF-1.7\n1 0 obj<</Length 400>>stream\n".to_vec();
        bytes.extend(std::iter::repeat_n(b'[', 400));
        bytes.extend_from_slice(b"\nendstream endobj\n");
        assert!(check_structure(&bytes).is_ok());
    }

    #[test]
    fn direct_length_is_not_a_chain() {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        for k in 0..80 {
            bytes.extend(
                format!(
                    "{} 0 obj<</Length 3>>stream\nabc\nendstream endobj\n",
                    k + 1
                )
                .bytes(),
            );
        }
        assert!(check_structure(&bytes).is_ok());
    }

    #[test]
    fn indirect_length_chain_is_rejected() {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        for k in 0..80u32 {
            bytes.extend(
                format!(
                    "{} 0 obj<</Length {} 0 R>>stream\nabc\nendstream endobj\n",
                    k + 1,
                    k + 2
                )
                .bytes(),
            );
        }
        bytes.extend(b"200 0 obj 3 endobj\n");
        assert!(check_structure(&bytes).is_err());
    }

    /// Katalog → sayfa ağacı → tek sayfa; 5 numaralı nesne bir nesne
    /// akışının (4) içinde ve katalogdan başvurulur. `filter` akışın süzgeci
    /// (`None`: süzgeçsiz), `deflate` gövdenin gerçekten sıkıştırılıp
    /// sıkıştırılmayacağı.
    fn pdf_with_object_stream(member: &str, filter: Option<&str>, deflate: bool) -> Vec<u8> {
        use std::io::Write;
        let header = "5 0 ";
        let mut body = format!("{header}{member}").into_bytes();
        if deflate {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(&body).unwrap();
            body = e.finish().unwrap();
        }
        let filter = filter.map(|f| format!("/Filter/{f}")).unwrap_or_default();
        let mut objects: Vec<Vec<u8>> = vec![
            b"1 0 obj<</Type/Catalog/Pages 2 0 R/Ek 5 0 R>>endobj\n".to_vec(),
            b"2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n".to_vec(),
            b"3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\n".to_vec(),
        ];
        let mut stream = format!(
            "4 0 obj<</Type/ObjStm/N 1/First {}{filter}/Length {}>>stream\n",
            header.len(),
            body.len()
        )
        .into_bytes();
        stream.extend_from_slice(&body);
        stream.extend_from_slice(b"\nendstream endobj\n");
        objects.push(stream);
        let mut pdf = b"%PDF-1.5\n".to_vec();
        let mut offsets = Vec::new();
        for o in &objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(o);
        }
        let xref = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
        for o in offsets {
            pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer<</Size 6/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );
        pdf
    }

    fn nested(depth: usize) -> String {
        format!("{}{}", "[".repeat(depth), "]".repeat(depth))
    }

    #[test]
    fn deep_nesting_inside_compressed_object_stream_is_rejected_before_parsing() {
        // 20 000 derinlik 2 MiB'lık bir rayon iş parçacığını kesin taşırır:
        // lopdf 0.34'te koruma çalışmazsa bu test süreci `abort` ile
        // düşürürdü. 0.47 derinliği kendisi sınırlar ve üyeyi sessizce atar;
        // test artık belgenin eksik üyeyle açılmayıp REDDEDİLDİĞİNİ ölçer.
        let pdf = pdf_with_object_stream(&nested(20_000), Some("FlateDecode"), true);
        assert!(
            check_structure(&pdf).is_ok(),
            "ham tarama akışın içini görmez"
        );
        assert!(load_mem_guarded(&pdf).is_err());
    }

    #[test]
    fn deep_nesting_in_object_stream_with_unknown_or_no_filter_is_rejected() {
        // Süzgeç çözülemezse lopdf 0.34 akışı ham hâliyle ayrıştırırdı (0.47
        // akışı atar); koruma iki durumda da ham içeriği ölçüp reddeder.
        let unknown = pdf_with_object_stream(&nested(20_000), Some("Bilinmeyen"), false);
        assert!(load_mem_guarded(&unknown).is_err());
        let plain = pdf_with_object_stream(&nested(20_000), None, false);
        assert!(load_mem_guarded(&plain).is_err());
    }

    #[test]
    fn object_stream_members_survive_the_guarded_load() {
        let pdf = pdf_with_object_stream("[1 [2 3] <</K (deger)>>]", Some("FlateDecode"), true);
        let guarded = load_mem_guarded(&pdf).unwrap().unwrap();
        let plain = LopdfDoc::load_mem(&pdf).unwrap();
        let member = guarded.get_object((5, 0)).unwrap();
        assert_eq!(member, plain.get_object((5, 0)).unwrap());
        assert_eq!(member.as_array().unwrap().len(), 3);
        assert_eq!(guarded.objects.len(), plain.objects.len());
        assert_eq!(guarded.get_pages().len(), 1);
    }

    #[test]
    fn content_stream_nesting_is_measured() {
        assert!(!exceeds_nesting(
            format!("BT {} TJ ET", nested(30)).as_bytes()
        ));
        assert!(exceeds_nesting(
            format!("BT {} TJ ET", nested(5_000)).as_bytes()
        ));
    }
}
