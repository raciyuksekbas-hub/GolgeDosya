// Dilekçe Ekleri yüzeyi — sözleşme testleri (§14–20, §66, §73).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AnnexWorkspace } from "./AnnexWorkspace";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(here, "AnnexWorkspace.tsx"), "utf8");
const html = renderToStaticMarkup(<AnnexWorkspace />);
const modes = readFileSync(resolve(here, "../../../shell/modes.ts"), "utf8");
const matrix = readFileSync(
  resolve(here, "../../../../src-tauri/src/features.rs"),
  "utf8",
);

describe("§11/§73 — ek yönetimi gerçekten var", () => {
  it("özellik matrisinde AYRI bir yetenek olarak duruyor", () => {
    // "PDF araçlarının taşınmış olması DüzenEk'in taşındığı anlamına gelmez."
    expect(matrix).toContain('("ekler", "Ekler", "/ekler")');
    expect(matrix).toContain('"ekler" => cfg!(feature = "feature_duzenek")');
  });

  it("kabukta kendi kipi var", () => {
    expect(modes).toContain('label: "Ekler"');
    expect(modes).toContain('emptyTitle: "Dilekçe ekleri"');
  });
});

describe("§14/§15 — iki yüzey", () => {
  it("Yüklenen Belgeler ve Dilekçe Ekleri başlıkları çizilir", () => {
    expect(html).toContain("Yüklenen Belgeler");
    expect(html).toContain("Dilekçe Ekleri");
  });

  it("boş durum ne yapılacağını söyler", () => {
    expect(html).toContain("Ek olarak kullanacağınız belgeleri ekleyin");
    expect(html).toContain("Ek-2 — Banka Dekontları");
  });
});

describe("§16/§66 — atama tek yola bağlı değil", () => {
  it("sürükle-bırak vardır", () => {
    expect(source).toContain("draggable");
    expect(source).toContain("onDrop=");
  });

  it("ama klavye/ekran okuyucu yolu da vardır", () => {
    // Drag/drop TEK YOL OLMAMALI.
    expect(source).toContain("<select");
    expect(source).toContain("hangi eke atansın");
  });

  it("atama değişikliği duyurulur", () => {
    expect(source).toContain("eke atandı.");
  });
});

describe("§17/§19/§20 — eylemler", () => {
  it("Otomatik Dağıt kapsamını söyler", () => {
    expect(html).toContain("Otomatik Dağıt");
    expect(source).toContain("elle yaptığınız atamalara dokunmaz");
  });

  it("Ekler Listesini Kopyala vardır", () => {
    expect(html).toContain("Ekler Listesini Kopyala");
    expect(source).toContain("navigator.clipboard.writeText(text)");
  });

  it("pano reddedilirse sessiz kalınmaz", () => {
    expect(source).toContain("Pano kullanılamadı");
  });

  it("nihai iş motora gider ve kaynak korunur", () => {
    expect(html).toContain("Ekleri Hazırla ve Kaydet");
    expect(source).toContain('invoke<{ outputs: { file_name: string }[]');
    expect(source).toContain('"duzenek_prepare_uyap"');
    expect(html).toContain("kaynak belgeleriniz korunur");
  });

  it("hiç ek yokken hazırlama kapalı", () => {
    expect(source).toContain("const canPrepare = project.exhibits.some((e) => e.sources.length > 0)");
  });
});

describe("sessiz hata yok", () => {
  it("tarama, hazırlama ve kopyalama korunur", () => {
    expect((source.match(/catch \(e\)/g) ?? []).length).toBeGreaterThanOrEqual(3);
    expect((source.match(/logFailure\(/g) ?? []).length).toBeGreaterThanOrEqual(3);
  });

  it("kısmi tarama sağlamları atmaz", () => {
    expect(source).toContain("dosya okunamadı");
    expect(source).toContain("if (!result.sources.length) throw");
  });
});
