// Düzenle'nin kaydetme penceresi: nerede açılır, hangi adı önerir.
//
// Saha: "Kayıt klasörleri arasında birlik yok. Farklı araçlarda farklı
// klasörlere kaydediliyor sanki." Pencere yalnız dosya adını alıyordu;
// klasörü işletim sistemi en son hatırladığı yerden seçiyordu. Ad da UTC
// damgalıydı: Türkiye'de üç saat geri, gece yarısından sonra bir gün önce.
process.env.TZ = "Europe/Istanbul";

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const dialog = vi.hoisted(() => ({ save: vi.fn(), open: vi.fn() }));
const core = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);
vi.mock("@tauri-apps/api/core", () => core);
vi.mock("@tauri-apps/api/path", () => ({ join: async (...parts: string[]) => parts.join("/") }));

import { copyDestination } from "./copyDestination";

const here = dirname(fileURLToPath(import.meta.url));
const SOURCE = "/Users/avukat/Dava/Müvekkil Dosyası/sözleşme.pdf";
const FOLDER = "/Users/avukat/Dava/Müvekkil Dosyası";

/** Arka ucun kararı: kaynağın klasörü ya da öneri yok. */
function backendSuggests(folder: string | null) {
  core.invoke.mockImplementation(async (cmd: string, args: { source: string }) => {
    if (cmd !== "output_start_folder") throw new Error(`beklenmeyen komut: ${cmd}`);
    expect(args.source).toBe(SOURCE);
    return folder;
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  // 2 Ekim 01:30:31 Türkiye saati = 1 Ekim 22:30:31 UTC.
  vi.setSystemTime(new Date("2026-10-01T22:30:31.130Z"));
  dialog.save.mockReset().mockResolvedValue(null);
  dialog.open.mockReset().mockResolvedValue(null);
  core.invoke.mockReset();
});
afterEach(() => vi.useRealTimers());

describe("kaydetme penceresi belgenin klasöründe açılır", () => {
  it("Kaydet: önerilen yol <kaynağın klasörü>/<ad>", async () => {
    backendSuggests(FOLDER);
    await copyDestination("GolgeDosya-compress", false, SOURCE);
    const options = dialog.save.mock.calls[0][0];
    expect(options.defaultPath).toBe(`${FOLDER}/GolgeDosya-compress-2026-10-02_013031.pdf`);
    expect(options.filters).toEqual([{ name: "Yeni PDF", extensions: ["pdf"] }]);
  });

  it("Klasör Seçerek Kaydet: klasör penceresi de aynı klasörde açılır", async () => {
    backendSuggests(FOLDER);
    dialog.open.mockResolvedValue("/Users/avukat/Masaüstü");
    const target = await copyDestination("GolgeDosya-merge", true, SOURCE);
    expect(dialog.open.mock.calls[0][0]).toMatchObject({ directory: true, defaultPath: FOLDER });
    // Kullanıcının seçtiği yer esas: öneri yalnız başlangıçtır.
    expect(target).toBe("/Users/avukat/Masaüstü/GolgeDosya-merge-2026-10-02_013031.pdf");
  });

  it("kaynak geçici bir yerdeyse öneri yok: işletim sisteminin varsayılanı kalır", async () => {
    backendSuggests(null);
    await copyDestination("GolgeDosya-select", false, SOURCE);
    expect(dialog.save.mock.calls[0][0].defaultPath).toBe("GolgeDosya-select-2026-10-02_013031.pdf");
    backendSuggests(null);
    await copyDestination("GolgeDosya-select", true, SOURCE);
    expect(dialog.open.mock.calls[0][0].defaultPath).toBeUndefined();
  });

  it("öneri alınamazsa pencere yine açılır", async () => {
    core.invoke.mockRejectedValue(new Error("komut yok"));
    await copyDestination("GolgeDosya-crop", false, SOURCE);
    expect(dialog.save).toHaveBeenCalledTimes(1);
    expect(dialog.save.mock.calls[0][0].defaultPath).toBe("GolgeDosya-crop-2026-10-02_013031.pdf");
  });
});

describe("addaki damga yerel saattir", () => {
  it("UTC değil: gece yarısından sonra tarih bir gün geride kalmaz", async () => {
    backendSuggests(null);
    await copyDestination("GolgeDosya-compress", false, SOURCE);
    const name: string = dialog.save.mock.calls[0][0].defaultPath;
    expect(name).toContain("2026-10-02_013031");
    expect(name).not.toMatch(/2026-10-01|T22-30|Z\.pdf$/);
    // Ürün öneki korunur.
    expect(name.startsWith("GolgeDosya-compress-")).toBe(true);
  });
});

describe("çalışma alanına bağlanma", () => {
  const workspace = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");

  it("PDF kopyası açık belgenin yolunu verir", () => {
    expect(workspace).toContain("copyDestination(`GolgeDosya-${kind}`, folderOnly, sources[0]?.path)");
  });

  it("görsel paketi penceresi de belgenin klasöründe, başlıklı açılır", () => {
    const at = workspace.indexOf("if (kind === 'raster') {");
    const raster = workspace.slice(at, workspace.indexOf("return;", at));
    expect(raster).toMatch(/open\(\{ directory: true,[^}]*title: 'Görsel paketinin kaydedileceği klasör'/);
    expect(raster).toContain("defaultPath: (await startFolder(sources[0]?.path)) ?? undefined");
  });
});
