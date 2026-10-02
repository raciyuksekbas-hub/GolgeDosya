// Kaydın yeri — her araçta aynı sonuç sözleşmesi.
//
// Saha: "Kayıt klasörleri arasında birlik yok." Dönüştür kayıttan sonra
// "Klasörde Göster" veriyordu; Ekler "Klasörü Aç" diyordu; Düzenle yalnız tam
// yolu durum satırına yazıyordu; Denetle yalnız dosya adını söylüyor, klasörü
// hiç göstermiyordu. Artık Düzenle ve Denetle de klasörün adını gösterir, tam
// yol ipucundadır ve tek eylem "Klasörde Göster"dir.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import postcss from "postcss";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

const core = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => core);

import {
  dialogPath,
  folderOf,
  inFolder,
  SavedPlace,
  showInFolder,
  SHOW_IN_FOLDER,
  START_FOLDER_WAIT_MS,
  startFolder,
} from "./savePlace";

const here = dirname(fileURLToPath(import.meta.url));
const read = (p: string) => readFileSync(resolve(here, p), "utf8");

describe("pencerenin önerdiği yol", () => {
  it("klasör + ad, klasörün kendi ayracıyla", () => {
    expect(dialogPath("/Users/avukat/Dava", "a.pdf")).toBe("/Users/avukat/Dava/a.pdf");
    expect(dialogPath("C:\\Users\\Çağrı Şahin\\Dava", "a.pdf")).toBe("C:\\Users\\Çağrı Şahin\\Dava\\a.pdf");
    expect(inFolder("/", "a.pdf")).toBe("/a.pdf");
    expect(inFolder("D:\\", "a.pdf")).toBe("D:\\a.pdf");
  });

  it("öneri yoksa yalnız ad: klasörü işletim sistemi seçer", () => {
    expect(dialogPath(null, "a.pdf")).toBe("a.pdf");
  });

  it("klasör kısmı her iki ayraçla", () => {
    expect(folderOf("/Users/avukat/Dava/a.pdf")).toBe("/Users/avukat/Dava");
    expect(folderOf("C:\\Users\\Çağrı Şahin\\Dava\\a.pdf")).toBe("C:\\Users\\Çağrı Şahin\\Dava");
    expect(folderOf("/a.pdf")).toBe("/");
  });
});

describe("pencerenin başlangıç klasörü", () => {
  afterEach(() => {
    core.invoke.mockReset();
    vi.useRealTimers();
  });

  it("kararı arka uç verir: kaynağın klasörü ya da öneri yok", async () => {
    core.invoke.mockResolvedValueOnce("/Users/avukat/Dava");
    expect(await startFolder("/Users/avukat/Dava/a.pdf")).toBe("/Users/avukat/Dava");
    expect(core.invoke).toHaveBeenCalledWith("output_start_folder", { source: "/Users/avukat/Dava/a.pdf" });
    // Geçici klasör, arşivin içi, yazılamayan klasör: işletim sisteminin varsayılanı.
    core.invoke.mockResolvedValueOnce(null);
    expect(await startFolder("/private/var/folders/x/T/ek.pdf")).toBeNull();
  });

  it("kaynak yoksa sorulmaz", async () => {
    expect(await startFolder(undefined)).toBeNull();
    expect(core.invoke).not.toHaveBeenCalled();
  });

  it("öneri alınamazsa ya da gecikirse pencere yine açılır", async () => {
    core.invoke.mockRejectedValueOnce(new Error("komut yok"));
    expect(await startFolder("/Users/avukat/Dava/a.pdf")).toBeNull();
    // Erişilemeyen ağ paylaşımı: yoklama bitmiyor.
    vi.useFakeTimers();
    core.invoke.mockReturnValueOnce(new Promise(() => undefined));
    const asked = startFolder("//nas/Dava/a.pdf");
    await vi.advanceTimersByTimeAsync(START_FOLDER_WAIT_MS);
    expect(await asked).toBeNull();
  });
});

describe("sonuç yüzeyi: klasörün adı ve Klasörde Göster", () => {
  const FILE = "C:\\Users\\Çağrı Şahin\\Dava Dosyaları\\GolgeDosya-compress-2026-10-02_013031.pdf";

  it("dosya: klasörün adı görünür, tam yol ipucunda", () => {
    const html = renderToStaticMarkup(<SavedPlace path={FILE} />);
    expect(html).toContain(">Klasör<");
    expect(html).toContain('title="C:\\Users\\Çağrı Şahin\\Dava Dosyaları">Dava Dosyaları<');
    expect(html).toContain(">Klasörde Göster<");
    // Tam yol görünen metin olarak yazılmaz.
    expect(html).not.toContain(">C:\\Users");
  });

  it("paket klasörü: kendi adı gösterilir", () => {
    const pkg = "/Users/avukat/Dava/GolgeDosya-Gorseller-jyT24K";
    const html = renderToStaticMarkup(<SavedPlace path={pkg} isFolder />);
    expect(html).toContain(`title="${pkg}">GolgeDosya-Gorseller-jyT24K<`);
  });

  it("eylem az önce kaydedilen yolu verir; açılamazsa sessiz kalmaz", async () => {
    const ok = vi.fn().mockResolvedValue(undefined);
    expect(await showInFolder(FILE, ok)).toBe("");
    expect(ok).toHaveBeenCalledWith(FILE);
    const gone = vi.fn().mockRejectedValue("Dosya bulunamadı. Taşınmış veya silinmiş olabilir.");
    expect(await showInFolder(FILE, gone)).toBe("Dosya bulunamadı. Taşınmış veya silinmiş olabilir.");
    // Teknik metin kullanıcıya gitmez.
    const raw = vi.fn().mockRejectedValue(new Error("os error 2: /Users/x"));
    expect(await showInFolder(FILE, raw)).toBe("Klasör açılamadı. Dosya taşınmış veya silinmiş olabilir.");
    const src = read("./savePlace.tsx");
    expect(src).toContain("setError(await showInFolder(path, onReveal))");
    expect(SHOW_IN_FOLDER).toBe("Klasörde Göster");
  });

  it("Düzenle'de satır durum metniyle aynı hizada durur", () => {
    // Düzenle'nin durumu kendi kenar boşluğuyla çizilir; ortak kural orada
    // satırı paneldeki durumun soluna ve üstüne kaydırıyordu.
    const css = postcss.parse(read("../modules/duzenek/pdf.css"));
    const margin = (selector: string) => {
      let value: string | undefined;
      css.walkRules((rule) => {
        if (rule.selectors.includes(selector)) rule.walkDecls("margin", (d) => void (value = d.value));
      });
      return value?.split(/\s+/) ?? [];
    };
    const status = margin(".pdf-root .status");
    const place = margin(".pdf-root .saved-place");
    expect(place[1]).toBe(status[1]);
    expect(place[0]).not.toMatch(/^calc\(-/);
  });

  it("platforma özgü dosya yöneticisi adı geçmez", () => {
    const src = read("./savePlace.tsx");
    expect(src).not.toMatch(/Finder|Explorer|Gezgin/);
  });
});

