// Ayarlar > Genel'deki klasör satırı kimin?
//
// Saha: "Kayıt klasörleri arasında birlik yok." Satır yalnız "Çıktı klasörü"
// diyordu ve bütün uygulamanın tercihi gibi duruyordu; onu yalnız Dönüştür
// okur (`tavzih.rs` effective_output_dir). Kullanıcı klasörü değiştirip
// Düzenle'de kaydedince dosya başka yere gidiyordu. Satır artık kapsamını
// söyler; tercihin anlamı ve komutları değişmedi.
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { OutputFolderRow, PetitionSpacingRow } from "./OutputFolderField";

const noop = () => undefined;
const render = (path: string, isDefault: boolean) =>
  renderToStaticMarkup(
    <OutputFolderRow folder={{ path, default_path: "/Users/avukat/Documents/GölgeDosya/Dönüştürülen Belgeler", is_default: isDefault }} onChoose={noop} onReset={noop} />,
  );

describe("Dönüştür'ün çıktı klasörü satırı", () => {
  it("etiket neyin buraya yazıldığını, ipucu öteki araçların ne yaptığını söyler", () => {
    const html = render("/Users/avukat/Documents/GölgeDosya/Dönüştürülen Belgeler", true).replace(/&#x27;/gu, "'");
    expect(html).toContain("Dönüştürülen belgeler");
    expect(html).toContain("Yalnız Dönüştür buraya kaydeder.");
    // Araçlar adıyla sayılır. "Öteki araçlar belgenin klasörünü önerir"
    // Karşılaştır için yanlıştı: rapor pencere açılmadan İndirilenler'e
    // yazılır (`degisikis.rs` report_directory). Kullanıcı raporu
    // karşılaştırdığı belgelerin yanında arıyordu.
    expect(html).toContain("Düzenle, Ekler ve Denetle kaydederken belgenin klasörünü önerir.");
    expect(html).toContain("Karşılaştır'ın raporu doğrudan İndirilenler klasörüne yazılır.");
    expect(html).not.toContain("Öteki araçlar");
    // Kapsamsız genel etiket kalmadı.
    expect(html).not.toMatch(/>\s*Çıktı klasörü\s*</);
  });

  it("kullanıcı klasörü değiştirince de kapsam görünür kalır", () => {
    // Eskiden "Dönüştürülen Belgeler · varsayılan" adı dolaylı bir ipucuydu;
    // klasör değişince o da kayboluyordu.
    const html = render("/Users/avukat/Masaüstü/Çıktılar", false);
    expect(html).toContain(">Çıktılar<");
    expect(html).toContain("Yalnız Dönüştür buraya kaydeder.");
    expect(html).toContain("Varsayılana Dön");
  });
});

// Saha: "Varsayılan olarak dönüştürülen UDF dosyalarında satır aralığı 1,5;
// öncesinde ve sonrasında boşluk olmamalı." Varsayılan açıktır; kapatılınca
// Word'deki aralıklar korunur. Motor tarafı `udf_spacing.rs`'te kilitli.
describe("Word'den UDF'ye dilekçe aralığı", () => {
  const html = (checked: boolean) =>
    renderToStaticMarkup(<PetitionSpacingRow checked={checked} onChange={noop} />).replace(/&#x27;/gu, "'");

  it("satır ne yaptığını ve kapalıyken ne olduğunu söyler", () => {
    const on = html(true);
    // "UDF'ye": kısaltma son harfinin okunuşuna göre ek alır (fe → 'ye).
    expect(on).toContain("Word'den UDF'ye satır aralığı 1,5");
    expect(on).not.toContain("UDF'e");
    // Ad ile açıklama ayrı: kutunun adı etiket, açıklaması ipucu.
    expect(on).toMatch(/<input type="checkbox" aria-label="Word'den UDF'ye satır aralığı 1,5" aria-describedby="ayar-dilekce-araligi-not"/);
    expect(on).toContain('id="ayar-dilekce-araligi-not"');
    expect(on).toContain("Paragraf öncesi ve sonrası boşluk bırakılmaz.");
    expect(on).toContain("Kapalıyken Word belgesindeki aralıklar korunur.");
    expect(on).toMatch(/<input type="checkbox"[^>]* checked=""/);
    expect(html(false)).not.toContain('checked=""');
  });

  it("tercih ayarlara bağlı; varsayılanı açık", async () => {
    const { readFileSync } = await import("node:fs");
    const { fileURLToPath } = await import("node:url");
    const { dirname, resolve } = await import("node:path");
    const here = dirname(fileURLToPath(import.meta.url));
    const settings = readFileSync(resolve(here, "../../shell/Settings.tsx"), "utf8");
    expect(settings).toContain("petitionSpacing={settings.udfPetitionSpacing}");
    expect(settings).toContain('set("udfPetitionSpacing", next)');
    const mock = readFileSync(resolve(here, "../../devMock.ts"), "utf8");
    expect(mock).toContain("udfPetitionSpacing: true");
    const rust = readFileSync(resolve(here, "../../../src-tauri/src/settings.rs"), "utf8");
    expect(rust).toMatch(/#\[serde\(default = "yes"\)\]\s*pub udf_petition_spacing: bool/);
  });
});

// Tercihlerdeki "Değiştir": seçici hiç açılamazsa bu vazgeçme değildir.
// Eskiden `.catch(() => null)` reddi vazgeçme sayıyor, düğme hiçbir şey
// yapmamış gibi görünüyordu (Dönüştür akışındaki kardeşi söylüyordu).
describe("tercihlerde klasör seçici hatası", () => {
  it("söylenir ve duyurulur; vazgeçme sessiz kalır", async () => {
    const { readFileSync } = await import("node:fs");
    const { fileURLToPath } = await import("node:url");
    const { dirname, resolve } = await import("node:path");
    const here = dirname(fileURLToPath(import.meta.url));
    const src = readFileSync(resolve(here, "OutputFolderField.tsx"), "utf8");
    const choose = src.slice(src.indexOf("const choose = useCallback("), src.indexOf("const reset = useCallback("));
    expect(choose).not.toContain(".catch(() => null)");
    expect(choose).toMatch(/catch \(e\) \{[\s\S]*setError\(message\);\s*announce\(message\);\s*return;/);
    expect(choose).toContain('if (typeof picked !== "string") return;');
    // <p> içinde <p> yok: Status kendisi bir paragraftır.
    expect(src).not.toMatch(/<p className="prefs-hint">\s*<Status/);
  });
});
