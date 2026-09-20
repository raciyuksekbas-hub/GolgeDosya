// Döndürme semantiği — saha turunun §8 regresyonu.
//
// SAHA BULGUSU: Döndür ergonomik değildi. Kullanıcı önce küçük resimde
// sayfayı İŞARETLEMEK, sonra aracı açmak, sonra "Sola 90°"/"Sağa 90°"
// seçmek zorundaydı; birden fazla kutu işaretliyse HEPSİ dönüyordu.
//
// Canonical davranış: Döndür = o anda ÖNİZLENEN sayfayı 90° saat yönünde
// döndür. Şeritteki çoklu seçim kapsamı belirlemez.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { rotatePages } from "./pdfWorkspaceState";

const source = readFileSync(
  resolve(dirname(fileURLToPath(import.meta.url)), "PdfWorkspace.tsx"),
  "utf8",
);

describe("§8 — kapsam yalnız aktif sayfa", () => {
  // "sayfa 1 aktif + sayfa 1,2,3 işaretli -> Döndür"
  it("işaretli diğer sayfalar DÖNMEZ", () => {
    const after = rotatePages({}, ["s1"], 90);
    expect(after.s1).toBe(90);
    expect(after.s2).toBeUndefined();
    expect(after.s3).toBeUndefined();
  });

  it("aktif sayfa iki tıklamada 180°", () => {
    let r = rotatePages({}, ["s2"], 90);
    r = rotatePages(r, ["s2"], 90);
    expect(r.s2).toBe(180);
  });

  it("üç tıklama 270°, dört tıklama başa döner", () => {
    let r: Record<string, number> = {};
    for (let i = 0; i < 3; i += 1) r = rotatePages(r, ["s1"], 90);
    expect(r.s1).toBe(270);
    r = rotatePages(r, ["s1"], 90);
    expect(r.s1).toBe(0);
  });
});

describe("§6/§7 — tek kontrol, tek yön", () => {
  it("sola/sağa ayrı düğmeler kalktı", () => {
    expect(source).not.toContain("Sola 90°");
    expect(source).not.toContain("Sağa 90°");
  });

  it("tek Döndür düğmesi var ve +90 uygular", () => {
    expect(source).toContain("↷ Döndür");
    expect(source).toContain("rotatePages(previous, [active.key], 90)");
    expect(source).not.toContain("rotatePages(previous, selected,");
  });

  it("düğme aktif sayfaya bağlı, seçime değil", () => {
    expect(source).toContain("disabled={busy || !active}");
  });

  it("döndürme kipinde sayfa kutusu çizilmez", () => {
    // "Döndürmek için ayrıca checkbox/seçim yapılması gerekmez."
    expect(source).toContain("const pageSelection = ['select', 'delete'].includes(kind);");
  });

  it("araç açıklaması kapsamı doğru söyler", () => {
    expect(source).toContain("Önizlediğiniz sayfayı her tıklamada 90° saat yönünde döndürür.");
    expect(source).not.toContain("İşaretlediğiniz sayfaları 90° adımlarla döndürür.");
  });
});
