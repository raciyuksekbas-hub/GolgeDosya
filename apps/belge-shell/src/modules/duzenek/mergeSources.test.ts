// Birleştir — açık belgenin üstüne ekleme (v0.3.2 saha bildirimi a05).
//
// Saha: "Bir PDF açıkken Birleştir'e tıkladığımda, açık olan PDF'in üstüne
// eklensin." Eskiden "Birleştirilecek Belge Ekle" seçilen dosyaları listenin
// YERİNE koyuyordu: açık A uyarısız düşüyor, yalnız B'den oluşan bir
// "birleştirme" kaydediliyordu. Tek belgeyle de Kaydet açıktı.
//
// Kural saf fonksiyonlarda (pdfWorkspaceState.ts) doğrulanır; bileşenin bu
// kuralı gerçekten kullandığı kaynak üzerinden. Tıklamalı gerçek tarayıcı
// doğrulaması bu testin işi değildir (bkz. teslim raporundaki CDP betiği).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import * as state from "./pdfWorkspaceState";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");

type Src = { path: string; file_name: string };
const doc = (name: string): Src => ({ path: `/belgeler/${name}`, file_name: name });
const A = doc("A.pdf"), B = doc("B.pdf"), C = doc("C.pdf");
const names = (list: Src[]) => list.map((s) => s.file_name);

// Fonksiyonlar tabanda yok; yokluğu "TypeError: not a function" yerine okunur
// bir başarısızlık olarak görünsün.
const fn = <K extends string>(key: K) => {
  const value = (state as Record<string, unknown>)[key];
  expect(typeof value, `${key} pdfWorkspaceState'te tanımlı olmalı`).toBe("function");
  return value as (...args: never[]) => unknown;
};

describe("appendSources — açık belge taban, yeniler sona", () => {
  const append = () => fn("appendSources") as unknown as (e: Src[], s: Src[]) => { next: Src[]; skipped: string[] };

  it("A açıkken yalnız B seçilir → [A, B]", () => {
    const { next, skipped } = append()([A], [B]);
    expect(names(next)).toEqual(["A.pdf", "B.pdf"]);
    expect(skipped).toEqual([]);
  });

  it("çoklu seçim seçim sırasıyla eklenir, A ilk sırada kalır", () => {
    expect(names(append()([A], [B, C]).next)).toEqual(["A.pdf", "B.pdf", "C.pdf"]);
    expect(names(append()([A], [C, B]).next)).toEqual(["A.pdf", "C.pdf", "B.pdf"]);
  });

  it("listede olan belge yeniden eklenmez ve adı söylenir", () => {
    const { next, skipped } = append()([A], [A, B]);
    expect(names(next)).toEqual(["A.pdf", "B.pdf"]);
    expect(skipped).toEqual(["A.pdf"]);
    // Aynı seçimde iki kez gelen dosya da tek girer (yinelenen React anahtarı yok).
    const twice = append()([A], [B, B]);
    expect(names(twice.next)).toEqual(["A.pdf", "B.pdf"]);
    expect(twice.skipped).toEqual(["B.pdf"]);
  });

  it("boş listeye ekleme seçimin kendisidir", () => {
    expect(names(append()([], [B, C]).next)).toEqual(["B.pdf", "C.pdf"]);
  });

  it("girdileri değiştirmez", () => {
    const existing = [A];
    append()(existing, [B]);
    expect(existing).toEqual([A]);
  });
});

describe("kaynak listesi düzeni", () => {
  it("yukarı / aşağı taşı; sınırda liste aynı kalır", () => {
    const shift = fn("shiftSource") as unknown as (l: Src[], i: number, d: -1 | 1) => Src[];
    expect(names(shift([A, B, C], 0, 1))).toEqual(["B.pdf", "A.pdf", "C.pdf"]);
    expect(names(shift([A, B, C], 2, -1))).toEqual(["A.pdf", "C.pdf", "B.pdf"]);
    expect(names(shift([A, B, C], 0, -1))).toEqual(["A.pdf", "B.pdf", "C.pdf"]);
    expect(names(shift([A, B, C], 2, 1))).toEqual(["A.pdf", "B.pdf", "C.pdf"]);
  });

  it("listeden çıkar yalnız o girdiyi düşürür", () => {
    const remove = fn("removeSource") as unknown as (l: Src[], i: number) => Src[];
    expect(names(remove([A, B, C], 1))).toEqual(["A.pdf", "C.pdf"]);
    expect(names(remove([A, B], 0))).toEqual(["B.pdf"]);
  });
});

describe("Birleştir en az iki belgeyle kaydeder", () => {
  it("tek belgede kaydetme kapalı ve nedeni söylenir", () => {
    const shortfall = fn("mergeShortfall") as unknown as (k: string, n: number) => string;
    expect(shortfall("merge", 1)).toBe("Birleştirmek için en az bir belge daha ekleyin.");
    expect(shortfall("merge", 0)).not.toBe("");
    expect(shortfall("merge", 2)).toBe("");
    // Öteki araçlar tek belgeyle çalışır; kapı yalnız Birleştir'de.
    expect(shortfall("compress", 1)).toBe("");
  });

  it("bileşen kapıyı Kaydet'e bağlar ve nedeni panelde gösterir", () => {
    const gate = source.slice(source.indexOf("const cannotSave"), source.indexOf("const pick ="));
    expect(gate).toMatch(/mergeShortfall\(kind, sources\.length\)/);
    expect(source).toMatch(/\{needMore && <p className="tool-hint"[^>]*>\{needMore\}<\/p>\}/);
  });
});

describe("ekleme sözcükleri 'eklendi / eklenemedi' — açık belge bozuk sanılmaz", () => {
  const note = () => fn("appendNote") as unknown as (a: number, s: string[], f: string[], d?: string) => { text: string; tone: string };

  it("kısmi hata eklenenleri ve eklenemeyeni adlandırır", () => {
    const { text, tone } = note()(1, [], ["bozuk.pdf"]);
    expect(text).toBe("1 belge eklendi. 1 dosya eklenemedi: bozuk.pdf.");
    expect(tone).toBe("error");
  });

  it("hiçbiri eklenemezse yalnız o dosya söylenir, 'Belge açılamadı' denmez", () => {
    const { text } = note()(0, [], ["bozuk.pdf"], "Bu PDF okunamadı. Dosya bozulmuş olabilir; farklı bir kopya deneyin.");
    expect(text).toBe("bozuk.pdf eklenemedi. Bu PDF okunamadı. Dosya bozulmuş olabilir; farklı bir kopya deneyin.");
    expect(text).not.toMatch(/açıl/i);
  });

  it("yinelenen dosya tek satırla söylenir; söylenecek bir şey yoksa boş", () => {
    expect(note()(1, ["A.pdf"], [])).toEqual({ text: "A.pdf zaten listede; yeniden eklenmedi.", tone: "info" });
    expect(note()(2, [], []).text).toBe("");
  });

  it("bileşen eklemede açma cümlelerini kullanmaz", () => {
    // Eklemenin her iki hata yolu appendNote'tan geçer.
    const load = source.slice(source.indexOf("const load = async"), source.indexOf("const choose = async"));
    expect(load).toMatch(/mode === 'append'[\s\S]*appendNote\(/);
    expect(load).toMatch(/catch \(e\) \{[\s\S]*mode === 'append' \? appendNote\(0, \[\], paths\.map\(baseNameOf\), detail\)\.text : `\$\{OPEN_FAILURE_TITLE\}/);
  });
});

describe("bileşen bağlaması", () => {
  it("Birleştir'de açık belge varken seçici EKLER, başlığı tabanı adlandırır", () => {
    const choose = source.slice(source.indexOf("const choose = async"), source.indexOf("const openAnother"));
    expect(choose).toMatch(/const appending = kind === 'merge' && sources\.length > 0/);
    expect(choose).toMatch(/title: appending \? `\$\{sources\[0\]\.file_name\} ile birleştirilecek PDF’ler`/);
    expect(choose).toMatch(/load\(picked, appending \? 'append' : 'replace'\)/);
  });

  it("ekleme son işlenen listeye yapılır; eski bir tarama yenisini ezmez", () => {
    const load = source.slice(source.indexOf("const load = async"), source.indexOf("const choose = async"));
    expect(load).toMatch(/const generation = \+\+loadGeneration\.current/);
    expect(load).toMatch(/if \(generation !== loadGeneration\.current\)\s*return;/);
    expect(load).toMatch(/const base = mode === 'append' \? sourcesRef\.current : \[\];\s*const \{ next, skipped \} = appendSources\(base, result\.sources\)/);
  });

  it("Görseller'den PDF aracına dönüş kabuğun açık belgesini yeniden taban yapar", () => {
    const pick = source.slice(source.indexOf("const pick ="), source.indexOf("const PAGE_TOOLS"));
    expect(pick).toMatch(/kind === 'images' && key !== 'images' && initialPaths\?\.length/);
    expect(pick).toMatch(/void load\(initialPaths, 'replace', message\)/);
  });

  it("kaynak listesinde yukarı / aşağı taşı ve 'Listeden çıkar' var; etiket korunuyor", () => {
    const list = source.slice(source.indexOf('className="source-list"'), source.indexOf("</ol>}"));
    expect(list).toContain("belgeyi yukarı taşı");
    expect(list).toContain("belgeyi aşağı taşı");
    expect(list).toContain("listeden çıkar");
    expect(list).toMatch(/sources\.length > 1 &&/);
    expect(source).toContain("Birleştirilecek Belge Ekle");
    // Liste değişince eski ekleme notu ("A.pdf zaten listede") susar.
    expect(source).toMatch(/const relist = \(next: SourceFile\[\]\) => \{ setSources\(next\); build\(next\); setStatus\(''\);/);
    expect(source).toMatch(/const dropSource = \(index: number\) => relist\(removeSource\(sources, index\)\)/);
  });

  it("panel metinleri ekleme davranışını anlatır", () => {
    expect(source).toContain("merge: ['Birleştir', 'Açık belgeye eklediğiniz PDF’leri listedeki sırayla tek dosyada birleştirir.']");
    expect(source).toContain("merge: { affects: 'Açık belge ilk sırada; eklediğiniz belgeler sonuna eklenir, sırayı oklarla değiştirebilirsiniz.'");
  });
});
