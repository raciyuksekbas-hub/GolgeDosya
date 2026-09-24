// Dilekçe Ekleri yüzeyi — sözleşme testleri (§14–20, §66, §73).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AnnexWorkspace, emptyProject } from "./AnnexWorkspace";
import { assignSource, createExhibit } from "./annexState";
import type { AnnexSource, Project } from "./types";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(here, "AnnexWorkspace.tsx"), "utf8");
const noop = () => undefined;
const render = (project: Project) =>
  renderToStaticMarkup(
    <AnnexWorkspace project={project} onProject={noop} prepared={null} onPrepared={noop} onNewOperation={noop} />,
  );
const doc = (id: string, name: string): AnnexSource => ({
  id, path: `C:\\Users\\Çağrı Şahin\\Belgeler\\${name}`, file_name: name, page_count: 2,
  is_signed: false, signed_policy: "create_derived_copy", is_approved_for_conversion: true,
});
const withDoc: Project = assignSource(
  createExhibit({ ...emptyProject(), sources: [doc("src-a", "İş Sözleşmesi.pdf")] }, "", "ek-a"),
  "src-a",
  "ek-a",
);
const empty = render(emptyProject());
const html = render(withDoc);
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
    expect(modes).toContain('emptyTitle: "Dilekçe ekleri, tek pakette"');
  });
});

describe("§14/§15 — iki yüzey", () => {
  it("Yüklenen Belgeler ve Dilekçe Ekleri başlıkları çizilir", () => {
    expect(html).toContain("Yüklenen Belgeler");
    expect(html).toContain("Dilekçe Ekleri");
  });

  it("boş durum kipin görevini söyler ve ilk adımı birincil eylem yapar (madde 6)", () => {
    // Başlık ve açıklama tanımlıydı ama HİÇ çizilmiyordu.
    // Ortak EmptyState: bir başlık, bir cümle, bir eylem (§49).
    expect(empty).toMatch(/<h1 class="empty-title">Dilekçe ekleri, tek pakette<\/h1>/);
    expect(empty).toContain("Belgeleri yükleyin, Ek-1, Ek-2… gruplarına ayırın ve düzenli bir çıktı klasörü hazırlayın.");
    // İlk birincil düğme ETKİN ve "Belge Ekle"dir; eskiden tek birincil
    // düğme devre dışı "Ekleri Hazırla ve Kaydet" idi.
    const primary = empty.match(/<button[^>]*class="btn btn-primary"[^>]*>([^<]*)<\/button>/);
    expect(primary?.[1]).toBe("Belge Ekle");
    expect(primary?.[0]).not.toContain("disabled");
    expect(empty).not.toContain("Ekleri Hazırla ve Kaydet");
  });

  it("belge eklenince iki yüzey gelir; ek yokken sağ yüzey ne yapılacağını söyler", () => {
    expect(html).not.toContain("empty-title");
    const docOnly = render({ ...emptyProject(), sources: [doc("src-a", "a.pdf")] });
    expect(docOnly).toContain("Ek-2 — Banka Dekontları");
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
    // Motor çağrısı taşınan komutla aynı; yalnız ana iş parçacığı dışında.
    expect(source).toContain('invoke<PreparedPackage>("ekler_prepare_package"');
    expect(html).toContain("kaynak belgeleriniz korunur");
  });

  it("boş ek varken hazırlama kapalı ve sebebi yazılı", () => {
    // Motor boş eki reddeder; düğme açıkken kullanıcı klasör seçtikten SONRA
    // hata alıyordu.
    const withEmpty = render(createExhibit(withDoc, "", "ek-b"));
    expect(withEmpty).toContain("Ek-2 boş: her eke en az bir belge atayın ya da boş eki silin.");
    expect(withEmpty).toMatch(/<button[^>]*disabled=""[^>]*>Ekleri Hazırla ve Kaydet<\/button>/);
    expect(html).not.toMatch(/<button[^>]*disabled=""[^>]*>Ekleri Hazırla ve Kaydet<\/button>/);
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
    // Maddenin durumu `level`dedir; `passed` diye bir alan yoktur.
    expect(source).toContain('i.level !== "pass"');
    expect(source).not.toContain("!i.passed");
    expect(source).toContain("Gözden geçirin:");
  });
});

describe("oturum kip değişince kaybolmaz", () => {
  // PAKETLENMİŞ SMOKE BULGUSU: durum bileşenin içindeyken başka bir kipe
  // geçmek bileşeni söküyor ve kullanıcının kurduğu bütün ek yapısı
  // SESSİZCE siliniyordu. Yirmi ekli bir dosya hazırlayan avukat
  // yanlışlıkla "Düzenle"ye tıkladığında hiçbir uyarı almadan her şeyi
  // kaybediyordu.
  const app = readFileSync(resolve(here, "../../../App.tsx"), "utf8");

  it("çalışma alanı durumu dışarıdan alır", () => {
    expect(source).toContain("export function AnnexWorkspace({ project, onProject, prepared, onPrepared, onNewOperation }");
    expect(source).not.toContain("const [project, setProject] = useState<Project>(emptyProject)");
  });

  it("oturum kabukta yaşar", () => {
    expect(app).toContain("const [annexProject, setAnnexProject] = useState<AnnexProject>(emptyProject)");
    expect(app).toContain("project={annexProject}");
    expect(app).toContain("onProject={setAnnexProject}");
    // Sonuç da kabukta: kip değiştirip dönünce "tamamlandı" yüzeyi yerinde.
    expect(app).toContain("prepared={annexPrepared}");
  });
});
