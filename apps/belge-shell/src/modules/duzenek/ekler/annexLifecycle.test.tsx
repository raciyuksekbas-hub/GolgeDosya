// Ekler yaşam döngüsü (§53) — saha maddeleri 19, 10, 9.
//
// Saha: "İşlemden sonra çıkış yapamıyorum. Yeni bir Ekler işlemine
// başlayamıyorum." Hazırlamadan sonra ekran hazırlamadan öncekinin aynısıydı;
// klasörü açmanın ve baştan başlamanın yolu yoktu.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { isValidElement, type ReactElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { AnnexDone, AnnexWorkspace, emptyProject } from "./AnnexWorkspace";
import { annexPhase, assignSource, createExhibit, discardsWork, renameExhibit } from "./annexState";
import type { AnnexSource, PreparedAnnex, PreparedPackage, Project } from "./types";

const here = dirname(fileURLToPath(import.meta.url));
const noop = () => undefined;
const doc = (id: string, name: string): AnnexSource => ({
  id, path: `C:\\Users\\Çağrı Şahin\\Belgeler\\${name}`, file_name: name, page_count: 2,
  is_signed: false, signed_policy: "create_derived_copy", is_approved_for_conversion: true,
});
const withDoc = (): Project =>
  assignSource(createExhibit({ ...emptyProject(), sources: [doc("src-a", "İş Sözleşmesi.pdf")] }, "", "ek-a"), "src-a", "ek-a");

// Motorun gerçek bir çalıştırmasından (denetim harness'ı) alınan biçim.
const PACKAGE = "C:\\Users\\Çağrı Şahin\\Documents\\GölgeDosya\\GolgeDosya-Dilekce_Ekleri-fCTl67";
const result: PreparedPackage = {
  outputs: [
    { file_name: "EK-01_Is_Sozlesmesi_S001-S002.pdf", is_continuation: false },
    { file_name: "EK-02_Banka_Dekontu_S001-S001.pdf", is_continuation: false },
    { file_name: "EK-02_Banka_Dekontu_DEVAM_1_S002-S002.pdf", is_continuation: true },
  ],
  exhibits_list_plain: "EKLER\n\nEk-1: İş Sözleşmesi — 2 sayfa\n",
  package_dir: PACKAGE,
  validation_report: { is_ready_for_uyap: true, items: [{ title: "Boyut", level: "pass" }] },
};

describe("yaşam döngüsü: empty → working → ready → completed", () => {
  it("gerçek dönüşümlerle geçişler", () => {
    const empty = emptyProject();
    expect(annexPhase(empty, null)).toBe("empty");
    const docOnly = { ...empty, sources: [doc("src-a", "a.pdf")] };
    expect(annexPhase(docOnly, null)).toBe("working");
    const ready = withDoc();
    expect(annexPhase(ready, null)).toBe("ready");
    expect(annexPhase(createExhibit(ready, "", "ek-b"), null)).toBe("working"); // boş ek
    const prepared: PreparedAnnex = { snapshot: ready, result };
    expect(annexPhase(ready, prepared)).toBe("completed");
    // Hazırlandıktan sonra değişen düzen artık "tamamlandı" değildir.
    expect(annexPhase(renameExhibit(ready, "ek-a", "Sözleşme"), prepared)).toBe("ready");
  });

  it("yeni işlem yalnız kaydedilmemiş düzen varken sorar", () => {
    const ready = withDoc();
    const prepared: PreparedAnnex = { snapshot: ready, result };
    expect(discardsWork(emptyProject(), null)).toBe(false);
    expect(discardsWork(ready, prepared)).toBe(false); // paket diskte
    expect(discardsWork(ready, null)).toBe(true);
    expect(discardsWork(renameExhibit(ready, "ek-a", "x"), prepared)).toBe(true);
  });
});

/** Öğe ağacında metni tam `text` olan tıklanabilir düğme. */
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

describe("tamamlandı yüzeyi (maddeler 10, 9)", () => {
  const html = renderToStaticMarkup(<AnnexDone result={result} stale={false} onOpenFolder={noop} onNew={noop} />);

  it("PDF'leri PDF diye sayar, iki yardımcı dosyayı tek satırla açıklar", () => {
    expect(html).toContain("3 ek PDF&#x27;i hazırlandı ve kaydedildi");
    expect(html).not.toMatch(/\d+ dosya üretildi/);
    expect(html).toContain("EKLER_LISTESI.txt");
    expect(html).toContain("dilekçenin EKLER bölümüne yapıştıracağınız liste");
    expect(html).toContain("manifest.json");
    expect(html).toContain("açmanız gerekmez, UYAP&#x27;a yüklemeyin");
    expect(html).not.toMatch(/düzenleyin|elle düzenle/i);
  });

  it("bölünme söylenir; klasör adı gösterilir, kullanıcı adıyla tam yol değil", () => {
    expect(html).toContain("1 ek boyut sınırı nedeniyle bölündü");
    expect(html).toContain(">GolgeDosya-Dilekce_Ekleri-fCTl67<");
    expect(html).not.toContain(">C:\\Users");
  });

  it("Klasörü Aç ve Yeni Ekler İşlemi gerçekten bağlı", () => {
    const onOpenFolder = vi.fn();
    const onNew = vi.fn();
    const tree = AnnexDone({ result, stale: false, onOpenFolder, onNew });
    findButton(tree, "Klasörü Aç")!.props.onClick();
    expect(onOpenFolder).toHaveBeenCalledTimes(1);
    findButton(tree, "Yeni Ekler İşlemi")!.props.onClick();
    expect(onNew).toHaveBeenCalledTimes(1);
  });

  it("çalışma alanı tamamlanınca sonucu gösterir; aynı paketi yeniden hazırlatmaz", () => {
    const ready = withDoc();
    const done = renderToStaticMarkup(
      <AnnexWorkspace project={ready} onProject={noop} prepared={{ snapshot: ready, result }} onPrepared={noop} onNewOperation={noop} />,
    );
    expect(done).toContain("Klasörü Aç");
    expect(done).toContain("Yeni Ekler İşlemi");
    expect(done).not.toContain("Ekleri Hazırla ve Kaydet");
    // Düzen değişince paket bayat sayılır ve hazırlama geri gelir.
    const edited = renameExhibit(ready, "ek-a", "Sözleşme");
    const stale = renderToStaticMarkup(
      <AnnexWorkspace project={edited} onProject={noop} prepared={{ snapshot: ready, result }} onPrepared={noop} onNewOperation={noop} />,
    );
    expect(stale).toContain("bu değişiklikler pakette yok");
    expect(stale).toContain("Ekleri Hazırla ve Kaydet");
  });
});

describe("kabuk sözleşmesi", () => {
  const app = readFileSync(resolve(here, "../../../App.tsx"), "utf8");
  const annex = readFileSync(resolve(here, "AnnexWorkspace.tsx"), "utf8");

  it("yeni işlem düzeni ve sonucu bırakır; tercihlere dokunmaz", () => {
    const at = app.indexOf("const newAnnexOperation");
    const body = app.slice(at, app.indexOf("}, []);", at));
    expect(body).toContain("setAnnexProject(emptyProject())");
    expect(body).toContain("setAnnexPrepared(null)");
    expect(body).not.toMatch(/saveSettings|setSettings|forgetDocuments|setDocuments/);
  });

  it("Ekler'de kabuğun 'Kapat'ı yok: başka kipin belgesini kapatıyordu", () => {
    expect(app).toContain('documents.length > 0 && active?.key !== "ekler"');
  });

  it("klasör, üst klasörü açan reveal ile değil kendi komutuyla açılır", () => {
    expect(annex).toContain('invoke("ekler_open_package_folder", { path: prepared.result.package_dir })');
    expect(annex).not.toMatch(/reveal/i);
  });
});
