use crate::{EklerError, Result};
use lopdf::{Document, Object, ObjectId};
use std::collections::HashSet;
fn invalid(message: impl Into<String>) -> EklerError {
    EklerError::InvalidPdf(message.into())
}
/// Validate reachable references and the full page tree before trusting get_pages.
pub fn validate_document(doc: &Document) -> Result<usize> {
    if doc.trailer.has(b"Encrypt") {
        return Err(invalid(
            "Şifreli PDF desteklenmiyor; kaynak uygulamada şifresiz bir kopya oluşturun",
        ));
    }
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|_| invalid("Catalog referansı yok"))?;
    let catalog = doc
        .get_dictionary(root)
        .map_err(|_| invalid("Catalog okunamıyor"))?;
    if catalog.get(b"Type").and_then(|v| v.as_name()).ok() != Some(b"Catalog") {
        return Err(invalid("Geçersiz Catalog"));
    }
    let mut visited = HashSet::new();
    let mut stack = vec![Object::Reference(root)];
    let mut budget = 2_000_000usize;
    while let Some(object) = stack.pop() {
        budget = budget
            .checked_sub(1)
            .ok_or_else(|| invalid("PDF nesne sınırı aşıldı"))?;
        match object {
            Object::Reference(id) => {
                if visited.insert(id) {
                    stack.push(
                        doc.get_object(id)
                            .map_err(|_| invalid(format!("Eksik nesne referansı: {:?}", id)))?
                            .clone(),
                    );
                }
            }
            Object::Array(a) => stack.extend(a),
            Object::Dictionary(d) => stack.extend(d.iter().map(|(_, v)| v.clone())),
            Object::Stream(s) => {
                validate_flate_stream(&s)?;
                stack.extend(s.dict.iter().map(|(_, v)| v.clone()));
            }
            _ => {}
        }
    }
    let pages = catalog
        .get(b"Pages")
        .and_then(|o| o.as_reference())
        .map_err(|_| invalid("Pages referansı yok"))?;
    fn walk(
        doc: &Document,
        id: ObjectId,
        parent: Option<ObjectId>,
        seen: &mut HashSet<ObjectId>,
        depth: usize,
    ) -> Result<usize> {
        if depth > 128 || !seen.insert(id) {
            return Err(invalid("Döngüsel veya mükerrer sayfa ağacı"));
        }
        let node = doc
            .get_dictionary(id)
            .map_err(|_| invalid("Sayfa ağacı düğümü geçersiz"))?;
        if let Some(parent) = parent {
            if node.get(b"Parent").and_then(|v| v.as_reference()).ok() != Some(parent) {
                return Err(invalid("Sayfa Parent ilişkisi tutarsız"));
            }
        }
        match node
            .get(b"Type")
            .and_then(|v| v.as_name())
            .map_err(|_| invalid("Sayfa Type eksik"))?
        {
            b"Page" => {
                super::resolved_page_dictionary(doc, id)?;
                if let Ok(contents) = node.get(b"Contents") {
                    let check = |v: &Object| -> Result<()> {
                        let id = v
                            .as_reference()
                            .map_err(|_| invalid("Contents referans olmalı"))?;
                        doc.get_object(id)
                            .and_then(|o| o.as_stream())
                            .map_err(|_| invalid("Contents stream eksik"))?;
                        Ok(())
                    };
                    match contents {
                        Object::Array(a) => {
                            for v in a {
                                check(v)?
                            }
                        }
                        v => check(v)?,
                    }
                }
                Ok(1)
            }
            b"Pages" => {
                let kids = node
                    .get(b"Kids")
                    .and_then(|v| v.as_array())
                    .map_err(|_| invalid("Kids eksik"))?;
                let mut count = 0;
                for kid in kids {
                    count += walk(
                        doc,
                        kid.as_reference()
                            .map_err(|_| invalid("Geçersiz Kids referansı"))?,
                        Some(id),
                        seen,
                        depth + 1,
                    )?;
                }
                if node.get(b"Count").and_then(|v| v.as_i64()).ok() != Some(count as i64) {
                    return Err(invalid("Pages Count tutarsız"));
                }
                Ok(count)
            }
            _ => Err(invalid("Bilinmeyen sayfa düğümü")),
        }
    }
    let count = walk(doc, pages, None, &mut HashSet::new(), 0)?;
    if count == 0 {
        return Err(invalid("PDF hiç sayfa içermiyor"));
    }
    Ok(count)
}

/// lopdf 0.34 logs certain zlib errors and returns partial data. Never accept
/// those errors as a successful repair. This is a bounded validation pass only.
fn validate_flate_stream(stream: &lopdf::Stream) -> Result<()> {
    use std::io::Read;
    let single_flate = match stream.dict.get(b"Filter") {
        Ok(Object::Name(n)) => n == b"FlateDecode" || n == b"Fl",
        Ok(Object::Array(a)) if a.len() == 1 => a[0].as_name().ok() == Some(b"FlateDecode"),
        _ => false,
    };
    if !single_flate {
        return Ok(());
    }
    if stream.content.len() < 6 {
        return Err(invalid("Eksik Flate akışı"));
    }
    let mut decoder = flate2::read::ZlibDecoder::new(stream.content.as_slice());
    let mut buffer = [0u8; 65536];
    let mut total = 0usize;
    loop {
        let read = decoder
            .read(&mut buffer)
            .map_err(|_| invalid("Bozuk sıkıştırılmış PDF akışı; kısmi içerik kabul edilmedi"))?;
        if read == 0 {
            break;
        }
        total += read;
        if total > 128 * 1024 * 1024 {
            return Err(invalid("PDF akışının açılmış boyutu güvenli sınırı aşıyor"));
        }
    }
    if decoder.total_in() != stream.content.len() as u64 {
        return Err(invalid("Sıkıştırılmış PDF akışı uzunluğu tutarsız"));
    }
    Ok(())
}
