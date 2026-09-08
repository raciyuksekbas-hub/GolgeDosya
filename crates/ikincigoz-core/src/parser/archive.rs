//! Bounded ZIP reading shared by both container parsers.
//!
//! Every limit here exists because the input is an untrusted file the user was
//! sent by someone else. A quadratically-expanding archive must be refused,
//! not survived.

use super::{limits, ParseError};
use std::io::{Cursor, Read};

pub type Archive = zip::ZipArchive<Cursor<Vec<u8>>>;

pub fn open(bytes: &[u8]) -> Result<Archive, ParseError> {
    let cursor = Cursor::new(bytes.to_vec());
    let archive = zip::ZipArchive::new(cursor).map_err(|_| ParseError::NotAnArchive)?;
    if archive.len() > limits::MAX_ENTRIES {
        return Err(ParseError::ResourceLimit("entry_count"));
    }
    let declared: u64 = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i).map(|_| ()))
        .count() as u64;
    let _ = declared;
    Ok(archive)
}

/// Read one entry by exact name, refusing anything oversized on the way.
pub fn read_entry(archive: &mut Archive, name: &str) -> Result<Vec<u8>, ParseError> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| ParseError::MissingPart("part"))?;
    read_bounded(&mut file)
}

/// Read one entry, returning `None` when it is simply absent.
pub fn read_optional(archive: &mut Archive, name: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(name).ok()?;
    read_bounded(&mut file).ok()
}

/// Find the first entry whose name matches a predicate. Used by UDF, whose
/// payload entry has been named differently across UYAP versions.
pub fn find_entry<F: Fn(&str) -> bool>(
    archive: &mut Archive,
    pred: F,
) -> Option<(String, Vec<u8>)> {
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i).map(|s| s.to_string()))
        .collect();
    let name = names.into_iter().find(|n| pred(n))?;
    let bytes = read_entry(archive, &name).ok()?;
    Some((name, bytes))
}

fn read_bounded(file: &mut zip::read::ZipFile<'_>) -> Result<Vec<u8>, ParseError> {
    let declared = file.size();
    let compressed = file.compressed_size().max(1);
    if declared > limits::MAX_ENTRY_UNCOMPRESSED {
        return Err(ParseError::ResourceLimit("entry_size"));
    }
    if declared / compressed > limits::MAX_COMPRESSION_RATIO {
        return Err(ParseError::ResourceLimit("compression_ratio"));
    }
    // Read through a hard cap rather than trusting the declared size, since
    // the header is attacker-controlled and may understate the real payload.
    let cap = limits::MAX_ENTRY_UNCOMPRESSED + 1;
    let mut buf = Vec::with_capacity(declared.min(1 << 20) as usize);
    let read = file
        .take(cap)
        .read_to_end(&mut buf)
        .map_err(|_| ParseError::NotAnArchive)?;
    if read as u64 > limits::MAX_ENTRY_UNCOMPRESSED {
        return Err(ParseError::ResourceLimit("entry_size"));
    }
    Ok(buf)
}

/// Decode bytes as UTF-8, tolerating a BOM and invalid sequences.
///
/// A replacement character surviving here is not a parse failure: it is a real
/// defect in the document, and `TYPO_REPLACEMENT_CHAR` reports it.
pub fn decode_utf8(bytes: &[u8]) -> String {
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(body).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_archive_is_rejected_cleanly() {
        assert_eq!(
            open(b"not a zip at all").unwrap_err(),
            ParseError::NotAnArchive
        );
    }

    #[test]
    fn decode_strips_a_utf8_bom() {
        assert_eq!(decode_utf8(&[0xEF, 0xBB, 0xBF, b'a']), "a");
        assert_eq!(decode_utf8("İG".as_bytes()), "İG");
    }

    #[test]
    fn invalid_utf8_becomes_a_replacement_char_rather_than_an_error() {
        assert!(decode_utf8(&[0xC3, 0x28]).contains('\u{FFFD}'));
    }
}
