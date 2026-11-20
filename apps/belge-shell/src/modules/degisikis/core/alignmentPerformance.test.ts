import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import { buildComparisonViewModel } from "../viewModels/comparisonViewModel";
import baseline from "./alignmentBaseline.json";
import { compareDocuments } from "./compare";
import { makeBlocks } from "./normalize";
import { baselineScenarios, benchPair, seededRandom } from "./syntheticCorpus";
import type { ComparisonResult, DocumentBlock } from "./types";

/**
 * Hizalama hızlandırmasının iki sözleşmesi.
 *
 * 1. Eşdeğerlik: blok biçimlerinin önceden hesaplanması ve DP bandı hiçbir
 *    eşleştirme kararını değiştirmez. `alignmentBaseline.json` ESKİ motorun
 *    (her DP hücresinde metni yeniden normalize eden O(N·M) sürüm) aynı
 *    sentetik belgelerdeki çıktısıdır; yeni motor satırları, kartları, iz
 *    kaydını ve görünüm modelini bayt bayt aynı üretmelidir.
 * 2. Süre: eski motor her paragrafı düzenlenmiş 200 blokta 34–47 saniye
 *    sürüyor, 400 blokta 120 saniyede bitmiyordu ve bu süre boyunca arayüz
 *    donuyordu. Bütçeler CI için cömerttir; yeni motor bu makinede aynı işi
 *    yarım saniyenin altında yapıyor.
 */

// Vitest'in 5 sn'lik varsayılan zaman aşımı yavaş bir CI makinesinde bütçe
// iddialarından önce devreye girmesin; süre sözleşmesini testlerin kendisi söyler.
const HEAVY_TEST_TIMEOUT = 60_000;

const digest = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");

const pairing = (result: ComparisonResult) => result.rows
  .map((row) => `${row.kind[0]}${row.composite ? "*" : ""}:${row.base?.id ?? "-"}>${row.revised?.id ?? "-"}`)
  .join(" ");

/** Her kaynak bloğun satırlarda kaç kez göründüğü (birleşik satırlar dahil). */
function appearances(result: ComparisonResult, side: "base" | "revised"): Map<string, number> {
  const seen = new Map<string, number>();
  for (const row of result.rows) {
    const block = row[side];
    for (const id of block?.sourceBlockIds ?? (block ? [block.id] : [])) seen.set(id, (seen.get(id) ?? 0) + 1);
  }
  return seen;
}

function expectEveryBlockExactlyOnce(result: ComparisonResult, base: DocumentBlock[], revised: DocumentBlock[]) {
  const baseSeen = appearances(result, "base");
  const revisedSeen = appearances(result, "revised");
  expect([...baseSeen.keys()].sort()).toEqual(base.map((block) => block.id).sort());
  expect([...revisedSeen.keys()].sort()).toEqual(revised.map((block) => block.id).sort());
  expect([...baseSeen.values(), ...revisedSeen.values()].every((count) => count === 1)).toBe(true);
}

describe("hizalama hızlandırması eski motorla bayt bayt eşdeğerdir", () => {
  const scenarios = baselineScenarios();

  it("taban senaryoları üreticiyle aynı sırada ve boyutta", () => {
    expect(scenarios.map((scenario) => [scenario.name, scenario.base.length, scenario.revised.length]))
      .toEqual(baseline.scenarios.map((expected) => [expected.name, expected.baseBlocks, expected.revisedBlocks]));
  });

  it.each(baseline.scenarios)("$name: satırlar, kartlar ve eşleştirme kararları aynı", (expected) => {
    const scenario = scenarios.find((candidate) => candidate.name === expected.name)!;
    const result = compareDocuments(scenario.base, scenario.revised, { trace: true });
    // Önce okunabilir özet: eşleştirme kararı değiştiyse fark burada görünür.
    expect(pairing(result)).toBe(expected.pairing);
    expect(result.rows).toHaveLength(expected.rows);
    expect(result.changes).toHaveLength(expected.changes);
    expect(digest(result)).toBe(expected.digest);
    expect(digest(buildComparisonViewModel(result))).toBe(expected.modelDigest);
  }, HEAVY_TEST_TIMEOUT);
});

describe("hizalama süresi sınırlıdır", () => {
  it("her paragrafı düzenlenmiş 200 blok 5 saniyenin altında karşılaştırılır", () => {
    const { base, revised } = benchPair(200, 40, 1);
    const started = performance.now();
    const result = compareDocuments(base, revised);
    buildComparisonViewModel(result);
    const elapsed = performance.now() - started;
    expect(elapsed).toBeLessThan(5000);
    expectEveryBlockExactlyOnce(result, base, revised);
  }, HEAVY_TEST_TIMEOUT);

  it("çapasız 1200 bloklu boşluk bantla doğrusal kalır ve her blok tam bir kez görünür", () => {
    // Eski tam DP bu boyutta 1,4 milyon hücre demekti (dakikalar).
    const { base, revised } = benchPair(1200, 12, 1, 19);
    const started = performance.now();
    const result = compareDocuments(base, revised);
    expect(performance.now() - started).toBeLessThan(10000);
    expectEveryBlockExactlyOnce(result, base, revised);
  }, HEAVY_TEST_TIMEOUT);

  it("nadir sözcük yoksa bant payından uzun taşıma kaba eşleşir ama içerik kaybolmaz", () => {
    // Bant sınırının bilinen bedeli: kılavuz tohumu çıkmayan (dar dağarcıklı,
    // her paragrafı düzenlenmiş) bir boşlukta PAY'dan uzun taşıma temel
    // bandın dışında kalır. Nadir sözcüklü taşıma (`moved-run-90-of-260`)
    // yukarıdaki eşdeğerlik tabanında eski tam DP ile aynı çıkar.
    const random = seededRandom(3);
    const words = ["kira", "bedel", "süre", "fesih", "ödeme", "teslim", "tebligat", "ceza"];
    const texts = Array.from({ length: 260 }, () => `${Array.from({ length: 20 }, () => words[Math.floor(random() * words.length)]).join(" ")}.`);
    const edited = texts.map((text) => `${text.slice(0, -1)} ayrıca.`);
    const base = makeBlocks(texts.map((text) => ({ text })), "base");
    const revised = makeBlocks([...edited.slice(90), ...edited.slice(0, 90)].map((text) => ({ text })), "revised");
    const result = compareDocuments(base, revised);
    expectEveryBlockExactlyOnce(result, base, revised);
  }, HEAVY_TEST_TIMEOUT);
});
