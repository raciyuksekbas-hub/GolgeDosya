// Karşılaştır: eş zamanlı kaydırma anahtarı (madde 28), "Değişiklikleri
// Kopyala" (madde 33) ve seçilebilir belge metni. Üçü de bağımsız Değişikİş'te
// vardı ve taşımada (b9a5ca4) düştü.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { isValidElement, type ReactElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { CompareActions, copyAllChanges } from "./CompareWorkspace";
import { compareDocuments } from "./core/compare";
import { copyChangesText } from "./uiLabels";
import { filterChanges, buildComparisonViewModel } from "./viewModels/comparisonViewModel";
import type { DocumentBlock } from "./core/types";

const here = dirname(fileURLToPath(import.meta.url));
const noop = () => undefined;

function find(node: ReactNode, match: (props: Record<string, unknown>, type: unknown) => boolean): ReactElement | null {
  if (Array.isArray(node)) {
    for (const child of node) {
      const hit = find(child, match);
      if (hit) return hit;
    }
    return null;
  }
  if (!isValidElement(node)) return null;
  const props = node.props as Record<string, unknown>;
  if (match(props, node.type)) return node;
  return find(props.children as ReactNode, match);
}

describe("Eş zamanlı kaydır (madde 28)", () => {
  it("gerçek bir onay kutusu, switch rolü, erişilebilir adıyla; varsayılan açık", () => {
    const html = renderToStaticMarkup(
      <CompareActions sync onSync={noop} canCopy copyState="idle" onCopy={noop} />,
    );
    expect(html).toMatch(/<label class="toolbar-switch"><input type="checkbox" role="switch" checked=""\/>Eş zamanlı kaydır<\/label>/);
    const off = renderToStaticMarkup(
      <CompareActions sync={false} onSync={noop} canCopy copyState="idle" onCopy={noop} />,
    );
    expect(off).not.toContain('checked=""');
  });

  it("anahtar kapatılınca durum bildirilir", () => {
    const onSync = vi.fn();
    const tree = CompareActions({ sync: true, onSync, canCopy: true, copyState: "idle", onCopy: noop });
    const input = find(tree, (p) => p.role === "switch");
    (input!.props as { onChange: (e: unknown) => void }).onChange({ target: { checked: false } });
    expect(onSync).toHaveBeenCalledWith(false);
  });

  it("kapalıyken kaydırma izletilmez; seçim oturumda hatırlanır, diske yazılmaz", () => {
    const src = readFileSync(resolve(here, "CompareWorkspace.tsx"), "utf8");
    expect(src).toContain("const session = { sync: true };");
    expect(src).toContain("useState(session.sync)");
    expect(src).toMatch(/if \(!sync\) return;\s+paneSync\.current\.follow\(/);
    expect(src).not.toMatch(/saveSettings|localStorage/);
  });
});

describe("Değişiklikleri Kopyala (madde 33)", () => {
  const block = (id: string, text: string, index: number): DocumentBlock =>
    ({ id, text, type: "paragraph", index }) as unknown as DocumentBlock;
  const base = [block("a1", "Kira bedeli 100.000 TL olup KDV hariçtir.", 0), block("a2", "Taraflar anlaşmıştır.", 1)];
  const revised = [
    block("b1", "Kira bedeli 118.000 TL olup KDV dahildir.", 0),
    block("b2", "Taraflar anlaşmıştır.", 1),
    block("b3", "Ek protokol imzalanmıştır.", 2),
  ];
  const comparison = compareDocuments(base, revised);

  it("BÜTÜN değişiklikleri Değişikİş biçiminde yazar — süzgeçten bağımsız", async () => {
    const model = buildComparisonViewModel(comparison);
    // Kullanıcı süzgeçte yalnız eklenenleri görüyor olsa da kopya tamdır.
    expect(filterChanges(model.changes, "added").length).toBeLessThan(comparison.changes.length);
    const written: string[] = [];
    const clipboard = { writeText: vi.fn(async (text: string) => void written.push(text)) };
    expect(await copyAllChanges(comparison.changes, clipboard)).toBe("done");
    const text = written[0];
    expect(text).toBe(copyChangesText(comparison.changes));
    expect(text.split("\n")).toHaveLength(comparison.changes.length);
    expect(text).toMatch(/^1\. /);
  });

  it("pano reddederse sessiz kalınmaz", async () => {
    const clipboard = { writeText: vi.fn(async () => Promise.reject(new Error("izin yok"))) };
    expect(await copyAllChanges(comparison.changes, clipboard)).toBe("failed");
  });

  it("düğme fark yokken kapalı; sonucu kendi etiketinde de söyler", () => {
    const none = renderToStaticMarkup(<CompareActions sync onSync={noop} canCopy={false} copyState="idle" onCopy={noop} />);
    expect(none).toMatch(/<button[^>]*disabled=""[^>]*>Değişiklikleri Kopyala<\/button>/);
    const done = renderToStaticMarkup(<CompareActions sync onSync={noop} canCopy copyState="done" onCopy={noop} />);
    expect(done).toContain(">Kopyalandı<");
  });
});

describe("belge metni seçilebilir (madde 33, Ctrl+C)", () => {
  it("kâğıt seçimi açar; satır numaraları seçilmez", () => {
    const pane = readFileSync(resolve(here, "DocumentPane.tsx"), "utf8");
    const css = readFileSync(resolve(here, "compare.css"), "utf8");
    const tokens = readFileSync(resolve(here, "../../shared-ui/tokens.css"), "utf8");
    expect(pane).toContain('<article className="paper selectable">');
    expect(tokens).toMatch(/\.selectable \{ user-select: text;/);
    expect(css).toMatch(/\.row-gutter \{[^}]*user-select: none;/);
  });
});
