//! Bağımlılıksız, platformdan bağımsız tohumlu rastgelelik (xorshift64*).

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Sıfır durumu xorshift'i kilitler; tohum karıştırılıp tek yapılır.
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// `0..n` aralığında (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "boş aralık");
        (self.next_u64() % n as u64) as usize
    }

    /// `lo..=hi` aralığında.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next_u64() % ((hi - lo + 1) as u64)) as i64
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }

    /// Boş olmayan, sırası karışık bir alt küme (1 tabanlı sayfa numaraları).
    pub fn subset(&mut self, total: usize) -> Vec<usize> {
        let mut pages: Vec<usize> = (1..=total).collect();
        self.shuffle(&mut pages);
        let keep = 1 + self.below(total);
        pages.truncate(keep);
        pages
    }
}

/// `VAR=3,7,11` ya da `VAR=100..140` biçiminde tohumlar; yoksa varsayılan.
pub fn seeds_from_env(var: &str, default: &[u64]) -> Vec<u64> {
    let Ok(text) = std::env::var(var) else {
        return default.to_vec();
    };
    if let Some((a, b)) = text.split_once("..") {
        let (a, b): (u64, u64) = (a.trim().parse().unwrap(), b.trim().parse().unwrap());
        return (a..b).collect();
    }
    text.split(',')
        .map(|s| s.trim().parse().expect("tohum sayısal olmalı"))
        .collect()
}

/// Ortam değişkeninden sayı; yoksa varsayılan.
pub fn usize_from_env(var: &str, default: usize) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
