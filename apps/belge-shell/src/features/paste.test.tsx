// Yapıştırarak belge açmak (saha maddesi 21: "Ctrl+V aktif olsun").
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { isPasteChord } from "./DocumentSurface";

const here = dirname(fileURLToPath(import.meta.url));
const surface = readFileSync(resolve(here, "DocumentSurface.tsx"), "utf8");
const key = (k: string, mods: Partial<Record<"ctrlKey" | "metaKey" | "shiftKey" | "altKey", boolean>> = {}) => ({
  key: k, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods,
});

describe("yapıştırma kısayolu", () => {
  it("Windows'ta Ctrl+V, macOS'ta ⌘V; başka bileşim yapıştırma değildir", () => {
    expect(isPasteChord(key("v", { ctrlKey: true }), "windows")).toBe(true);
    expect(isPasteChord(key("V", { ctrlKey: true }), "windows")).toBe(true);
    expect(isPasteChord(key("v", { metaKey: true }), "mac")).toBe(true);
    expect(isPasteChord(key("v", { metaKey: true }), "windows")).toBe(false);
    expect(isPasteChord(key("v", { ctrlKey: true, shiftKey: true }), "windows")).toBe(false);
    expect(isPasteChord(key("v", { ctrlKey: true, altKey: true }), "windows")).toBe(false); // AltGr
    expect(isPasteChord(key("v"), "windows")).toBe(false);
  });
});

describe("yanlış yere belge yüklenmez", () => {
  const handler = surface.slice(surface.indexOf("const onKey = (event: KeyboardEvent)"), surface.indexOf("window.addEventListener(\"keydown\", onKey)"));
  it("metin alanında ve açık pencerede kısayol metnin/pencerenindir", () => {
    expect(handler).toContain('closest?.("input, textarea, select, [contenteditable]")');
    expect(handler).toContain('document.querySelector(".sheet-backdrop")');
  });
  it("pano yerelden okunur ve sürükle-bırakla aynı süzgeçten geçer; boşsa söylenir", () => {
    expect(handler).toContain("await pastedDocumentPaths()");
    expect(handler).toContain("accept(paths)");
    expect(handler).toContain("Panoda açılabilecek bir belge yok");
  });
  it("yetenek görünür: ipucu kısayolu söyler", () => {
    expect(surface).toContain('ya da {shortcutLabel("V")} ile yapıştırın');
  });
});
