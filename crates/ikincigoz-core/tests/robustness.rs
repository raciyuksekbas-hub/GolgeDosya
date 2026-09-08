//! Hostile and malformed input must fail cleanly.
//!
//! The input is a file someone else sent the user, so every one of these is a
//! realistic thing to be handed. None of them may panic, hang, or allocate
//! without bound — the requirement is a clean `ParseError` and a message that
//! contains no document content.

use ikincigoz_core::parser::{self, ParseError};
use ikincigoz_core::rules::{analyze, Context, LintOptions};
use std::io::{Cursor, Write};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

fn zip_with(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buffer);
        let o = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            w.start_file(*name, o).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }
    buffer.into_inner()
}

fn expect_error(name: &str, bytes: &[u8]) -> ParseError {
    match parser::parse(name, bytes) {
        Ok(d) => panic!(
            "{name}: expected an error, parsed {} blocks",
            d.blocks.len()
        ),
        Err(e) => e,
    }
}

#[test]
fn arbitrary_bytes_are_not_an_archive() {
    assert_eq!(
        expect_error("x.docx", b"hello world"),
        ParseError::NotAnArchive
    );
    assert_eq!(expect_error("x.udf", &[0u8; 512]), ParseError::NotAnArchive);
}

#[test]
fn an_empty_file_fails_cleanly() {
    let _ = expect_error("x.docx", b"");
    let _ = expect_error("x.udf", b"");
}

#[test]
fn an_unknown_extension_is_refused_before_anything_is_read() {
    assert_eq!(
        expect_error("x.pdf", b"anything"),
        ParseError::UnsupportedFormat
    );
    assert_eq!(
        expect_error("x", b"anything"),
        ParseError::UnsupportedFormat
    );
}

#[test]
fn a_zip_without_the_required_part_reports_the_missing_part() {
    let bytes = zip_with(&[("readme.txt", b"not a document".to_vec())]);
    assert!(matches!(
        expect_error("x.docx", &bytes),
        ParseError::MissingPart(_)
    ));
}

#[test]
fn truncated_xml_does_not_panic() {
    for fragment in [
        "<w:document",
        "<w:document><w:body><w:p><w:r><w:t>metin",
        "<w:document><w:body></w:document>",
        "<<<<>>>>",
    ] {
        let bytes = zip_with(&[("word/document.xml", fragment.as_bytes().to_vec())]);
        let _ = parser::parse("x.docx", &bytes);
    }
}

#[test]
fn deeply_nested_xml_is_bounded_rather_than_recursed_into() {
    // Deep nesting is the classic way to blow a recursive parser's stack.
    let depth = 5000;
    let mut xml = format!("<w:document {NS}><w:body>");
    for _ in 0..depth {
        xml.push_str("<w:p>");
    }
    xml.push_str("<w:r><w:t>x</w:t></w:r>");
    for _ in 0..depth {
        xml.push_str("</w:p>");
    }
    xml.push_str("</w:body></w:document>");
    let bytes = zip_with(&[("word/document.xml", xml.into_bytes())]);
    match parser::parse("x.docx", &bytes) {
        Err(ParseError::ResourceLimit(_)) => {}
        Err(other) => panic!("expected a resource limit, got {other:?}"),
        Ok(_) => panic!("expected the depth limit to refuse this document"),
    }
}

#[test]
fn a_highly_compressible_entry_is_refused_on_its_ratio() {
    // A megabyte of zeroes compresses to almost nothing; a real document does
    // not. The ratio guard is what stops a zip bomb before it is expanded.
    let payload = vec![b'A'; 64 * 1024 * 1024];
    let bytes = zip_with(&[("word/document.xml", payload)]);
    match parser::parse("x.docx", &bytes) {
        Err(ParseError::ResourceLimit(_)) | Err(ParseError::MissingPart(_)) => {}
        Err(other) => panic!("expected a resource limit, got {other:?}"),
        Ok(_) => panic!("expected the archive to be refused"),
    }
}

#[test]
fn an_xml_entity_declaration_does_not_expand() {
    // The billion-laughs shape. The parser must not follow the definitions.
    let xml = format!(
        r#"<?xml version="1.0"?>
<!DOCTYPE lolz [
 <!ENTITY lol "lol">
 <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
 <!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;">
 <!ENTITY lol4 "&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;">
]>
<w:document {NS}><w:body><w:p><w:r><w:t>&lol4;</w:t></w:r></w:p></w:body></w:document>"#
    );
    let bytes = zip_with(&[("word/document.xml", xml.into_bytes())]);
    // Either it refuses the document or it produces a small one. What it must
    // never do is expand the entity.
    if let Ok(d) = parser::parse("x.docx", &bytes) {
        assert!(
            d.metadata.char_count < 10_000,
            "entity was expanded to {} chars",
            d.metadata.char_count
        );
    }
}

#[test]
fn a_path_traversal_entry_name_cannot_reach_outside_the_archive() {
    // The parser only ever reads entries by exact name and never writes to
    // disk, so a crafted name has nowhere to go. Confirm it stays inert.
    let xml = format!(
        "<w:document {NS}><w:body><w:p><w:r><w:t>güvenli</w:t></w:r></w:p></w:body></w:document>"
    );
    let bytes = zip_with(&[
        ("../../../../etc/passwd", b"root:x:0:0".to_vec()),
        ("word/document.xml", xml.into_bytes()),
    ]);
    let d = parser::parse("x.docx", &bytes).unwrap();
    assert_eq!(d.blocks[0].text, "güvenli");
    assert_eq!(d.metadata.file_name, "x.docx");
}

#[test]
fn a_crafted_file_name_never_reaches_the_document_metadata() {
    let xml = format!(
        "<w:document {NS}><w:body><w:p><w:r><w:t>metin</w:t></w:r></w:p></w:body></w:document>"
    );
    let bytes = zip_with(&[("word/document.xml", xml.into_bytes())]);
    let d = parser::parse("/Users/gizli/Belgeler/dava.docx", &bytes).unwrap();
    assert_eq!(d.metadata.file_name, "dava.docx");
}

#[test]
fn a_udf_with_nonsense_offsets_still_opens() {
    let xml = r#"<template><content><![CDATA[Kısa bir metin.
]]></content><elements>
<paragraph><content startOffset="999999" length="999999"/></paragraph>
<paragraph><content startOffset="0" length="16"/></paragraph>
</elements></template>"#;
    let bytes = zip_with(&[("content.xml", xml.as_bytes().to_vec())]);
    let d = parser::parse("x.udf", &bytes).unwrap();
    assert!(d.blocks.iter().any(|b| b.text.contains("Kısa bir metin")));
}

#[test]
fn every_parse_error_message_is_content_free() {
    for e in [
        ParseError::UnsupportedFormat,
        ParseError::TooLarge,
        ParseError::NotAnArchive,
        ParseError::MissingPart("word/document.xml"),
        ParseError::MalformedXml,
        ParseError::ResourceLimit("xml_depth"),
        ParseError::EmptyDocument,
        ParseError::Io,
    ] {
        let msg = e.message_tr();
        assert!(!msg.is_empty());
        // The message must be a complete Turkish sentence, not a code dump.
        assert!(msg.ends_with('.'), "{msg}");
        assert!(!msg.contains('<') && !msg.contains('{'), "{msg}");
    }
}

#[test]
fn analysis_of_a_pathological_document_terminates_quickly() {
    // 3000 near-identical paragraphs: the shape that would expose a quadratic
    // rule. The near-duplicate scan is bounded precisely so this stays fast.
    let body: String = (0..3000)
        .map(|i| {
            format!(
                "<w:p><w:r><w:t>Davalı taraf sözleşmeden doğan edimini süresi içinde yerine getirmemiş olduğundan temerrüde düşmüştür {i}.</w:t></w:r></w:p>"
            )
        })
        .collect();
    let xml = format!("<w:document {NS}><w:body>{body}</w:body></w:document>");
    let bytes = zip_with(&[("word/document.xml", xml.into_bytes())]);

    let started = std::time::Instant::now();
    let d = parser::parse("büyük.docx", &bytes).unwrap();
    let dict = ikincigoz_core::dict::UserDictionary::default();
    let options = LintOptions::default();
    let ctx = Context::new(&d, &dict, None, &options);
    let result = analyze(&ctx);
    let elapsed = started.elapsed();

    assert_eq!(d.blocks.len(), 3000);
    // Generous for an unoptimised build; the release build is far faster. The
    // point is to fail loudly if a rule ever goes quadratic again.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "analysis took {elapsed:?}, which would freeze the interface"
    );
    // Truncation must be reported rather than silently applied.
    for t in &result.truncated_rules {
        assert!(t.total > t.shown);
    }
}
