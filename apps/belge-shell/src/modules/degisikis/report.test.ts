import { describe, expect, it, vi } from "vitest";
import {
  buildReport, buildReportHtml, buildReportJson, buildReportMarkdown,
  formatTimestamp, generateAndOpenReport, openReport, reportFileName, saveReport,
  type ReportFormat, type ReportInput,
} from "./report";
import { summarize } from "./viewModels/comparisonViewModel";
import type { ComparisonChange } from "./viewModels/comparisonViewModel";

const changes: ComparisonChange[] = [
  {
    id: "a", index: 1, displayIndex: "01", kind: "modified", sectionLabel: "Madde 3.1",
    summary: ['"100.000 TL" → "118.000 TL"'], rowIndices: [8],
    leftText: "hizmet bedeli KDV hariç 100.000 TL'dir.",
    rightText: "hizmet bedeli KDV dahil 118.000 TL'dir.",
  },
  {
    id: "b", index: 2, displayIndex: "02", kind: "added", sectionLabel: "Madde 3.3",
    summary: ["Bölüm bütünüyle eklendi."], rowIndices: [10], rightText: "<sabit & değişmez>",
  },
];

const input: ReportInput = {
  baseName: "Hizmet Sözleşmesi_v3.docx",
  revisedName: "Hizmet Sözleşmesi_v4.docx",
  generatedAt: new Date(2026, 7, 21, 14, 5),
  summary: summarize(changes),
  changes,
};

describe("rapor üretimi", () => {
  it("HTML raporunda belgeler, özet ve bütün farklar bulunur", () => {
    const html = buildReportHtml(input);
    expect(html).toContain("Hizmet Sözleşmesi_v3.docx");
    expect(html).toContain("Hizmet Sözleşmesi_v4.docx");
    expect(html).toContain("21.08.2026 14:05");
    expect(html).toContain("Toplam fark");
    expect(html).toContain("01");
    expect(html).toContain("02");
    expect(html).toContain("Madde 3.1");
    expect(html).toContain("Madde 3.3");
  });

  it("HTML raporu belge metnini kaçırır", () => {
    const html = buildReportHtml(input);
    expect(html).toContain("&lt;sabit &amp; değişmez&gt;");
    expect(html).not.toContain("<sabit");
  });

  it("HTML raporu ağ kaynağı kullanmaz", () => {
    const html = buildReportHtml(input);
    expect(html).not.toMatch(/https?:\/\//);
    expect(html).not.toContain("<script");
  });

  it("oranlar özet ile aynıdır", () => {
    const html = buildReportHtml(input);
    expect(html).toContain(`%${input.summary.addedPct}`);
    expect(html).toContain(`%${input.summary.modifiedPct}`);
    expect(input.summary.addedPct + input.summary.removedPct + input.summary.modifiedPct).toBe(100);
  });

  it("Markdown raporu özet tablosu ve fark başlıkları içerir", () => {
    const markdown = buildReportMarkdown(input);
    expect(markdown).toContain("# GölgeDosya — Karşılaştırma Raporu");
    expect(markdown).toContain("| Ekleme | 1 |");
    expect(markdown).toContain("### 01 · Değiştirilen · Madde 3.1");
    expect(markdown).toContain("### 02 · Eklenen · Madde 3.3");
  });

  it("JSON çıktısı ayrıştırılabilir ve özetle tutarlıdır", () => {
    const parsed = JSON.parse(buildReportJson(input));
    expect(parsed.summary.total).toBe(2);
    expect(parsed.changes).toHaveLength(2);
    expect(parsed.changes[0].displayIndex).toBe("01");
    expect(parsed.documents.base).toBe("Hizmet Sözleşmesi_v3.docx");
  });

  it("biçim seçimi doğru üreticiye gider", () => {
    expect(buildReport(input, "markdown").startsWith("# GölgeDosya")).toBe(true);
    expect(buildReport(input, "json").startsWith("{")).toBe(true);
    expect(buildReport(input, "html").startsWith("<!doctype html>")).toBe(true);
  });

  it("dosya adı zaman damgalı ve güvenli karakterlerden oluşur", () => {
    const name = reportFileName(input, "html");
    expect(name).toBe("GolgeDosya-Karsilastirma-20260821-1405.html");
    expect(name).toMatch(/^[A-Za-z0-9._-]+$/);
    expect(reportFileName(input, "markdown").endsWith(".md")).toBe(true);
    expect(reportFileName(input, "json").endsWith(".json")).toBe(true);
  });

  it("zaman damgası yerel biçimde üretilir", () => {
    expect(formatTimestamp(new Date(2026, 0, 5, 9, 7))).toBe("05.01.2026 09:07");
  });

  it("kaydetme masaüstü komutunu kullanır ve yolu döndürür", async () => {
    const invoker = vi.fn(async () => "/Users/x/Downloads/rapor.html");
    const bytes = new TextEncoder().encode("<html></html>");
    const saved = await saveReport("rapor.html", bytes, "html", invoker);
    expect(invoker).toHaveBeenCalledWith("rapor.html", bytes);
    expect(saved.path).toBe("/Users/x/Downloads/rapor.html");
  });
});

describe("üret → kaydet → aç akışı", () => {
  const formats: ReportFormat[] = ["html", "docx", "markdown", "json"];

  it("kaydetme başarılıysa açma çağrılır ve tam başarı bildirilir", async () => {
    const open = vi.fn(async () => undefined);
    const save = vi.fn(async () => ({ path: "/Users/x/Downloads/GolgeDosya-Karsilastirma-20260821-1405.html" }));
    const outcome = await generateAndOpenReport(input, "html", { save, open });
    expect(open).toHaveBeenCalledWith("/Users/x/Downloads/GolgeDosya-Karsilastirma-20260821-1405.html");
    expect(outcome.opened).toBe(true);
    expect(outcome.message).toContain("açıldı");
  });

  it("kaydetme başarısızsa açma hiç denenmez", async () => {
    const open = vi.fn(async () => undefined);
    const save = vi.fn(async () => { throw new Error("REPORT_WRITE"); });
    await expect(generateAndOpenReport(input, "html", { save, open })).rejects.toThrow();
    expect(open).not.toHaveBeenCalled();
  });

  it("kaydedildi fakat açılamadıysa dosya korunur ve yol bildirilir", async () => {
    const path = "/Users/x/Downloads/GolgeDosya-Karsilastirma-20260821-1405.docx";
    const open = vi.fn(async () => { throw new Error("REPORT_OPEN"); });
    const outcome = await generateAndOpenReport(input, "docx", { save: async () => ({ path }), open });
    expect(outcome.opened).toBe(false);
    expect(outcome.path).toBe(path);
    expect(outcome.message).toContain("açılamadı");
    expect(outcome.message).toContain(path);
  });

  it("DOCX yolu açma katmanına olduğu gibi iletilir", async () => {
    const path = "/Users/x/Downloads/GolgeDosya-Karsilastirma-20260821-1405.docx";
    const open = vi.fn(async () => undefined);
    const save = vi.fn(async (fileName: string, bytes: Uint8Array) => {
      expect(fileName.endsWith(".docx")).toBe(true);
      expect([bytes[0], bytes[1]]).toEqual([0x50, 0x4b]);
      return { path };
    });
    await generateAndOpenReport(input, "docx", { save, open });
    expect(open).toHaveBeenCalledWith(path);
  });

  it("bütün biçimler aynı akışı kullanır", async () => {
    for (const format of formats) {
      const open = vi.fn(async () => undefined);
      const save = vi.fn(async (fileName: string) => ({ path: `/Users/x/Downloads/${fileName}` }));
      const outcome = await generateAndOpenReport(input, format, { save, open });
      expect(save).toHaveBeenCalledTimes(1);
      expect(open).toHaveBeenCalledTimes(1);
      expect(outcome.opened).toBe(true);
    }
  });

  it("tarayıcı yedeğinde işletim sistemi uygulaması açılmaya çalışılmaz", async () => {
    const open = vi.fn(async () => undefined);
    const outcome = await generateAndOpenReport(input, "html", { save: async () => ({}), open });
    expect(open).not.toHaveBeenCalled();
    expect(outcome.opened).toBe(false);
    expect(outcome.message).toContain("indirildi");
  });

  it("açma katmanına yalnız yol geçer, komut dizesi kurulmaz", async () => {
    const received: unknown[] = [];
    const path = "/Users/x/Downloads/GolgeDosya-Karsilastirma-20260821-1405.json";
    await generateAndOpenReport(input, "json", {
      save: async () => ({ path }),
      open: async (...args) => { received.push(args); },
    });
    expect(received).toEqual([[path]]);
    expect(String(received[0])).not.toMatch(/[;&|`$]/u);
  });

  it("openReport hatayı yutar ve false döndürür", async () => {
    await expect(openReport("/x", async () => { throw new Error("yok"); })).resolves.toBe(false);
    await expect(openReport("/x", async () => undefined)).resolves.toBe(true);
  });
});
