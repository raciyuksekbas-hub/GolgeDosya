// Windows yol sözleşmesi (§55) — ad ve uzantı.
//
// Saha: kullanıcı Windows'ta. Yol yalnız `/` ile bölündüğü için son belgeler
// listesi, bardaki belge adı ve hata satırları kullanıcının adını taşıyan TAM
// yolu gösteriyordu.
import { describe, expect, it } from "vitest";
import { extensionOf, fileNameOf, modeAccepts } from "./modes";

const WIN = "C:\\Users\\Çağrı Şahin\\Masaüstü\\Müvekkil'in Dosyası\\ihtarname (son).docx";

describe("dosya adı her iki ayraçla", () => {
  it("Windows yolu: yalnız dosya adı; kullanıcı adı görünmez", () => {
    expect(fileNameOf(WIN)).toBe("ihtarname (son).docx");
    expect(fileNameOf(WIN)).not.toContain("Çağrı");
  });

  it("UNC, karışık ayraç ve sondaki ayraç", () => {
    expect(fileNameOf("\\\\sunucu\\paylaşım\\dava.pdf")).toBe("dava.pdf");
    expect(fileNameOf("C:/Users/Çağrı Şahin\\Belgeler/dilekçe.udf")).toBe("dilekçe.udf");
    expect(fileNameOf("C:\\Users\\Çağrı Şahin\\Documents\\GölgeDosya\\Dönüştürülen Belgeler\\")).toBe(
      "Dönüştürülen Belgeler",
    );
  });

  it("macOS yolu aynı kalır", () => {
    expect(fileNameOf("/Users/örnek/Belgeler/dilekçe.docx")).toBe("dilekçe.docx");
  });

  it("uzantı klasör adındaki noktaya kanmaz", () => {
    expect(extensionOf("C:\\Users\\a.b\\dosya")).toBe("");
    expect(extensionOf(WIN)).toBe("docx");
    expect(modeAccepts("tavzih", [WIN])).toBe(true);
  });
});
