// Dönüştür'ün sonuç yüzeyi: "Klasörde Göster" neyi gösterir, "Yeni Dönüştürme"
// neyi bırakır.
//
// Saha (Windows): madde 13 — "Klasörde Göster" bir ÜST klasörü açıyordu; madde
// 16 — "Yeniden Dönüştür" aynı belgeyi yeniden dönüştürüp sessizce "X (2).udf"
// üretiyordu, yeni bir işleme başlamanın yolu yoktu.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { isValidElement, type ReactElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { ConvertDone, revealTargets } from "./ConvertWorkspace";
import type { BatchResult, ConversionResult, OutputFolder } from "./types";

const DIR = "C:\\Users\\Çağrı Şahin\\Documents\\GölgeDosya\\Dönüştürülen Belgeler";
const result = (name: string, output: string | null, status = "success") =>
  ({
    status,
    source: `C:\\Users\\Çağrı Şahin\\Masaüstü\\${name}.docx`,
    source_name: `${name}.docx`,
    output,
    output_name: output ? `${name}.udf` : null,
    warnings: [],
    error: null,
    source_unchanged: true,
  }) as unknown as ConversionResult;

const batch = (items: ConversionResult[], zip: string | null): BatchResult =>
  ({ items, is_zip: zip !== null, output: zip, output_name: zip ? "arşiv.zip" : null }) as unknown as BatchResult;

describe("Klasörde Göster neyi seçer (madde 13)", () => {
  it("tek belge: üretilen dosyanın KENDİSİ; klasörü değil", () => {
    const out = `${DIR}\\Müvekkil'in dilekçesi.udf`;
    expect(revealTargets(result("Müvekkil'in dilekçesi", out))).toEqual([out]);
  });

  it("toplu iş arşiv yazdıysa arşiv; tekil çıktılar motor tarafından silinmiştir", () => {
    const zip = `${DIR}\\arşiv.zip`;
    expect(revealTargets(batch([result("a", `${DIR}\\a.udf`)], zip))).toEqual([zip]);
  });

  it("arşivsiz toplu iş: yalnız ÜRETİLEN çıktılar; başarısız öğe atlanır", () => {
    const items = [result("a", `${DIR}\\a.udf`), result("b", null, "failure"), result("c", `${DIR}\\c.udf`)];
    expect(revealTargets(batch(items, null))).toEqual([`${DIR}\\a.udf`, `${DIR}\\c.udf`]);
  });

  it("çıktı yoksa boş: arka uç çıktı klasörünün kendisini açar", () => {
    expect(revealTargets(null)).toEqual([]);
    expect(revealTargets(batch([result("b", null, "failure")], null))).toEqual([]);
  });
});

/** Bileşenin döndürdüğü öğe ağacında, metni tam olarak `text` olan düğmeyi bulur. */
function findButton(node: ReactNode, text: string): ReactElement<{ onClick: () => void }> | null {
  if (Array.isArray(node)) {
    for (const child of node) {
      const hit = findButton(child, text);
      if (hit) return hit;
    }
    return null;
  }
  if (!isValidElement(node)) return null;
  const props = node.props as { children?: ReactNode; onClick?: () => void };
  if (props.onClick && props.children === text) return node as ReactElement<{ onClick: () => void }>;
  return findButton(props.children, text);
}

describe("Yeni Dönüştürme (madde 16)", () => {
  const folder = { path: DIR, default_path: DIR, is_default: true } as OutputFolder;

  it("düğme yeni işlemi başlatır; aynı belgeyi yeniden dönüştürmez", () => {
    const onNew = vi.fn();
    const onReveal = vi.fn();
    const tree = ConvertDone({
      items: [result("dilekçe", `${DIR}\\dilekçe.udf`)],
      from: "Word (.docx)",
      to: "UYAP (.udf)",
      folder,
      onReveal,
      onNew,
    });
    expect(findButton(tree, "Yeniden Dönüştür")).toBeNull();
    const button = findButton(tree, "Yeni Dönüştürme");
    expect(button).not.toBeNull();
    button!.props.onClick();
    expect(onNew).toHaveBeenCalledTimes(1);
    expect(onReveal).not.toHaveBeenCalled();
  });

  it("kabuk belgeyi bırakır; tercihlere (çıktı klasörü, onay) dokunulmaz", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const app = readFileSync(resolve(here, "../../App.tsx"), "utf8");
    expect(app).toContain("<ConvertWorkspace paths={carried} onNewConversion={closeDocuments} />");
    // closeDocuments yalnız belge ve bağlamı bırakır.
    const at = app.indexOf("const closeDocuments");
    const body = app.slice(at, app.indexOf("}, []);", at));
    expect(body).toContain("setDocuments([])");
    expect(body).not.toMatch(/setOutputFolder|acceptTerms|forgetDocuments|saveSettings/);
  });
});
