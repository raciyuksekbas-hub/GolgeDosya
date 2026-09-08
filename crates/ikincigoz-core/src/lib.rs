//! İkinciGöz core: deterministic legal-document linting.
//!
//! This crate performs no network access and contains no machine-learned
//! component. Every finding is produced by an explicit rule over an explicit
//! document model, so any result can be traced back to the code that made it.

pub mod abbrev;
pub mod cdm;
pub mod dict;
pub mod finding;
pub mod morph;
pub mod numbering;
pub mod numwords;
pub mod parser;
pub mod profile;
pub mod rules;
pub mod text;
pub mod writeback;
pub mod zones;
