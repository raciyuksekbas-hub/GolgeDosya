// Yerel pencere çerçevesi uygulamanın temasını izler (Windows başlık çubuğu).
import { describe, expect, it, vi } from "vitest";

const setTheme = vi.fn(async (_: unknown) => {});
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ setTheme }) }));

const { syncWindowTheme } = await import("./theme");

describe("syncWindowTheme", () => {
  it("Koyu/Açık çerçeveye aynen, Sistem ise işletim sistemine bırakılır", async () => {
    await syncWindowTheme("dark");
    await syncWindowTheme("light");
    await syncWindowTheme("system");
    expect(setTheme.mock.calls.map((c) => c[0])).toEqual(["dark", "light", null]);
  });

  it("pencere yoksa ya da izin reddedilirse sessizce geçer; içerik teması etkilenmez", async () => {
    setTheme.mockRejectedValueOnce(new Error("not allowed"));
    await expect(syncWindowTheme("dark")).resolves.toBeUndefined();
  });
});
