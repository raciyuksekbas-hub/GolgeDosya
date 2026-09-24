// Kaybettiren eylem onayı (§54): erişilebilir ad, açıklama, güvenli varsayılan.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ConfirmSheet } from "./ConfirmSheet";

describe("ConfirmSheet", () => {
  const html = renderToStaticMarkup(
    <ConfirmSheet title="Mevcut çalışma temizlenecek" confirmLabel="Temizle" onConfirm={() => {}} onCancel={() => {}}>
      Devam edilsin mi?
    </ConfirmSheet>,
  );

  it("ekran okuyucuya uyarı penceresi olarak adıyla ve açıklamasıyla gelir", () => {
    expect(html).toContain('role="alertdialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain('aria-labelledby="onay-baslik"');
    expect(html).toContain('aria-describedby="onay-metin"');
    expect(html).toContain('<h2 id="onay-baslik">Mevcut çalışma temizlenecek</h2>');
  });

  it("Vazgeç önce gelir ve odak onda başlar; Escape vazgeçer", () => {
    expect(html.indexOf(">Vazgeç<")).toBeLessThan(html.indexOf(">Temizle<"));
    const here = dirname(fileURLToPath(import.meta.url));
    const src = readFileSync(resolve(here, "ConfirmSheet.tsx"), "utf8");
    expect(src).toContain("useFocusTrap(sheet, true, onCancel)");
    expect(src).toContain("cancel.current?.focus()");
  });
});
