// §54 kablolaması: kaybettiren her eylem bekçiden geçer; ürün işareti yalnız
// anlamlıyken düğmedir (madde 29); Düzenle kaydedilmemiş düzenlemeyi bildirir.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Sidebar } from "./Sidebar";
import { editSignature, hasUnsavedEdits } from "../modules/duzenek/PdfWorkspace";

const here = dirname(fileURLToPath(import.meta.url));
const app = readFileSync(resolve(here, "../App.tsx"), "utf8");

describe("Düzenle: kaydedilmemiş düzenleme", () => {
  const order = [{ key: "a#1" }, { key: "a#2" }];
  it("sıra ya da döndürme yoksa kirli değildir; 360° dönüş de değildir", () => {
    expect(editSignature(order, {}, false)).toBe("");
    expect(editSignature(order, { "a#1": 360 }, false)).toBe("");
    expect(hasUnsavedEdits("", "")).toBe(false);
  });
  it("döndürme ya da sıralama kirletir; AYNI düzen kaydedilince temizlenir", () => {
    const turned = editSignature(order, { "a#2": 90 }, false);
    expect(hasUnsavedEdits(turned, "")).toBe(true);
    expect(hasUnsavedEdits(turned, turned)).toBe(false);
    // Kayıttan sonra bir değişiklik daha: yeniden kirli.
    expect(hasUnsavedEdits(editSignature(order, { "a#2": 180 }, false), turned)).toBe(true);
    expect(hasUnsavedEdits(editSignature([order[1], order[0]], {}, true), "")).toBe(true);
  });
});

describe("ürün işareti (madde 29)", () => {
  const features = [{ key: "duzenek", label: "Düzenle", route: "/duzenle", enabled: true }] as never;
  it("dönülecek yer varken erişilebilir adıyla bir düğmedir", () => {
    const html = renderToStaticMarkup(
      <Sidebar features={features} current="/duzenle" onNavigate={() => {}} onOpenSettings={() => {}} onHome={() => {}} />,
    );
    expect(html).toMatch(/<div class="sidebar-head"[^>]*><button type="button" class="sidebar-home" aria-label="Başlangıç — son belgeler">/);
  });
  it("yoksa düğme değildir: anlamsız bir eylem verilmez", () => {
    const html = renderToStaticMarkup(
      <Sidebar features={features} current="/duzenle" onNavigate={() => {}} onOpenSettings={() => {}} />,
    );
    expect(html).not.toContain("sidebar-home");
  });
  it("Ekler'de ve belge yokken işaret düğme değildir", () => {
    expect(app).toMatch(/onHome=\{documents\.length > 0 && active && active\.key !== "ekler" \? \(\) => requestClose\("home"\) : undefined\}/);
  });
});

describe("bekçi her kaybettiren yolda", () => {
  it("kip değiştirmek, Kapat, işaret ve Düzenle'den belge değiştirmek bekçiden geçer", () => {
    expect(app).toMatch(/guard\("navigate", \(\) => navigateNow\(next\), target\)/);
    expect(app).toContain("onClick={() => requestClose()}");
    expect(app).toContain("onOpenDocument={replaceDocuments} onDirtyChange={markDuzenekDirty}");
    expect(app).toMatch(/guard\("replace", \(\) => void openDocuments\(paths\)\)/);
  });
  it("pencere kapanışı kaydedilmemiş iş varken sorulur; izin dar", () => {
    expect(app).toMatch(/onCloseRequested\(\(event\) => \{\s*if \(!leaveLoses\("window", workRef\.current\)\) return;\s*event\.preventDefault\(\);/);
    const caps = JSON.parse(readFileSync(resolve(here, "../../src-tauri/capabilities/default.json"), "utf8"));
    expect(caps.permissions).toContain("core:window:allow-destroy");
    expect(caps.permissions).not.toContain("core:window:default");
  });
  it("son belgeleri temizlemek geri alınamaz: her seferinde sorulur", () => {
    const at = app.indexOf("const forgetDocuments = useCallback(");
    expect(app.slice(at, at + 500)).toContain("setPending({");
    expect(app.slice(at, at + 500)).toContain("Bu işlem geri alınamaz.");
  });
});
