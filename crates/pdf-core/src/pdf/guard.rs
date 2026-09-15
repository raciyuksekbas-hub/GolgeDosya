//! Ayrıştırma öncesi yapısal güvenlik taraması.
//!
//! lopdf 0.34 ayrıştırıcısı iç içe dizileri/sözlükleri ve dolaylı `/Length`
//! başvurularını ÖZYİNELEMELİ çözer. Yeterince derin bir girdi, komutları
//! taşıyan iş parçacığının yığınını taşırır; Rust yığın taşmasını `abort` ile
//! karşılar — yakalanamaz, süreç ölür. `load_pdf_tolerant`'ın çağırdığı
//! `LopdfDoc::load_mem` bu özyinelemeye açıktır.
//!
//! Bu tarama, ayrıştırıcı çağrılmadan ÖNCE bir kez baytların üzerinden geçer ve
//! iki patolojik derinliği üst sınırla reddeder. Sınırlar gerçek belgelerin çok
//! üstünde (gerçekte iç içelik ~30, `/Length` zinciri 1), yığın taşması
//! eşiğinin çok altındadır (2 MiB iş parçacığında derinlik ~700, zincir ~200'de
//! taşar). Kurtarma değil, çökmeyi önlemedir: karar deterministiktir.

use crate::error::{EklerError, Result};

/// İç içe dizi/sözlük derinliği üst sınırı.
const MAX_NESTING: usize = 100;
/// Dolaylı `/Length` başvuru zincirinin derinlik üst sınırı.
const MAX_LENGTH_CHAIN: usize = 50;

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
}
