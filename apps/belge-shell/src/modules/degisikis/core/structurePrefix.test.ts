// Yapısal önek: paragrafın başındaki "1.", "1)", "Madde 5-" gibi numara.
//
// Saha bulgusu: tutarla ya da tarihle başlayan, sayısı sonraki sözcüğe bitişik
// paragraf ("12.500TL ...", "01.10.2026fesih ...") numaralı paragraf sanılıyordu.
// Desen "12.500"den sonra ayraç bulamayınca geri sarıyor, "12." önek, "500TL"
// gövde oluyordu. Özet tutarı "500 TL → 750 TL" diye yazıyor, konum "Madde 12"
// çıkıyor, araya numaralı paragraf eklenince "18.500TL → 19.500TL" numara
// kayması sayılıp hiç gösterilmiyordu. Ayraçtan hemen sonra gelen rakam sayının
// devamıdır (binlik, ondalık, tarih, aralık); numara değildir.
import { describe, expect, it } from "vitest";
import { appendixHeading, splitStructuralPrefix } from "./structure";

describe("sayının devamı önek sayılmaz", () => {
  it.each([
    "12.500TL tutarındaki alacak tahsil edilmiştir.",
    "18.000TL kira bedeli her ay ödenecektir.",
    "12.500,00 TL alacak tahsil edilmiştir.",
    "1.250,50 TL fark iade edilir.",
    "12.500.000TL bedelle satılmıştır.",
    "01.10.2026fesih bildirimi yapılmıştır.",
    "01.10.2026 tarihinde fesih bildirilmiştir.",
    "01-10-2026 tarihinde fesih bildirilmiştir.",
    "2020-2021 döneminde kira bedeli ödenmiştir.",
    "5-10 gün içinde ödeme yapılır.",
    "Madde 5.1'de belirtilen hususlar saklıdır.",
  ])("%s", (text) => {
    expect(splitStructuralPrefix(text)).toEqual({ body: text });
  });

  it("ek başlığı da tutarı ek numarası saymaz", () => {
    expect(appendixHeading("Ek 2.500 TL ödeme yapılır.")).toBeUndefined();
  });
});

describe("gerçek numaralı önekler aynen tanınır", () => {
  it.each([
    ["1.Kira bedeli 18.000 TL'dir.", "paragraph", "1", "1.", "Kira bedeli 18.000 TL'dir."],
    ["1. Kira bedeli 18.000 TL'dir.", "paragraph", "1", "1.", "Kira bedeli 18.000 TL'dir."],
    ["1) Kira bedeli 18.000 TL'dir.", "paragraph", "1", "1)", "Kira bedeli 18.000 TL'dir."],
    ["1 ) Kira bedeli 18.000 TL'dir.", "paragraph", "1", "1 )", "Kira bedeli 18.000 TL'dir."],
    ["1- Kira bedeli 18.000 TL'dir.", "paragraph", "1", "1-", "Kira bedeli 18.000 TL'dir."],
    ["1.1. Kira bedeli 18.000 TL'dir.", "paragraph", "1.1", "1.1.", "Kira bedeli 18.000 TL'dir."],
    ["1.1.1 Kira bedeli 18.000 TL'dir.", "paragraph", "1.1.1", "1.1.1", "Kira bedeli 18.000 TL'dir."],
    // Ayraçtan sonra BOŞLUK varsa numara okuması kalır; Word'ün görünür liste
    // etiketi de metne "1. " olarak yazılır (numbering.ts).
    ["1. 2026 yılı kira artışı TÜFE oranındadır.", "paragraph", "1", "1.", "2026 yılı kira artışı TÜFE oranındadır."],
    ["5- KİRA BEDELİ", "article", "5", "5-", "KİRA BEDELİ"],
    ["Madde 5- Kira bedeli 18.000 TL'dir.", "article", "5", "Madde 5-", "Kira bedeli 18.000 TL'dir."],
    ["Madde 5 - Kira bedeli 18.000 TL'dir.", "article", "5", "Madde 5 -", "Kira bedeli 18.000 TL'dir."],
    ["MADDE 5 – KİRA BEDELİ", "article", "5", "MADDE 5 –", "KİRA BEDELİ"],
    ["Madde 5: Kira bedeli 18.000 TL'dir.", "article", "5", "Madde 5:", "Kira bedeli 18.000 TL'dir."],
    ["Madde 5.1. Kira bedeli 18.000 TL'dir.", "article", "5.1", "Madde 5.1.", "Kira bedeli 18.000 TL'dir."],
    ["Geçici Madde 1 - Uyum süresi", "article", "1", "Geçici Madde 1 -", "Uyum süresi"],
  ])("%s", (text, kind, number, raw, body) => {
    const structural = splitStructuralPrefix(text);
    expect(structural.body).toBe(body);
    expect(structural.prefix).toMatchObject({ kind, number, raw });
  });

  it.each([
    ["EK-1 Vekaletname", { label: "EK-1", title: "Vekaletname" }],
    ["EK 2: Fatura", { label: "EK-2", title: "Fatura" }],
    ["Ek 1.Fatura", { label: "EK-1", title: "Fatura" }],
    ["Ek-3", { label: "EK-3" }],
  ])("ek başlığı: %s", (text, expected) => {
    expect(appendixHeading(text)).toEqual(expected);
  });
});
