import { describe, expect, it } from "vitest";
import { compareDocuments } from "./compare";
import { diagnoseSentinels } from "./diagnostics";
import { makeBlocks } from "./normalize";

describe("sentinel pipeline tanısı", () => {
  it("ilk kayıp aşamasını yalnız güvenli boolean ve indeks metadata'sıyla gösterir", () => {
    const baseExtracted = ["3.2 Ödeme 60 (Altmış) takvim günü içinde yapılır."];
    const revisedExtracted = ["3.2 Ödeme 3 (Üç) iş günü içinde yapılır ve aylık %5 gecikme faizi uygulanır."];
    const baseBlocks = makeBlocks(baseExtracted.map((text) => ({ text })), "base");
    const revisedBlocks = makeBlocks(revisedExtracted.map((text) => ({ text })), "revised");
    const comparison = compareDocuments(baseBlocks, revisedBlocks);
    const diagnostics = diagnoseSentinels({
      baseExtracted,
      revisedExtracted,
      baseBlocks,
      revisedBlocks,
      comparison,
      renderedRowIndices: new Set([0]),
    }, [
      { id: "payment_days", side: "revised", needle: "3 (Üç) iş günü" },
      { id: "missing_clause", side: "revised", needle: "anonim eksik hüküm" },
    ]);

    expect(diagnostics[0]).toMatchObject({
      stages: {
        extractedFinalText: true,
        documentBlock: true,
        alignedPair: true,
        wordDiff: true,
        userChange: true,
        renderer: true,
      },
      firstMissingStage: null,
      indexes: { extracted: 0, block: 0, row: 0, change: 0 },
    });
    expect(diagnostics[1]).toMatchObject({
      stages: { extractedFinalText: false },
      firstMissingStage: "extractedFinalText",
      indexes: { extracted: -1, block: -1, row: -1, change: -1 },
    });
    expect(JSON.stringify(diagnostics)).not.toContain("anonim eksik hüküm");
  });
});
