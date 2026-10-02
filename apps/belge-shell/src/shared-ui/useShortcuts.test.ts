// Kısayollar yalnız gerçek metin girişinde susar (v0.3.2 a11y incelemesi).
//
// Sırala'da ⌘Z / Ctrl+Z, odak bir Seç kutusundayken ya da Yakınlaştır
// listesindeyken hiçbir şey yapmıyordu: koruma her INPUT ve SELECT'i "metin
// girişi" sayıyordu. Windows'ta (WebView2) Seç kutusuna tıklamak ona odak
// verir; klavye kullanıcısı da kutuyu Boşluk ile işaretler. İkisinde de Geri Al
// düğmesinin ipucundaki kısayol sessizce ölüyordu.
import { describe, expect, it } from "vitest";
import * as hook from "./useShortcuts";

const el = (tagName: string, extra: Record<string, unknown> = {}) =>
  ({ tagName, isContentEditable: false, ...extra }) as unknown as EventTarget;

const inTextEntry = () => {
  const f = (hook as Record<string, unknown>).inTextEntry;
  expect(typeof f, "inTextEntry useShortcuts'tan dışa açılmalı").toBe("function");
  return f as (target: EventTarget | null) => boolean;
};

describe("kısayol koruması yalnız metin girişini sayar", () => {
  it("metin yazılan alanlar kısayolu yutar", () => {
    for (const type of ["text", "search", "number", "email", "url", "tel", "password", "date"])
      expect(inTextEntry()(el("INPUT", { type })), type).toBe(true);
    expect(inTextEntry()(el("TEXTAREA"))).toBe(true);
    expect(inTextEntry()(el("DIV", { isContentEditable: true }))).toBe(true);
  });

  it("Seç kutusu, düğme ve Yakınlaştır listesi kısayolu yutmaz", () => {
    for (const type of ["checkbox", "radio", "button", "submit", "range", "color", "file"])
      expect(inTextEntry()(el("INPUT", { type })), type).toBe(false);
    expect(inTextEntry()(el("SELECT"))).toBe(false);
    expect(inTextEntry()(el("BUTTON"))).toBe(false);
    expect(inTextEntry()(null)).toBe(false);
  });
});
