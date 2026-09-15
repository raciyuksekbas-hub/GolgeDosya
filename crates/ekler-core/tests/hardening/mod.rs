//! v0.2.0 hardening — ortak sentetik corpus, tohumlu rastgelelik ve değişmez
//! denetimleri.
//!
//! Bütün fixture'lar kodla üretilir: repoya ikili belge girmez, gerçek kullanıcı
//! belgesi kullanılmaz, her şey geçici dizine yazılır. Rastgelelik tohumludur;
//! bir hata tohumuyla birlikte raporlanır ve aynı tohum aynı hatayı üretir.
#![allow(dead_code)]

pub mod check;
pub mod corpus;
pub mod raw;
pub mod rng;
