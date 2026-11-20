/**
 * Hizalama motorunun ölçüm ve eşdeğerlik testleri için belirlenimci sentetik
 * belgeler.
 *
 * Uygulama kodu bu dosyayı içe aktarmaz; yalnız `alignmentPerformance.test.ts`
 * ve elle koşulan ölçüm betikleri kullanır. Üretici tohumlu bir sayı
 * dizisinden beslenir: aynı tohum her makinede aynı belgeyi üretir, bu yüzden
 * `alignmentBaseline.json` içindeki özetler (eski motorun çıktısı) yeniden
 * üretilebilir kalır. Buradaki herhangi bir değişiklik o dosyayı geçersiz
 * kılar; üreticiye dokunulursa taban çizgisi ESKİ motorla yeniden alınmalıdır.
 */
import { makeBlocks } from "./normalize";
import type { BlockKind, DocumentBlock } from "./types";

/** mulberry32 — 32 bitlik, her JS motorunda aynı diziyi veren küçük üreteç. */
export function seededRandom(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export interface SyntheticPair {
  name: string;
  base: DocumentBlock[];
  revised: DocumentBlock[];
}

/**
 * Önceki incelemenin ölçüm düzeneği: 37 sözcüklük dar bir dağarcıktan rastgele
 * paragraflar, düzenlenen her paragrafta bir sözcük değişir ve bir sözcük
 * eklenir. Dar dağarcık her paragraf çiftini "benzer" gösterdiği için en kötü
 * durumdur: kesin eşleşme (çapa) yoktur, birleşik grup denetimleri de en
 * pahalı dallarına iner.
 */
const BENCH_WORDS = "sözleşme taraflar madde kira bedeli ödeme süre fesih teslim yükümlülük hak alacak borç tutar gün ay yıl tebligat mahkeme yetkili uyuşmazlık ceza şart teminat sigorta vergi damga noter ihtar temerrüt faiz kiracı kiraya veren mecur tahliye".split(" ");

export function benchPair(blocks: number, wordsPerBlock: number, editedFraction: number, seed = 7): SyntheticPair {
  const random = seededRandom(seed);
  const paragraph = () => {
    const words: string[] = [];
    for (let index = 0; index < wordsPerBlock; index++) words.push(BENCH_WORDS[Math.floor(random() * BENCH_WORDS.length)]);
    return `${words.join(" ")}.`;
  };
  const base: DocumentBlock[] = Array.from({ length: blocks }, (_, order) => ({
    id: `b${order}`,
    kind: "paragraph" as const,
    text: paragraph(),
    order,
  }));
  const revised = base.map((block, order) => {
    if (random() < editedFraction) {
      const words = block.text.split(" ");
      words[Math.floor(random() * words.length)] = "değiştirildi";
      words.splice(Math.floor(random() * words.length), 0, "ayrıca");
      return { ...block, id: `r${order}`, text: words.join(" ") };
    }
    return { ...block, id: `r${order}` };
  });
  return { name: `bench-${blocks}x${wordsPerBlock}-${editedFraction}`, base, revised };
}

const LEGAL_WORDS = (
  "sözleşme taraflar kira bedeli ödeme süre fesih teslim yükümlülük hak alacak borç tutar gün ay yıl "
  + "tebligat mahkeme yetkili uyuşmazlık ceza şart teminat sigorta vergi damga noter ihtar temerrüt faiz "
  + "kiracı kiraya veren mecur tahliye işveren işçi hizmet bildirim yazılı onay gizlilik bilgi belge "
  + "rapor teslimat fatura iade bakım onarım kullanım amaç bölüm ek protokol değişiklik yürürlük "
  + "tarih imza yetki temsil devir temlik garanti kusur zarar tazminat sorumluluk sınır mücbir sebep "
  + "olay haller süresince içinde dışında kadar itibaren dolayı nedeniyle uyarınca göre ilişkin ait "
  + "olarak üzere ile ve veya da de bu her hiçbir tüm ilgili gerekli makul derhal ayrıca"
).split(" ");
const SYLLABLES = "ka ra se li mu to ne da şi gö çe ül ar en ım or ba ve ti lo sa ku pe re yo zi ha me nu".split(" ");

/** Gerçekçi bir dağarcık: hukuk sözcükleri ile sentetik özel adlar. */
function vocabulary(random: () => number): () => string {
  const invented = Array.from({ length: 420 }, () => {
    const length = 2 + Math.floor(random() * 3);
    let word = "";
    for (let index = 0; index < length; index++) word += SYLLABLES[Math.floor(random() * SYLLABLES.length)];
    return word;
  });
  return () => (random() < 0.55
    ? LEGAL_WORDS[Math.floor(random() * LEGAL_WORDS.length)]
    : invented[Math.floor(random() * invented.length)]);
}

function sentence(random: () => number, word: () => string, minWords: number, maxWords: number): string {
  const count = minWords + Math.floor(random() * (maxWords - minWords + 1));
  const words = Array.from({ length: count }, word);
  if (random() < 0.25) words.splice(Math.floor(random() * words.length), 0, `${1 + Math.floor(random() * 900)}.${Math.floor(random() * 10)}00 TL`);
  if (random() < 0.15) words.splice(Math.floor(random() * words.length), 0, `${1 + Math.floor(random() * 28)}.${1 + Math.floor(random() * 12)}.2026`);
  const text = words.join(" ");
  return `${text.charAt(0).toLocaleUpperCase("tr-TR")}${text.slice(1)}.`;
}

function editSentenceText(random: () => number, word: () => string, text: string): string {
  const words = text.split(" ");
  const operation = random();
  if (operation < 0.4) words[Math.floor(random() * words.length)] = word();
  else if (operation < 0.7) words.splice(Math.floor(random() * words.length), 0, word(), word());
  else if (words.length > 6) words.splice(1 + Math.floor(random() * (words.length - 2)), 1);
  else words.push(word());
  return words.join(" ");
}

interface Entry {
  text: string;
  kind?: BlockKind;
  sourceListOrdinal?: string;
}

interface ContractOptions {
  name: string;
  seed: number;
  articles: number;
  /** Paragrafların sözcük düzeyinde düzenlenme oranı (0–1). */
  editRate: number;
  /** Madde başlıkları yerine numarasız uzun bir metin (çapa yok). */
  memo?: boolean;
}

/**
 * Sözleşme benzeri belge çifti: madde başlıkları, çok düzeyli fıkra
 * numaraları, liste öğeleri, "etiket: değer" satırları, tutar ve tarihler.
 * Değişik sürümde sözcük düzeyi düzenlemeler, silinen ve eklenen fıkralar,
 * bölünen ve birleşen paragraflar, taşınan bir madde ve küçük bir taşınan
 * paragraf dizisi bulunur.
 */
export function contractPair(options: ContractOptions): SyntheticPair {
  const random = seededRandom(options.seed);
  const word = vocabulary(random);
  type Article = { title: string; entries: Entry[] };
  const articles: Article[] = Array.from({ length: options.articles }, (_, articleIndex) => {
    const title = Array.from({ length: 1 + Math.floor(random() * 3) }, word).join(" ").toLocaleUpperCase("tr-TR");
    const entries: Entry[] = [];
    const clauses = 3 + Math.floor(random() * 8);
    for (let clause = 1; clause <= clauses; clause++) {
      const shape = random();
      const sentences = Array.from({ length: 1 + Math.floor(random() * 3) }, () => sentence(random, word, 6, 22));
      if (options.memo) {
        entries.push({ text: sentences.join(" ") });
      } else if (shape < 0.12) {
        entries.push({ text: `${Array.from({ length: 1 + Math.floor(random() * 2) }, word).join(" ")}: ${1 + Math.floor(random() * 90)}.500 TL` });
      } else if (shape < 0.24) {
        const items = 2 + Math.floor(random() * 3);
        entries.push({ text: `${articleIndex + 1}.${clause}. ${sentences[0].replace(/\.$/u, ":")}` });
        for (let item = 1; item <= items; item++) {
          entries.push({ text: sentence(random, word, 4, 10), kind: "list", sourceListOrdinal: String(item) });
        }
      } else {
        entries.push({ text: `${articleIndex + 1}.${clause}. ${sentences.join(" ")}` });
      }
    }
    return { title, entries };
  });

  const headingFor = (index: number, title: string) => (options.memo ? undefined : { text: `MADDE ${index + 1} – ${title}` });
  const flatten = (list: Article[]): Entry[] => list.flatMap((article, index) => {
    const heading = headingFor(index, article.title);
    return heading ? [heading, ...article.entries] : article.entries;
  });
  const baseEntries = flatten(articles);

  // Değişik sürüm: maddeleri kopyala, sonra sırayla düzenle.
  const revisedArticles: Article[] = articles.map((article) => ({
    title: article.title,
    entries: article.entries.map((entry) => ({ ...entry })),
  }));
  for (const article of revisedArticles) {
    const next: Entry[] = [];
    for (let index = 0; index < article.entries.length; index++) {
      const entry = article.entries[index];
      const roll = random();
      if (roll < 0.03 && article.entries.length > 3) continue; // silinen fıkra
      if (random() < options.editRate) entry.text = editSentenceText(random, word, entry.text);
      const sentences = entry.text.split(/(?<=\.)\s+(?=\p{Lu})/u);
      if (roll > 0.97 && sentences.length >= 2 && entry.kind !== "list") {
        // Bölünen paragraf: ilk cümle yerinde kalır, kalanı ayrı blok olur.
        next.push({ ...entry, text: sentences[0] });
        next.push({ text: sentences.slice(1).join(" ") });
        continue;
      }
      const following = article.entries[index + 1];
      if (roll > 0.945 && roll <= 0.97 && following && !following.kind && !entry.kind && options.memo) {
        // Birleşen paragraflar (yalnız numarasız metinde anlamlı).
        next.push({ text: `${entry.text} ${following.text}` });
        index++;
        continue;
      }
      next.push(entry);
      if (random() < 0.04) next.push({ text: sentence(random, word, 8, 20) }); // eklenen paragraf
    }
    article.entries = next;
  }
  // Bir madde başka yere taşınır (bölüm eşleştirmesi ve sıra kayması).
  if (revisedArticles.length > 6) {
    const [moved] = revisedArticles.splice(2, 1);
    revisedArticles.splice(revisedArticles.length - 2, 0, moved);
  }
  let revisedEntries = flatten(revisedArticles);
  // Numarasız metinde küçük bir paragraf dizisi ileri taşınır.
  if (options.memo && revisedEntries.length > 60) {
    const moved = revisedEntries.splice(10, 12);
    revisedEntries = [...revisedEntries.slice(0, 40), ...moved, ...revisedEntries.slice(40)];
  }
  return {
    name: options.name,
    base: makeBlocks(baseEntries, "base"),
    revised: makeBlocks(revisedEntries, "revised"),
  };
}

/**
 * PDF ↔ DOCX benzeri bölümleme farkı: değişik sürümde paragraflar 2–3 satıra
 * bölünmüş ve bir kısmı düzenlenmiştir. Bloklar arası hizalama sürekli kayar
 * (bir temel bloğa birden çok değişik blok düşer); bant sınırının kaymayı
 * izleyebildiğini sınar.
 */
export function segmentationPair(paragraphs: number, seed: number): SyntheticPair {
  const random = seededRandom(seed);
  const word = vocabulary(random);
  const baseEntries: Entry[] = [];
  const revisedEntries: Entry[] = [];
  for (let index = 0; index < paragraphs; index++) {
    const sentences = Array.from({ length: 2 + Math.floor(random() * 2) }, () => sentence(random, word, 7, 16));
    baseEntries.push({ text: sentences.join(" ") });
    const edited = sentences.map((text) => (random() < 0.3 ? editSentenceText(random, word, text) : text));
    if (random() < 0.5) revisedEntries.push({ text: edited.join(" ") });
    else edited.forEach((text) => revisedEntries.push({ text }));
  }
  return {
    name: `segmentation-${paragraphs}-${seed}`,
    base: makeBlocks(baseEntries, "base"),
    revised: makeBlocks(revisedEntries, "revised"),
  };
}

/**
 * Çapasız uzun taşıma: her paragrafı düzenlenmiş metinde ilk `moved` paragraf
 * sona taşınmıştır. Taşıma bandın payından uzunsa yol temel bandın dışına
 * çıkar; nadir sözcükler (sentetik özel adlar) kılavuz yolu taşınan diziye
 * götürür.
 */
export function movedRunPair(paragraphs: number, moved: number, seed: number): SyntheticPair {
  const random = seededRandom(seed);
  const word = () => `s${Math.floor(random() * 4000)}`;
  const texts = Array.from({ length: paragraphs }, () => `${Array.from({ length: 18 }, word).join(" ")}.`);
  const edited = texts.map((text) => text.replace(/^(\S+)/u, "$1 ayrıca"));
  return {
    name: `moved-run-${moved}-of-${paragraphs}`,
    base: makeBlocks(texts.map((text) => ({ text })), "base"),
    revised: makeBlocks([...edited.slice(moved), ...edited.slice(0, moved)].map((text) => ({ text })), "revised"),
  };
}

/** Eşdeğerlik tabanının kapsadığı senaryolar (sıra ve parametreler sabittir). */
export function baselineScenarios(): SyntheticPair[] {
  return [
    benchPair(60, 40, 1),
    benchPair(200, 40, 1),
    benchPair(200, 40, 0.05),
    contractPair({ name: "contract-mixed", seed: 11, articles: 26, editRate: 0.25 }),
    contractPair({ name: "contract-all-edited", seed: 23, articles: 30, editRate: 1 }),
    contractPair({ name: "memo-all-edited", seed: 37, articles: 34, editRate: 1, memo: true }),
    segmentationPair(170, 51),
    movedRunPair(260, 90, 5),
  ];
}
