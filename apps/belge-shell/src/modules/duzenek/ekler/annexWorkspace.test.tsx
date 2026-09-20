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
    expect(source).toContain('file_name: string; is_continuation: boolean');
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

describe("motora giden gövde gerçekten geçerli", () => {
  // ÇEKİŞMELİ İNCELEME BULGUSU (P1): ilk sürüm `stamp_config: null` ve
  // `target_size_bytes: 0` gönderiyordu. Birincisi serde tarafından komut
  // gövdesine girmeden, ikincisi motorun doğrulaması tarafından reddedilir;
  // yani "Ekleri Hazırla ve Kaydet" HER çalıştırmada düşerdi. Yukarıdaki
  // yüzey testi yalnız kaynak metnini aradığı için bunu kaçırmıştı.
  it("stamp_config null DEĞİL, tam bir yapı", () => {
    expect(source).not.toContain("stamp_config: null");
    expect(source).toContain("stamp_config: defaultStamp()");
    for (const field of ["enabled:", "position:", "font_size:", "margin_pt:", "show_badge:"])
      expect(source).toContain(field);
  });

  it("hedef boyut motorun varsayılanı, sıfır değil", () => {
    expect(source).not.toContain("target_size_bytes: 0");
    expect(source).toContain("target_size_bytes: DEFAULT_TARGET_SIZE_BYTES");
    expect(source).toContain("const DEFAULT_TARGET_SIZE_BYTES = 9_961_472");
  });
});

describe("çekişmeli incelemede bulunan çıkmazlar kapandı", () => {
  it("P1-3: aynı belge iki kez eklenemez", () => {
    // Kaynak kimliği İÇERİKTEN türetilir (`src-<sha256>`); yineleme motorun
    // doğrulamasını kalıcı olarak düşürüyordu ("Mükerrer kaynak kimliği").
    expect(source).toContain("const known = new Set(p.sources.map((x) => x.id))");
    expect(source).toContain("zaten listedeydi, tekrar eklenmedi");
  });

  it("P1-3: yanlış eklenen belge listeden çıkarılabilir", () => {
    // Eskiden tek çare kipten çıkıp baştan başlamaktı.
    expect(source).toContain("belgesini listeden çıkar");
    expect(source).toContain("sources: p.sources.filter((x) => x.id !== source.id)");
  });

  it("P1-4: imzalı belge çıkmaz değil", () => {
    expect(source).toContain('signed_policy: derived ? "create_derived_copy" : "use_original_as_is"');
    expect(source).toContain("TEK BAŞINA bir ek olmalıdır");
    expect(source).toContain("doğrulanabilirliğini");
  });

  it("P1-5: bölünme ve doğrulama sonucu söylenir", () => {
    // Motor boyut sınırını aşan eki kendisi böler; bu, mahkemeye giden dosya
    // kümesini değiştirir ve sessizce olmamalı.
    expect(source).toContain("o.is_continuation");
    expect(source).toContain("boyut sınırı nedeniyle bölündü");
    expect(source).toContain("validation_report");
    expect(source).toContain("Gözden geçirin:");
  });
});
