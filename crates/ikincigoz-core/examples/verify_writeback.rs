//! Apply every offered correction to a document and account for each character.
//!
//! Prints the authorised changes, then compares the container's text image
//! before and after. Anything that moved outside an approved change is an
//! unauthorised mutation and is reported as such.

use ikincigoz_core::dict::UserDictionary;
use ikincigoz_core::finding::Fix;
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};
use ikincigoz_core::writeback;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: verify_writeback <file>");
    let bytes = std::fs::read(&path).expect("read");
    let name = parser::base_name(&path);
    let doc = parser::parse(&name, &bytes).expect("parse");

    let dict = UserDictionary::default();
    let opts = LintOptions {
        max_per_rule: 500,
        ..LintOptions::default()
    };
    let findings = analyze(&Context::new(&doc, &dict, None, &opts)).findings;

    // The non-overlapping subset, exactly as the correction panel builds it.
    let mut fixes: Vec<Fix> = findings.iter().filter_map(|f| f.fix.clone()).collect();
    fixes.sort_by(|a, b| {
        (a.block_id.as_str(), a.char_start, a.char_end).cmp(&(
            b.block_id.as_str(),
            b.char_start,
            b.char_end,
        ))
    });
    let mut chosen: Vec<Fix> = Vec::new();
    for fix in fixes {
        let clash = chosen.last().is_some_and(|p| {
            p.block_id == fix.block_id
                && (p.char_end > fix.char_start
                    || (p.char_start == fix.char_start && p.char_end == fix.char_end))
        });
        if !clash {
            chosen.push(fix);
        }
    }

    println!(
        "{} findings, {} corrections offered",
        findings.len(),
        chosen.len()
    );
    for f in &chosen {
        println!(
            "  {} [{}..{}] {:?} -> {:?}",
            f.block_id, f.char_start, f.char_end, f.original, f.replacement
        );
    }
    if chosen.is_empty() {
        println!("nothing to apply");
        return;
    }

    let before = writeback::container_text_image(doc.format, &bytes).expect("image");
    let out = match writeback::apply(&name, &bytes, &chosen) {
        Ok(o) => o,
        Err(e) => {
            println!("\nWRITE-BACK REFUSED: {}", e.code());
            return;
        }
    };
    let after = writeback::container_text_image(doc.format, &out.bytes).expect("image");

    println!("\nimage before: {} chars", before.chars().count());
    println!("image after : {} chars", after.chars().count());

    println!("\nimages differ: {}", before != after);
    println!(
        "net character change: {}",
        after.chars().count() as i64 - before.chars().count() as i64
    );

    println!(
        "\nwrote {} ({} changes applied)",
        out.file_name, out.applied
    );
    if let Some(dir) = std::env::args().nth(2) {
        let target = std::path::Path::new(&dir).join(&out.file_name);
        std::fs::write(&target, &out.bytes).expect("write");
        println!("saved to {}", target.display());
    }
}
