// Sessiz başarısızlık regresyonları.
//
// Buradaki her madde, düğmeye basıldığında bir şeyin ÇALIŞMADIĞI ama
// kullanıcıya hiçbir şey söylenmediği bir yol. Hepsini düzeltmeden önce
// kendim yeniden ürettim; hepsinin belirtisi "hiçbir şey olmadı" idi.
//
// Bu testler kaynağı okur. Sebep, korunan şeyin bir ÇIKTI değil bir KOD
// YOLU olmasıdır: yakalanmayan bir söz reddi, tanımı gereği hiçbir görünür
// iz bırakmaz — yeşil kalan bir çıktı testi tam da bu hatayı kaçırırdı.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { FirstUseAcceptance } from "../modules/tavzih/Consent";

const here = dirname(fileURLToPath(import.meta.url));
const read = (p: string) => readFileSync(resolve(here, p), "utf8");
const app = read("../App.tsx");
const convert = read("../modules/tavzih/ConvertWorkspace.tsx");
const review = read("../modules/ikincigoz/ReviewWorkspace.tsx");

/** Bir fonksiyon gövdesini adından itibaren `span` karakter boyunca alır. */
function body(source: string, marker: string, span = 1400): string {
  const at = source.indexOf(marker);
  expect(at, `${marker} kaynakta yok`).toBeGreaterThan(-1);
  return source.slice(at, at + span);
}

describe("son belgeler yazması", () => {
  // Yeniden üretim: ayar dosyası salt okunur yapılınca belge açmak
  // `rememberDocuments` sözünü reddediyor. Belge açılıyor (kullanıcının
  // istediği buydu) ama red yakalanmıyordu: yakalanmamış söz reddi, ve son
  // belgeler listesi sessizce eskimiş kalıyordu.
  it("belge açma, hatırlama düşse bile sürer", () => {
    const open = body(app, "const openDocuments");
    expect(open).toContain("try {");
    expect(open).toContain("api.rememberDocuments(paths)");
    expect(open).toContain("catch");
  });

  it("hatırlanamadığı kullanıcıya söylenir", () => {
    expect(body(app, "const openDocuments")).toContain(
      "Belge açıldı ancak son belgeler listesine eklenemedi.",
    );
  });

  // Yeniden üretim: "Tümünü Temizle" düğmesi. Yazma düşerse liste olduğu gibi
  // duruyor, hiçbir şey söylenmiyordu — kullanıcı sildiğini SANIYORDU.
  it("liste temizlenemezse söylenir, silindiği sanılmaz", () => {
    const forget = body(app, "const forgetDocuments");
    expect(forget).toContain("try {");
    expect(forget).toContain("catch");
    expect(forget).toContain("Liste temizlenemedi");
    expect(forget).toContain("setSettingsError");
  });

  it("başarıyla temizlendiğinde de söylenir", () => {
    expect(body(app, "const forgetDocuments")).toContain("Son belgeler listesi temizlendi.");
  });
});

describe("kullanım koşulları onayı", () => {
  // Yeniden üretim: onay yazması düşünce `setAccepted(true)` hiç
  // çalışmıyordu. Kullanıcı "Kabul Ediyorum" düğmesine basıyor, ekran olduğu
  // gibi duruyor, hiçbir açıklama gelmiyor. Ürün, sebebi söylenmeden
  // kullanılamaz hâle geliyordu — bu turun en ağır sessiz hatasıydı.
  it("onay yazması korunur", () => {
    const accept = body(convert, "if (accepted === false)");
    expect(accept).toContain("try {");
    expect(accept).toContain("api.acceptTerms()");
    expect(accept).toContain("catch");
  });

  it("düşerse sebep ekranda kalır", () => {
    expect(body(convert, "if (accepted === false)")).toContain("Onayınız kaydedilemedi");
    expect(convert).toContain("const [acceptError, setAcceptError]");
  });

  it("pencere hatayı çizebilir ve uyarı olarak duyurur", () => {
    const html = renderToStaticMarkup(
      <FirstUseAcceptance onAccept={() => undefined} error="Onayınız kaydedilemedi." />,
    );
    expect(html).toContain('role="alert"');
    expect(html).toContain("Onayınız kaydedilemedi.");
  });

  it("hata yokken uyarı kutusu çizilmez", () => {
    expect(renderToStaticMarkup(<FirstUseAcceptance onAccept={() => undefined} />)).not.toContain(
      'role="alert"',
    );
  });
});

describe("Finder'da Göster", () => {
  // Yeniden üretim: çıktı klasörünü Finder'da elle sil, sonra düğmeye bas.
  // `onReveal={() => api.revealOutputFolder()}` yüzen bir sözdü: Finder
  // açılmıyor, hiçbir şey söylenmiyordu.
  it("açılamayan klasör söylenir", () => {
    const reveal = body(convert, "onReveal={");
    expect(reveal).toContain("try {");
    expect(reveal).toContain("await api.revealOutputFolder()");
    expect(reveal).toContain("catch");
    expect(reveal).toContain("Çıktı klasörü açılamadı");
  });
});

describe("⌘O yutulmaz", () => {
  // Yeniden üretim: bir belge aç, Dönüştür'e geç, ⌘O'ya bas. Kısayol
  // eşleşiyor, `preventDefault()` çalışıyor ve sayaç kimsenin okumadığı bir
  // yere artıyordu: tuş yutuluyor, karşılığında hiçbir şey olmuyordu.
  // `openRequest` yalnız belge yüzeyinde tüketilir.
  it("yalnız onu tüketen yüzey açıkken bağlanır", () => {
    expect(app).toContain('{ key: "o", run: () => setOpenRequest((n) => n + 1), enabled: !prefsTab && !usable }');
  });

  it("tercihler penceresi açıkken hâlâ kapalı", () => {
    // Odak tuzağının içinden arka plandaki eylemi tetiklemek odak modelini bozar.
    expect(app).toMatch(/enabled: !prefsTab/);
  });
});

describe("klasör seçiciler", () => {
  // Yeniden üretim: her ikisinde de seçici çağrısı try'ın DIŞINDAYDI. Pencere
  // hiç açılamazsa (izin, panel çökmesi) söz reddi yakalanmıyor, kullanıcı
  // düğmeye basıyor ve hiçbir şey olmuyordu. Vazgeçmek sessizdir;
  // açılamamak değil.
  it("Dönüştür'de seçici çağrısı korunur", () => {
    const choose = body(convert, "const chooseFolder");
    expect(choose.indexOf("try {")).toBeLessThan(choose.indexOf("await open("));
  });

  it("Denetle'de kaydetme penceresi korunur", () => {
    const apply = body(review, "const applyChosen", 3000);
    expect(apply.indexOf("try {")).toBeLessThan(apply.indexOf("await save("));
  });

  // Akıştaki "Varsayılana Dön" satır içi ve korumasizdi
  // (`async () => setFolder(await api.setOutputFolder(null))`): yazma düşerse
  // klasör varsayılan dışı kalıyor, düğme hiçbir şey yapmamış gibi
  // görünüyordu. Tercihler penceresindeki kardeşi zaten korunuyordu.
  it("Varsayılana Dön korunur ve sonucu söyler", () => {
    expect(convert).toContain("const resetFolder = useCallback");
    const reset = body(convert, "const resetFolder");
    expect(reset).toContain("Varsayılana dönülemedi");
    expect(reset).toContain("Çıktı klasörü varsayılana döndürüldü.");
    expect(convert).toContain("onResetFolder={resetFolder}");
  });
});

describe("Denetle makbuzu eskimez", () => {
  // Yeniden üretim: bir kez başarıyla uygula (yeşil "kaydedildi" satırı
  // belirir), sonra hedefi salt okunur yapıp yeniden uygula. Ekranda kırmızı
  // hata ile BİRLİKTE önceki denemenin yeşil satırı duruyordu; kullanıcı
  // dosyanın yazıldığını sanabiliyordu.
  it("yeni deneme önceki makbuzu geçersiz kılar", () => {
    const apply = body(review, "const applyChosen", 3000);
    expect(apply).toContain("setWritten(null)");
    // Temizlik, yazma çağrısından ÖNCE olmalı.
    expect(apply.indexOf("setWritten(null)")).toBeLessThan(apply.indexOf("api.applyFixes"));
  });

  it("başarısızlıkta yeşil satır kalmaz", () => {
    const apply = body(review, "const applyChosen", 3000);
    const katch = apply.slice(apply.indexOf("} catch"));
    expect(katch).toContain("setWritten(null)");
    expect(katch).toContain("setApplyError");
  });
});
