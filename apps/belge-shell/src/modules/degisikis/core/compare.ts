import { comparisonForm, displayLocation, looseMatchForm, looseMatchFromComparison } from "./normalize";
import { splitStructuralPrefix, type StructuralText } from "./structure";
import type { ComparisonResult, ComparisonRow, DocumentBlock, DocumentChange, DiffFragment } from "./types";
import { sourceTokens, tokenize, wordDiff, type WordDiffHunk } from "./wordDiff";

const ARTICLE_TITLE_ALIASES = new Map([
  ["cezai şartlar", "cezai şart"],
  ["ihtilafların çözümü", "uyuşmazlık"],
]);

function canonicalArticleTitle(text: string): string {
  const structural = splitStructuralPrefix(text);
  if (structural.prefix?.kind !== "article") return "";
  const normalized = looseMatchForm(structural.body.replace(/^[-–—.:)\s]+|[-–—.:)\s]+$/gu, ""));
  return ARTICLE_TITLE_ALIASES.get(normalized) ?? normalized;
}

interface LabelValue {
  label: string;
  value: string;
  hasSeparator: boolean;
}

function labelValue(text: string): LabelValue | undefined {
  const parts = text.split(/\s*(?:\t|\|)\s*/u);
  const hasSeparator = parts.length > 1;
  const label = comparisonForm(parts[0] ?? "");
  if (label.length < 4 || label.length > 48 || tokenize(label).length > 6) return undefined;
  return { label, value: comparisonForm(parts.slice(1).join(" | ")), hasSeparator };
}

/**
 * Hizalama çağrısına özel sözcük sözlüğü.
 *
 * Sözcükler tamsayıya çevrilir: DP hücresindeki küme kesişimi ve ortak dizi
 * taraması dizge yerine sayı karşılaştırır. Sözlük `align` çağrısıyla birlikte
 * atılır; modül düzeyinde büyüyen bir önbellek yoktur.
 */
class TokenIds {
  private readonly ids = new Map<string, number>();

  of(tokens: string[]): Int32Array {
    const result = new Int32Array(tokens.length);
    tokens.forEach((token, index) => {
      let id = this.ids.get(token);
      if (id === undefined) {
        id = this.ids.size;
        this.ids.set(token, id);
      }
      result[index] = id;
    });
    return result;
  }
}

/**
 * Hizalamanın tek bir bloktan okuduğu her şey, blok başına BİR KEZ.
 *
 * Eskiden DP'nin her hücresi `similarity`, `highConfidencePair` ve beş
 * birleşik grup denetimi için aynı blok metnini yeniden normalize ediyordu
 * (`comparisonForm`: yedi düzenli ifade geçişi + NFC; ardından
 * `looseMatchForm`, `tokenize`, `splitStructuralPrefix`). Profilde sürenin
 * ~%90'ı buydu: her paragrafı düzenlenmiş 200 bloklu bir belge 30–45 saniye
 * sürüyor, arayüz o süre boyunca donuyordu. Biçimler artık `align` çağrısı
 * başına bir kez hesaplanır. Karar kuralları ve eşikler AYNIDIR; satırlar,
 * kartlar ve eşleştirme kararları bayt bayt aynı kalır
 * (`alignmentPerformance.test.ts` eski motorun çıktısıyla karşılaştırır).
 */
interface BlockForms {
  block: DocumentBlock;
  /** `comparisonForm(text)` */
  comparison: string;
  /** Öneki atılmış gövdenin gevşek sözcükleri (sıralı, tekrarlı). */
  bodyTokens: Int32Array;
  /** Aynı sözcüklerin kümesi: Jaccard ve kapsama bunun üzerinden. */
  bodySet: Set<number>;
  articleTitle: string;
  labelValue: LabelValue | undefined;
  /** Ham metindeki sözcük sayısı: kısa blok kuralı buna bakar. */
  rawTokenCount: number;
  /** Yapısal kapsama denetiminin sözcükleri; yalnız gerekince hesaplanır. */
  containmentTokens?: string[];
}

function blockForms(block: DocumentBlock, ids: TokenIds): BlockForms {
  const comparison = comparisonForm(block.text);
  const structural = splitStructuralPrefix(block.text);
  // Öneki olmayan blokta gövde metnin kendisidir: gevşek biçim, elde olan
  // karşılaştırma biçiminden türetilir.
  const body = structural.prefix ? looseMatchForm(structural.body) : looseMatchFromComparison(comparison);
  const bodyTokens = ids.of(tokenize(body));
  return {
    block,
    comparison,
    bodyTokens,
    bodySet: new Set(bodyTokens),
    articleTitle: canonicalArticleTitle(block.text),
    labelValue: labelValue(block.text),
    rawTokenCount: tokenize(block.text).length,
  };
}

function containmentTokens(forms: BlockForms): string[] {
  forms.containmentTokens ??= tokenize(comparisonForm(splitStructuralPrefix(forms.block.text).body));
  return forms.containmentTokens;
}

function sharesLabelValueStructure(a: BlockForms, b: BlockForms): boolean {
  const left = a.labelValue;
  const right = b.labelValue;
  return Boolean(
    left && right
    && (left.hasSeparator || right.hasSeparator)
    && left.label === right.label
    && left.value !== right.value,
  );
}

function exactStructuralContainment(a: BlockForms, b: BlockForms): boolean {
  if (!a.block.location || a.block.location !== b.block.location) return false;
  const sameClause = Boolean(a.block.sourceClauseNumber && a.block.sourceClauseNumber === b.block.sourceClauseNumber);
  const sameListItem = Boolean(a.block.sourceListOrdinal && a.block.sourceListOrdinal === b.block.sourceListOrdinal);
  if (!sameClause && !sameListItem) return false;
  const left = containmentTokens(a);
  const right = containmentTokens(b);
  const shorter = left.length <= right.length ? left : right;
  const longer = left.length <= right.length ? right : left;
  if (shorter.length < 6 || longer.length <= shorter.length) return false;
  return shorter.every((token, index) => token === longer[index]);
}

/**
 * `similarity` ile `highConfidencePair`'in ortak kesin dalları: aynı metin,
 * aynı fıkrada sözcük sözcük kapsama, aynı etiketli "etiket | değer" satırı.
 * Biri tutarsa skor sabittir ve çift her iki ölçüte göre de güvenilirdir.
 */
function decisivePairScore(a: BlockForms, b: BlockForms): number | undefined {
  if (a.comparison === b.comparison) return 1;
  if (exactStructuralContainment(a, b)) return 0.96;
  if (a.block.kind === b.block.kind && sharesLabelValueStructure(a, b)) return 0.94;
  return undefined;
}

/** Gövde sözcük kümelerinin kesişimi; benzerlik de güven ölçütü de bu sayıyı kullanır. */
function sharedBodyTokens(a: BlockForms, b: BlockForms): number {
  let intersection = 0;
  for (const token of a.bodySet) if (b.bodySet.has(token)) intersection++;
  return intersection;
}

/**
 * En az `minimum` uzunlukta ortak ardışık sözcük dizisi var mı?
 *
 * `longestCommonRun(left, right) >= minimum` ile aynı yanıtı verir (aynı
 * tablo, aynı taşma davranışı); motor en uzun diziyi hiçbir yerde değer olarak
 * kullanmaz, yalnız eşikle karşılaştırır. Tarama eşik bulunduğu anda durur.
 */
function hasCommonRun(left: Int32Array, right: Int32Array, minimum: number): boolean {
  if (minimum <= 0) return true;
  const previous = new Uint16Array(right.length + 1);
  for (let i = 1; i <= left.length; i++) {
    let diagonal = 0;
    for (let j = 1; j <= right.length; j++) {
      const above = previous[j];
      previous[j] = left[i - 1] === right[j - 1] ? diagonal + 1 : 0;
      if (previous[j] >= minimum) return true;
      diagonal = above;
    }
  }
  return false;
}

/**
 * Kesin dallar (`decisivePairScore`) tutmadığında çiftin güvenilir olup
 * olmadığı. `intersection`, `sharedBodyTokens(a, b)`'dir. En uzun ortak dizi
 * ≥ 1 koşulu "en az bir ortak sözcük" demektir; ≥ 2 koşulu yalnız gerekince
 * taranır.
 */
function highConfidencePair(a: BlockForms, b: BlockForms, intersection: number, trustedSection: boolean): boolean {
  const left = a.block;
  const right = b.block;
  const shorterLength = Math.min(a.bodyTokens.length, b.bodyTokens.length);
  const jaccard = intersection / Math.max(1, a.bodySet.size + b.bodySet.size - intersection);
  const shorterCoverage = intersection / Math.max(1, Math.min(a.bodySet.size, b.bodySet.size));
  const lengthRatio = shorterLength / Math.max(1, Math.max(a.bodyTokens.length, b.bodyTokens.length));
  if (left.kind === "article" || right.kind === "article") {
    if (left.kind !== "article" || right.kind !== "article") return false;
    if (a.articleTitle && a.articleTitle === b.articleTitle) return true;
    return Boolean(left.label && left.label === right.label && jaccard >= 0.16 && shorterCoverage >= 0.28);
  }
  if (left.kind !== right.kind && (left.kind === "heading" || right.kind === "heading")) return false;
  if (shorterLength <= 2) {
    if (Math.max(a.rawTokenCount, b.rawTokenCount) >= 8) return false;
    return jaccard >= 0.3 && lengthRatio >= 0.34;
  }
  if (
    shorterLength <= 12
    && left.kind === right.kind
    && jaccard >= 0.12
    && shorterCoverage >= 0.2
    && lengthRatio >= 0.45
    && intersection >= 1
  ) return true;
  if (trustedSection) {
    return (jaccard >= 0.18 && shorterCoverage >= 0.4 && lengthRatio >= 0.18)
      || (shorterCoverage >= 0.62 && lengthRatio >= 0.12 && hasCommonRun(a.bodyTokens, b.bodyTokens, 2));
  }
  return (jaccard >= 0.2 && shorterCoverage >= 0.34 && lengthRatio >= 0.2)
    || (shorterCoverage >= 0.62 && lengthRatio >= 0.15 && hasCommonRun(a.bodyTokens, b.bodyTokens, 2));
}

/** Kesin dallar tutmadığında hücre benzerliği; `intersection`, `sharedBodyTokens(a, b)`'dir. */
function similarity(a: BlockForms, b: BlockForms, intersection: number): number {
  const union = a.bodySet.size + b.bodySet.size - intersection;
  let score = union ? intersection / union : 0;
  if (a.articleTitle && a.articleTitle === b.articleTitle) score = Math.max(score, 0.92);
  if (a.block.label && b.block.label && a.block.label === b.block.label) score = Math.max(score, 0.78);
  if (a.block.kind === b.block.kind) score += 0.06;
  else if (a.block.kind === "article" || b.block.kind === "article") score -= 0.25;
  return Math.max(0, Math.min(1, score));
}

type Pair = {
  base?: DocumentBlock;
  revised?: DocumentBlock;
  composite?: boolean;
  segmentationOnly?: boolean;
  structuralRewrite?: boolean;
  ignoredOrdinalPrefix?: { base: string; revised: string };
};

function combinedBlock(blocks: DocumentBlock[]): DocumentBlock {
  if (blocks.length === 1) return blocks[0];
  const first = blocks[0];
  return {
    ...first,
    id: blocks.map((block) => block.id).join("+"),
    text: blocks.map((block) => block.text).join(" "),
    sourceBlockIds: blocks.flatMap((block) => block.sourceBlockIds ?? [block.id]),
    sourceTexts: blocks.flatMap((block) => block.sourceTexts ?? [block.text]),
    sourceUnits: blocks.flatMap((block) => block.sourceUnits ?? [{
      id: block.id,
      text: block.text,
      kind: block.kind,
      order: block.order,
      location: block.location,
    }]),
  };
}

function articleContext(block: DocumentBlock): string | undefined {
  return block.location?.match(/^(Madde [^/]+?)(?:\s*\/|$)/u)?.[1].trim();
}

function localStructuralContext(block: DocumentBlock): string | undefined {
  if (block.appendixSection) return block.appendixSection;
  const article = articleContext(block);
  const parentArticle = article?.match(/^Madde\s+(\d+)/u)?.[1];
  return parentArticle ? `Madde ${parentArticle}` : article ?? block.appendixLabel;
}

function canJoin(blocks: DocumentBlock[]): boolean {
  if (blocks.length < 2 || blocks.some((block) => block.kind !== "paragraph" || splitStructuralPrefix(block.text).prefix)) return false;
  const contexts = new Set(blocks.map(articleContext).filter(Boolean));
  return contexts.size <= 1;
}

function segmentationText(blocks: DocumentBlock[]): string {
  return tokenize(looseMatchForm(blocks.map((block) => block.text)
    .join(" ")
    .replace(/(?:^|\s)[●•▪◦-](?=\s|$)/gu, " ")))
    .filter((token) => token !== "ve")
    .join(" ");
}

/** Bölümleme denetiminin ardışık bir blok grubundan okuduğu her şey. */
interface SegmentationForms {
  size: number;
  /** Yalnız paragraf/liste ve kaynakta ardışık. */
  supported: boolean;
  contextCount: number;
  context: string | undefined;
  text: string;
}

function segmentationForms(blocks: DocumentBlock[], text = segmentationText(blocks)): SegmentationForms {
  const contexts = new Set(blocks.map(articleContext).filter(Boolean));
  return {
    size: blocks.length,
    supported: blocks.every((block, index) => (
      ["paragraph", "list"].includes(block.kind)
      && (index === 0 || blocks[index - 1].order + 1 === block.order)
    )),
    contextCount: contexts.size,
    context: [...contexts][0],
    text,
  };
}

function sameSegmentation(base: SegmentationForms, revised: SegmentationForms): boolean {
  if (base.size === 1 && revised.size === 1) return false;
  if (!base.supported || !revised.supported) return false;
  if (base.contextCount > 1 || revised.contextCount > 1) return false;
  if (base.context && revised.context && base.context !== revised.context) return false;
  return base.text === revised.text;
}

function exactAdjacentSegmentation(base: DocumentBlock[], revised: DocumentBlock[]): boolean {
  if (base.length === 1 && revised.length === 1) return false;
  return sameSegmentation(segmentationForms(base), segmentationForms(revised));
}

function longestCommonRun(left: string[], right: string[]): number {
  const previous = new Uint16Array(right.length + 1);
  let longest = 0;
  for (let i = 1; i <= left.length; i++) {
    let diagonal = 0;
    for (let j = 1; j <= right.length; j++) {
      const above = previous[j];
      previous[j] = left[i - 1] === right[j - 1] ? diagonal + 1 : 0;
      longest = Math.max(longest, previous[j]);
      diagonal = above;
    }
  }
  return longest;
}

/**
 * Birleşik aday grubun (1–3 ardışık blok) birleştirilmiş metninden
 * türetilenler, grup başına bir kez. Birleşik metin blok biçimlerinden parça
 * parça kurulmaz: normalizasyon blok sınırını aşabilir (ör. iki bloğa yayılan
 * form boşluğu noktaları), bu yüzden eski kodun yaptığı gibi metin
 * birleştirilip normalize edilir.
 */
interface GroupForms extends SegmentationForms {
  joinable: boolean;
  firstContext: string | undefined;
  /** `comparisonForm(birleşik metin)` */
  comparison: string;
  /** `tokenize(looseMatchForm(birleşik metin))` */
  words: string[];
  /** Sözlükteki sayılarıyla sözcükler ve tekrar sayıları; yalnız gerekince. */
  joined?: { tokens: Int32Array; counts: Map<number, number> };
}

/** `segmentationText`'in sildiği madde imlerinden biri metinde geçiyor mu? */
const SEGMENTATION_MARK = /[●•▪◦-]/u;

function groupForms(
  blocks: DocumentBlock[],
  comparison = comparisonForm(blocks.map((block) => block.text).join(" ")),
): GroupForms {
  const words = tokenize(looseMatchFromComparison(comparison));
  // İm yoksa `segmentationText` aynı sözcüklerden "ve" atılarak elde edilir;
  // varsa (tire dahil) eski yoldan hesaplanır.
  const segmentation = blocks.some((block) => SEGMENTATION_MARK.test(block.text))
    ? segmentationText(blocks)
    : words.filter((token) => token !== "ve").join(" ");
  return {
    ...segmentationForms(blocks, segmentation),
    joinable: canJoin(blocks),
    firstContext: articleContext(blocks[0]),
    comparison,
    words,
  };
}

function joinedTokens(group: GroupForms, ids: TokenIds): { tokens: Int32Array; counts: Map<number, number> } {
  if (!group.joined) {
    const tokens = ids.of(group.words);
    const counts = new Map<number, number>();
    for (const token of tokens) counts.set(token, (counts.get(token) ?? 0) + 1);
    group.joined = { tokens, counts };
  }
  return group.joined;
}

function compositeSimilarity(base: GroupForms, revised: GroupForms, trustedSection: boolean, ids: TokenIds): number | undefined {
  if (sameSegmentation(base, revised)) return 1;
  if (Math.max(base.size, revised.size) > 2) return undefined;
  if ((base.size > 1 && !base.joinable) || (revised.size > 1 && !revised.joinable)) return undefined;
  const baseContext = base.firstContext;
  const revisedContext = revised.firstContext;
  if (!trustedSection && baseContext && revisedContext && baseContext !== revisedContext) return undefined;
  if (base.comparison === revised.comparison) return 1;
  const shorter = Math.min(base.words.length, revised.words.length);
  if (shorter < 3) return undefined;
  const left = joinedTokens(base, ids);
  const right = joinedTokens(revised, ids);
  // Çok kümeli kesişim: her sözcük iki yandaki tekrar sayısının küçüğü kadar sayılır.
  let shared = 0;
  for (const [token, count] of right.counts) shared += Math.min(count, left.counts.get(token) ?? 0);
  const shorterCoverage = shared / shorter;
  const longerCoverage = shared / Math.max(base.words.length, revised.words.length);
  const requiredRun = Math.min(8, Math.max(2, Math.floor(shorter * 0.2)));
  if (shorterCoverage < 0.72 || longerCoverage < 0.4 || !hasCommonRun(left.tokens, right.tokens, requiredRun)) return undefined;
  return shorterCoverage * 0.55 + longerCoverage * 0.45;
}

/**
 * A composite group scores its similarity over the joined text and is then weighted by the
 * number of blocks it spans, so absorbing a short neighbour can outscore two exact one-to-one
 * pairs even when every token of that neighbour is surplus. Reject a grouping that reaches
 * across a block boundary to take in text which already has its own exact counterpart
 * immediately outside the group: that counterpart is the more natural pairing, and borrowing
 * it would also carry unchanged text into somebody else's replacement.
 */
function borrowsExactNeighbour(
  base: BlockForms[],
  revised: BlockForms[],
  i: number,
  j: number,
  baseCount: number,
  revisedCount: number,
): boolean {
  const sameText = (left?: BlockForms, right?: BlockForms) =>
    Boolean(left && right && left.comparison === right.comparison);
  const outside = (blocks: BlockForms[], start: number, end: number) => [blocks[start - 1], blocks[end]];
  const baseOutside = outside(base, i - baseCount, i);
  const revisedOutside = outside(revised, j - revisedCount, j);
  return revised.slice(j - revisedCount, j).some((block) => baseOutside.some((neighbour) => sameText(block, neighbour)))
    || base.slice(i - baseCount, i).some((block) => revisedOutside.some((neighbour) => sameText(block, neighbour)));
}

/**
 * Kesin çapalar arasındaki bir boşlukta tam DP'nin koştuğu en büyük boyut.
 * Boşluk İKİ yanda da bunu aşarsa DP bir bantla sınırlanır (bkz.
 * `alignmentBand`).
 */
const FULL_ALIGNMENT_LIMIT = 160;
/** Bandın kılavuz yolun (ve blok sayısı farkının) iki yanına taşma payı (blok). */
const ALIGNMENT_BAND_SLACK = 64;
/** Kılavuzlu bandın en çok hücre sayısı; aşılırsa yalnız temel bant kullanılır. */
const GUIDED_BAND_CELL_LIMIT = 600_000;

/**
 * Bandın kılavuz yolu.
 *
 * Boşluğun iki yanında da yalnız BİR blokta geçen gövde sözcükleri (özel ad,
 * tutar, nadir terim) aynı temel–değişik çiftinde en az iki kez buluşuyorsa o
 * çift bir tohumdur. Tohumların i ve j'de kesin artan en uzun zinciri
 * (0,0)'dan (N,M)'ye kırık bir yol çizer. Tohum hiçbir eşleşmeyi ZORLAMAZ:
 * yalnız DP'nin bakacağı alanı genişletir, karar yine DP'nindir. Dar
 * dağarcıklı ya da her şeyi değişmiş metinde tohum çıkmaz; yol köşegendir.
 */
function guidePath(base: BlockForms[], revised: BlockForms[]): Array<[number, number]> {
  const owners = (forms: BlockForms[]) => {
    const owner = new Map<number, number>();
    forms.forEach((form, index) => {
      for (const token of form.bodySet) owner.set(token, owner.has(token) ? -1 : index);
    });
    return owner;
  };
  const left = owners(base);
  const right = owners(revised);
  const votes = new Map<number, number>();
  for (const [token, i] of left) {
    const j = right.get(token);
    if (i < 0 || j === undefined || j < 0) continue;
    const key = i * revised.length + j;
    votes.set(key, (votes.get(key) ?? 0) + 1);
  }
  // Aynı satırdan iki tohum zincire girmesin: satır içinde j azalan sıralanır.
  const seeds = [...votes]
    .filter(([, count]) => count >= 2)
    .map(([key]): [number, number] => [Math.floor(key / revised.length), key % revised.length])
    .sort((a, b) => a[0] - b[0] || b[1] - a[1]);
  const tails: number[] = [];
  const previous = new Int32Array(seeds.length).fill(-1);
  seeds.forEach(([, j], index) => {
    let lower = 0;
    let upper = tails.length;
    while (lower < upper) {
      const middle = (lower + upper) >> 1;
      if (seeds[tails[middle]][1] < j) lower = middle + 1;
      else upper = middle;
    }
    if (lower > 0) previous[index] = tails[lower - 1];
    tails[lower] = index;
  });
  const chain: Array<[number, number]> = [];
  for (let index = tails.length ? tails[tails.length - 1] : -1; index >= 0; index = previous[index]) {
    chain.push([seeds[index][0] + 1, seeds[index][1] + 1]);
  }
  return [[0, 0], ...chain.reverse(), [base.length, revised.length]];
}

/**
 * Hizalama DP'sinin her satırda hesapladığı sütun aralığı [first[i], last[i]].
 *
 * Tam DP O(N·M)'dir: hücre başına iş ne kadar azaltılsa da çapasız 1000 bloklu
 * bir boşluk milyon hücre demektir. Yanlardan biri `FULL_ALIGNMENT_LIMIT` blok
 * ya da daha küçükse DP tamdır (en çok `FULL_ALIGNMENT_LIMIT`·max(N, M) hücre).
 * İki yan da büyükse DP iki bandın birleşimiyle sınırlanır (d = j − i,
 * PAY = `ALIGNMENT_BAND_SLACK`):
 *
 * - Temel bant: min(0, M − N) − PAY ≤ d ≤ max(0, M − N) + PAY. Eklenen ya da
 *   silinen bloklar yolu köşegenden tek yöne kaydırır; bu kayma ne kadar büyük
 *   olursa olsun (en çok M − N) temel bandın içindedir, bölümleme farkından
 *   doğan sürekli kayma da öyle. Hücre sayısı (N + 1)·(|M − N| + 2·PAY + 1).
 * - Kılavuzlu bant: `guidePath`'in her parçası boyunca, parçanın uç
 *   köşegenlerinin PAY ötesine kadar. Bir yanda silinip öbür yanda geri
 *   eklenen, yani PAY bloktan uzun bir TAŞIMA yolu temel bandın dışına
 *   iter; nadir sözcükleri taşınan dizinin yeni yerini gösterdiği sürece
 *   kılavuz o yolu bandın içine alır. Kılavuzlu bant
 *   `GUIDED_BAND_CELL_LIMIT` hücreyi aşarsa bırakılır.
 *
 * Satır aralıkları tekdüze artandır ve her hücrenin bant içinde bir öncülü
 * vardır; (0,0) ile (N,M) her zaman banttadır. Bant dışına düşen bir yol
 * kaybolmaz, yalnız o kısım silme + ekleme olarak gösterilir: her blok yine
 * tam bir kez görünür.
 *
 * Güvenlik payı ölçüldü: temel bant HER boşluğa PAY = 1 ile zorlandığında
 * bile modülün bütün testleri geçti; sentetik taban senaryoları (çapasız 200+
 * bloklu boşluklar, taşınan 12 paragraflık dizi, bölümleme kayması) PAY = 8
 * ile eski tam DP'nin çıktısını bayt bayt verdi. Seçilen PAY bunun sekiz
 * katıdır (`alignmentPerformance.test.ts`).
 */
function alignmentBand(base: BlockForms[], revised: BlockForms[]): { first: number[]; last: number[] } {
  const rows = base.length;
  const columns = revised.length;
  const clamp = (value: number) => Math.max(0, Math.min(columns, value));
  if (rows <= FULL_ALIGNMENT_LIMIT || columns <= FULL_ALIGNMENT_LIMIT) {
    return { first: Array<number>(rows + 1).fill(0), last: Array<number>(rows + 1).fill(columns) };
  }
  const drift = columns - rows;
  const low = Math.min(0, drift) - ALIGNMENT_BAND_SLACK;
  const high = Math.max(0, drift) + ALIGNMENT_BAND_SLACK;
  const first: number[] = [];
  const last: number[] = [];
  for (let i = 0; i <= rows; i++) {
    first.push(i + low);
    last.push(i + high);
  }
  const guide = guidePath(base, revised);
  for (let k = 1; k < guide.length; k++) {
    const [fromRow, fromColumn] = guide[k - 1];
    const [toRow, toColumn] = guide[k];
    const lowest = Math.min(fromColumn - fromRow, toColumn - toRow) - ALIGNMENT_BAND_SLACK;
    const highest = Math.max(fromColumn - fromRow, toColumn - toRow) + ALIGNMENT_BAND_SLACK;
    for (let i = fromRow; i <= toRow; i++) {
      first[i] = Math.min(first[i], i + lowest);
      last[i] = Math.max(last[i], i + highest);
    }
  }
  for (let i = 0; i <= rows; i++) {
    first[i] = clamp(first[i]);
    last[i] = clamp(last[i]);
  }
  for (let i = rows - 1; i >= 0; i--) first[i] = Math.min(first[i], first[i + 1]);
  for (let i = 1; i <= rows; i++) last[i] = Math.max(last[i], last[i - 1]);
  let cells = 0;
  for (let i = 0; i <= rows; i++) cells += last[i] - first[i] + 1;
  if (cells <= GUIDED_BAND_CELL_LIMIT) return { first, last };
  return {
    first: first.map((_, i) => clamp(i + low)),
    last: last.map((_, i) => clamp(i + high)),
  };
}

/** [temel blok sayısı, değişik blok sayısı, hamle kodu]; sıra skor eşitliğinde kazananı belirler. */
const GROUPED_MOVES: ReadonlyArray<readonly [number, number, number]> = [
  [1, 2, 4], [2, 1, 5], [2, 2, 6], [1, 3, 7], [3, 1, 8],
];

function align(base: DocumentBlock[], revised: DocumentBlock[], trustedSection = false): Pair[] {
  const ids = new TokenIds();
  const baseForms = base.map((block) => blockForms(block, ids));
  const revisedForms = revised.map((block) => blockForms(block, ids));
  // groups[boyut − 1][son] = [son − boyut + 1, son] aralığındaki ardışık bloklar.
  // Tek bloklu grubun birleşik metni bloğun kendisidir.
  const groupsOf = (blocks: DocumentBlock[], forms: BlockForms[]) => [1, 2, 3].map((size) => blocks.map((_, end) => (
    end + 1 < size ? undefined
      : size === 1 ? groupForms([blocks[end]], forms[end].comparison)
        : groupForms(blocks.slice(end + 1 - size, end + 1))
  )));
  const baseGroups = groupsOf(base, baseForms);
  const revisedGroups = groupsOf(revised, revisedForms);

  // Bant içindeki hücreler satır satır sıkıştırılmış dizide tutulur; tam DP'de
  // düzen eskisiyle aynıdır (satır başına M + 1 hücre).
  const band = alignmentBand(baseForms, revisedForms);
  const firstColumn = (i: number) => band.first[i];
  const lastColumn = (i: number) => band.last[i];
  const rowStart = [0];
  for (let i = 0; i <= base.length; i++) rowStart.push(rowStart[i] + lastColumn(i) - firstColumn(i) + 1);
  const cell = (i: number, j: number) => (
    j < firstColumn(i) || j > lastColumn(i) ? -1 : rowStart[i] + j - firstColumn(i)
  );
  const scores = new Float32Array(rowStart[base.length + 1]);
  const moves = new Uint8Array(rowStart[base.length + 1]);
  // Bant dışındaki hücre hiçbir yola katılamaz.
  const scoreAt = (i: number, j: number) => {
    const at = cell(i, j);
    return at < 0 ? -Infinity : scores[at];
  };
  const gap = -0.42;
  for (let i = 1; i <= base.length && firstColumn(i) === 0; i++) {
    scores[cell(i, 0)] = i * gap;
    moves[cell(i, 0)] = 2;
  }
  for (let j = 1; j <= lastColumn(0); j++) {
    scores[cell(0, j)] = j * gap;
    moves[cell(0, j)] = 3;
  }
  for (let i = 1; i <= base.length; i++) {
    const a = baseForms[i - 1];
    for (let j = Math.max(1, firstColumn(i)); j <= lastColumn(i); j++) {
      const b = revisedForms[j - 1];
      const decisive = decisivePairScore(a, b);
      const intersection = decisive === undefined ? sharedBodyTokens(a, b) : 0;
      const sim = decisive ?? similarity(a, b, intersection);
      const diagonal = scoreAt(i - 1, j - 1) + (sim * 1.65 - 0.62);
      const up = scoreAt(i - 1, j) + gap;
      const left = scoreAt(i, j - 1) + gap;
      let bestScore: number;
      let bestMove: number;
      if (
        diagonal >= up && diagonal >= left
        && (decisive !== undefined || highConfidencePair(a, b, intersection, trustedSection))
      ) {
        bestScore = diagonal;
        bestMove = 1;
      } else if (up >= left) {
        bestScore = up;
        bestMove = 2;
      } else {
        bestScore = left;
        bestMove = 3;
      }
      for (const [baseCount, revisedCount, move] of GROUPED_MOVES) {
        if (i < baseCount || j < revisedCount) continue;
        const before = scoreAt(i - baseCount, j - revisedCount);
        if (before === -Infinity) continue;
        const groupedSimilarity = compositeSimilarity(
          baseGroups[baseCount - 1][i - 1]!,
          revisedGroups[revisedCount - 1][j - 1]!,
          trustedSection,
          ids,
        );
        if (groupedSimilarity === undefined) continue;
        // Exact segmentation and identical joined text score 1 and are always genuine
        // regroupings of the same content; only inexact groups can borrow a neighbour.
        if (groupedSimilarity < 1 && borrowsExactNeighbour(baseForms, revisedForms, i, j, baseCount, revisedCount)) continue;
        const groupedScore = before + (groupedSimilarity * 1.65 - 0.62) * Math.max(baseCount, revisedCount);
        if (groupedScore > bestScore + 0.02) {
          bestScore = groupedScore;
          bestMove = move;
        }
      }
      const at = cell(i, j);
      scores[at] = bestScore;
      moves[at] = bestMove;
    }
  }

  const result: Pair[] = [];
  let i = base.length;
  let j = revised.length;
  while (i || j) {
    const move = moves[cell(i, j)];
    if (move === 1) result.push({ base: base[--i], revised: revised[--j] });
    else if (move === 4) {
      const baseBlocks = base.slice(i - 1, i);
      const revisedBlocks = revised.slice(j - 2, j);
      result.push({
        base: combinedBlock(baseBlocks),
        revised: combinedBlock(revisedBlocks),
        composite: true,
        segmentationOnly: exactAdjacentSegmentation(baseBlocks, revisedBlocks),
      });
      i--;
      j -= 2;
    } else if (move === 5) {
      const baseBlocks = base.slice(i - 2, i);
      const revisedBlocks = revised.slice(j - 1, j);
      result.push({
        base: combinedBlock(baseBlocks),
        revised: combinedBlock(revisedBlocks),
        composite: true,
        segmentationOnly: exactAdjacentSegmentation(baseBlocks, revisedBlocks),
      });
      i -= 2;
      j--;
    } else if (move === 6) {
      const baseBlocks = base.slice(i - 2, i);
      const revisedBlocks = revised.slice(j - 2, j);
      result.push({
        base: combinedBlock(baseBlocks),
        revised: combinedBlock(revisedBlocks),
        composite: true,
        segmentationOnly: exactAdjacentSegmentation(baseBlocks, revisedBlocks),
      });
      i -= 2;
      j -= 2;
    } else if (move === 7) {
      const baseBlocks = base.slice(i - 1, i);
      const revisedBlocks = revised.slice(j - 3, j);
      result.push({
        base: combinedBlock(baseBlocks),
        revised: combinedBlock(revisedBlocks),
        composite: true,
        segmentationOnly: exactAdjacentSegmentation(baseBlocks, revisedBlocks),
      });
      i--;
      j -= 3;
    } else if (move === 8) {
      const baseBlocks = base.slice(i - 3, i);
      const revisedBlocks = revised.slice(j - 1, j);
      result.push({
        base: combinedBlock(baseBlocks),
        revised: combinedBlock(revisedBlocks),
        composite: true,
        segmentationOnly: exactAdjacentSegmentation(baseBlocks, revisedBlocks),
      });
      i -= 3;
      j--;
    }
    else if (move === 2 || j === 0) result.push({ base: base[--i] });
    else result.push({ revised: revised[--j] });
  }
  return result.reverse();
}

function alignWithExactAnchors(base: DocumentBlock[], revised: DocumentBlock[], trustedSection = false): Pair[] {
  const uniquePositions = (blocks: DocumentBlock[]) => {
    const positions = new Map<string, number[]>();
    blocks.forEach((block, index) => {
      const text = comparisonForm(block.text);
      if (text.length < 24 && !/^\[•\]\s+\p{L}/u.test(text)) return;
      positions.set(text, [...(positions.get(text) ?? []), index]);
    });
    return new Map([...positions].flatMap(([text, matches]) => matches.length === 1 ? [[text, matches[0]]] : []));
  };
  const left = uniquePositions(base);
  const right = uniquePositions(revised);
  const candidates = [...left].flatMap(([text, baseIndex]) => {
    const revisedIndex = right.get(text);
    return revisedIndex === undefined ? [] : [{ baseIndex, revisedIndex }];
  }).sort((a, b) => a.baseIndex - b.baseIndex);
  if (!candidates.length) return align(base, revised, trustedSection);

  const length = new Uint16Array(candidates.length);
  const previous = new Int32Array(candidates.length).fill(-1);
  let best = 0;
  candidates.forEach((candidate, index) => {
    length[index] = 1;
    for (let prior = 0; prior < index; prior++) {
      if (candidates[prior].revisedIndex < candidate.revisedIndex && length[prior] + 1 > length[index]) {
        length[index] = length[prior] + 1;
        previous[index] = prior;
      }
    }
    if (length[index] > length[best]) best = index;
  });
  const anchors: typeof candidates = [];
  for (let index = best; index >= 0; index = previous[index]) {
    anchors.push(candidates[index]);
    if (previous[index] < 0) break;
  }
  anchors.reverse();

  const result: Pair[] = [];
  let baseStart = 0;
  let revisedStart = 0;
  for (const anchor of anchors) {
    result.push(...align(base.slice(baseStart, anchor.baseIndex), revised.slice(revisedStart, anchor.revisedIndex), trustedSection));
    result.push({ base: base[anchor.baseIndex], revised: revised[anchor.revisedIndex] });
    baseStart = anchor.baseIndex + 1;
    revisedStart = anchor.revisedIndex + 1;
  }
  result.push(...align(base.slice(baseStart), revised.slice(revisedStart), trustedSection));
  return result;
}

function sentenceAnchors(text: string): Set<string> {
  const sentences = text.split(/(?<=[.!?])\s+(?=[“"'\p{L}\p{N}])/u);
  return new Set(sentences
    .map((sentence) => comparisonForm(sentence))
    .filter((sentence) => sentence.length >= 48 && tokenize(sentence).length >= 7));
}

function sharesExactSentence(left: DocumentBlock, right: DocumentBlock): boolean {
  const rightSentences = sentenceAnchors(right.text);
  for (const sentence of sentenceAnchors(left.text)) if (rightSentences.has(sentence)) return true;
  return false;
}

function sharesStrongLocalSentence(left: DocumentBlock, right: DocumentBlock): boolean {
  return localSentences(left).some((baseSentence) => localSentences(right).some((revisedSentence) => (
    sentenceAnchorScore(baseSentence.text, revisedSentence.text) !== undefined
  )));
}

function sameLocalStructuralContext(left: DocumentBlock, right: DocumentBlock): boolean {
  const leftContext = localStructuralContext(left);
  const rightContext = localStructuralContext(right);
  return Boolean(leftContext && leftContext === rightContext);
}

function bodyTokens(text: string): string[] {
  return tokenize(looseMatchForm(splitStructuralPrefix(text).body.replace(/^[-●•▪◦]\s*/u, "")));
}

function structuralVocabulary(block: DocumentBlock): string[] {
  const units = block.sourceUnits ?? [{ text: block.text }];
  return [...new Set(units.flatMap((unit) => bodyTokens(unit.text)).filter((token) => !["şu", "ve"].includes(token)))].sort();
}

function sameStructuralVocabulary(left: DocumentBlock, right: DocumentBlock): boolean {
  const leftVocabulary = structuralVocabulary(left);
  const rightVocabulary = structuralVocabulary(right);
  return leftVocabulary.length >= 4 && leftVocabulary.join("\0") === rightVocabulary.join("\0");
}

function containsExactBody(container: DocumentBlock, candidate: DocumentBlock): boolean {
  const haystack = bodyTokens(container.text);
  const needle = bodyTokens(candidate.text);
  if (needle.length < 3 || needle.length > haystack.length) return false;
  return haystack.some((_, offset) => needle.every((token, index) => haystack[offset + index] === token));
}

function stabilizeBoundaryRelocations(pairs: Pair[]): Pair[] {
  const stabilized: Pair[] = [];
  for (let index = 0; index < pairs.length;) {
    const first = pairs[index];
    const second = pairs[index + 1];
    const sourceCount = (block: DocumentBlock) => block.id.split("+").length;
    const combinedTextIsEqual = first?.base && first.revised && second?.base && second.revised
      && comparisonForm(`${first.base.text} ${second.base.text}`) === comparisonForm(`${first.revised.text} ${second.revised.text}`);
    const hasBoundaryShift = first?.base && first.revised && second?.base && second.revised
      && (comparisonForm(first.base.text) !== comparisonForm(first.revised.text)
        || comparisonForm(second.base.text) !== comparisonForm(second.revised.text));
    const canStabilize = first?.base && first.revised && second?.base && second.revised
      && first.base.order + sourceCount(first.base) === second.base.order
      && first.revised.order + sourceCount(first.revised) === second.revised.order
      && ((combinedTextIsEqual && hasBoundaryShift)
        || sharesExactSentence(first.base, second.revised)
        || sharesExactSentence(second.base, first.revised));
    if (
      first?.revised && !first.base && second?.base && second.revised
      && sameLocalStructuralContext(first.revised, second.revised)
      && containsExactBody(second.base, first.revised)
    ) {
      stabilized.push({
        base: second.base,
        revised: combinedBlock([first.revised, second.revised]),
        composite: true,
        segmentationOnly: exactAdjacentSegmentation([second.base], [first.revised, second.revised]),
      });
      index += 2;
      continue;
    }
    if (
      first?.base && !first.revised && second?.base && second.revised
      && sameLocalStructuralContext(first.base, second.base)
      && containsExactBody(second.revised, first.base)
    ) {
      stabilized.push({
        base: combinedBlock([first.base, second.base]),
        revised: second.revised,
        composite: true,
        segmentationOnly: exactAdjacentSegmentation([first.base, second.base], [second.revised]),
      });
      index += 2;
      continue;
    }
    if (first?.base && first.revised) {
      const removed: DocumentBlock[] = [];
      let cursor = index + 1;
      while (
        cursor < pairs.length && removed.length < 3
        && pairs[cursor].base && !pairs[cursor].revised
        && sameLocalStructuralContext(first.base, pairs[cursor].base!)
        && containsExactBody(first.revised, pairs[cursor].base!)
      ) {
        removed.push(pairs[cursor].base!);
        cursor++;
      }
      if (removed.length) {
        const baseBlocks = [first.base, ...removed];
        stabilized.push({
          base: combinedBlock(baseBlocks),
          revised: first.revised,
          composite: true,
          segmentationOnly: exactAdjacentSegmentation(baseBlocks, [first.revised]),
          structuralRewrite: true,
        });
        index = cursor;
        continue;
      }
      const added: DocumentBlock[] = [];
      cursor = index + 1;
      while (
        cursor < pairs.length && added.length < 3
        && pairs[cursor].revised && !pairs[cursor].base
        && sameLocalStructuralContext(first.revised, pairs[cursor].revised!)
        && containsExactBody(first.base, pairs[cursor].revised!)
      ) {
        added.push(pairs[cursor].revised!);
        cursor++;
      }
      if (added.length) {
        const revisedBlocks = [first.revised, ...added];
        stabilized.push({
          base: first.base,
          revised: combinedBlock(revisedBlocks),
          composite: true,
          segmentationOnly: exactAdjacentSegmentation([first.base], revisedBlocks),
          structuralRewrite: true,
        });
        index = cursor;
        continue;
      }
    }
    if (canStabilize) {
      stabilized.push({
        base: combinedBlock([first.base!, second.base!]),
        revised: combinedBlock([first.revised!, second.revised!]),
        composite: true,
      });
      index += 2;
    } else if (
      first?.base && first.revised && second?.base && !second.revised
      && first.base.order + sourceCount(first.base) === second.base.order
      && sameLocalStructuralContext(first.base, second.base)
      && sharesStrongLocalSentence(second.base, first.revised)
    ) {
      stabilized.push({
        base: combinedBlock([first.base, second.base]),
        revised: first.revised,
        composite: true,
      });
      index += 2;
    } else if (
      first?.base && !first.revised && second?.base && second.revised
      && first.base.order + sourceCount(first.base) === second.base.order
      && sameLocalStructuralContext(first.base, second.base)
      && sharesStrongLocalSentence(first.base, second.revised)
    ) {
      stabilized.push({
        base: combinedBlock([first.base, second.base]),
        revised: second.revised,
        composite: true,
      });
      index += 2;
    } else if (
      first?.base && first.revised && second?.revised && !second.base
      && first.revised.order + sourceCount(first.revised) === second.revised.order
      && sameLocalStructuralContext(first.revised, second.revised)
      && sharesStrongLocalSentence(first.base, second.revised)
    ) {
      stabilized.push({
        base: first.base,
        revised: combinedBlock([first.revised, second.revised]),
        composite: true,
      });
      index += 2;
    } else if (
      first?.revised && !first.base && second?.base && second.revised
      && first.revised.order + sourceCount(first.revised) === second.revised.order
      && sameLocalStructuralContext(first.revised, second.revised)
      && sharesStrongLocalSentence(second.base, first.revised)
    ) {
      stabilized.push({
        base: second.base,
        revised: combinedBlock([first.revised, second.revised]),
        composite: true,
      });
      index += 2;
    } else {
      stabilized.push(first);
      index++;
    }
  }
  return stabilized;
}

function stabilizeLocalExactRelocations(pairs: Pair[]): Pair[] {
  const paired = new Map<number, number>();
  const candidatesFor = (index: number): number[] => {
    const source = pairs[index].base ?? pairs[index].revised;
    if (!source || Boolean(pairs[index].base) === Boolean(pairs[index].revised)) return [];
    return pairs.flatMap((candidate, candidateIndex) => {
      if (Math.abs(candidateIndex - index) > 4 || candidateIndex === index) return [];
      const target = pairs[index].base ? candidate.revised : candidate.base;
      if (!target || Boolean(candidate.base) === Boolean(candidate.revised)) return [];
      const sourceBody = looseMatchForm(splitStructuralPrefix(source.text).body);
      const targetBody = looseMatchForm(splitStructuralPrefix(target.text).body);
      const exactIdentity = comparisonForm(source.text) === comparisonForm(target.text)
        || (tokenize(sourceBody).length >= 3 && sourceBody === targetBody);
      return exactIdentity
        && sameLocalStructuralContext(source, target) ? [candidateIndex] : [];
    });
  };
  pairs.forEach((pair, index) => {
    if (Boolean(pair.base) === Boolean(pair.revised) || paired.has(index)) return;
    const candidates = candidatesFor(index);
    if (candidates.length !== 1 || candidatesFor(candidates[0]).length !== 1) return;
    paired.set(index, candidates[0]);
    paired.set(candidates[0], index);
  });
  const consumed = new Set<number>();
  return pairs.flatMap((pair, index) => {
    if (consumed.has(index)) return [];
    const counterpartIndex = paired.get(index);
    if (counterpartIndex === undefined) return [pair];
    consumed.add(counterpartIndex);
    const counterpart = pairs[counterpartIndex];
    return [{
      base: pair.base ?? counterpart.base,
      revised: pair.revised ?? counterpart.revised,
      segmentationOnly: true,
    }];
  });
}

interface OrdinalText { prefix: string; body: string; number: number }

function ordinalText(block: DocumentBlock): OrdinalText | undefined {
  if (block.kind === "table") {
    const match = block.text.match(/^(\s*(\d+)\s*\|\s*)([\s\S]+)$/u);
    return match ? { prefix: match[1], number: Number(match[2]), body: match[3] } : undefined;
  }
  const structural = splitStructuralPrefix(block.text);
  if (structural.prefix?.kind !== "paragraph" || !/^\d+$/u.test(structural.prefix.number)) return undefined;
  return { prefix: structural.prefix.raw, number: Number(structural.prefix.number), body: structural.body };
}

function stabilizeOrdinalShifts(pairs: Pair[]): Pair[] {
  let expectedDelta: number | undefined;
  return pairs.map((pair) => {
    const baseOrdinal = pair.base ? ordinalText(pair.base) : undefined;
    const revisedOrdinal = pair.revised ? ordinalText(pair.revised) : undefined;
    if (!pair.base && revisedOrdinal) {
      expectedDelta = 1;
      return pair;
    }
    if (!pair.revised && baseOrdinal) {
      expectedDelta = -1;
      return pair;
    }
    if (pair.base && pair.revised && baseOrdinal && revisedOrdinal && expectedDelta !== undefined) {
      if (revisedOrdinal.number - baseOrdinal.number === expectedDelta) {
        return { ...pair, ignoredOrdinalPrefix: { base: baseOrdinal.prefix, revised: revisedOrdinal.prefix } };
      }
      expectedDelta = undefined;
      return pair;
    }
    if (pair.base && pair.revised && comparisonForm(pair.base.text) === comparisonForm(pair.revised.text)) return pair;
    expectedDelta = undefined;
    return pair;
  });
}

interface ArticleSection {
  start: number;
  end: number;
  title: string;
  number: string;
  blocks: DocumentBlock[];
}

function articleSections(blocks: DocumentBlock[]): ArticleSection[] {
  const starts = blocks.flatMap((block, index) => block.kind === "article" ? [index] : []);
  return starts.map((start, index) => {
    const end = starts[index + 1] ?? blocks.length;
    return {
      start,
      end,
      title: canonicalArticleTitle(blocks[start].text),
      number: splitStructuralPrefix(blocks[start].text).prefix?.number ?? "",
      blocks: blocks.slice(start, end),
    };
  }).filter((section) => section.title.length >= 3);
}

function uniqueSectionsByTitle(sections: ArticleSection[]): Map<string, ArticleSection> {
  const grouped = new Map<string, ArticleSection[]>();
  for (const section of sections) grouped.set(section.title, [...(grouped.get(section.title) ?? []), section]);
  return new Map([...grouped].flatMap(([title, matches]) => matches.length === 1 ? [[title, matches[0]]] : []));
}

function highConfidenceTitleOverlap(left: ArticleSection, right: ArticleSection): boolean {
  const leftTokens = tokenize(left.title);
  const rightTokens = tokenize(right.title);
  const leftSet = new Set(leftTokens);
  const rightSet = new Set(rightTokens);
  let shared = 0;
  for (const token of leftSet) if (rightSet.has(token)) shared++;
  const jaccard = shared / Math.max(1, leftSet.size + rightSet.size - shared);
  const shorterCoverage = shared / Math.max(1, Math.min(leftSet.size, rightSet.size));
  return jaccard >= 0.24 && shorterCoverage >= 0.5 && longestCommonRun(leftTokens, rightTokens) >= 2;
}

function alignWithArticleSections(base: DocumentBlock[], revised: DocumentBlock[]): Pair[] {
  const baseSections = articleSections(base);
  const revisedSections = articleSections(revised);
  const baseByTitle = uniqueSectionsByTitle(baseSections);
  const revisedByTitle = uniqueSectionsByTitle(revisedSections);
  const exactMatched = [...revisedByTitle].flatMap(([title, revisedSection]) => {
    const baseSection = baseByTitle.get(title);
    return baseSection && baseSection.number !== revisedSection.number ? [{ baseSection, revisedSection }] : [];
  });
  const usedBase = new Set(exactMatched.map(({ baseSection }) => baseSection.start));
  const usedRevised = new Set(exactMatched.map(({ revisedSection }) => revisedSection.start));
  const overlapMatched = revisedSections.flatMap((revisedSection) => {
    if (usedRevised.has(revisedSection.start)) return [];
    const candidates = baseSections.filter((baseSection) => !usedBase.has(baseSection.start) && highConfidenceTitleOverlap(baseSection, revisedSection));
    if (candidates.length !== 1) return [];
    const baseSection = candidates[0];
    const reciprocal = revisedSections.filter((candidate) => !usedRevised.has(candidate.start) && highConfidenceTitleOverlap(baseSection, candidate));
    if (reciprocal.length !== 1 || baseSection.number === revisedSection.number) return [];
    usedBase.add(baseSection.start);
    usedRevised.add(revisedSection.start);
    return [{ baseSection, revisedSection }];
  });
  const matched = [...exactMatched, ...overlapMatched];
  if (!matched.length) return stabilizeBoundaryRelocations(alignWithExactAnchors(base, revised));

  const matchedBase = new Set(matched.flatMap(({ baseSection }) => baseSection.blocks.map((block) => block.id)));
  const matchedRevised = new Set(matched.flatMap(({ revisedSection }) => revisedSection.blocks.map((block) => block.id)));
  const remaining = alignWithExactAnchors(
    base.filter((block) => !matchedBase.has(block.id)),
    revised.filter((block) => !matchedRevised.has(block.id)),
  );
  const relocated = matched
    .map(({ baseSection, revisedSection }) => ({
      revisedOrder: revisedSection.blocks[0].order,
      pairs: stabilizeBoundaryRelocations(alignWithExactAnchors(baseSection.blocks, revisedSection.blocks, true)),
    }))
    .sort((left, right) => left.revisedOrder - right.revisedOrder);

  const result: Pair[] = [];
  let sectionIndex = 0;
  for (const pair of remaining) {
    const revisedOrder = pair.revised?.order;
    while (sectionIndex < relocated.length && revisedOrder !== undefined && relocated[sectionIndex].revisedOrder < revisedOrder) {
      result.push(...relocated[sectionIndex++].pairs);
    }
    result.push(pair);
  }
  while (sectionIndex < relocated.length) result.push(...relocated[sectionIndex++].pairs);
  return stabilizeBoundaryRelocations(result);
}

function quote(text: string, max = 72): string {
  const clean = text.length > max ? `${text.slice(0, max - 1)}…` : text;
  return `“${clean}”`;
}

function clippedExcerpt(fragments: DiffFragment[], kind: "added" | "removed", max = 108): string {
  const first = fragments.findIndex((fragment) => fragment.kind === kind);
  let last = -1;
  for (let index = fragments.length - 1; index >= 0; index--) {
    if (fragments[index].kind === kind) {
      last = index;
      break;
    }
  }
  if (first < 0 || last < 0) return "";
  const raw = joinTokens(tokenize(fragments.slice(first, last + 1).map((fragment) => fragment.text).join(" ")));
  const leading = first > 0 ? "…" : "";
  const trailing = last < fragments.length - 1 ? "…" : "";
  const available = Math.max(24, max - leading.length - trailing.length);
  if (raw.length <= available) return `${leading}${raw}${trailing}`;
  const startLength = Math.floor((available - 3) * 0.42);
  const endLength = available - 3 - startLength;
  return `${leading}${raw.slice(0, startLength).trimEnd()} … ${raw.slice(-endLength).trimStart()}${trailing}`;
}

function joinTokens(tokens: string[]): string {
  return tokens.join(" ").replace(/\s+([,.;:!?%])/g, "$1");
}

interface Segment {
  changed: string;
  beforeEqual: string;
  afterEqual: string;
}

function changeSegments(fragments: DiffFragment[], kind: "added" | "removed"): Segment[] {
  const segments: Segment[] = [{ changed: "", beforeEqual: "", afterEqual: "" }];
  for (const fragment of fragments) {
    const current = segments.at(-1)!;
    if (fragment.kind === "equal") {
      current.afterEqual = fragment.text;
      segments.push({ changed: "", beforeEqual: fragment.text, afterEqual: "" });
    } else if (fragment.kind === kind) {
      current.changed += `${current.changed ? " " : ""}${fragment.text}`;
    }
  }
  return segments;
}

type CandidateKind = "date" | "money" | "number" | "reference" | "word";

interface Candidate {
  kind: CandidateKind;
  display: string;
  start: number;
  end: number;
  position: number;
  numeric?: number;
  currency?: string;
}

const DATE_RE = /^(?:0?[1-9]|[12]\d|3[01])[./-](?:0?[1-9]|1[0-2])[./-](?:\d{2}|\d{4})$/u;
const REFERENCE_RE = /^\d+(?:\/\d+)+$/u;
const NUMBER_RE = /^\d+(?:[.,:]\d+)*$/u;
const CURRENCY_RE = /^(TL|TRY|₺|USD|EUR|€|\$)(?:['’].*)?$/iu;

function numericValue(token: string): number | undefined {
  let normalized = token;
  if (normalized.includes(",")) normalized = normalized.replace(/\./g, "").replace(",", ".");
  else if (/^\d{1,3}(?:\.\d{3})+$/.test(normalized)) normalized = normalized.replace(/\./g, "");
  const value = Number(normalized);
  return Number.isFinite(value) ? value : undefined;
}

function candidates(text: string, afterEqual: string): { items: Candidate[]; tokens: string[] } {
  const tokens = tokenize(text);
  const trailingUnitToken = tokenize(afterEqual)[0];
  const trailingUnit = trailingUnitToken?.match(CURRENCY_RE)?.[1]?.toLocaleUpperCase("tr-TR");
  const items: Candidate[] = [];
  for (let index = 0; index < tokens.length; index++) {
    const token = tokens[index];
    const position = tokens.length <= 1 ? 0.5 : index / (tokens.length - 1);
    if (DATE_RE.test(token)) {
      items.push({ kind: "date", display: token, start: index, end: index + 1, position });
      continue;
    }
    if (REFERENCE_RE.test(token)) {
      items.push({ kind: "reference", display: token, start: index, end: index + 1, position });
      continue;
    }
    if (NUMBER_RE.test(token)) {
      const inlineUnit = tokens[index + 1]?.match(CURRENCY_RE)?.[1]?.toLocaleUpperCase("tr-TR");
      const currency = inlineUnit ?? trailingUnit;
      items.push({
        kind: currency ? "money" : "number",
        display: `${token}${currency ? ` ${currency}` : ""}`,
        start: index,
        end: index + (inlineUnit ? 2 : 1),
        position,
        numeric: numericValue(token),
        currency,
      });
      if (inlineUnit) index++;
      continue;
    }
    if (/^\p{L}+(?:['’]\p{L}+)*$/u.test(token) && token.length <= 24 && !CURRENCY_RE.test(token)) {
      items.push({ kind: "word", display: token, start: index, end: index + 1, position });
    }
  }
  return { items, tokens };
}

function compatible(base: Candidate, revised: Candidate): boolean {
  if (base.kind !== revised.kind) return false;
  if (base.kind === "reference") return false;
  if (base.kind === "date") return true;
  if (base.kind === "word") {
    const ratio = Math.max(base.display.length, revised.display.length) / Math.max(1, Math.min(base.display.length, revised.display.length));
    return ratio <= 2.5;
  }
  if (base.kind === "money" && base.currency !== revised.currency) return false;
  if (base.numeric === undefined || revised.numeric === undefined) return false;
  if (base.numeric === revised.numeric) return true;
  const baseDigits = base.display.replace(/\D/g, "").length;
  const revisedDigits = revised.display.replace(/\D/g, "").length;
  const ratio = Math.abs(revised.numeric / (base.numeric || 1));
  return Math.abs(baseDigits - revisedDigits) <= 1 && ratio >= 0.1 && ratio <= 10;
}

function replacementPairs(baseItems: Candidate[], revisedItems: Candidate[]): Array<[Candidate, Candidate]> {
  const result: Array<[Candidate, Candidate]> = [];
  const hasTypedValue = [...baseItems, ...revisedItems].some((item) => item.kind !== "word");
  for (const kind of ["date", "money", "number", "word"] as CandidateKind[]) {
    const left = baseItems.filter((item) => item.kind === kind);
    const right = revisedItems.filter((item) => item.kind === kind);
    if (!left.length || left.length !== right.length) continue;
    if (kind === "word" && (left.length !== 1 || hasTypedValue)) continue;
    if (left.length === 1) {
      if (compatible(left[0], right[0])) result.push([left[0], right[0]]);
      continue;
    }
    left.forEach((item, index) => {
      if (Math.abs(item.position - right[index].position) <= 0.25 && compatible(item, right[index])) result.push([item, right[index]]);
    });
  }
  return result;
}

function remainingText(tokens: string[], used: Set<number>): string {
  return joinTokens(tokens.filter((_, index) => !used.has(index))).trim();
}

function isSubstantialRewrite(base: DiffFragment[], revised: DiffFragment[]): boolean {
  const removed = base.filter((fragment) => fragment.kind === "removed").flatMap((fragment) => tokenize(fragment.text)).length;
  const added = revised.filter((fragment) => fragment.kind === "added").flatMap((fragment) => tokenize(fragment.text)).length;
  const baseTotal = base.flatMap((fragment) => tokenize(fragment.text)).length;
  const revisedTotal = revised.flatMap((fragment) => tokenize(fragment.text)).length;
  return removed >= 5 && added >= 5 && removed + added >= 14
    && (removed / Math.max(1, baseTotal) >= 0.28 || added / Math.max(1, revisedTotal) >= 0.28);
}

interface PhraseReplacement {
  base: string;
  revised: string;
}

// Parça metni kaynaktan kesilir; aynı tutar bir belgede "18.000 TL", ötekinde
// "18.000TL" yazılmış olabilir. Sözcük farkı ikisini eşit sayar ve panelde
// işaretlenecek bir şey olmaz; özet de "değişti" dememeli. Özet ham metni
// karşılaştırınca satır değişmiş sayılıyor, rayda işaretsiz bir değişiklik
// çıkıyordu. Bu yüzden özetin tutar, süre ve mahkeme ifadeleri sözcüklerin
// tek biçimli birleşiminde aranır ve sözcüklerle karşılaştırılır (parça metni
// kaynaktan kesilmeden önceki davranış); yalnız gösterilen metin kaynaktan
// gelir.

/** Eşitlik anahtarı: sözcük birimleri, aralarındaki boşluk hariç. */
function wordsKey(text: string): string {
  return tokenize(comparisonForm(text)).join(" ");
}

const NO_SPACE_BEFORE_RE = /^[,.;:!?%)\]}»”’]$/u;
const NO_SPACE_AFTER_RE = /^[([{«“‘]$/u;

interface PhraseText {
  /** Sözcükler tek biçimde birleşmiş: "18.000 TL", "olup 30 (otuz) gün". */
  words: string;
  /** `words` içindeki bir aralığın kaynaktaki yazımı. */
  shown(start: number, end: number): string;
}

/**
 * İfadeler `words` üzerinde aranır: "olup30 (otuz) gün" de "olup 30 (otuz)
 * gün" de aynı süreyi, aynı sırada verir; böylece yalnız boşluğu farklı iki
 * yazımda ifadelerin sayısı ve eşleşmesi değişmez.
 */
function phraseText(fragments: DiffFragment[]): PhraseText {
  const source = fragments.map((fragment) => fragment.text).join(" ");
  const toSource: number[] = [];
  let words = "";
  let fragmentStart = 0;
  fragments.forEach((fragment, fragmentIndex) => {
    if (fragmentIndex > 0) words += " ";
    let previous: string | undefined;
    for (const token of sourceTokens(fragment.text)) {
      if (previous !== undefined && !NO_SPACE_BEFORE_RE.test(token.text) && !NO_SPACE_AFTER_RE.test(previous)) words += " ";
      for (let offset = 0; offset < token.text.length; offset++) toSource[words.length + offset] = fragmentStart + token.start + offset;
      words += token.text;
      previous = token.text;
    }
    fragmentStart += fragment.text.length + 1;
  });
  return {
    words,
    shown: (start, end) => {
      const from = toSource[start];
      const to = toSource[end - 1];
      return from === undefined || to === undefined ? words.slice(start, end) : source.slice(from, to + 1);
    },
  };
}

const DURATION_RE = /(?<![\p{L}\p{N}])\d+\s*\(\s*\p{L}+\s*\)\s*(?:(?:iş|takvim)\s+)?(?:gün(?:ü|dür)?|hafta(?:dan)?|ay|yıl)(?!\p{L})/giu;

function durationReplacements(base: DiffFragment[], revised: DiffFragment[]): PhraseReplacement[] {
  const collect = (fragments: DiffFragment[]) => {
    const text = phraseText(fragments);
    return Array.from(text.words.matchAll(DURATION_RE), (match) => ({
      display: text.shown(match.index, match.index + match[0].length)
        .replace(/\s+/gu, " ").replace(/\(\s+/gu, "(").replace(/\s+\)/gu, ")").trim(),
      position: match.index / Math.max(1, text.words.length),
    }));
  };
  const left = collect(base);
  const right = collect(revised);
  if (!left.length || left.length !== right.length) return [];
  return left.flatMap((item, index) => {
    const counterpart = right[index];
    if (
      wordsKey(item.display) === wordsKey(counterpart.display)
      || (left.length > 1 && Math.abs(item.position - counterpart.position) > 0.25)
    ) return [];
    return [{ base: item.display, revised: counterpart.display }];
  });
}

function coveredByPhraseReplacement(base: Candidate, revised: Candidate, replacements: PhraseReplacement[]): boolean {
  const baseValue = looseMatchForm(base.display);
  const revisedValue = looseMatchForm(revised.display);
  return replacements.some((replacement) => {
    const baseWords = ` ${looseMatchForm(replacement.base)} `;
    const revisedWords = ` ${looseMatchForm(replacement.revised)} `;
    return baseWords.includes(` ${baseValue} `) && revisedWords.includes(` ${revisedValue} `);
  });
}

function typedValueReplacements(base: DiffFragment[], revised: DiffFragment[]): PhraseReplacement[] {
  const leftSegments = changeSegments(base, "removed");
  const rightSegments = changeSegments(revised, "added");
  return leftSegments.flatMap((segment, index) => {
    const counterpart = rightSegments[index];
    if (!segment.changed || !counterpart?.changed) return [];
    const left = candidates(segment.changed, segment.afterEqual).items;
    const right = candidates(counterpart.changed, counterpart.afterEqual).items;
    return replacementPairs(left, right).flatMap(([oldValue, newValue]) => oldValue.kind === "word"
      ? []
      : [{ base: oldValue.display, revised: newValue.display }]);
  });
}

const MONEY_PHRASE_RE = /\d+(?:[.,:]\d+)*\s*(?:TL|TRY|₺|USD|EUR|€|\$)/giu;

function moneyReplacements(base: DiffFragment[], revised: DiffFragment[]): PhraseReplacement[] {
  const collect = (fragments: DiffFragment[]) => {
    const text = phraseText(fragments);
    return Array.from(
      text.words.matchAll(MONEY_PHRASE_RE),
      (match) => text.shown(match.index, match.index + match[0].length).replace(/\s+/gu, " ").trim(),
    );
  };
  const left = collect(base);
  const right = collect(revised);
  if (!left.length || left.length !== right.length) return [];
  return left.flatMap((value, index) => wordsKey(value) === wordsKey(right[index])
    ? []
    : [{ base: value, revised: right[index] }]);
}

function venueReplacements(base: DiffFragment[], revised: DiffFragment[]): PhraseReplacement[] {
  const collect = (fragments: DiffFragment[]) => {
    const text = phraseText(fragments);
    const match = text.words.match(/İstanbul(?:\s+Anadolu)?(?=\s+Mahkemeleri)/iu);
    return match?.index === undefined ? undefined : text.shown(match.index, match.index + match[0].length).replace(/\s+/gu, " ");
  };
  const left = collect(base);
  const right = collect(revised);
  return left && right && wordsKey(left) !== wordsKey(right) ? [{ base: left, revised: right }] : [];
}

function summarizeBodyDiff(base: DiffFragment[], revised: DiffFragment[]): string[] {
  const durationPairs = durationReplacements(base, revised);
  const venuePairs = venueReplacements(base, revised);
  const moneyPairs = moneyReplacements(base, revised);
  const typedPairs = typedValueReplacements(base, revised).filter((pair) => !coveredByPhraseReplacement(
    { kind: pair.base.match(CURRENCY_RE) ? "money" : "number", display: pair.base, start: 0, end: 1, position: 0.5 },
    { kind: pair.revised.match(CURRENCY_RE) ? "money" : "number", display: pair.revised, start: 0, end: 1, position: 0.5 },
    durationPairs,
  ));
  // Tutar iki kaynaktan gelir: tutar ifadesi belgedeki yazımla ("18.500TL"),
  // sayı adayı sözcüklerden ("18.500 TL"). Aynı değişiklik iki kez yazılmasın;
  // ilk gelen, yani belgedeki yazım kalır.
  const priorityPairs = [...durationPairs, ...moneyPairs, ...typedPairs, ...venuePairs].filter((pair, index, all) => (
    all.findIndex((candidate) => wordsKey(candidate.base) === wordsKey(pair.base)
      && wordsKey(candidate.revised) === wordsKey(pair.revised)) === index
  ));
  const prioritySummaries = priorityPairs.map((pair) => `${quote(pair.base)} → ${quote(pair.revised)}`).slice(0, 3);
  if (isSubstantialRewrite(base, revised)) {
    return [...prioritySummaries, ...[
      `− ${quote(clippedExcerpt(base, "removed"), 120)}`,
      `+ ${quote(clippedExcerpt(revised, "added"), 120)}`,
    ]].slice(0, 3);
  }
  const baseSegments = changeSegments(base, "removed");
  const revisedSegments = changeSegments(revised, "added");
  const contractions = baseSegments.flatMap((segment, index) => {
    if (!segment.changed || revisedSegments[index]?.changed) return [];
    const changed = tokenize(segment.changed);
    const anchor = tokenize(segment.beforeEqual).at(-1);
    if (!anchor || !/^\p{L}+$/u.test(anchor) || changed.length < 1 || changed.length > 2 || changed.some((token) => !/^\p{L}+$/u.test(token))) return [];
    return [{ index, summary: `${quote(joinTokens([anchor, ...changed]))} → ${quote(anchor)}` }];
  });
  const summaries: string[] = [...new Set([...prioritySummaries, ...contractions.map((item) => item.summary)])].slice(0, 3);
  const segmentCount = Math.max(baseSegments.length, revisedSegments.length);

  for (let index = 0; index < segmentCount && summaries.length < 3; index++) {
    const left = baseSegments[index] ?? { changed: "", beforeEqual: "", afterEqual: "" };
    const right = revisedSegments[index] ?? { changed: "", beforeEqual: "", afterEqual: "" };
    if (!left.changed && !right.changed) continue;
    const baseCandidates = candidates(left.changed, left.afterEqual);
    const revisedCandidates = candidates(right.changed, right.afterEqual);
    const pairs = replacementPairs(baseCandidates.items, revisedCandidates.items);
    const usedBase = new Set<number>();
    const usedRevised = new Set<number>();
    if (contractions.some((item) => item.index === index)) {
      baseCandidates.tokens.forEach((_, token) => usedBase.add(token));
    }
    for (const [oldValue, newValue] of pairs) {
      if (summaries.length >= 3) break;
      if (!coveredByPhraseReplacement(oldValue, newValue, durationPairs)) {
        const replacement = `${quote(oldValue.display)} → ${quote(newValue.display)}`;
        if (!summaries.some((line) => wordsKey(line) === wordsKey(replacement))) summaries.push(replacement);
      }
      for (let token = oldValue.start; token < oldValue.end; token++) usedBase.add(token);
      for (let token = newValue.start; token < newValue.end; token++) usedRevised.add(token);
    }
    const removed = remainingText(baseCandidates.tokens, usedBase);
    const added = remainingText(revisedCandidates.tokens, usedRevised);
    if (removed && summaries.length < 3) summaries.push(`− ${quote(removed)}`);
    if (added && summaries.length < 3) summaries.push(`+ ${quote(added)}`);
  }
  return summaries;
}

/**
 * İki önek sözcükleriyle aynı mı? "Madde 5 -" ile "Madde 5-", "1)" ile "1 )"
 * aynı önektir. Ham metin karşılaştırılınca satır "değişti" sayılıyor, özet
 * "Madde numarası: 5 → 5" diyordu. Gösterilen önek yine kaynaktan gelir.
 */
function samePrefix(base: StructuralText, revised: StructuralText): boolean {
  if (!base.prefix || !revised.prefix) return !base.prefix && !revised.prefix;
  return wordsKey(base.prefix.raw) === wordsKey(revised.prefix.raw);
}

function prefixSummary(base: StructuralText, revised: StructuralText): string[] {
  if (samePrefix(base, revised)) return [];
  if (base.prefix && revised.prefix && base.prefix.kind === revised.prefix.kind) {
    // Numara aynı, yazım farklıysa ("Madde 5 –" / "Madde 5 -", "1." / "1)")
    // "5 → 5" kendisiyle çelişir; değişen yazım gösterilir.
    if (base.prefix.number === revised.prefix.number) {
      return [`${quote(base.prefix.raw)} → ${quote(revised.prefix.raw)}`];
    }
    const label = base.prefix.kind === "article" ? "Madde numarası" : "Paragraf numarası";
    return [`${label}: ${base.prefix.number} → ${revised.prefix.number}`];
  }
  return [
    ...(base.prefix ? [`− ${quote(base.prefix.raw)}`] : []),
    ...(revised.prefix ? [`+ ${quote(revised.prefix.raw)}`] : []),
  ];
}

/**
 * Önek ile gövde kaynakta bitişikse ("1.Kira", "1 | Kira") gövdenin ilk
 * parçası bitişik işaretlenir; panel araya kaynakta olmayan boşluğu koymaz.
 */
function gluedToPrefix(fragments: DiffFragment[], glued: boolean): DiffFragment[] {
  if (!glued || !fragments.length) return fragments;
  return [{ ...fragments[0], spaceBefore: false }, ...fragments.slice(1)];
}

function bodyGluedToPrefix(text: string, structure: StructuralText): boolean {
  return !!structure.prefix && !/\s$/u.test(text.slice(0, text.length - structure.body.length));
}

function prefixedDiff(baseText: string, revisedText: string, ignoredOrdinalPrefix?: Pair["ignoredOrdinalPrefix"]): {
  base: DiffFragment[];
  revised: DiffFragment[];
  summary: string[];
  hunks: WordDiffHunk[];
} {
  if (ignoredOrdinalPrefix) {
    const baseBody = baseText.slice(ignoredOrdinalPrefix.base.length);
    const revisedBody = revisedText.slice(ignoredOrdinalPrefix.revised.length);
    const body = wordDiff(baseBody, revisedBody);
    return {
      base: [{ kind: "equal", text: ignoredOrdinalPrefix.base }, ...gluedToPrefix(body.base, !/^\s/u.test(baseBody))],
      revised: [{ kind: "equal", text: ignoredOrdinalPrefix.revised }, ...gluedToPrefix(body.revised, !/^\s/u.test(revisedBody))],
      summary: summarizeBodyDiff(body.base, body.revised).slice(0, 3),
      hunks: body.hunks,
    };
  }
  const baseStructure = splitStructuralPrefix(baseText);
  const revisedStructure = splitStructuralPrefix(revisedText);
  const body = wordDiff(baseStructure.body, revisedStructure.body);
  const prefixUnchanged = samePrefix(baseStructure, revisedStructure);
  const basePrefix: DiffFragment[] = baseStructure.prefix
    ? [{ kind: prefixUnchanged ? "equal" : "removed", text: baseStructure.prefix.raw }]
    : [];
  const revisedPrefix: DiffFragment[] = revisedStructure.prefix
    ? [{ kind: prefixUnchanged ? "equal" : "added", text: revisedStructure.prefix.raw }]
    : [];
  return {
    base: [...basePrefix, ...gluedToPrefix(body.base, bodyGluedToPrefix(baseText, baseStructure))],
    revised: [...revisedPrefix, ...gluedToPrefix(body.revised, bodyGluedToPrefix(revisedText, revisedStructure))],
    summary: [...prefixSummary(baseStructure, revisedStructure), ...summarizeBodyDiff(body.base, body.revised)].slice(0, 3),
    hunks: body.hunks,
  };
}

interface LocalSentence {
  text: string;
  blockIndex: number;
  sentenceIndex: number;
  sourceBlockIds: string[];
}

function localSentences(block: DocumentBlock): LocalSentence[] {
  const units = block.sourceUnits ?? (block.sourceTexts ?? [block.text]).map((text, index) => ({
    id: block.sourceBlockIds?.[index] ?? block.id,
    text,
  }));
  const result: LocalSentence[] = [];
  let pending: LocalSentence | undefined;
  units.forEach((unit, blockIndex) => {
    // Cümle yalnız noktalamadan SONRA boşluk gelen yerde biter. Eskiden her
    // noktada bölünüyordu: "18.500" "18." + "500", "A.Ş.'nin" "A." + "Ş." +
    // "'nin" oluyor; panel parçaları boşlukla birleştirip "18. 500"
    // gösteriyor, özet de tutarı "000" → "500" diye yazıyordu.
    const parts = unit.text.split(/(?<=[.!?;])\s+/u).map((sentence) => sentence.trim()).filter(Boolean);
    parts.forEach((text) => {
      const sentence = pending
        ? { ...pending, text: `${pending.text} ${text}`, sourceBlockIds: [...new Set([...pending.sourceBlockIds, unit.id])] }
        : { text, blockIndex, sentenceIndex: result.length, sourceBlockIds: [unit.id] };
      if (/[.!?;]+$/u.test(text)) {
        result.push(sentence);
        pending = undefined;
      } else {
        pending = sentence;
      }
    });
  });
  if (pending) result.push(pending);
  return result;
}

function sentenceAnchorScore(left: string, right: string): number | undefined {
  const leftTokens = tokenize(looseMatchForm(left));
  const rightTokens = tokenize(looseMatchForm(right));
  if (Math.min(leftTokens.length, rightTokens.length) < 8) return undefined;
  if (comparisonForm(left) === comparisonForm(right)) return 1;
  const counts = new Map<string, number>();
  leftTokens.forEach((token) => counts.set(token, (counts.get(token) ?? 0) + 1));
  let shared = 0;
  rightTokens.forEach((token) => {
    const count = counts.get(token) ?? 0;
    if (count > 0) {
      shared++;
      counts.set(token, count - 1);
    }
  });
  const shorterCoverage = shared / Math.min(leftTokens.length, rightTokens.length);
  const longerCoverage = shared / Math.max(leftTokens.length, rightTokens.length);
  const run = longestCommonRun(leftTokens, rightTokens);
  if (shorterCoverage < 0.88 || longerCoverage < 0.72 || run < 6) return undefined;
  return shorterCoverage * 0.6 + longerCoverage * 0.4;
}

interface AtomicSentencePair {
  base?: LocalSentence;
  revised?: LocalSentence;
  atomicPairId: string;
}

interface ProvenancedHunk extends WordDiffHunk {
  atomicPairId: string;
  baseSourceBlockIds: string[];
  revisedSourceBlockIds: string[];
}

function atomicSentenceScore(left: string, right: string): number | undefined {
  if (comparisonForm(left) === comparisonForm(right)) return 1;
  const strongAnchor = sentenceAnchorScore(left, right);
  if (strongAnchor !== undefined) return strongAnchor;
  const leftTokens = tokenize(looseMatchForm(left));
  const rightTokens = tokenize(looseMatchForm(right));
  if (Math.min(leftTokens.length, rightTokens.length) < 3) return undefined;
  const counts = new Map<string, number>();
  leftTokens.forEach((token) => counts.set(token, (counts.get(token) ?? 0) + 1));
  let shared = 0;
  rightTokens.forEach((token) => {
    const count = counts.get(token) ?? 0;
    if (count > 0) {
      shared++;
      counts.set(token, count - 1);
    }
  });
  const shorterCoverage = shared / Math.min(leftTokens.length, rightTokens.length);
  const longerCoverage = shared / Math.max(leftTokens.length, rightTokens.length);
  if (shorterCoverage < 0.68 || longerCoverage < 0.55 || longestCommonRun(leftTokens, rightTokens) < 2) return undefined;
  return shorterCoverage * 0.6 + longerCoverage * 0.4;
}

function alignAtomicSentences(base: DocumentBlock, revised: DocumentBlock): AtomicSentencePair[] {
  const left = localSentences(base);
  const right = localSentences(revised);
  const cols = right.length + 1;
  const scores = new Float32Array((left.length + 1) * cols);
  const moves = new Uint8Array((left.length + 1) * cols);
  const gap = -0.46;
  for (let i = 1; i <= left.length; i++) {
    scores[i * cols] = i * gap;
    moves[i * cols] = 2;
  }
  for (let j = 1; j <= right.length; j++) {
    scores[j] = j * gap;
    moves[j] = 3;
  }
  for (let i = 1; i <= left.length; i++) {
    for (let j = 1; j <= right.length; j++) {
      const at = i * cols + j;
      const score = atomicSentenceScore(left[i - 1].text, right[j - 1].text);
      const diagonal = score === undefined ? Number.NEGATIVE_INFINITY : scores[(i - 1) * cols + j - 1] + score * 1.7;
      const up = scores[(i - 1) * cols + j] + gap;
      const backward = scores[i * cols + j - 1] + gap;
      if (diagonal >= up && diagonal >= backward) {
        scores[at] = diagonal;
        moves[at] = 1;
      } else if (up >= backward) {
        scores[at] = up;
        moves[at] = 2;
      } else {
        scores[at] = backward;
        moves[at] = 3;
      }
    }
  }
  const result: AtomicSentencePair[] = [];
  let i = left.length;
  let j = right.length;
  while (i || j) {
    const move = moves[i * cols + j];
    const baseSentence = move === 1 || move === 2 || j === 0 ? left[i - 1] : undefined;
    const revisedSentence = move === 1 || move === 3 || i === 0 ? right[j - 1] : undefined;
    result.push({
      base: baseSentence,
      revised: revisedSentence,
      atomicPairId: `sentence:${baseSentence?.sourceBlockIds.join("+") ?? "-"}:${baseSentence?.sentenceIndex ?? "-"}:${revisedSentence?.sourceBlockIds.join("+") ?? "-"}:${revisedSentence?.sentenceIndex ?? "-"}`,
    });
    if (move === 1) { i--; j--; }
    else if (move === 2 || j === 0) i--;
    else j--;
  }
  return result.reverse();
}

// Cümleler arasındaki ayraç parçası kaynaktaki boşluğun KENDİSİDİR; panel
// yanına ikinci, üçüncü bir boşluk koymasın diye iki yanı bitişik işaretlenir.
function appendFragments(target: DiffFragment[], fragments: DiffFragment[]): void {
  if (!fragments.length) return;
  if (!target.length) {
    target.push(...fragments);
    return;
  }
  target.push({ kind: "equal", text: " ", spaceBefore: false }, { ...fragments[0], spaceBefore: false }, ...fragments.slice(1));
}

function appendProvenancedHunk(target: ProvenancedHunk[], hunk: ProvenancedHunk): void {
  const prior = target.at(-1);
  const mergeRemoved = prior && prior.removed && !prior.added && hunk.removed && !hunk.added
    && prior.revisedSourceBlockIds.length === 0 && hunk.revisedSourceBlockIds.length === 0
    && prior.baseSourceBlockIds.join("|") === hunk.baseSourceBlockIds.join("|");
  const mergeAdded = prior && prior.added && !prior.removed && hunk.added && !hunk.removed
    && prior.baseSourceBlockIds.length === 0 && hunk.baseSourceBlockIds.length === 0
    && prior.revisedSourceBlockIds.join("|") === hunk.revisedSourceBlockIds.join("|");
  if (prior && (mergeRemoved || mergeAdded)) {
    prior.removed = [prior.removed, hunk.removed].filter(Boolean).join(" ");
    prior.added = [prior.added, hunk.added].filter(Boolean).join(" ");
    prior.atomicPairId = `${prior.atomicPairId}+${hunk.atomicPairId}`;
    return;
  }
  target.push(hunk);
}

function atomicCompositeDiff(base: DocumentBlock, revised: DocumentBlock): {
  base: DiffFragment[];
  revised: DiffFragment[];
  summary: string[];
  hunks: ProvenancedHunk[];
  atomicPairs: AtomicSentencePair[];
} {
  const baseFragments: DiffFragment[] = [];
  const revisedFragments: DiffFragment[] = [];
  const hunks: ProvenancedHunk[] = [];
  const atomicPairs = alignAtomicSentences(base, revised);
  for (const pair of atomicPairs) {
    if (pair.base && pair.revised) {
      const diff = wordDiff(pair.base.text, pair.revised.text);
      appendFragments(baseFragments, diff.base);
      appendFragments(revisedFragments, diff.revised);
      diff.hunks.forEach((hunk) => appendProvenancedHunk(hunks, {
        ...hunk,
        atomicPairId: pair.atomicPairId,
        baseSourceBlockIds: pair.base!.sourceBlockIds,
        revisedSourceBlockIds: pair.revised!.sourceBlockIds,
      }));
    } else if (pair.base) {
      appendFragments(baseFragments, [{ kind: "removed", text: pair.base.text }]);
      appendProvenancedHunk(hunks, {
        removed: pair.base.text,
        added: "",
        atomicPairId: pair.atomicPairId,
        baseSourceBlockIds: pair.base.sourceBlockIds,
        revisedSourceBlockIds: [],
      });
    } else if (pair.revised) {
      appendFragments(revisedFragments, [{ kind: "added", text: pair.revised.text }]);
      appendProvenancedHunk(hunks, {
        removed: "",
        added: pair.revised.text,
        atomicPairId: pair.atomicPairId,
        baseSourceBlockIds: [],
        revisedSourceBlockIds: pair.revised.sourceBlockIds,
      });
    }
  }
  const summary = hunks.flatMap((hunk) => summarizeBodyDiff(
    hunk.removed ? [{ kind: "removed", text: hunk.removed }] : [],
    hunk.added ? [{ kind: "added", text: hunk.added }] : [],
  )).slice(0, 3);
  return { base: baseFragments, revised: revisedFragments, summary, hunks, atomicPairs };
}

function genericSummary(kind: ComparisonRow["kind"]): string[] {
  if (kind === "added") return ["Bölüm bütünüyle eklendi."];
  if (kind === "removed") return ["Bölüm bütünüyle silindi."];
  return [];
}

function singleHunkText(hunks: WordDiffHunk[], side: "added" | "removed"): string | undefined {
  const texts = hunks.map((hunk) => hunk[side]).filter(Boolean);
  return texts.length === 1 ? texts[0] : undefined;
}

interface PresentedChange {
  kind: DocumentChange["kind"];
  summary: string[];
  baseText?: string;
  revisedText?: string;
  presentation?: NonNullable<DocumentChange["presentation"]>;
  provenance?: NonNullable<DocumentChange["provenance"]>;
}

const CARD_SEGMENT_LIMIT = 3;
const TYPED_SIGNAL_RE = /(?:\d+(?:[.,:/-]\d+)*(?:\s*\(\s*\p{L}+\s*\))?|[%₺€$]|\b(?:TL|TRY|USD|EUR)\b)/iu;

function signalExcerpt(text: string, max = 112): string {
  if (text.length <= max) return text;
  const match = TYPED_SIGNAL_RE.exec(text);
  const atTokenBoundary = (value: string, trimStart: boolean, trimEnd: boolean): string => {
    let result = value;
    if (trimStart) result = result.replace(/^\S+\s+/u, "");
    if (trimEnd) result = result.replace(/\s+\S*$/u, "");
    return result.trim();
  };
  if (!match?.index) return `${atTokenBoundary(text.slice(0, max - 1), false, true)}…`;
  const center = match.index + Math.floor(match[0].length / 2);
  const start = Math.max(0, Math.min(text.length - max, center - Math.floor(max * 0.55)));
  const excerpt = atTokenBoundary(text.slice(start, start + max), start > 0, start + max < text.length);
  return `${start > 0 ? "…" : ""}${excerpt}${start + max < text.length ? "…" : ""}`;
}

function hunkSignalScore(hunk: WordDiffHunk): number {
  const text = `${hunk.removed} ${hunk.added}`;
  const tokenCount = tokenize(text).length;
  return (TYPED_SIGNAL_RE.test(text) ? 10_000 : 0)
    + (tokenCount >= 8 ? 1_000 : tokenCount * 20)
    + Math.min(500, text.length);
}

function hunkSummary(hunk: WordDiffHunk): string[] {
  if (hunk.removed && !hunk.added) return [`− ${quote(signalExcerpt(hunk.removed), 120)}`];
  if (hunk.added && !hunk.removed) return [`+ ${quote(signalExcerpt(hunk.added), 120)}`];
  const base: DiffFragment[] = hunk.removed ? [{ kind: "removed", text: hunk.removed }] : [];
  const revised: DiffFragment[] = hunk.added ? [{ kind: "added", text: hunk.added }] : [];
  return summarizeBodyDiff(base, revised);
}

function summaryCoversText(summary: string[], text: string): boolean {
  const summaryTokens = new Set(tokenize(looseMatchForm(summary.join(" "))));
  const textTokens = tokenize(looseMatchForm(text));
  return textTokens.every((token) => summaryTokens.has(token));
}

function missingShortHunkSummaries(hunks: WordDiffHunk[], fallbackSummary: string[]): string[] {
  return hunks.flatMap((hunk) => {
    const tokenCount = tokenize(`${hunk.removed} ${hunk.added}`).length;
    if (tokenCount > 12) return [];
    const removedCovered = !hunk.removed || summaryCoversText(fallbackSummary, hunk.removed);
    const addedCovered = !hunk.added || summaryCoversText(fallbackSummary, hunk.added);
    if (removedCovered && addedCovered) return [];
    return hunkSummary(hunk).filter((line) => !summaryCoversText(fallbackSummary, line));
  });
}

function summaryUsesOneHunk(line: string, hunks: WordDiffHunk[]): boolean {
  if (!line.includes("→")) return true;
  if (TYPED_SIGNAL_RE.test(line) || /İstanbul(?:\s+Anadolu)?[^”]*”\s*→\s*“İstanbul/u.test(line)) return true;
  const literals = Array.from(line.matchAll(/“([^”]+)”/gu), (match) => match[1]);
  if (literals.length < 2) return true;
  return hunks.some((hunk) => hunk.removed.includes(literals[0]) && hunk.added.includes(literals[1]));
}

/**
 * Özet satırları, tekrarsız. Satırın genel özeti sözcüklerden, parçanın
 * kendi özeti kaynaktan yazılır; aynı ekleme "md. 119/1 - e" ve "md.119/1-e"
 * diye iki kez çıkmasın. Yalnız boşluğu farklı satırlardan ilki kalır.
 */
function uniqueLines(lines: string[]): string[] {
  const seen = new Set<string>();
  return lines.filter((line) => {
    const key = wordsKey(line);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function cardPresentation(
  hunks: WordDiffHunk[],
  fallbackSummary: string[],
): { summary: string[]; presentation?: NonNullable<DocumentChange["presentation"]> } {
  if (!hunks.length) return { summary: fallbackSummary };
  const segments = hunks.map((hunk, index) => ({ hunk, id: `segment-${index}`, index }));
  if (segments.length <= CARD_SEGMENT_LIMIT) {
    const localFallback = hunks.length > 1
      ? uniqueLines([
        ...fallbackSummary.filter((line) => summaryUsesOneHunk(line, hunks)),
        ...hunks.flatMap((hunk) => hunkSummary(hunk)),
      ])
      : fallbackSummary;
    const missingPreview = missingShortHunkSummaries(hunks, localFallback);
    return {
      summary: uniqueLines([...localFallback, ...missingPreview]),
      presentation: {
        segmentCount: segments.length,
        visibleSegmentIds: segments.map(({ id }) => id),
        collapsedSegmentIds: [],
      },
    };
  }
  const ranked = [...segments].sort((left, right) => (
    hunkSignalScore(right.hunk) - hunkSignalScore(left.hunk) || left.index - right.index
  ));
  const visible = ranked.slice(0, CARD_SEGMENT_LIMIT);
  const visibleIds = new Set(visible.map(({ id }) => id));
  const collapsed = segments.filter(({ id }) => !visibleIds.has(id));
  const priorityFallback = fallbackSummary.filter((line) => (
    /^(?:Madde|Paragraf) numarası:/u.test(line)
    || (line.includes("→") && TYPED_SIGNAL_RE.test(line))
  ));
  const previewLines = visible.flatMap(({ hunk }) => hunkSummary(hunk));
  return {
    summary: [
      ...priorityFallback,
      ...previewLines.filter((line) => !priorityFallback.some((kept) => wordsKey(kept) === wordsKey(line))),
      `+ ${collapsed.length} değişiklik daha`,
    ],
    presentation: {
      segmentCount: segments.length,
      visibleSegmentIds: visible.map(({ id }) => id),
      collapsedSegmentIds: collapsed.map(({ id }) => id),
    },
  };
}

function presentedKind(baseFragments: DiffFragment[], revisedFragments: DiffFragment[]): DocumentChange["kind"] {
  const hasRemoved = baseFragments.some((fragment) => fragment.kind === "removed");
  const hasAdded = revisedFragments.some((fragment) => fragment.kind === "added");
  if (hasAdded && !hasRemoved) return "added";
  if (hasRemoved && !hasAdded) return "removed";
  return "modified";
}

function hunkProvenance(hunks: ProvenancedHunk[]): NonNullable<DocumentChange["provenance"]> {
  return {
    atomicPairIds: [...new Set(hunks.map((hunk) => hunk.atomicPairId))],
    baseSourceBlockIds: [...new Set(hunks.flatMap((hunk) => hunk.baseSourceBlockIds))],
    revisedSourceBlockIds: [...new Set(hunks.flatMap((hunk) => hunk.revisedSourceBlockIds))],
  };
}

function compositePresentedChanges(
  hunks: ProvenancedHunk[],
  baseFragments: DiffFragment[],
  revisedFragments: DiffFragment[],
): PresentedChange[] {
  if (hunks.length >= 4) {
    const atomicSummary = hunks.flatMap((hunk) => hunkSummary(hunk));
    const preview = cardPresentation(hunks, atomicSummary);
    return [{
      kind: presentedKind(baseFragments, revisedFragments),
      ...preview,
      provenance: hunkProvenance(hunks),
    }];
  }
  return hunks.flatMap((hunk) => {
    const { removed, added } = hunk;
    if (!removed && !added) return [];
    const basePart: DiffFragment[] = removed ? [{ kind: "removed", text: removed }] : [];
    const revisedPart: DiffFragment[] = added ? [{ kind: "added", text: added }] : [];
    const kind = presentedKind(basePart, revisedPart);
    const preview = cardPresentation([{ removed, added }], summarizeBodyDiff(basePart, revisedPart));
    return [{
      kind,
      ...preview,
      baseText: removed || undefined,
      revisedText: added || undefined,
      provenance: hunkProvenance([hunk]),
    }];
  });
}

function ordinaryPresentedChangesWithLongDeletion(
  hunks: WordDiffHunk[],
  // Kardeş fonksiyonlarla imza simetrisi için duruyorlar; gövdede kullanılmıyorlar.
  // `_` öneki TypeScript'in "bilerek kullanılmıyor" konvansiyonudur; davranış
  // değişmez. Kabuk noUnusedParameters ile derlendiği için gereklidir.
  _baseFragments: DiffFragment[],
  _revisedFragments: DiffFragment[],
): PresentedChange[] | undefined {
  const significant = hunks.filter((hunk) => {
    const removed = tokenize(hunk.removed).length;
    const added = tokenize(hunk.added).length;
    return removed >= 40 && added <= 8 && removed >= Math.max(1, added) * 4;
  });
  if (!significant.length) return undefined;
  const significantSet = new Set(significant);
  const remaining = hunks.filter((hunk) => !significantSet.has(hunk));
  const changes: PresentedChange[] = [];
  if (remaining.some((hunk) => hunk.removed || hunk.added)) {
    const basePart: DiffFragment[] = remaining.flatMap((hunk) => hunk.removed ? [{ kind: "removed" as const, text: hunk.removed }] : []);
    const revisedPart: DiffFragment[] = remaining.flatMap((hunk) => hunk.added ? [{ kind: "added" as const, text: hunk.added }] : []);
    const preview = cardPresentation(remaining, summarizeBodyDiff(basePart, revisedPart));
    changes.push({
      kind: presentedKind(basePart, revisedPart),
      ...preview,
      baseText: singleHunkText(remaining, "removed"),
      revisedText: singleHunkText(remaining, "added"),
    });
  }
  changes.push(...significant.map((hunk) => ({
    kind: "removed" as const,
    ...cardPresentation([hunk], [`− ${quote(hunk.removed, 120)}`]),
    baseText: hunk.removed,
  })));
  return changes;
}

function articleTitle(block: DocumentBlock): string {
  const body = splitStructuralPrefix(block.text).body.replace(/^[-–—.:)\s]+/u, "").trim();
  if (!body) return block.label ?? "";
  const display = body === body.toLocaleUpperCase("tr-TR")
    ? body.toLocaleLowerCase("tr-TR").replace(/(^|\s)(\p{L})/gu, (_, space: string, letter: string) => `${space}${letter.toLocaleUpperCase("tr-TR")}`)
    : body;
  return `${block.label ?? ""} — ${display}`;
}

function groupStructuralChanges(rows: ComparisonRow[], rawChanges: DocumentChange[]): DocumentChange[] {
  const grouped: DocumentChange[] = [];
  for (let index = 0; index < rawChanges.length;) {
    const change = rawChanges[index];
    const source = change.kind === "added" ? rows[change.rowIndex].revised : rows[change.rowIndex].base;
    if ((change.kind === "added" || change.kind === "removed") && rows[change.rowIndex].kind === change.kind && source?.kind === "article" && source.label) {
      const members = [change];
      let nextIndex = index + 1;
      let previousRow = change.rowIndex;
      while (nextIndex < rawChanges.length) {
        const next = rawChanges[nextIndex];
        const nextSource = change.kind === "added" ? rows[next.rowIndex].revised : rows[next.rowIndex].base;
        if (
          next.kind !== change.kind
          || next.rowIndex !== previousRow + 1
          || !nextSource?.location?.startsWith(`${source.label} / `)
          || nextSource.kind === "article"
        ) break;
        members.push(next);
        previousRow = next.rowIndex;
        nextIndex++;
      }
      if (members.length > 1) {
        const rowIndices = members.map((member) => member.rowIndex);
        const sourceTexts = rowIndices.map((rowIndex) => change.kind === "added" ? rows[rowIndex].revised?.text : rows[rowIndex].base?.text).filter(Boolean) as string[];
        grouped.push({
          ...change,
          id: `change-${grouped.length}`,
          location: articleTitle(source),
          summary: [
            `Madde bütünüyle ${change.kind === "added" ? "eklendi" : "silindi"}.`,
            `${members.length - 1} içerik paragrafı`,
          ],
          rowIndices,
          baseText: change.kind === "removed" ? sourceTexts.join("\n") : undefined,
          revisedText: change.kind === "added" ? sourceTexts.join("\n") : undefined,
          structuralGroup: { articleLabel: source.label, contentCount: members.length - 1 },
        });
        index = nextIndex;
        continue;
      }
    }
    grouped.push({ ...change, id: `change-${grouped.length}` });
    index++;
  }
  return grouped;
}

export interface CompareOptions {
  trace?: boolean;
}

interface RowTraceDraft {
  atomicPairs: AtomicSentencePair[];
  hunks: ProvenancedHunk[];
}

function sourceUnitsFor(block: DocumentBlock | undefined) {
  if (!block) return [];
  return block.sourceUnits ?? [{ id: block.id, text: block.text, kind: block.kind, order: block.order, location: block.location }];
}

export function compareDocuments(base: DocumentBlock[], revised: DocumentBlock[], options: CompareOptions = {}): ComparisonResult {
  const pairs = stabilizeOrdinalShifts(stabilizeLocalExactRelocations(alignWithArticleSections(base, revised)));
  const rows: ComparisonRow[] = [];
  const rawChanges: DocumentChange[] = [];
  const rowTraceDrafts: RowTraceDraft[] = [];

  pairs.forEach((pair, rowIndex) => {
    let kind: ComparisonRow["kind"];
    let baseFragments: DiffFragment[] = [];
    let revisedFragments: DiffFragment[] = [];
    let summary: string[] = [];
    let rowHunks: WordDiffHunk[] = [];
    let presentedChanges: PresentedChange[] | undefined;
    let atomicPairs: AtomicSentencePair[] = [];
    let provenanceHunks: ProvenancedHunk[] = [];
    if (!pair.base) {
      kind = "added";
      revisedFragments = [{ kind: "added", text: pair.revised!.text }];
      const atomicPairId = `block:-:${pair.revised!.id}`;
      atomicPairs = [{
        revised: { text: pair.revised!.text, blockIndex: 0, sentenceIndex: 0, sourceBlockIds: pair.revised!.sourceBlockIds ?? [pair.revised!.id] },
        atomicPairId,
      }];
      provenanceHunks = [{ removed: "", added: pair.revised!.text, atomicPairId, baseSourceBlockIds: [], revisedSourceBlockIds: pair.revised!.sourceBlockIds ?? [pair.revised!.id] }];
    } else if (!pair.revised) {
      kind = "removed";
      baseFragments = [{ kind: "removed", text: pair.base.text }];
      const atomicPairId = `block:${pair.base.id}:-`;
      atomicPairs = [{
        base: { text: pair.base.text, blockIndex: 0, sentenceIndex: 0, sourceBlockIds: pair.base.sourceBlockIds ?? [pair.base.id] },
        atomicPairId,
      }];
      provenanceHunks = [{ removed: pair.base.text, added: "", atomicPairId, baseSourceBlockIds: pair.base.sourceBlockIds ?? [pair.base.id], revisedSourceBlockIds: [] }];
    } else if (
      pair.segmentationOnly
      || (pair.structuralRewrite && sameStructuralVocabulary(pair.base, pair.revised))
      || comparisonForm(pair.base.text) === comparisonForm(pair.revised.text)
    ) {
      kind = "unchanged";
      baseFragments = [{ kind: "equal", text: pair.base.text }];
      revisedFragments = [{ kind: "equal", text: pair.revised.text }];
    } else {
      kind = "modified";
      const atomicPairId = `block:${pair.base.id}:${pair.revised.id}`;
      const diff = pair.composite
        ? atomicCompositeDiff(pair.base, pair.revised)
        : prefixedDiff(pair.base.text, pair.revised.text, pair.ignoredOrdinalPrefix);
      if (pair.composite) {
        atomicPairs = (diff as ReturnType<typeof atomicCompositeDiff>).atomicPairs;
        provenanceHunks = diff.hunks as ProvenancedHunk[];
      } else {
        atomicPairs = [{
          base: { text: pair.base.text, blockIndex: 0, sentenceIndex: 0, sourceBlockIds: pair.base.sourceBlockIds ?? [pair.base.id] },
          revised: { text: pair.revised.text, blockIndex: 0, sentenceIndex: 0, sourceBlockIds: pair.revised.sourceBlockIds ?? [pair.revised.id] },
          atomicPairId,
        }];
        provenanceHunks = diff.hunks.map((hunk) => ({
          ...hunk,
          atomicPairId,
          baseSourceBlockIds: pair.base!.sourceBlockIds ?? [pair.base!.id],
          revisedSourceBlockIds: pair.revised!.sourceBlockIds ?? [pair.revised!.id],
        }));
      }
      if (!diff.hunks.length && !diff.summary.length) {
        kind = "unchanged";
        baseFragments = [{ kind: "equal", text: pair.base.text }];
        revisedFragments = [{ kind: "equal", text: pair.revised.text }];
      } else {
        baseFragments = diff.base;
        revisedFragments = diff.revised;
        summary = diff.summary;
        rowHunks = diff.hunks;
        presentedChanges = pair.composite
          ? compositePresentedChanges(diff.hunks as ProvenancedHunk[], diff.base, diff.revised)
          : ordinaryPresentedChangesWithLongDeletion(diff.hunks, diff.base, diff.revised);
      }
    }
    const location = displayLocation(pair.revised ?? pair.base, rowIndex);
    const id = `row-${rowIndex}`;
    rows.push({ id, ...pair, kind, baseFragments, revisedFragments, location });
    rowTraceDrafts.push({ atomicPairs, hunks: provenanceHunks });
    if (kind !== "unchanged") {
      const displayKind = kind === "modified" ? presentedKind(baseFragments, revisedFragments) : kind;
      const defaultPreview = kind === "modified"
        ? cardPresentation(rowHunks, summary.length ? summary : genericSummary(kind))
        : { summary: summary.length ? summary : genericSummary(kind) };
      const changesForRow = presentedChanges?.length ? presentedChanges : [{
        kind: displayKind,
        ...defaultPreview,
        baseText: kind === "modified"
          ? displayKind === "removed" ? singleHunkText(rowHunks, "removed") : displayKind === "added" ? undefined : pair.base?.text
          : pair.base?.text,
        revisedText: kind === "modified"
          ? displayKind === "added" ? singleHunkText(rowHunks, "added") : displayKind === "removed" ? undefined : pair.revised?.text
          : pair.revised?.text,
      }];
      changesForRow.forEach((change) => {
        const rawId = `raw-change-${rawChanges.length}`;
        const presentation = change.presentation ? {
          ...change.presentation,
          visibleSegmentIds: change.presentation.visibleSegmentIds.map((id) => `${rawId}:${id}`),
          collapsedSegmentIds: change.presentation.collapsedSegmentIds.map((id) => `${rawId}:${id}`),
        } : undefined;
        rawChanges.push({
          id: rawId,
          location,
          rowIndex,
          rowIndices: [rowIndex],
          ...change,
          summary: change.summary,
          provenance: change.provenance ?? (provenanceHunks.length ? hunkProvenance(provenanceHunks) : undefined),
          presentation,
        });
      });
    }
  });
  const changes = groupStructuralChanges(rows, rawChanges);
  const result: ComparisonResult = { rows, rawChanges, changes };
  if (options.trace) {
    result.trace = {
      rows: rows.map((row, rowIndex) => {
        const draft = rowTraceDrafts[rowIndex];
        const cards = changes.filter((change) => change.rowIndices.includes(rowIndex));
        const baseSourceBlockIds = row.base?.sourceBlockIds ?? (row.base ? [row.base.id] : []);
        const revisedSourceBlockIds = row.revised?.sourceBlockIds ?? (row.revised ? [row.revised.id] : []);
        return {
          rowId: row.id,
          cardIds: cards.map((card) => card.id),
          structuralGroup: row.location,
          alignmentReason: row.kind === "added" ? "unmatched-target-block"
            : row.kind === "removed" ? "unmatched-source-block"
              : row.composite ? row.segmentationOnly ? "exact-split-merge" : (row as Pair).structuralRewrite ? "structural-rewrite" : "atomic-split-merge"
                : comparisonForm(row.base?.text ?? "") === comparisonForm(row.revised?.text ?? "") ? "exact-block-anchor"
                  : "high-confidence-block-pair",
          splitMergeGroup: row.composite ? { baseSourceBlockIds, revisedSourceBlockIds } : undefined,
          oldSourceUnits: sourceUnitsFor(row.base),
          newSourceUnits: sourceUnitsFor(row.revised),
          atomicPairs: draft.atomicPairs.map((pair) => ({
            atomicPairId: pair.atomicPairId,
            oldSourceBlockIds: pair.base?.sourceBlockIds ?? [],
            newSourceBlockIds: pair.revised?.sourceBlockIds ?? [],
            oldText: pair.base?.text,
            newText: pair.revised?.text,
          })),
          tokenDiffHunks: draft.hunks.map((hunk) => ({
            removed: hunk.removed,
            added: hunk.added,
            atomicPairId: hunk.atomicPairId,
            oldSourceBlockIds: hunk.baseSourceBlockIds,
            newSourceBlockIds: hunk.revisedSourceBlockIds,
            replacementReason: hunk.removed && hunk.added ? "same-atomic-pair" as const : "separate-add-delete" as const,
          })),
          finalPresentationSegments: cards.map((card) => ({
            cardId: card.id,
            kind: card.kind,
            summary: card.summary,
            visibleSegmentIds: card.presentation?.visibleSegmentIds ?? [],
            collapsedSegmentIds: card.presentation?.collapsedSegmentIds ?? [],
          })),
        };
      }),
    };
  }
  return result;
}
