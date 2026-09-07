import { defineConfig } from "vitest/config";

// Taşınan Karşılaştır motoru kendi test koşucusuyla gelir: testler yeniden
// yazılmadığı için koşucu da değişmiyor. Kabuğun kendi sözleşme testleri
// `node --test` ile qa/tests altında koşar; vitest onlara bakmamalı.
export default defineConfig({
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
    // Bütün paket tek forkta koşar: paralel forklar bu makinede bellek
    // tükettiği için baseline koşusu SIGKILL almıştı.
    pool: "forks",
    poolOptions: { forks: { singleFork: true } },
  },
});
