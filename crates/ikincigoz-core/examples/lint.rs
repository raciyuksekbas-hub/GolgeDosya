//! A command-line view of what İkinciGöz finds in a document.
//!
//! Useful for checking a rule's real-world behaviour without opening the
//! application, and for looking at a document that cannot be shared: it runs
//! entirely locally and prints nothing it was not given.
//!
//!     cargo run -p ikincigoz-core --example lint -- samples/ornek-dilekce-hatali.docx

use ikincigoz_core::dict::UserDictionary;
use ikincigoz_core::parser;
use ikincigoz_core::rules::{analyze, Context, LintOptions};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("kullanım: lint <belge.docx|belge.udf>");
        std::process::exit(2);
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("Dosya açılamadı.");
            std::process::exit(1);
        }
    };
    let file_name = parser::base_name(&path);
    let document = match parser::parse(&file_name, &bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}", e.message_tr());
            std::process::exit(1);
        }
    };

    let dictionary = UserDictionary::default();
    let options = LintOptions::default();
    let started = std::time::Instant::now();
    let result = analyze(&Context::new(&document, &dictionary, None, &options));
    let elapsed = started.elapsed();

    println!(
        "{}  ·  {}  ·  {} paragraf  ·  {} kelime  ·  {:?}",
        document.metadata.file_name,
        document.format.label(),
        document.metadata.block_count,
        document.metadata.word_count,
        elapsed
    );
    println!(
        "{} kesin hata  ·  {} uyarı  ·  {} incele\n",
        result.error_count, result.warning_count, result.review_count
    );

    for f in &result.findings {
        println!(
            "[{:7}] {:34} p{:<3} {}",
            format!("{:?}", f.severity).to_lowercase(),
            f.rule_id,
            f.location.block_index + 1,
            f.message
        );
        if let Some(fix) = &f.fix {
            println!(
                "            düzeltme: {:?} -> {:?}  ({})",
                fix.original, fix.replacement, fix.description
            );
        }
    }
    for t in &result.truncated_rules {
        println!(
            "\n(not) {} kuralı {} bulgudan {} tanesi gösterildi.",
            t.rule_id, t.total, t.shown
        );
    }
}
