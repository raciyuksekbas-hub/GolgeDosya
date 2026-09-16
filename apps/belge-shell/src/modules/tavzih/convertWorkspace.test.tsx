// Dönüştür — durum sözleşmesi.
//
// Motor çağrılmaz; `InspectOutcome` ve `ConversionResult` sözleşmesinin şekli
// sentetik bir belgeyle kurulur. Burada kilitlenen şey görünüş değil, hangi
// yüzeyin hangi durumda VAR OLDUĞUDUR.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ConvertDone, ConvertFlow, formatParts } from "./ConvertWorkspace";
import type { ConversionResult, InspectOutcome, OutputFolder } from "./types";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(here, "ConvertWorkspace.tsx"), "utf8");

const FOLDER: OutputFolder = {
  path: "/Users/ornek/Belgeler/Dönüştürülen Belgeler",
  is_default: true,
} as OutputFolder;

const SELECTED = [
  {
    path: "/Users/ornek/Belgeler/ornek-dilekce.docx",
    info: {
      name: "ornek-dilekce.docx",
      source_format: "Word (.docx)",
      size_label: "2,6 KB",
      target_format: "UYAP (.udf)",
    },
  },
] as unknown as InspectOutcome[];

const RESULT = [
  {
    source: "/Users/ornek/Belgeler/ornek-dilekce.docx",
    source_name: "ornek-dilekce.docx",
    output_name: "ornek-dilekce.udf",
    status: "success",
    source_unchanged: true,
    warnings: [],
  },
] as unknown as ConversionResult[];

const noop = () => undefined;

const flow = renderToStaticMarkup(
  <ConvertFlow
    selected={SELECTED}
    target="UYAP (.udf)"
    folder={FOLDER}
    onConvert={noop}
    onChooseFolder={noop}
    onResetFolder={noop}
  />,
);
const done = renderToStaticMarkup(
  <ConvertDone
    items={RESULT}
    from="Word (.docx)"
    to="UYAP (.udf)"
    folder={FOLDER}
    onReveal={noop}
    onAgain={noop}
  />,
);

describe("durum sözleşmesi", () => {
  it("hazır: iki durak, bir ok, tek eylem", () => {
    // Akış iki durağını ADIYLA söyler; kullanıcı neyin neye dönüşeceğini
    // ekrandaki yerleşimden okur, bir cümleyi çözmek zorunda kalmaz.
    expect(flow).toContain("Kaynak");
    expect(flow).toContain("Hedef");
    expect(flow).toContain("ornek-dilekce.docx");
    expect(flow).toContain("Word (.docx)");
    expect(flow).toContain("→");
    // Hedef ad ve uzantı olarak iki satıra ayrılır.
    expect(flow).toContain("UYAP");
    expect(flow).toContain(".udf");
    // Çıktı klasörü: adı görünür, tam yol ipucunda. Monospace bir yol bloğu değil.
    expect(flow).toContain("Çıktı");
    expect(flow).toMatch(/title="[^"]*Dönüştürülen Belgeler"[^>]*>Dönüştürülen Belgeler</);
    expect(flow).not.toContain("folder-path");
    expect(flow).toContain("Değiştir");
    // Varsayılan klasörde "Varsayılana dön" anlamsız.
    expect(flow).not.toContain("Varsayılana dön");
  });

  it("birincil eylem akışın içinde ve tek", () => {
    // İş burada yapılıyor; düğme de burada. Yardımcı barda ikinci bir kopyası
    // olursa ekranda iki baskın eylem olur.
    expect(flow).toContain("Dönüştür");
    expect((flow.match(/class="btn btn-primary"/g) ?? []).length).toBe(1);
    expect(source).not.toContain("ToolbarActions");
  });

  it("tamamlandı: akış kaybolmaz, hedefi dosyaya döner", () => {
    // "Az önce ne dönüştürdüm?" sorusunun cevabı ekranda kalmalı: iki durak
    // aynı yerde durur, hedef artık biçim adı değil üretilen dosyadır.
    expect(done).toContain("flow-pair");
    expect(done).toContain("Kaynak");
    expect(done).toContain("Hedef");
    expect(done).toContain("ornek-dilekce.docx");
    expect(done).toContain("ornek-dilekce.udf");
    expect(done).toContain("Word (.docx)");
    expect(done).toContain("UYAP (.udf)");
    expect(done).toContain("Dönüştürme tamamlandı");
    expect(done).toContain("Kaynak belge değiştirilmedi.");
    // Çıktının yeri de görünür kalır.
    expect(done).toContain("Çıktı");
    expect(done).toContain("Dönüştürülen Belgeler");
    // Tek baskın eylem; ikincisi sessiz.
    expect(done).toMatch(/Finder(&#x27;|')da Göster/);
    expect((done.match(/class="btn btn-primary"/g) ?? []).length).toBe(1);
    expect(done).toContain("Yeniden Dönüştür");
  });

  it("bar yönü taşır, düğmeyi değil", () => {
    expect(source).toContain("ToolbarStatus");
    expect(source).toMatch(/\{usable\[0\]\.info\?\.source_format\} → \{target \?\? "—"\}/);
  });

  it("kip kendi sağ panelini açmaz", () => {
    // Bir klasör adı üçüncü bir kolonu hak etmiyordu.
    expect(source).not.toContain("InspectorPanel");
    expect(source).not.toContain("InspectorSection");
  });

  it("durumlar birbirini dışlar", () => {
    // Pencere, dalın İÇİNDEKİ kodun uzunluğuna değil dalların SIRASINA
    // bakar. `onReveal`'in sessiz hata yolu kapatılınca (yüzen söz ->
    // try/catch) gövde uzadı ve 400 karakterlik pencere sözleşme hiç
    // değişmediği hâlde kırıldı.
    expect(source).toMatch(
      /\{done \? \(\s*<ConvertDone[\s\S]{0,1200}?\) : selected\.length > 0 \? \(\s*<ConvertFlow/,
    );
  });

  it("biçim etiketi ad ve uzantı olarak ayrılır, uydurulmaz", () => {
    expect(formatParts("Word (.docx)")).toEqual({ name: "Word", ext: ".docx" });
    expect(formatParts("UYAP (.udf)")).toEqual({ name: "UYAP", ext: ".udf" });
    // Parantez yoksa etiket olduğu gibi kalır.
    expect(formatParts("PDF")).toEqual({ name: "PDF", ext: "" });
    expect(formatParts(null)).toEqual({ name: "—", ext: "" });
  });
});
