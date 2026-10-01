// Sıkıştırma "kazanç yok" sonucu — kullanıcıya gerçek bayt sonucu.
//
// Sahada 9,4 MB'lık bir taramada motor 204 KB (%2,1) gerçek kazanç buldu;
// kullanıcıya "Bu belge zaten yeterince optimize" dendi. Bu cümle, motorun
// bulamadığı küçülmeden belgenin optimum olduğu sonucunu çıkarıyordu.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  NO_MEANINGFUL_SAVING,
  PREVIOUSLY_PROCESSED,
  noBenefitMessage,
  noBenefitDetail,
  noBenefitStatus,
} from "./noBenefitMessage";

const here = dirname(fileURLToPath(import.meta.url));
const TAIL =
  "küçültülebildi. Bu değer anlamlı küçülme eşiğinin altında kaldığı için çıktı oluşturulmadı.";

// Motorla PAYLAŞILAN tablo: crates/ekler-core/tests/compression_policy.rs ·
// `no_benefit_message_states_the_real_bytes`. Aşağıdaki test iki tablonun
// birebir aynı kaldığını da doğrular.
const CASES: [number, number, string][] = [
  [9_834_246, 9_625_452, `204 KB (%2,1) ${TAIL}`],
  [447_945, 447_540, `405 bayt (%0,1) ${TAIL}`],
  [1_048_576, 1_043_456, `5,0 KB (%0,5) ${TAIL}`],
  [4_000, 3_910, `90 bayt (%2,3) ${TAIL}`],
  [10_000_000, 9_989_248, `11 KB (%0,1) ${TAIL}`],
  [1_048_576, 1_048_556, NO_MEANINGFUL_SAVING],
  [1_048_576, 1_048_576, NO_MEANINGFUL_SAVING],
  [1_048_576, 1_053_576, NO_MEANINGFUL_SAVING],
  [0, 0, NO_MEANINGFUL_SAVING],
];

describe("kazanç yok cümlesi", () => {
  it.each(CASES)("%i → %i", (source, candidate, expected) => {
    const text = noBenefitMessage(source, candidate);
    expect(text).toBe(expected);
    expect(text).not.toMatch(/optimize/);
    expect(text).not.toMatch(/-/);
  });

  it("motorun tablosuyla birebir aynı örnekleri sınar", () => {
    const rust = readFileSync(
      resolve(here, "../../../../../crates/ekler-core/tests/compression_policy.rs"),
      "utf8",
    );
    const body = rust.slice(rust.indexOf("fn no_benefit_message_states_the_real_bytes"));
    const block = body.slice(body.indexOf("["), body.indexOf("];"));
    const rows = [
      ...block.matchAll(
        /\(\s*([\d_]+),\s*([\d_]+),\s*(?:format!\("([^"]*)"\)|NONE\.to_string\(\))\s*,?\s*\)/g,
      ),
    ].map(([, s, c, f]) => [
      Number(s.replaceAll("_", "")),
      Number(c.replaceAll("_", "")),
      f === undefined ? NO_MEANINGFUL_SAVING : f.replace("{TAIL}", TAIL),
    ]);
    expect(rows).toEqual(CASES);
  });
});

describe("alt bilgi ve bağlam", () => {
  // Alt satır sahada okunmuyordu: "Kaynak: 9,38 MB · Olası çıktı: 9,18 MB".
  // Artık belgenin boyutunu ve — gerçekten küçülebildiyse — sıkıştırılsaydı
  // ne olacağını söyler; sayılar başarı satırıyla aynı dilde.
  it("sahadaki sonuç: gerçek rakam, belgenin boyutu ve sıkıştırılsaydı", () => {
    expect(
      noBenefitStatus({ source_bytes: 9_834_246, candidate_bytes: 9_625_452 }),
    ).toBe(`204 KB (%2,1) ${TAIL}\nBelgenin boyutu: 9,4 MB · Sıkıştırılsaydı: yaklaşık 9,2 MB`);
  });

  it("aday büyüdüyse 'olası çıktı' karşılaştırması yazılmaz", () => {
    // Gerçek motor sonucu: vektör belge, marka bandıyla aday büyüdü.
    const vector = noBenefitStatus({ source_bytes: 1_123, candidate_bytes: 2_407 });
    expect(vector.split("\n")).toEqual([NO_MEANINGFUL_SAVING, "Belgenin boyutu: 1 KB"]);
    expect(vector).not.toMatch(/Olası çıktı|Sıkıştırılsaydı/);
    // Görselsiz, zaten sıkı belge: 797 KB → 798 KB.
    expect(noBenefitDetail(816_361, 817_437)).toBe("Belgenin boyutu: 797 KB");
  });

  it("GölgeDosya çıktısında 'daha önce işlenmiş' bağlamı eklenir, engel olmaz", () => {
    const text = noBenefitStatus({
      source_bytes: 447_945,
      candidate_bytes: 447_540,
      previously_processed: true,
    });
    expect(text.split("\n")).toEqual([
      `405 bayt (%0,1) ${TAIL}`,
      // Yuvarlanınca aynı görünen iki boyut yan yana yazılmaz.
      "Belgenin boyutu: 437 KB",
      PREVIOUSLY_PROCESSED,
    ]);
  });

  it("bağlam yalnız motor bildirdiğinde gösterilir", () => {
    for (const previously_processed of [false, undefined]) {
      expect(
        noBenefitStatus({ source_bytes: 1_317_864, candidate_bytes: 1_283_662, previously_processed }),
      ).not.toContain(PREVIOUSLY_PROCESSED);
    }
  });

  it("alt satır ortak boyut biçimini kullanır: KB tamsayı, MB tek ondalık, virgül", () => {
    expect(noBenefitDetail(95_133, 90_000)).toBe("Belgenin boyutu: 93 KB · Sıkıştırılsaydı: yaklaşık 88 KB");
    expect(noBenefitDetail(1_317_864, 1_283_662)).toBe("Belgenin boyutu: 1,3 MB · Sıkıştırılsaydı: yaklaşık 1,2 MB");
    expect(noBenefitDetail(10_485_760, 10_276_045)).toBe("Belgenin boyutu: 10,0 MB · Sıkıştırılsaydı: yaklaşık 9,8 MB");
  });
});

describe("kabuğa bağlanma", () => {
  const workspace = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");
  const branch = workspace.slice(
    workspace.indexOf("outcome.status === 'no_benefit'"),
    workspace.indexOf("return;", workspace.indexOf("outcome.status === 'no_benefit'")),
  );

  it("kazanç yok dalı durumu bu modülden kurar", () => {
    expect(branch).toContain("setStatus(noBenefitStatus(outcome))");
  });

  it("kanıtsız iddialar kabuktan kalktı", () => {
    expect(workspace).not.toContain("zaten yeterince optimize");
    // Eski "%3" kuralı panelde küçük harfle kalmıştı ("en az %3 küçülen"):
    // büyük/küçük harfe duyarlı koruma onu kaçırdı. Motor %2 + 100 KB ya da
    // 500 KB kazancı da kaydediyor; panel sayı vermeden gerçeği söyler.
    expect(workspace.toLocaleLowerCase("tr")).not.toContain("en az %3");
    expect(workspace).not.toMatch(/%3(?![\d,])/);
    expect(workspace).toContain("Küçülme çok azsa yeni dosya oluşturulmaz.");
    // Yeniden kodlanmayan görselin SEBEBİ sonuçta yok; uydurulmaz.
    expect(branch).not.toContain("renk uzayı");
  });
});
