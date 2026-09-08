//! Read-only diagnostic: show every fix with the exact source text it targets.
//!
//! Prints the block-local span, the container address it maps to, and the
//! surrounding original text, so a fix that points at the wrong place is
//! visible without applying it.

use ikincigoz_core::dict::UserDictionary;
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};

fn main() {
    let path = std::env::args().nth(1).expect("usage: diagnose <file>");
    let filter = std::env::args().nth(2);
    let bytes = std::fs::read(&path).expect("read");
    let name = parser::base_name(&path);
    let doc = parser::parse(&name, &bytes).expect("parse");

    let dict = UserDictionary::default();
    let opts = LintOptions {
        max_per_rule: 500,
        ..LintOptions::default()
    };
    let result = analyze(&Context::new(&doc, &dict, None, &opts));

    println!(
        "{} blocks, {} findings",
        doc.blocks.len(),
        result.findings.len()
    );
    for f in &result.findings {
        if let Some(want) = &filter {
            if !f.rule_id.contains(want.as_str()) {
                continue;
            }
        }
        let Some(fix) = &f.fix else { continue };
        let block = doc.block(&fix.block_id).unwrap();
        let ctx_start = fix.char_start.saturating_sub(30);
        let ctx_end = (fix.char_end + 30).min(block.char_len());
        let before = ikincigoz_core::cdm::char_slice(&block.text, ctx_start, fix.char_start);
        let target = ikincigoz_core::cdm::char_slice(&block.text, fix.char_start, fix.char_end);
        let after = ikincigoz_core::cdm::char_slice(&block.text, fix.char_end, ctx_end);
        println!(
            "\n{} {} [{}..{}] {:?} -> {:?}",
            fix.block_id, f.rule_id, fix.char_start, fix.char_end, target, fix.replacement
        );
        println!("   …{before}\u{2502}{target}\u{2502}{after}…");
        // Which run backs the change, and what container address it carries.
        for (i, r) in block.runs.iter().enumerate() {
            let (rs, re) = (r.char_start, r.char_end());
            let touches = if fix.char_start == fix.char_end {
                fix.char_start >= rs && fix.char_start <= re
            } else {
                fix.char_start < re && fix.char_end > rs
            };
            if touches {
                println!(
                    "   run {i} [{rs}..{re}] path={:?} text={:?}",
                    r.container_path,
                    if r.text.chars().count() > 40 {
                        format!("{}…", r.text.chars().take(40).collect::<String>())
                    } else {
                        r.text.clone()
                    }
                );
            }
        }
    }
}
