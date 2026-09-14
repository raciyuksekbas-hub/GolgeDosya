// Kaydetme hedefi — kabuk ile motor arasındaki uzantı sözleşmesi.
//
// Motor uzantısız bir hedefe de yazar; ama kabuk kaydettiği dosyayı tarayıcıyla
// yeniden açar ve tarayıcı biçimi UZANTIDAN okur. Uzantısız hedef = dosya
// diskte, kullanıcıya "kaydedilemedi". Bu test o boşluğu kilitler.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { withPdfExtension } from "./copyDestination";
import { MODES } from "../../shell/modes";

const here = dirname(fileURLToPath(import.meta.url));
const workspace = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");
const destination = readFileSync(resolve(here, "copyDestination.ts"), "utf8");

describe("kaydetme hedefi", () => {
  it("uzantısız seçim .pdf ile tamamlanır, mevcut uzantı korunur", () => {
    expect(withPdfExtension("/Belgeler/kopya")).toBe("/Belgeler/kopya.pdf");
    expect(withPdfExtension("/Belgeler/kopya.pdf")).toBe("/Belgeler/kopya.pdf");
    expect(withPdfExtension("/Belgeler/KOPYA.PDF")).toBe("/Belgeler/KOPYA.PDF");
    // Türkçe ve boşluklu yollar dokunulmadan geçer.
    expect(withPdfExtension("/Belgeler/Müvekkil Dosyası/ek-3 rapor")).toBe(
      "/Belgeler/Müvekkil Dosyası/ek-3 rapor.pdf",
    );
    // Noktayla biten ad `.pdf.` gibi çift uzantı üretmez.
    expect(withPdfExtension("/Belgeler/rapor.v2")).toBe("/Belgeler/rapor.v2.pdf");
  });

  it("kaydetme penceresinin sonucu normalize edilmeden motora gitmez", () => {
    expect(destination).toMatch(/withPdfExtension\(picked\)/);
  });
});

describe("silme aracı", () => {
  it("boş seçimle başlar: sayfa 1 kendiliğinden çıkarılacak sayfa olamaz", () => {
    // `build` ilk sayfayı işaretler (Seç ve Döndür için doğru varsayılan).
    // Sil aracında aynı varsayılan, kullanıcı hiçbir şey yapmadan kaydet
    // düğmesini açıyor ve sayfa 1'i çıkarıyordu.
    expect(workspace).toMatch(/if \(key === 'delete'\)\s*setSelected\(\[\]\);/);
  });
});

describe("kipin kapısı", () => {
  it("Düzenle yalnız PDF kabul eder: çalışma alanı ham yol üzerinde çalışır", () => {
    // Önizleme (`duzenek_preview_pdf_page`) ve her araç (`duzenek_run_pdf_tool`)
    // kaynağın yolunu doğrudan motora verir; motor PDF olmayan yolu reddeder.
    // Kabukta ofis dönüşümü çağrılmadığı sürece kapı PDF'den geniş olamaz —
    // aksi hâlde son belgeler listesi açılamayan satırlar gösterir.
    expect(MODES.duzenek.extensions).toEqual(["pdf"]);
    expect(workspace).not.toContain("duzenek_convert_office_pdf");
    expect(MODES.duzenek.hint).not.toMatch(/Word|UYAP|dönüştürülür/);
  });
});
