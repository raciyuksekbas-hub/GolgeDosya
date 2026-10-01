// Kaydedilen PDF'nin durum metni.
//
// Sıkıştırmada sonuç ilk satırda insan diliyle söylenir; motorun iç ölçüsü
// ("Sıkıştırılmış belge … · Marka dahil …") kullanıcı metninden çıktı.
// Alındı (yol ve sayfa) her araçta kalır: kullanıcı dosyanın yerini oradan
// öğrenir.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { compressResultLine, savedStatus } from "./savedMessage";

const here = dirname(fileURLToPath(import.meta.url));
const PATH = "/Users/ornek/Belgeler/GolgeDosya-compress.pdf";

describe("sıkıştırma başarı satırı", () => {
  it("kullanıcının örneği: 9,8 MB → 4,1 MB", () => {
    expect(compressResultLine(10_276_045, 4_299_161)).toBe(
      "Sıkıştırıldı: 9,8 MB → 4,1 MB (yaklaşık %58 küçüldü).",
    );
  });

  it("gerçek motor sonucu (13,2 MB tarama) ve alındı", () => {
    const text = savedStatus({
      kind: "compress",
      outputPath: PATH,
      pageCount: 1,
      sourceBytes: 13_801_597,
      outputBytes: 534_529,
    });
    expect(text.split("\n")).toEqual([
      "Sıkıştırıldı: 13,2 MB → 522 KB (yaklaşık %96 küçüldü).",
      `PDF kaydedildi ve yeniden açılarak doğrulandı: ${PATH}`,
      "1 sayfa",
    ]);
    expect(text).not.toMatch(/Marka dahil|Sıkıştırılmış belge/);
    expect(text).not.toMatch(/\d\.\d/);
  });

  it("politikanın kaydettiği küçük kazanç: tamsayı yüzde", () => {
    expect(compressResultLine(9_834_246, 9_625_452)).toBe(
      "Sıkıştırıldı: 9,4 MB → 9,2 MB (yaklaşık %2 küçüldü).",
    );
  });

  it("oran çıkmıyorsa uydurulmaz", () => {
    expect(compressResultLine(1_123, 2_407)).toBe("Sıkıştırıldı: 1 KB → 2 KB.");
  });
});

describe("öteki araçlar aynı sayı dilini konuşur", () => {
  it.each(["merge", "select", "delete", "reorder", "rotate", "crop", "watermark", "number", "images"])(
    "%s",
    (kind) => {
      const text = savedStatus({
        kind,
        outputPath: PATH,
        pageCount: 30,
        sourceBytes: 10_276_045,
        outputBytes: 4_299_161,
      });
      expect(text).toBe(`PDF kaydedildi ve yeniden açılarak doğrulandı: ${PATH}\n30 sayfa · 4,1 MB`);
    },
  );
});

describe("kabuğa bağlanma", () => {
  const workspace = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");

  it("başarı durumu bu modülden kurulur, kabukta sayı biçimlenmez", () => {
    expect(workspace).toContain("setStatus(savedStatus(");
    // toFixed nokta koyar: kullanıcı metninde Türkçe ondalık kalmaz.
    expect(workspace).not.toContain("toFixed(");
    expect(workspace).not.toContain("Marka dahil");
    expect(workspace).not.toContain("küçültüldü.");
  });
});
