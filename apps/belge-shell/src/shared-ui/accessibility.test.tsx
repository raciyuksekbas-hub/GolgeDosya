// Erişilebilirlik regresyonları.
//
// Buradaki her test, ürünü klavye ve ekran okuyucuyla kullanan bir hukukçunun
// GERÇEKTEN takıldığı bir noktayı kilitler. Hiçbiri kozmetik değildir; her
// birinin karşılığı, düzeltmeden önce kendi elimle yeniden ürettiğim bir
// kullanıcı davranışıdır.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import { Status } from "./primitives";
import { PdfWorkspace } from "../modules/duzenek/PdfWorkspace";
import { ChangeInspector } from "../modules/degisikis/ChangeInspector";
import { compareDocuments } from "../modules/degisikis/core/compare";
import { makeBlocks } from "../modules/degisikis/core/normalize";
import {
  buildComparisonViewModel,
  summarize,
} from "../modules/degisikis/viewModels/comparisonViewModel";

const here = dirname(fileURLToPath(import.meta.url));
const read = (p: string) => readFileSync(resolve(here, p), "utf8");
const tokens = postcss.parse(read("tokens.css"));
const shell = postcss.parse(read("shell.css"));
const pdfSource = read("../modules/duzenek/PdfWorkspace.tsx");
const consentSource = read("../modules/tavzih/Consent.tsx");
const compareSource = read("../modules/degisikis/CompareWorkspace.tsx");
const appSource = read("../App.tsx");

/** Gerçek bir karşılaştırma — filtre düğmeleri gerçek veriyle çizilsin. */
const changes = buildComparisonViewModel(
  compareDocuments(
    makeBlocks([{ text: "Bedel 100.000 TL'dir." }, { text: "Yalnız temelde." }], "base"),
    makeBlocks([{ text: "Bedel 118.000 TL'dir." }, { text: "Yalnız değişikte." }], "revised"),
  ),
).changes;

/** Bir seçici için yürürlükteki bildirimler. */
function decls(sheet: postcss.Root, selector: string): Record<string, string> {
  const out: Record<string, string> = {};
  sheet.walkRules((rule) => {
    if (!rule.selectors.includes(selector)) return;
    rule.walkDecls((d) => {
      out[d.prop] = d.value;
    });
  });
  return out;
}

describe("hareketi azalt", () => {
  // Yeniden üretim: Tercihler → Erişilebilirlik → "Hareketi azalt: Açık".
  // Genel kural her animasyonun süresini 1 ms'ye indiriyor. Bu, bir KEZ oynayan
  // animasyonu söndürür ama `.spinner` gibi `infinite` bir döngüyü saniyede
  // ~1000 tura çıkarır: tercihi açan kullanıcı, kapalıyken olduğundan çok daha
  // saldırgan bir titreme görür.
  it("genel kural tek seferlik animasyonları hâlâ söndürür", () => {
    expect(decls(tokens, ':root[data-motion="reduced"] *')["animation-duration"]).toBe("1ms");
  });

  it("sonsuz dönen gösterge stroba dönüşmez", () => {
    const spinner = decls(tokens, ':root[data-motion="reduced"] .spinner');
    const duration = spinner["animation-duration"];
    expect(duration, "hareketi azalt kipinde .spinner için ayrı süre yok").toBeTruthy();
    const ms = Number(/([\d.]+)ms/.exec(duration)?.[1] ?? 0);
    // İnsan gözünün tek bir turu izleyebildiği, titremeyen bir hız.
    expect(ms).toBeGreaterThanOrEqual(1000);
  });

  it("gösterge tümden kaldırılmaz; meşguliyet geri bildirimi kalır", () => {
    expect(decls(tokens, ':root[data-motion="reduced"] .spinner')["display"]).toBeUndefined();
    expect(decls(tokens, ':root[data-motion="reduced"] .spinner')["animation-name"]).toBeUndefined();
  });
});

describe("canlı durum bölgesi", () => {
  // Yeniden üretim: bir canlı bölge, İÇERİĞİYLE AYNI anda DOM'a girerse ekran
  // okuyucuların çoğu hiçbir şey duyurmaz — bölgenin önce durması, sonra
  // içeriğinin değişmesi gerekir. `{status && <Status/>}` tam olarak bunu
  // yapıyordu: kullanıcı kaydete basıyor, sonuç ekrana yazılıyor, ekran
  // okuyucu susuyordu.
  it("boş durumda bile erişilebilirlik ağacında durur", () => {
    const html = renderToStaticMarkup(<Status tone="info">{""}</Status>);
    expect(html).toContain('role="status"');
    expect(html).toContain('data-empty="true"');
  });

  it("boşken kutusu yer kaplamaz", () => {
    expect(decls(shell, '.status[data-empty="true"]')["margin"]).toBe("0");
  });

  it("boşken meşgul göstergesi çizmez", () => {
    expect(renderToStaticMarkup(<Status tone="busy">{""}</Status>)).not.toContain("spinner");
    expect(renderToStaticMarkup(<Status tone="busy">Hazırlanıyor…</Status>)).toContain("spinner");
  });

  it("Düzenle çalışma alanı bölgeyi belge yokken de çizer", () => {
    // Görseller → PDF aracında sonuç satırı hiç çizilmiyordu: kullanıcı
    // düğmeye basıp hiçbir şey olmadığını sanıyordu.
    expect(renderToStaticMarkup(<PdfWorkspace />)).toContain('role="status"');
  });
});

describe("sayfa kutularının erişilebilir adı", () => {
  // Yeniden üretim: 30 sayfalık bir PDF'te Düzenle → Sayfa Çıkar. Ekran
  // okuyucu her kutuda aynı şeyi söylüyordu: "Çıkar, onay kutusu, işaretli
  // değil". Kullanıcı hangi sayfayı işaretlediğini ayırt edemiyor; yanlış
  // sayfayı çıkarma riski doğrudan.
  it("her kutu hangi sayfa olduğunu söyler", () => {
    expect(pdfSource).toMatch(
      /<input type="checkbox" aria-label=\{`\$\{index \+ 1\}\. sayfa \(kaynak s\. \$\{item\.page\}\)/,
    );
  });

  it("görünen sözcük erişilebilir adın içinde kalır (WCAG 2.5.3)", () => {
    // Sesle komut veren kullanıcı "Dahil et" der; ad bu metni İÇERMELİ.
    const label = /aria-label=\{`[^`]*\$\{pickVerb\}`\}/.test(pdfSource);
    expect(label, "erişilebilir ad görünen etiketi içermiyor").toBe(true);
    expect(pdfSource).toMatch(/\/>\{pickVerb\}<\/label>/);
  });

  it("taşıma düğmeleri görünen sıra numarasıyla konuşur", () => {
    // Sayfalar yeniden sıralandığında görünen numara ÇIKTI sırasıdır; düğme
    // adı KAYNAK sayfasını söylüyordu, ikisi birbirini tutmuyordu.
    expect(pdfSource).toContain("${index + 1}. sayfayı yukarı taşı");
    expect(pdfSource).toContain("${index + 1}. sayfayı aşağı taşı");
  });
});

describe("durum yalnız renkle anlatılmaz", () => {
  // Yeniden üretim: Karşılaştır → Fark Özeti. Dört filtre düğmesinden hangisinin
  // yürürlükte olduğu SADECE `is-active` sınıfının rengiyle belliydi. Ekran
  // okuyucu dört özdeş "düğme" okuyor; renk körü bir kullanıcı da ayırt
  // edemiyordu.
  const inspector = (filter: "all" | "added") =>
    renderToStaticMarkup(
      <ChangeInspector
        summary={summarize(changes)}
        filter={filter}
        onFilterChange={() => undefined}
        filteredChanges={changes}
        selectedChange={undefined}
        onSelect={() => undefined}
      />,
    );

  it("yürürlükteki filtre erişilebilirlik ağacında da işaretli", () => {
    expect(inspector("all")).toContain('aria-pressed="true"');
  });

  it("yalnız BİR filtre işaretli görünür", () => {
    const pressed = inspector("added").match(/aria-pressed="true"/g) ?? [];
    expect(pressed).toHaveLength(1);
  });

  it("işaret sınıfla birlikte yürür, sınıfın yerine geçmez", () => {
    // Görsel gösterge kaldırılmadı; yanına erişilebilir olanı eklendi.
    expect(inspector("all")).toContain("is-active");
  });
});

describe("kip pencereleri odağı içeride tutar", () => {
  // Yeniden üretim: ilk açılışta Kullanım Koşulları penceresi açıkken Tab.
  // `aria-modal` ekran okuyucunun sanal imlecini kısıtlar ama FİZİKSEL odağı
  // kısıtlamaz: odak arkadaki, o an kullanılamaz uygulamanın düğmelerine
  // kaçıyordu. Klavye kullanıcısı cevaplayamadığı bir kapının ardında
  // dolaşıyordu.
  it("kullanım koşulları penceresi tuzak kurar", () => {
    expect(consentSource).toMatch(/useFocusTrap\(sheet, true\)/);
    expect(consentSource).toContain('<div className="sheet" ref={sheet} role="dialog"');
  });

  it("dönüştürme uyarısı tuzak kurar ve Escape'i vazgeçmeye bağlar", () => {
    expect(consentSource).toMatch(/useFocusTrap\(sheet, true, onCancel\)/);
  });

  it("koşullar penceresinde Escape hâlâ kapatmaz", () => {
    // Cevaplanması gereken kapı Escape ile atlanamaz; tuzak bunu gevşetmedi.
    expect(consentSource).toMatch(/if \(e\.key === "Escape"\) e\.preventDefault\(\);/);
  });
});

describe("sürüm değiştirme bara yansır", () => {
  // Yeniden üretim: Karşılaştır'da sürümleri değiştir. Paneller doğru
  // etiketleniyordu ("TEMEL SÜRÜM" + dosya adı) ama yardımcı bardaki çipler
  // açılış sırasında kalıyordu; "önce yazan Temel'dir" okumasıyla çelişiyordu.
  it("çalışma alanı yeni sırayı kabuğa bildirir", () => {
    expect(compareSource).toContain("onPairChange?.(swapped.map((x) => x.path))");
  });

  it("değişiklik ekran okuyucuya söylenir", () => {
    expect(compareSource).toContain("Sürümler değiştirildi. Temel sürüm artık");
  });

  it("bar bildirilen sırayı izler", () => {
    expect(appSource).toContain("shown = comparePair");
    // Bildirim ancak AYNI belgelerin bir dizilişiyse geçerli: yeni bir çift
    // açıldığında eski bildirim kendiliğinden düşmeli.
    expect(appSource).toContain("comparePair.every((p) => shown.includes(p))");
  });
});

describe("seçili değişiklik satırı", () => {
  // Ray (`rail-node`) seçili farkı `aria-current` ile söylüyordu; LİSTE
  // söylemiyordu, orada seçim yalnız `is-active` sınıfının rengiydi. Aynı
  // bilgi iki yüzeyde farklı davranıyordu.
  const inspectorSource = read("../modules/degisikis/ChangeInspector.tsx");
  it("liste satırı da seçili olduğunu söyler", () => {
    expect(inspectorSource).toContain("aria-current={active || undefined}");
  });
});

describe("büyük metin arayüzü kırpmaz", () => {
  // Yeniden üretim: Tercihler → Görünüm → "Metin boyutu: En büyük" (%200).
  // Yazı büyüyor ama kenar çubuğu 220 px, tercih rayı 160 px olarak SABİT
  // kalıyordu. İkisi de `white-space: nowrap` kullanır, yani sığmayan ad
  // sarmaz KESİLİR: erişilebilirlik ayarının kendisi arayüzü bozuyordu.
  // Yükseklikler (`--row-h`, `--control-h`, `--toolbar-h`) zaten ölçeği
  // izliyordu; genişlikler unutulmuştu.
  it("kenar çubuğu genişliği metin ölçeğini izler", () => {
    expect(decls(tokens, ":root")["--sidebar-w"]).toContain("var(--text-scale)");
  });

  it("tercih rayı genişliği metin ölçeğini izler", () => {
    expect(decls(tokens, ":root")["--prefs-rail-w"]).toContain("var(--text-scale)");
    expect(decls(shell, ".sheet.prefs")["grid-template-columns"]).toContain("var(--prefs-rail-w)");
  });

  it("genişlikler görüş alanıyla sınırlı: çubuk çalışma alanını yutmaz", () => {
    expect(decls(tokens, ":root")["--sidebar-w"]).toContain("vw");
  });

  it("varsayılan ölçekte ölçü değişmez", () => {
    // `--text-scale: 1` iken min(220px, 34vw) -> 220px: %100'de yerleşim aynı.
    expect(decls(tokens, ":root")["--text-scale"]).toBe("1");
    expect(decls(tokens, ":root")["--sidebar-w"]).toContain("220px");
  });
});

describe("Karşılaştır panelleri klavyeyle okunur", () => {
  // Yeniden üretim: Karşılaştır'da fare kullanmadan belgeyi okumaya çalış.
  // Paneller kaydırılabilir (`overflow: auto`) ama İÇLERİNDE odaklanabilir
  // hiçbir öğe yok — satırlar düz `div`. Odak panele hiç giremediği için ok
  // tuşları, PageDown ve Home/End çalışmıyor: fare kullanamayan bir hukukçu
  // belgenin ilk ekrandan sonrasını HİÇ okuyamıyordu.
  const paneSource = read("../modules/degisikis/DocumentPane.tsx");
  it("kaydırma bölgesi sekme sırasında", () => {
    expect(paneSource).toContain("tabIndex={0}");
  });

  it("bölgenin ekran okuyucuda adı var", () => {
    expect(paneSource).toContain('role="region"');
    expect(paneSource).toContain("aria-label={SIDE_LABEL[side]}");
  });

  it("odak görünür: genel odak halkası kaldırılmamış", () => {
    expect(decls(tokens, ":focus-visible")["outline"]).toContain("var(--focus)");
  });
});

describe("kısmi tarama sağlam belgeleri atmaz", () => {
  // Yeniden üretim: 3 PDF seç, biri bozuk olsun. Eskiden `errors.length`
  // doluysa hata fırlatılıyor ve `result.sources` HİÇ kullanılmıyordu: 2
  // sağlam belge de çöpe gidiyordu. Üstelik hangisinin bozuk olduğu
  // söylenmiyordu — yalnız `reason` gösteriliyordu, `path` değil.
  it("yalnız hepsi düştüyse vazgeçer", () => {
    expect(pdfSource).toContain("if (result.errors.length && !result.sources.length)");
  });

  it("kısmi düşüşte sağlamlar açılır ve durum söylenir", () => {
    expect(pdfSource).toContain("belge açıldı. ${result.errors.length} dosya açılamadı");
  });

  it("düşen dosya ADIYLA söylenir", () => {
    expect(pdfSource).toContain("result.errors.map(e => baseNameOf(e.path))");
  });
});
