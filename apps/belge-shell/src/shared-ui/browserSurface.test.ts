// Tarayıcı menüsü koruması.
//
// Saha: Windows'ta sayfaya sağ tıklayınca WebView2'nin "Yenile / Farklı kaydet
// / Paylaş" menüsü açılıyor, "Yenile" bütün çalışmayı uyarısız siliyordu. Burada
// korunan KARAR: menü nerede kalır, nerede kapanır. Gerçek WebView2 menüsünün
// artık açılmadığını Windows CI'daki uçtan uca kapı ölçer (Kapı 6).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { installBrowserSurfaceGuard, keepsNativeMenu } from "./browserSurface";

/** `closest` için yeterli kadar DOM: etiket adı ve öznitelikler, üst zincir. */
class FakeElement {
  constructor(
    readonly tag: string,
    readonly parentElement: FakeElement | null = null,
    readonly attrs: Record<string, string> = {},
  ) {}
  private matches(simple: string): boolean {
    const attr = /^\[([a-z-]+)="([^"]*)"\]$/.exec(simple);
    if (attr) return this.attrs[attr[1]] === attr[2];
    return simple === this.tag;
  }
  closest(selector: string): FakeElement | null {
    const parts = selector.split(",").map((s) => s.trim());
    for (let at: FakeElement | null = this; at; at = at.parentElement) {
      if (parts.some((p) => at!.matches(p))) return at;
    }
    return null;
  }
}

const body = new FakeElement("body");
const pane = new FakeElement("div", body);
const paragraph = new FakeElement("p", pane);
const textNode = { parentElement: paragraph }; // closest'i yok
const preview = new FakeElement("img", pane);
const field = new FakeElement("input", pane);
const area = new FakeElement("textarea", pane);
const editable = new FakeElement("div", pane, { contenteditable: "true" });

const noSelection = { isCollapsed: true, containsNode: () => false };
const selecting = (...inside: unknown[]) => ({
  isCollapsed: false,
  containsNode: (node: never) => inside.includes(node),
});

describe("tarayıcı menüsü nerede açılır", () => {
  it("sayfanın boş yerinde ve belge metninde açılmaz (Yenile / Farklı kaydet / Paylaş)", () => {
    expect(keepsNativeMenu(body, noSelection)).toBe(false);
    expect(keepsNativeMenu(pane, noSelection)).toBe(false);
    expect(keepsNativeMenu(paragraph, noSelection)).toBe(false);
    expect(keepsNativeMenu(textNode, noSelection)).toBe(false);
    expect(keepsNativeMenu(null, noSelection)).toBe(false);
  });

  it("metin alanlarında açılır: Kes / Kopyala / Yapıştır orada beklenir", () => {
    expect(keepsNativeMenu(field, noSelection)).toBe(true);
    expect(keepsNativeMenu(area, noSelection)).toBe(true);
    expect(keepsNativeMenu(editable, noSelection)).toBe(true);
  });

  it("seçili metnin üzerinde açılır (Kopyala), seçimin DIŞINDA açılmaz", () => {
    expect(keepsNativeMenu(paragraph, selecting(paragraph))).toBe(true);
    expect(keepsNativeMenu(textNode, selecting(paragraph))).toBe(true);
    // Sayfada unutulmuş bir seçim, başka bir yerde menüyü geri getirmemeli.
    expect(keepsNativeMenu(pane, selecting(paragraph))).toBe(false);
  });

  it("önizleme görselinde hiç açılmaz, seçimin içinde olsa bile", () => {
    // "Resmi farklı kaydet" işaretsiz önizleme PNG'sini dışarı verirdi.
    expect(keepsNativeMenu(preview, noSelection)).toBe(false);
    expect(keepsNativeMenu(preview, selecting(preview, pane))).toBe(false);
  });
});

describe("koruma kurulumu", () => {
  function fakeDocument(selection: ReturnType<typeof selecting> | typeof noSelection) {
    const listeners: Array<{ fn: (e: MouseEvent) => void; capture: boolean }> = [];
    return {
      listeners,
      addEventListener: (_: "contextmenu", fn: (e: MouseEvent) => void, o: { capture: boolean }) =>
        listeners.push({ fn, capture: o.capture }),
      removeEventListener: (_: "contextmenu", fn: (e: MouseEvent) => void) => {
        const i = listeners.findIndex((l) => l.fn === fn);
        if (i >= 0) listeners.splice(i, 1);
      },
      getSelection: () => selection,
    };
  }
  function rightClick(doc: ReturnType<typeof fakeDocument>, target: unknown): boolean {
    let prevented = false;
    const event = { target, preventDefault: () => (prevented = true) } as unknown as MouseEvent;
    for (const l of doc.listeners) l.fn(event);
    return prevented;
  }

  it("yakalama evresinde dinler; boş yerde menüyü engeller, alanda engellemez", () => {
    const doc = fakeDocument(noSelection);
    const remove = installBrowserSurfaceGuard(doc);
    expect(doc.listeners).toHaveLength(1);
    expect(doc.listeners[0].capture).toBe(true);
    expect(rightClick(doc, paragraph)).toBe(true);
    expect(rightClick(doc, field)).toBe(false);
    remove();
    expect(doc.listeners).toHaveLength(0);
  });

  it("paketlenmiş uygulamanın girişi korumayı kurar (geliştirme dalının dışında)", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const main = readFileSync(resolve(here, "../main.tsx"), "utf8");
    const dev = main.indexOf("if (import.meta.env.DEV)");
    const orElse = main.indexOf("} else {", dev);
    const install = main.indexOf("installBrowserSurfaceGuard()", orElse);
    expect(dev).toBeGreaterThan(-1);
    expect(orElse).toBeGreaterThan(dev);
    // Kurulum else dalında: üretimde çalışır, geliştirmede "İncele" açık kalır.
    expect(install).toBeGreaterThan(orElse);
    expect(main.indexOf("ReactDOM.createRoot")).toBeGreaterThan(install);
  });
});
