// Dilekçe ekleri alan semantiği (saha turu §14–19).
import { describe, expect, it } from "vitest";
import {
  assignSource, assignmentLabel, autoDistribute, createExhibit, exhibitPageCount,
  exhibitsListText, moveExhibit, nameFromFile, removeExhibit, renameExhibit,
  unassignSource, unassignedSources,
} from "./annexState";
import type { Project } from "./types";

const src = (id: string, name: string, pages = 2) =>
  ({ id, path: `/x/${name}`, file_name: name, page_count: pages, is_signed: false });

const base = (): Project => ({
  version: "1.0.0", name: "Dava", created_at: "", updated_at: "",
  sources: [src("A", "ihtarname.pdf", 3), src("B", "dekont.pdf", 5),
            src("C", "yazisma.pdf", 4), src("D", "fatura.pdf", 2)],
  exhibits: [], target_size_bytes: 9_961_472,
  stamp_config: { enabled: true, position: "top_right", font_size: 10, margin_pt: 20, show_badge: true },
});

describe("§14 — atama durumu", () => {
  it("atanmamış kaynak 'Atanmadı' der", () => {
    expect(assignmentLabel(base(), "A")).toBe("Atanmadı");
  });

  it("atanan kaynak ek numarasını söyler", () => {
    let p = createExhibit(base(), "İhtarname", "e1");
    p = assignSource(p, "A", "e1");
    expect(assignmentLabel(p, "A")).toBe("Ek-1");
  });
});

describe("§15/§16 — atama", () => {
  it("bir ek birden fazla kaynak alır", () => {
    let p = createExhibit(base(), "Banka Dekontları", "e1");
    p = assignSource(p, "B", "e1");
    p = assignSource(p, "D", "e1");
    expect(p.exhibits[0].sources.map((s) => s.source_id)).toEqual(["B", "D"]);
    expect(exhibitPageCount(p, p.exhibits[0])).toBe(7);
  });

  it("kaynak başka eke taşınınca eskisinden ÇIKAR", () => {
    // Aksi hâlde aynı belge nihai pakette iki kez basılırdı.
    let p = createExhibit(createExhibit(base(), "Bir", "e1"), "İki", "e2");
    p = assignSource(p, "A", "e1");
    p = assignSource(p, "A", "e2");
    expect(p.exhibits[0].sources).toHaveLength(0);
    expect(p.exhibits[1].sources.map((s) => s.source_id)).toEqual(["A"]);
  });

  it("atama kaldırılabilir", () => {
    let p = assignSource(createExhibit(base(), "Bir", "e1"), "A", "e1");
    p = unassignSource(p, "A");
    expect(assignmentLabel(p, "A")).toBe("Atanmadı");
  });
});

describe("§17 — Otomatik Dağıt mevcut atamaları BOZMAZ", () => {
  // Talimattaki senaryo birebir: A -> Ek-1, B atanmadı, C -> Ek-3, D atanmadı.
  const arranged = () => {
    let p = base();
    p = createExhibit(p, "Bir", "e1");
    p = createExhibit(p, "İki", "e2");
    p = createExhibit(p, "Üç", "e3");
    p = assignSource(p, "A", "e1");
    p = assignSource(p, "C", "e3");
    return p;
  };

  it("elle yapılan atamalar korunur", () => {
    const after = autoDistribute(arranged(), (i) => `yeni-${i}`);
    expect(after.exhibits.find((e) => e.id === "e1")!.sources.map((s) => s.source_id)).toEqual(["A"]);
    expect(after.exhibits.find((e) => e.id === "e3")!.sources.map((s) => s.source_id)).toEqual(["C"]);
  });

  it("yalnız atanmamışlar için yeni ek açılır", () => {
    const after = autoDistribute(arranged(), (i) => `yeni-${i}`);
    expect(after.exhibits).toHaveLength(5);
    expect(unassignedSources(after)).toHaveLength(0);
    const fresh = after.exhibits.slice(3);
    expect(fresh.map((e) => e.sources[0].source_id)).toEqual(["B", "D"]);
    expect(fresh.map((e) => e.order)).toEqual([4, 5]);
  });

  it("yeni ekin adı dosya adından gelir, uzantısız", () => {
    const after = autoDistribute(arranged(), (i) => `yeni-${i}`);
    expect(after.exhibits[3].name).toBe("dekont");
    expect(nameFromFile("Sulh Sözleşmesi.udf")).toBe("Sulh Sözleşmesi");
  });

  it("atanmamış belge yoksa hiçbir şey değişmez", () => {
    let p = createExhibit(base(), "Hepsi", "e1");
    for (const id of ["A", "B", "C", "D"]) p = assignSource(p, id, "e1");
    expect(autoDistribute(p, (i) => `yeni-${i}`)).toEqual(p);
  });
});

describe("§18 — adlandırma ve sıra", () => {
  it("ek yeniden adlandırılır", () => {
    const p = renameExhibit(createExhibit(base(), "Ek", "e1"), "e1", "Faturalar");
    expect(p.exhibits[0].name).toBe("Faturalar");
  });

  it("sıra değişince numaralar yeniden kurulur", () => {
    let p = createExhibit(createExhibit(base(), "Bir", "e1"), "İki", "e2");
    p = moveExhibit(p, 1, -1);
    expect(p.exhibits.map((e) => [e.id, e.order])).toEqual([["e2", 1], ["e1", 2]]);
  });

  it("sınırın dışına taşınmaz", () => {
    const p = createExhibit(base(), "Bir", "e1");
    expect(moveExhibit(p, 0, -1)).toEqual(p);
    expect(moveExhibit(p, 0, 1)).toEqual(p);
  });

  it("ek silinince kaynakları kaybolmaz, 'Atanmadı'ya döner", () => {
    let p = assignSource(createExhibit(base(), "Bir", "e1"), "A", "e1");
    p = removeExhibit(p, "e1");
    expect(p.sources).toHaveLength(4);
    expect(assignmentLabel(p, "A")).toBe("Atanmadı");
  });
});

describe("§19 — EKLER listesi", () => {
  it("sıralı, sayfa sayılı ve motorla aynı biçimde", () => {
    let p = createExhibit(base(), "İhtarname", "e1");
    p = createExhibit(p, "Banka Dekontları", "e2");
    p = assignSource(p, "A", "e1");
    p = assignSource(p, "B", "e2");
    p = assignSource(p, "D", "e2");
    expect(exhibitsListText(p)).toBe(
      "EKLER\n\nEk-1: İhtarname — 3 sayfa\nEk-2: Banka Dekontları — 7 sayfa\n",
    );
  });
});
