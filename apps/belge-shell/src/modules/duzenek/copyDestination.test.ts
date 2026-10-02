// Düzenle'nin kaydetme penceresinin önerdiği ad.
//
// Ad UTC damgalıydı (`toISOString`): Türkiye'de üç saat geri, gece
// yarısından sonra bir gün önceki tarih.
process.env.TZ = "Europe/Istanbul";

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const dialog = vi.hoisted(() => ({ save: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);
vi.mock("@tauri-apps/api/path", () => ({ join: async (...parts: string[]) => parts.join("/") }));

import { copyDestination } from "./copyDestination";

beforeEach(() => {
  vi.useFakeTimers();
  // 2 Ekim 01:30:31 Türkiye saati = 1 Ekim 22:30:31 UTC.
  vi.setSystemTime(new Date("2026-10-01T22:30:31.130Z"));
  dialog.save.mockReset().mockResolvedValue(null);
  dialog.open.mockReset().mockResolvedValue(null);
});
afterEach(() => vi.useRealTimers());

describe("addaki damga yerel saattir", () => {
  it("UTC değil: gece yarısından sonra tarih bir gün geride kalmaz", async () => {
    await copyDestination("GolgeDosya-compress", false);
    const name: string = dialog.save.mock.calls[0][0].defaultPath;
    expect(name).toContain("2026-10-02_013031");
    expect(name).not.toMatch(/2026-10-01|T22-30|Z\.pdf$/);
    // Ürün öneki korunur.
    expect(name.startsWith("GolgeDosya-compress-")).toBe(true);
  });
});
