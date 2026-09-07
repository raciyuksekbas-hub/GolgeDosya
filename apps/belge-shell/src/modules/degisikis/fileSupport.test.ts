import { describe, expect, it } from "vitest";
import {
  DOCUMENT_FILE_ACCEPT, MAX_SELECTED_FILES, droppedFiles, fileExtension,
  firstDroppedFile, isSupportedDocument, planFileDrop,
} from "./fileSupport";

const file = (name: string) => new File([new Uint8Array([1])], name);

describe("dosya desteği", () => {
  it("kabul edilen uzantıları listeler", () => {
    expect(DOCUMENT_FILE_ACCEPT).toBe(".pdf,.doc,.docx,.udf");
  });

  it("bırakılan dosyaları okur", () => {
    const transfer = { files: [file("a.pdf"), file("b.docx")] } as unknown as DataTransfer;
    expect(firstDroppedFile(transfer)?.name).toBe("a.pdf");
    expect(droppedFiles(transfer).map((item) => item.name)).toEqual(["a.pdf", "b.docx"]);
  });

  it("uzantıyı ve desteği belirler", () => {
    expect(fileExtension("Sözleşme_v3.DOCX")).toBe("docx");
    expect(isSupportedDocument({ name: "a.udf" })).toBe(true);
    expect(isSupportedDocument({ name: "a.txt" })).toBe(false);
    expect(isSupportedDocument({ name: "uzantısız" })).toBe(false);
  });
});

describe("bırakma planı", () => {
  const empty = { baseFilled: false, revisedFilled: false };

  it("iki dosyayı sırayla temel ve değişik sürüme atar", () => {
    const plan = planFileDrop([file("v3.docx"), file("v4.docx")], empty);
    expect(plan.error).toBeUndefined();
    expect(plan.assignments.map((item) => [item.side, item.file.name]))
      .toEqual([["base", "v3.docx"], ["revised", "v4.docx"]]);
  });

  it("iki dosya bırakıldığında dolu yuvaları da yeniler", () => {
    const plan = planFileDrop([file("v3.docx"), file("v4.docx")], { baseFilled: true, revisedFilled: true });
    expect(plan.assignments).toHaveLength(2);
    expect(plan.error).toBeUndefined();
  });

  it("tek dosyayı ilk boş yuvaya koyar", () => {
    expect(planFileDrop([file("a.pdf")], empty).assignments[0].side).toBe("base");
    expect(planFileDrop([file("a.pdf")], { baseFilled: true, revisedFilled: false }).assignments[0].side).toBe("revised");
  });

  it("panel üzerine bırakılan tek dosya o panele gider", () => {
    const plan = planFileDrop([file("a.pdf")], { baseFilled: true, revisedFilled: true, target: "base" });
    expect(plan.assignments).toEqual([{ side: "base", file: expect.objectContaining({ name: "a.pdf" }) }]);
  });

  it("iki yuva doluyken hedefsiz tek dosyayı sessizce düşürmez", () => {
    const plan = planFileDrop([file("a.pdf")], { baseFilled: true, revisedFilled: true });
    expect(plan.assignments).toHaveLength(0);
    expect(plan.error).toContain("paneline bırakın");
  });

  it("üç dosyayı sessizce kırpmaz, sınırı bildirir", () => {
    const plan = planFileDrop([file("a.pdf"), file("b.pdf"), file("c.pdf")], empty);
    expect(plan.assignments).toHaveLength(0);
    expect(plan.error).toContain(String(MAX_SELECTED_FILES));
    expect(plan.error).toContain("3 dosya");
  });

  it("desteklenmeyen dosyayı adıyla reddeder", () => {
    const plan = planFileDrop([file("notlar.txt")], empty);
    expect(plan.assignments).toHaveLength(0);
    expect(plan.error).toContain("notlar.txt");
  });

  it("karışık kümede desteklenmeyeni bildirir ve hiçbirini yüklemez", () => {
    const plan = planFileDrop([file("v3.docx"), file("resim.png")], empty);
    expect(plan.assignments).toHaveLength(0);
    expect(plan.error).toContain("resim.png");
  });

  it("boş bırakmayı hata olarak döndürür", () => {
    expect(planFileDrop([], empty).error).toBeTruthy();
  });
});
