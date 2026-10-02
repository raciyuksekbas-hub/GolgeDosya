// Kaydetme penceresinin başlangıç klasörü — Düzenle, Ekler ve Denetle.
//
// Saha: "Kayıt klasörleri arasında birlik yok." Pencereler başlangıç klasörü
// almıyordu; işletim sistemi en son hatırladığı klasörü açıyordu.
import { afterEach, describe, expect, it, vi } from "vitest";

const core = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => core);

import { dialogPath, inFolder, START_FOLDER_WAIT_MS, startFolder } from "./savePlace";

describe("pencerenin önerdiği yol", () => {
  it("klasör + ad, klasörün kendi ayracıyla", () => {
    expect(dialogPath("/Users/avukat/Dava", "a.pdf")).toBe("/Users/avukat/Dava/a.pdf");
    expect(dialogPath("C:\\Users\\Çağrı Şahin\\Dava", "a.pdf")).toBe("C:\\Users\\Çağrı Şahin\\Dava\\a.pdf");
    expect(inFolder("/", "a.pdf")).toBe("/a.pdf");
    expect(inFolder("D:\\", "a.pdf")).toBe("D:\\a.pdf");
  });

  it("öneri yoksa yalnız ad: klasörü işletim sistemi seçer", () => {
    expect(dialogPath(null, "a.pdf")).toBe("a.pdf");
  });
});

describe("pencerenin başlangıç klasörü", () => {
  afterEach(() => {
    core.invoke.mockReset();
    vi.useRealTimers();
  });

  it("kararı arka uç verir: kaynağın klasörü ya da öneri yok", async () => {
    core.invoke.mockResolvedValueOnce("/Users/avukat/Dava");
    expect(await startFolder("/Users/avukat/Dava/a.pdf")).toBe("/Users/avukat/Dava");
    expect(core.invoke).toHaveBeenCalledWith("output_start_folder", { source: "/Users/avukat/Dava/a.pdf" });
    // Geçici klasör, arşivin içi, yazılamayan klasör: işletim sisteminin varsayılanı.
    core.invoke.mockResolvedValueOnce(null);
    expect(await startFolder("/private/var/folders/x/T/ek.pdf")).toBeNull();
  });

  it("kaynak yoksa sorulmaz", async () => {
    expect(await startFolder(undefined)).toBeNull();
    expect(core.invoke).not.toHaveBeenCalled();
  });

  it("öneri alınamazsa ya da gecikirse pencere yine açılır", async () => {
    core.invoke.mockRejectedValueOnce(new Error("komut yok"));
    expect(await startFolder("/Users/avukat/Dava/a.pdf")).toBeNull();
    // Erişilemeyen ağ paylaşımı: yoklama bitmiyor.
    vi.useFakeTimers();
    core.invoke.mockReturnValueOnce(new Promise(() => undefined));
    const asked = startFolder("//nas/Dava/a.pdf");
    await vi.advanceTimersByTimeAsync(START_FOLDER_WAIT_MS);
    expect(await asked).toBeNull();
  });
});
