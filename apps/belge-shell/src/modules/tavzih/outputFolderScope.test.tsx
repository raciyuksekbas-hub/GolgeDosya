// Ayarlar > Genel'deki klasör satırı kimin?
//
// Saha: "Kayıt klasörleri arasında birlik yok." Satır yalnız "Çıktı klasörü"
// diyordu ve bütün uygulamanın tercihi gibi duruyordu; onu yalnız Dönüştür
// okur (`tavzih.rs` effective_output_dir). Kullanıcı klasörü değiştirip
// Düzenle'de kaydedince dosya başka yere gidiyordu. Satır artık kapsamını
// söyler; tercihin anlamı ve komutları değişmedi.
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { OutputFolderRow } from "./OutputFolderField";

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
