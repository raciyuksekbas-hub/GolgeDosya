// a06 — Sırala: sayfa sırasının saf modeli.
//
// Saha: "Sayfaları doğrudan seçebilip oynayabilmeliyiz. Birini ya da
// birkaçını diğerinin altına veya üstüne fareyle sürükleyebilmeliyiz."
// Bugüne dek tek yol komşu iki sayfayı yer değiştiren ↑/↓ idi: bir sayfayı
// başa almak sayfa sayısı kadar tıklama istiyordu; birden fazla sayfa
// taşınamıyor, yanlış taşıma geri alınamıyordu. Sürükleme, düğmeler ve
// klavye aynı saf işlemleri çağırır; burada tarayıcısız sınanırlar.
import { describe, expect, it } from "vitest";
import {
  describeMove, dropGap, edgeScroll, HISTORY_LIMIT, isReordered, moveKeys, pushHistory, sameOrder, selectPages, shiftKeys,
} from "./pageOrder";
import * as pageOrder from "./pageOrder";

const pages = (...n: number[]) => n.map((page) => ({ key: `a#${page}`, page }));
const keys = (...n: number[]) => n.map((page) => `a#${page}`);
const seq = (order: { page: number }[]) => order.map((p) => p.page).join(",");
const isPermutation = (order: { key: string }[], of: { key: string }[]) =>
  order.length === of.length && new Set(order.map((p) => p.key)).size === of.length && of.every((p) => order.some((q) => q.key === p.key));

describe("moveKeys — seçili sayfaları bir boşluğa taşır", () => {
  const six = pages(1, 2, 3, 4, 5, 6);
  it("tek sayfa: önüne ve arkasına", () => {
    expect(seq(moveKeys(six, keys(1), 4))).toBe("2,3,4,1,5,6");
    expect(seq(moveKeys(six, keys(5), 1))).toBe("1,5,2,3,4,6");
  });
  it("bitişik olmayan iki sayfa kendi sıralarını koruyarak birlikte gider", () => {
    expect(seq(moveKeys(six, keys(2, 5), 0))).toBe("2,5,1,3,4,6");
    expect(seq(moveKeys(six, keys(2, 5), 6))).toBe("1,3,4,6,2,5");
    // Anahtarların veriliş sırası değil belgedeki sıra korunur.
    expect(seq(moveKeys(six, keys(5, 2), 0))).toBe("2,5,1,3,4,6");
    // Kümenin içine bırakmak: boşluk taşınanlar çıkarılmadan önceki listededir.
    expect(seq(moveKeys(six, keys(2, 5), 4))).toBe("1,3,4,2,5,6");
  });
  it("kendi yerine bırakmak sırayı değiştirmez", () => {
    expect(sameOrder(moveKeys(six, keys(3), 2), six)).toBe(true);
    expect(sameOrder(moveKeys(six, keys(3), 3), six)).toBe(true);
    expect(sameOrder(moveKeys(six, keys(3, 4), 3), six)).toBe(true);
  });
  it("her sonuç bir permütasyondur (Rust Reorder her sayfayı tam bir kez ister)", () => {
    for (let gap = -2; gap <= 9; gap++)
      for (const set of [keys(1), keys(6), keys(2, 5), keys(1, 3, 6), keys(1, 2, 3, 4, 5, 6), keys(9)])
        expect(isPermutation(moveKeys(six, set, gap), six)).toBe(true);
  });
});

describe("shiftKeys — Yukarı/Aşağı her seçili sayfayı bir sıra taşır", () => {
  const six = pages(1, 2, 3, 4, 5, 6);
  it("aradaki boşluklar korunur; Yukarı ile Aşağı birbirini geri alır", () => {
    expect(seq(shiftKeys(six, keys(3, 5), -1))).toBe("1,3,2,5,4,6");
    expect(seq(shiftKeys(shiftKeys(six, keys(3, 5), -1), keys(3, 5), 1))).toBe("1,2,3,4,5,6");
  });
  it("kenara dayanmış sayfa yerinde kalır, arkasındaki onun üstünden atlamaz", () => {
    expect(seq(shiftKeys(six, keys(1, 2), -1))).toBe("1,2,3,4,5,6");
    expect(seq(shiftKeys(six, keys(1, 3), -1))).toBe("1,3,2,4,5,6");
    expect(seq(shiftKeys(six, keys(6), 1))).toBe("1,2,3,4,5,6");
    expect(seq(shiftKeys(six, keys(4, 6), 1))).toBe("1,2,3,5,4,6");
  });
});

describe("seçim — tıklama, Shift aralığı, ⌘/Ctrl ekle-çıkar", () => {
  const six = pages(1, 2, 3, 4, 5, 6);
  it("düz tıklama tek sayfa seçer ve çapayı taşır", () => {
    expect(selectPages(six, keys(1, 2), "a#1", "a#4")).toEqual({ selected: keys(4), anchor: "a#4" });
  });
  it("⌘/Ctrl tek sayfayı ekler ya da çıkarır; sonuç belge sırasındadır", () => {
    expect(selectPages(six, keys(5), "a#5", "a#2", { toggle: true })).toEqual({ selected: keys(2, 5), anchor: "a#2" });
    expect(selectPages(six, keys(2, 5), "a#2", "a#5", { toggle: true })).toEqual({ selected: keys(2), anchor: "a#5" });
  });
  it("Shift çapadan aralık seçer, iki yönde de; çapa yerinde kalır", () => {
    expect(selectPages(six, keys(2), "a#2", "a#5", { range: true })).toEqual({ selected: keys(2, 3, 4, 5), anchor: "a#2" });
    expect(selectPages(six, keys(5), "a#5", "a#2", { range: true })).toEqual({ selected: keys(2, 3, 4, 5), anchor: "a#5" });
  });
  it("klavyeyle görüntüle (Enter/Boşluk) seçimi bozmaz; yalnız önizleme değişir", () => {
    // Seç kutularıyla 2, 4 ve 6'yı seçen klavye kullanıcısı 4'e bakmak için
    // görüntüle düğmesine Enter basar: seçim sessizce tek sayfaya inmemeli.
    expect(selectPages(six, keys(2, 4, 6), "a#6", "a#4", { keyboard: true })).toEqual({ selected: keys(2, 4, 6), anchor: "a#6" });
    expect(selectPages(six, keys(2, 4, 6), "a#6", "a#5", { keyboard: true, range: true })).toEqual({ selected: keys(2, 4, 6), anchor: "a#6" });
    expect(selectPages(six, [], "", "a#3", { keyboard: true })).toEqual({ selected: [], anchor: "" });
  });
});

describe("sürükleme geometrisi", () => {
  it("bırakma boşluğu sayfanın orta noktasına göre: üst yarı önüne, alt yarı arkasına", () => {
    const mids = [100, 300, 500];
    expect(dropGap(mids, 20)).toBe(0);
    expect(dropGap(mids, 101)).toBe(1);
    expect(dropGap(mids, 299)).toBe(1);
    expect(dropGap(mids, 520)).toBe(3);
  });
  it("kenarda kendiliğinden kaydırma: bölge dışında sıfır, kenara yaklaştıkça hızlı", () => {
    expect(edgeScroll(400, 0, 800)).toBe(0);
    expect(edgeScroll(30, 0, 800)).toBeLessThan(0);
    expect(edgeScroll(2, 0, 800)).toBeLessThan(edgeScroll(30, 0, 800));
    expect(edgeScroll(790, 0, 800)).toBeGreaterThan(0);
    expect(edgeScroll(-50, 0, 800)).toBe(-16);
  });
});

describe("Geri Al geçmişi ve kirli durum", () => {
  it("geçmiş sınırlıdır; en eski adım düşer", () => {
    let history: number[] = [];
    for (let i = 0; i < HISTORY_LIMIT + 5; i++) history = pushHistory(history, i);
    expect(history).toHaveLength(HISTORY_LIMIT);
    expect(history[0]).toBe(5);
  });
  it("geçmişten geri dönmek her taşımayı tersine, sırayla geri alır", () => {
    const six = pages(1, 2, 3, 4, 5, 6);
    let order = six;
    let history: (typeof six)[] = [];
    for (const [set, gap] of [[keys(6), 0], [keys(2, 4), 6], [keys(1), 3]] as const) {
      history = pushHistory(history, order);
      order = moveKeys(order, set, gap);
    }
    expect(seq(order)).toBe("6,3,1,5,2,4");
    const seen: string[] = [];
    while (history.length) {
      order = history[history.length - 1];
      history = history.slice(0, -1);
      seen.push(seq(order));
    }
    expect(seen).toEqual(["6,1,3,5,2,4", "6,1,2,3,4,5", "1,2,3,4,5,6"]);
    expect(isReordered(order, six)).toBe(false);
  });
  it("sıra kaynakla karşılaştırılır: taşıyıp geri getirmek değişiklik değildir", () => {
    const six = pages(1, 2, 3, 4, 5, 6);
    const moved = moveKeys(six, keys(1), 3);
    expect(isReordered(moved, six)).toBe(true);
    expect(isReordered(moveKeys(moved, keys(1), 0), six)).toBe(false);
  });
});

describe("duyuru", () => {
  it("tek sayfada kaynağı ve vardığı sıra, birden fazla sayfada sayısı söylenir", () => {
    expect(describeMove([{ page: 7 }], "drop", 2)).toBe("Kaynak s. 7 2. sıraya taşındı.");
    expect(describeMove([{ page: 7 }], "start", 1)).toBe("Kaynak s. 7 başa taşındı.");
    expect(describeMove([{ page: 2 }, { page: 5 }, { page: 6 }], "start", 1)).toBe("3 sayfa başa taşındı.");
    expect(describeMove([{ page: 2 }, { page: 5 }], "end", 5)).toBe("2 sayfa sona taşındı.");
    expect(describeMove([{ page: 2 }, { page: 5 }], "up", 1)).toBe("2 sayfa birer sıra yukarı taşındı.");
    expect(describeMove([{ page: 2 }, { page: 5 }], "drop", 4)).toBe("2 sayfa 4. sıraya taşındı.");
  });
});

describe("şeritte klavyeyle sayfadan sayfaya (Sırala, tek sekme durağı)", () => {
  const step = () => {
    const f = (pageOrder as Record<string, unknown>).stripStep;
    expect(typeof f, "stripStep pageOrder'da tanımlı olmalı").toBe("function");
    return f as (key: string, index: number, count: number) => number | null;
  };
  it("↑/↓ komşu sayfaya, Home/End uçlara gider; kenarda yerinde kalır", () => {
    expect(step()("ArrowDown", 2, 6)).toBe(3);
    expect(step()("ArrowUp", 2, 6)).toBe(1);
    expect(step()("ArrowUp", 0, 6)).toBe(0);
    expect(step()("ArrowDown", 5, 6)).toBe(5);
    expect(step()("Home", 4, 6)).toBe(0);
    expect(step()("End", 1, 6)).toBe(5);
  });
  it("başka tuşlar şeridin işi değildir", () => {
    for (const key of ["Enter", " ", "Tab", "ArrowLeft", "ArrowRight", "z"]) expect(step()(key, 2, 6)).toBeNull();
  });
});
