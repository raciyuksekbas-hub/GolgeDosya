// Kapı 6 — v0.3.2 saha düzeltmeleri, gerçek WebView2'de.
//
// Her senaryo v0.3.1'de DÜŞEN, düzeltilmiş derlemede GEÇEN en az bir gerçek
// iddia (check) üretir; ekran görüntüsü alır, kendi fikstürünü kendi yazar,
// sayfayı yeniden yükleyerek başlar ve süre sınırı içinde biter. Bir senaryonun
// yarıda kalması (açık kalan dosya penceresi, emülasyon, Gezgin) sonrakini
// etkilemez: başta ve sonda temizlik yapılır.
//
// Yerel pencereler (Kaydet / Aç) gerçek ortak dosya penceresidir; native.ps1
// `filedialog` onları UI Automation ile doldurup onaylar. Ekran ölçeği CDP
// Emulation ile verilir (runner ekranı %100'dür).
//
// Saf yardımcılar (PDF ve DOCX yazıcıları, PDF kutu okuyucu, PNG çözücü)
// dışa açıktır: macOS'ta `node` ile, Windows'suz sınanabilirler.
import { AsyncLocalStorage } from "node:async_hooks";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateRawSync, deflateSync, inflateSync } from "node:zlib";
import { minimalPdf } from "./preview-stability.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(here, "../../../..");

// ======================================================================
// Saf yardımcılar
// ======================================================================

/**
 * Asgari PDF yazıcı. `objects[i]`, (i + 1) numaralı nesnenin gövdesidir:
 * dize ya da `{ dict, stream }` (dict: `<< >>` içi, stream: Buffer; /Length
 * eklenir). Nesne 1 katalogdur. Klasik xref tablosu yazılır.
 */
export function writePdf(objects) {
  const parts = [Buffer.from("%PDF-1.7\n%\xE2\xE3\xCF\xD3\n", "latin1")];
  let size = parts[0].length;
  const offsets = [];
  objects.forEach((body, i) => {
    offsets.push(size);
    const head = Buffer.from(`${i + 1} 0 obj\n`, "latin1");
    const chunk = typeof body === "string"
      ? Buffer.concat([head, Buffer.from(`${body}\nendobj\n`, "latin1")])
      : Buffer.concat([head, Buffer.from(`<< ${body.dict} /Length ${body.stream.length} >>\nstream\n`, "latin1"),
        body.stream, Buffer.from("\nendstream\nendobj\n", "latin1")]);
    parts.push(chunk);
    size += chunk.length;
  });
  parts.push(Buffer.from(`xref\n0 ${objects.length + 1}\n0000000000 65535 f \n${offsets.map((o) => `${String(o).padStart(10, "0")} 00000 n \n`).join("")}`
    + `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${size}\n%%EOF\n`, "latin1"));
  return Buffer.concat(parts);
}

const stream = (dict, text) => ({ dict, stream: Buffer.from(text, "latin1") });

/** Açıklamaların PDF koordinatı (pt) ve beklenen rengi; dikdörtgenin merkezinden örneklenir. */
export const ANNOTATION_PAGE = [0, 0, 612, 792];
export const ANNOTATION_PROBES = [
  { name: "Highlight (vurgu)", x: 172, y: 620, rgb: [255, 255, 0] },
  { name: "Ink (mürekkep)", x: 172, y: 430, rgb: [255, 0, 0] },
  { name: "FreeText (metin kutusu)", x: 430, y: 640, rgb: [0, 0, 255] },
  { name: "boş sayfa (kontrol)", x: 450, y: 300, rgb: [255, 255, 255] },
];

/**
 * Letter sayfa; üç açıklama, her biri kendi görünüm akışıyla (/AP /N):
 * Highlight (sarı, /BM /Multiply), Ink (kalın kırmızı çizgi), FreeText
 * (mavi kutu). Sayfa içeriği yalnız ince bir çerçeve: açıklamaların altı beyaz,
 * renk doğrudan açıklamadan gelir. `appearance: false`: yalnız /AP'siz bir
 * FreeText (dışa aktarma açıkça reddetmeli).
 */
export function annotatedPdf({ appearance = true } = {}) {
  const page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Contents 4 0 R /Annots [5 0 R"
    + (appearance ? " 7 0 R 9 0 R" : "") + "] >>";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    page,
    stream("", "q 0 0 0 RG 1 w 36 36 540 720 re S 0.5 g 72 160 468 12 re f Q"),
  ];
  if (!appearance) {
    objects.push("<< /Type /Annot /Subtype /FreeText /Rect [320 600 540 680] /F 4 /DA (/Helv 12 Tf 0 0 1 rg) /Contents (Not) >>");
    return writePdf(objects);
  }
  objects.push(
    "<< /Type /Annot /Subtype /Highlight /Rect [72 600 272 640] /F 4 /C [1 1 0] /QuadPoints [72 640 272 640 72 600 272 600] /AP << /N 6 0 R >> >>",
    stream("/Type /XObject /Subtype /Form /BBox [0 0 200 40] /Resources << /ExtGState << /M << /Type /ExtGState /BM /Multiply >> >> >>",
      "/M gs 1 1 0 rg 0 0 200 40 re f"),
    "<< /Type /Annot /Subtype /Ink /Rect [72 400 272 460] /F 4 /C [1 0 0] /InkList [[82 430 262 430]] /BS << /W 14 >> /AP << /N 8 0 R >> >>",
    stream("/Type /XObject /Subtype /Form /BBox [0 0 200 60]", "1 0 0 RG 14 w 1 J 10 30 m 190 30 l S"),
    "<< /Type /Annot /Subtype /FreeText /Rect [320 600 540 680] /F 4 /DA (/Helv 12 Tf 0 0 1 rg) /Contents (Not) /AP << /N 10 0 R >> >>",
    stream("/Type /XObject /Subtype /Form /BBox [0 0 220 80]", "0 0 1 rg 0 0 220 80 re f"),
  );
  return writePdf(objects);
}

/**
 * Marka geometrisi korpusu. Sayfa 1 MediaBox'ı /Pages'ten MİRAS alır (A4);
 * 2: CropBox ≠ MediaBox; 3: yatay ve /Rotate 90; 4: başlangıcı (0,0) olmayan
 * küçük MediaBox.
 */
export const BOX_PAGES = [
  { media: [0, 0, 595, 842], inherited: true },
  { media: [0, 0, 612, 792], crop: [36, 36, 576, 756] },
  { media: [0, 0, 842, 595], rotate: 90 },
  { media: [100, 100, 400, 520] },
];
export function boxesPdf() {
  const objects = ["<< /Type /Catalog /Pages 2 0 R >>", null];
  const kids = [];
  for (const p of BOX_PAGES) {
    const [x0, y0, x1, y1] = p.crop ?? p.media;
    const content = objects.push(stream("", `q 0 0 0 RG 2 w ${x0 + 6} ${y0 + 6} ${x1 - x0 - 12} ${y1 - y0 - 12} re S 0.2 0.3 0.4 rg ${x0 + 20} ${y1 - 60} ${(x1 - x0) / 2} 24 re f Q`));
    const own = [
      p.inherited ? "" : ` /MediaBox [${p.media.join(" ")}]`,
      p.crop ? ` /CropBox [${p.crop.join(" ")}]` : "",
      p.rotate ? ` /Rotate ${p.rotate}` : "",
    ].join("");
    kids.push(objects.push(`<< /Type /Page /Parent 2 0 R${own} /Resources << >> /Contents ${content} 0 R >>`));
  }
  objects[1] = `<< /Type /Pages /Kids [${kids.map((k) => `${k} 0 R`).join(" ")}] /Count ${kids.length} /MediaBox [0 0 595 842] >>`;
  return writePdf(objects);
}

/** Sayfa sırası çıktıda okunabilsin diye her sayfanın genişliği farklı. */
export const sizedPdf = (widths, height = 300) => minimalPdf(widths.map((w) => ({ media: [0, 0, w, height] })));

/**
 * Sıkıştırılabilir taranmış sayfa: A4'e yayılmış `w × h` RGB görsel, Flate.
 * Yumuşak geçiş + düşük genlikli gürültü: Flate'e direnir, JPEG'e iyi gider
 * ve motorun kalite denetimini (ortalama sapma ≤ 20) rahat geçer.
 */
export function compressiblePdf(w = 1600, h = 1600) {
  const raw = Buffer.alloc(w * h * 3);
  let seed = 0x2f6b1d;
  const noise = () => { seed = (Math.imul(seed, 1103515245) + 12345) >>> 0; return ((seed >>> 16) % 21) - 10; };
  for (let y = 0, o = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      raw[o++] = Math.max(0, Math.min(255, Math.round((x * 255) / w) + noise()));
      raw[o++] = Math.max(0, Math.min(255, Math.round((y * 255) / h) + noise()));
      raw[o++] = Math.max(0, Math.min(255, 140 + noise()));
    }
  }
  return writePdf([
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>",
    stream("", "q 595 0 0 842 0 0 cm /Im0 Do Q"),
    { dict: `/Type /XObject /Subtype /Image /Width ${w} /Height ${h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode`, stream: deflateSync(raw) },
  ]);
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};

/** Bağımlılıksız ZIP (deflate). Girdiler: [ad, Buffer|string]. */
export function zip(entries) {
  const locals = [], centrals = [];
  let offset = 0;
  for (const [name, content] of entries) {
    const data = Buffer.isBuffer(content) ? content : Buffer.from(content, "utf8");
    const packed = deflateRawSync(data);
    const fname = Buffer.from(name, "utf8");
    const crc = crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0); local.writeUInt16LE(20, 4); local.writeUInt16LE(0x0800, 6); local.writeUInt16LE(8, 8);
    local.writeUInt16LE(0, 10); local.writeUInt16LE(0x21, 12); local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(packed.length, 18); local.writeUInt32LE(data.length, 22); local.writeUInt16LE(fname.length, 26); local.writeUInt16LE(0, 28);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0); central.writeUInt16LE(20, 4); central.writeUInt16LE(20, 6); central.writeUInt16LE(0x0800, 8);
    central.writeUInt16LE(8, 10); central.writeUInt16LE(0, 12); central.writeUInt16LE(0x21, 14); central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(packed.length, 20); central.writeUInt32LE(data.length, 24); central.writeUInt16LE(fname.length, 28);
    central.writeUInt32LE(offset, 42);
    locals.push(local, fname, packed);
    centrals.push(central, fname);
    offset += 30 + fname.length + packed.length;
  }
  const dir = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(dir.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, dir, end]);
}

const xmlText = (t) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
/** Asgari DOCX: her öğe bir paragraf, tek koşu, boşluklar korunur. */
export function docx(paragraphs) {
  const body = paragraphs.map((p) => `<w:p><w:r><w:t xml:space="preserve">${xmlText(p)}</w:t></w:r></w:p>`).join("");
  return zip([
    ["[Content_Types].xml", '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'],
    ["_rels/.rels", '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'],
    ["word/_rels/document.xml.rels", '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>'],
    ["word/document.xml", `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${body}<w:sectPr/></w:body></w:document>`],
  ]);
}

/**
 * Karşılaştır korpusu: tutar (15.000 → 18.500, bitişik "TL'dir" ve cümle
 * sonu noktası), sözcük (beşinci → onuncu), ekleme (+Kaya) ve silinen bir
 * paragraf. Tutar paragrafın başında değil (o ayrı bir kural). Değişiklikler
 * arasında değişmeyen paragraflar durur: hizalama komşu değişiklikleri tek
 * satırda toplamasın, tutar satırları kaynaktaki paragrafın kendisi olsun.
 */
const COMPARE_ANCHORS = [
  "Kiralanan taşınmaz konut olarak kullanılacaktır.",
  "Taraflar bu sözleşmeyi okuyarak imzalamıştır.",
  "Sözleşme bir yıl süreyle geçerlidir.",
  "Uyuşmazlıklarda İstanbul mahkemeleri yetkilidir.",
];
export const COMPARE_BASE = [
  "KİRA SÖZLEŞMESİ",
  "Aylık kira bedeli 15.000TL'dir.",
  COMPARE_ANCHORS[0],
  "Kira bedeli her ayın beşinci günü ödenir.",
  COMPARE_ANCHORS[1],
  "Depozito hiçbir koşulda iade edilmez.",
  COMPARE_ANCHORS[2],
  "Kiracı: Ahmet Yılmaz.",
  COMPARE_ANCHORS[3],
  "Toplam bedel 18.000.",
];
export const COMPARE_REVISED = [
  "KİRA SÖZLEŞMESİ",
  "Aylık kira bedeli 18.500TL'dir.",
  COMPARE_ANCHORS[0],
  "Kira bedeli her ayın onuncu günü ödenir.",
  COMPARE_ANCHORS[1],
  COMPARE_ANCHORS[2],
  "Kiracı: Ahmet Yılmaz Kaya.",
  COMPARE_ANCHORS[3],
  "Toplam bedel 18.500.",
];
/** Karşılaştır'ın tutar satırları: kaynaktaki paragrafın BİREBİR kendisi olmalı. */
export const COMPARE_AMOUNT_ROWS = ["Aylık kira bedeli 18.500TL'dir.", "Toplam bedel 18.500."];

// ---------------------------------------------------------------- PDF okuyucu

const WS = new Set([0, 9, 10, 12, 13, 32].map((c) => String.fromCharCode(c)));
const DELIMS = new Set("()<>[]{}/%");
const regular = (c) => c !== undefined && !WS.has(c) && !DELIMS.has(c);

function skipSpace(s, i) {
  while (i < s.length) {
    if (s[i] === "%") { while (i < s.length && s[i] !== "\n" && s[i] !== "\r") i++; }
    else if (WS.has(s[i])) i++;
    else break;
  }
  return i;
}

/** Tek PDF değeri: sayı, "/Ad", { str }, { ref }, dizi, sözlük (nesne), { op }. */
function parseValue(s, i) {
  i = skipSpace(s, i);
  const c = s[i];
  if (c === "<" && s[i + 1] === "<") {
    i += 2;
    const dict = {};
    for (;;) {
      i = skipSpace(s, i);
      if (i >= s.length) throw new Error("sözlük kapanmadı");
      if (s[i] === ">" && s[i + 1] === ">") return [dict, i + 2];
      const [key, j] = parseValue(s, i);
      const [value, k] = parseValue(s, j);
      if (typeof key === "string" && key.startsWith("/")) dict[key.slice(1)] = value;
      i = k;
    }
  }
  if (c === "[") {
    i++;
    const list = [];
    for (;;) {
      i = skipSpace(s, i);
      if (i >= s.length) throw new Error("dizi kapanmadı");
      if (s[i] === "]") return [list, i + 1];
      const [value, j] = parseValue(s, i);
      list.push(value);
      i = j;
    }
  }
  if (c === "/") {
    let j = i + 1;
    while (regular(s[j])) j++;
    return ["/" + s.slice(i + 1, j).replace(/#([0-9a-fA-F]{2})/g, (_, h) => String.fromCharCode(parseInt(h, 16))), j];
  }
  if (c === "(") {
    let depth = 1, j = i + 1, out = "";
    while (j < s.length && depth > 0) {
      const ch = s[j];
      if (ch === "\\") { out += s[j + 1] ?? ""; j += 2; continue; }
      if (ch === "(") depth++;
      if (ch === ")" && --depth === 0) break;
      out += ch;
      j++;
    }
    return [{ str: out }, j + 1];
  }
  if (c === "<") {
    const end = s.indexOf(">", i);
    const hex = s.slice(i + 1, end).replace(/\s+/g, "");
    return [{ str: Buffer.from(hex.length % 2 ? hex + "0" : hex, "hex").toString("latin1") }, end + 1];
  }
  let j = i;
  while (regular(s[j])) j++;
  if (j === i) throw new Error(`beklenmeyen karakter ${JSON.stringify(c)} @${i}`);
  const token = s.slice(i, j);
  if (/^[+-]?(\d+\.?\d*|\.\d+)$/.test(token)) {
    if (/^\d+$/.test(token)) {
      const ref = /^[\0\t\n\f\r ]+(\d+)[\0\t\n\f\r ]+R(?![^\0\t\n\f\r ()<>[\]{}/%])/.exec(s.slice(j, j + 32));
      if (ref) return [{ ref: Number(token), gen: Number(ref[1]) }, j + ref[0].length];
    }
    return [Number(token), j];
  }
  if (token === "true") return [true, j];
  if (token === "false") return [false, j];
  if (token === "null") return [null, j];
  return [{ op: token }, j];
}

function decodeStream(obj) {
  const filters = [obj.dict.Filter].flat().filter(Boolean);
  let data = obj.raw;
  for (const f of filters) {
    if (f === "/FlateDecode" || f === "/Fl") data = inflateSync(data);
    else return null;
  }
  return data;
}

/**
 * Küçük PDF kutu okuyucu: nesneleri metinden ve nesne akışlarından toplar,
 * sayfa ağacını mirasla yürür. Sayfa başına MediaBox, CropBox, /Rotate,
 * açıklama sayısı ve GölgeDosya işareti çağrı sayısı (`marks`) döner.
 *
 * İşaret: sayfa kaynaklarındaki bir Form XObject'in içeriği marka formunun
 * baytlarıysa (`q /BrandAlpha gs /Mark Do Q`) ya da kaynakları /BrandAlpha +
 * /Mark taşıyorsa marka formudur; sayfa içeriğinde o adın her `Do`'su bir işarettir.
 */
export function readPdf(buffer) {
  const s = buffer.toString("latin1");
  const objects = new Map();
  const head = /(\d+)[\0\t\n\f\r ]+(\d+)[\0\t\n\f\r ]+obj\b/g;
  let m;
  while ((m = head.exec(s))) {
    try {
      const [value, after] = parseValue(s, m.index + m[0].length);
      const obj = { value };
      const k = skipSpace(s, after);
      if (s.startsWith("stream", k) && value && typeof value === "object" && !Array.isArray(value)) {
        let start = k + 6;
        if (s[start] === "\r") start++;
        if (s[start] === "\n") start++;
        const declared = typeof value.Length === "number" ? value.Length : -1;
        const end = declared >= 0 && s.startsWith("endstream", skipSpace(s, start + declared)) ? start + declared : s.indexOf("endstream", start);
        obj.dict = value;
        obj.raw = buffer.subarray(start, end);
        head.lastIndex = end;
      }
      objects.set(Number(m[1]), obj);
    } catch {
      // Bozuk nesne: atlanır.
    }
  }
  for (const obj of [...objects.values()]) {
    if (obj.dict?.Type !== "/ObjStm") continue;
    const data = decodeStream(obj)?.toString("latin1");
    if (!data) continue;
    const numbers = data.slice(0, obj.dict.First).trim().split(/\s+/).map(Number);
    for (let n = 0; n + 1 < numbers.length; n += 2) {
      if (objects.has(numbers[n])) continue;
      try { objects.set(numbers[n], { value: parseValue(data, obj.dict.First + numbers[n + 1])[0] }); } catch { /* atla */ }
    }
  }
  const get = (v) => {
    for (let guard = 0; v && typeof v === "object" && "ref" in v && guard < 32; guard++) v = objects.get(v.ref)?.value;
    return v;
  };
  const object = (v) => (v && typeof v === "object" && "ref" in v ? objects.get(v.ref) : null);
  let root = null;
  const trailer = s.lastIndexOf("trailer");
  if (trailer >= 0) { try { root = parseValue(s, trailer + 7)[0].Root; } catch { /* aşağıda */ } }
  if (!root) {
    for (const [n, obj] of objects) {
      if (obj.dict?.Type === "/XRef" && obj.dict.Root) root = obj.dict.Root;
      else if (!root && get(obj.value)?.Type === "/Catalog") root = { ref: n, gen: 0 };
    }
  }
  const catalog = get(root);
  if (!catalog) throw new Error("katalog bulunamadı");
  const pages = [];
  const seen = new Set();
  const walk = (ref, inherited) => {
    const node = get(ref);
    if (!node || seen.has(node)) return;
    seen.add(node);
    const own = { ...inherited };
    for (const key of ["MediaBox", "CropBox", "Rotate", "Resources"]) if (node[key] !== undefined) own[key] = get(node[key]);
    if (get(node.Type) === "/Pages" || (Array.isArray(get(node.Kids)) && get(node.Type) !== "/Page")) {
      for (const kid of get(node.Kids) ?? []) walk(kid, own);
    } else pages.push({ node, ...own });
  };
  walk(catalog.Pages, {});
  const box = (b) => {
    const a = Array.isArray(b) ? b.map(get) : null;
    if (!a || a.length !== 4 || a.some((v) => typeof v !== "number")) return null;
    return [Math.min(a[0], a[2]), Math.min(a[1], a[3]), Math.max(a[0], a[2]), Math.max(a[1], a[3])];
  };
  const isBrandForm = (ref) => {
    const obj = object(ref);
    if (!obj?.dict || get(obj.dict.Subtype) !== "/Form") return false;
    const body = (() => { try { return decodeStream(obj)?.toString("latin1").trim(); } catch { return null; } })();
    if (body === "q /BrandAlpha gs /Mark Do Q") return true;
    const res = get(obj.dict.Resources);
    return Boolean(get(res?.ExtGState)?.BrandAlpha !== undefined && get(res?.XObject)?.Mark !== undefined);
  };
  return {
    pageCount: pages.length,
    pages: pages.map((p) => {
      const contents = [get(p.node.Contents)].flat().filter(Boolean);
      const refs = Array.isArray(get(p.node.Contents)) ? get(p.node.Contents) : [p.node.Contents];
      const text = refs.map((r) => { const o = object(r); try { return o ? decodeStream(o)?.toString("latin1") ?? "" : ""; } catch { return ""; } }).join("\n");
      const xobjects = get(get(p.Resources)?.XObject) ?? {};
      const brandNames = Object.entries(xobjects).filter(([, ref]) => isBrandForm(ref)).map(([name]) => name);
      const marks = brandNames.reduce((n, name) => n + (text.match(new RegExp(`/${name.replace(/[^A-Za-z0-9_]/g, (c) => `\\${c}`)}[\\0\\t\\n\\f\\r ]+Do\\b`, "g"))?.length ?? 0), 0);
      return {
        media: box(p.MediaBox),
        crop: p.CropBox === undefined ? null : box(p.CropBox),
        rotate: typeof p.Rotate === "number" ? p.Rotate : 0,
        annots: (get(p.node.Annots) ?? []).length,
        brandNames,
        marks,
        streams: contents.length,
      };
    }),
  };
}

// ---------------------------------------------------------------- PNG çözücü

/** 8 bit, taramasız PNG (gri, RGB, gri+alfa, RGBA). `at(x, y)` → [r, g, b]. */
export function decodePng(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error("PNG değil");
  let i = 8, width = 0, height = 0, depth = 0, type = 0, interlace = 0;
  const idat = [];
  while (i < buf.length) {
    const len = buf.readUInt32BE(i);
    const kind = buf.toString("latin1", i + 4, i + 8);
    const data = buf.subarray(i + 8, i + 8 + len);
    if (kind === "IHDR") { width = data.readUInt32BE(0); height = data.readUInt32BE(4); depth = data[8]; type = data[9]; interlace = data[12]; }
    else if (kind === "IDAT") idat.push(data);
    else if (kind === "IEND") break;
    i += 12 + len;
  }
  const channels = { 0: 1, 2: 3, 4: 2, 6: 4 }[type];
  if (depth !== 8 || !channels || interlace) throw new Error(`desteklenmeyen PNG: derinlik ${depth}, tür ${type}, tarama ${interlace}`);
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const out = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? out[y * stride + x - channels] : 0;
      const b = y > 0 ? out[(y - 1) * stride + x] : 0;
      const c = x >= channels && y > 0 ? out[(y - 1) * stride + x - channels] : 0;
      let v = line[x];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) { const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c); v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c; }
      out[y * stride + x] = v & 0xff;
    }
  }
  const at = (x, y) => {
    const o = (Math.min(height - 1, Math.max(0, y)) * width + Math.min(width - 1, Math.max(0, x))) * channels;
    return channels >= 3 ? [out[o], out[o + 1], out[o + 2]] : [out[o], out[o], out[o]];
  };
  return { width, height, channels, at };
}

/** Renk eşleşmesi: her kanal ±`tolerance`. */
export const near = (rgb, want, tolerance = 60) => rgb.every((v, k) => Math.abs(v - want[k]) <= tolerance);

/** PDF noktasının (pt) görseldeki pikseli: görünür kutu tüm görsele yayılır. */
export const pixelOf = (probe, page, width, height) => ({
  x: Math.floor(((probe.x - page[0]) / (page[2] - page[0])) * width),
  y: Math.floor(((page[3] - probe.y) / (page[3] - page[1])) * height),
});

const sameBox = (a, b) => (a === null && b === null) || (Array.isArray(a) && Array.isArray(b) && a.every((v, k) => Math.abs(v - b[k]) < 0.01));

// ======================================================================
// Senaryolar
// ======================================================================

/**
 * @param ctx { cdp, invoke, native, mode, clickText, check, observe, sleep, dir, shot, shots }
 */
export async function v032Saha(ctx) {
  const { cdp, native, sleep } = ctx;
  const ROOT = join(ctx.dir, "v0.3.2 saha");
  const SHOTS = ctx.shots ?? ROOT;
  const RUN = Date.now().toString(36);
  mkdirSync(ROOT, { recursive: true });

  // Senaryo süresi dolunca arka planda süren iş bir sonraki yardımcı
  // çağrısında durur: sonraki senaryoya tıklama, tuş ya da iddia göndermez.
  // Belirteç senaryonun async bağlamında taşınır (AsyncLocalStorage).
  const scope = new AsyncLocalStorage();
  const alive = () => { if (scope.getStore()?.dead) throw new Error("senaryo süresi doldu"); };
  const check = (...a) => { alive(); ctx.check(...a); };
  const observe = (...a) => { alive(); ctx.observe(...a); };
  const shot = (name) => { alive(); return ctx.shot(name); };
  const eval_ = async (expression) => { alive(); return cdp.eval(expression); };
  const send = async (method, params) => { alive(); return cdp.send(method, params); };
  const invoke = async (cmd, payload) => { alive(); return ctx.invoke(cmd, payload); };
  /**
   * Komutun sonucu ya da HATA METNİ. Tauri komutu `Err(String)` ile düz bir
   * dizeyle reddeder; CDP'ye kadar gelen istisna yalnız "Uncaught (in
   * promise)" der ve metin kaybolur. Ret sayfanın içinde yakalanır.
   */
  const attempt = (cmd, payload) => eval_(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(payload)})
    .then((value) => ({ value }), (e) => ({ error: typeof e === 'string' ? e : String(e?.message ?? JSON.stringify(e)) }))`);
  const clickText = async (...a) => { alive(); return ctx.clickText(...a); };
  const waitFor = async (expression, timeoutMs, what) => { alive(); return cdp.until(expression, { timeoutMs, what }); };
  const nativeLive = (action, extra) => { alive(); return native(action, extra); };
  const folder = (name) => { const d = join(ROOT, name); mkdirSync(d, { recursive: true }); return d; };
  const ready = () => waitFor("document.readyState === 'complete' && !!document.querySelector('.sidebar-nav')", 30_000, "kabuk hazır");
  const clearEmulation = () => cdp.send("Emulation.clearDeviceMetricsOverride").catch(() => {});

  /** Kip + son belgelerden açma: kullanıcının yolu (yeniden yüklemeyle temiz başlangıç). */
  const openFrom = async (label, paths, row) => {
    await invoke("remember_documents", { paths });
    await send("Page.reload");
    await ready();
    await sleep(600);
    alive();
    await ctx.mode(label);
    if (!(await clickText(".file-row", row))) throw new Error(`son belgelerde satır yok: ${row}`);
  };
  const openPdf = async (path, pages) => {
    await openFrom("Düzenle", [path], basename(path));
    await waitFor(`document.querySelectorAll('.thumbnail-list .thumbnail').length === ${pages}
      && !!document.querySelector('.pdf-page-viewport .pdf-page-surface img, .pdf-preview-panel [data-tone="error"]')`, 60_000, `Düzenle: ${basename(path)}`);
    await sleep(500);
  };
  const pickTool = (key, label) => eval_(`(() => {
    const seg = document.querySelector('.pdf-strip [data-tool="${key}"]');
    if (seg && !seg.disabled) { seg.click(); return 'şerit'; }
    const btn = [...document.querySelectorAll('button.tool')].find((b) => b.textContent.trim() === ${JSON.stringify(label)});
    if (btn && !btn.disabled) { btn.click(); return 'panel'; }
    return null;
  })()`);
  const pdfStatus = () => eval_(`(() => {
    const s = document.querySelector('.pdf-preview-panel > .status');
    if (!s) return null;
    const c = s.cloneNode(true); c.querySelectorAll('.status-mark, .spinner').forEach((e) => e.remove());
    return { tone: s.dataset.tone ?? null, text: c.textContent.trim() };
  })()`);
  /**
   * Bardaki kaydetme düğmesi → gerçek Kaydet penceresi → sonuç satırı.
   * `target` verilirse ad kutusuna tam yol yazılır; verilmezse pencerenin
   * önerdiği ad ve klasör kabul edilir.
   */
  const saveVia = async (button, target, timeoutMs = 90_000) => {
    const before = (await pdfStatus())?.text ?? "";
    if (!(await clickText("button", button, true))) return { clicked: false };
    const dialog = nativeLive("filedialog", target ? { Path: target } : {});
    if (!dialog.found || !dialog.closed) return { clicked: true, dialog };
    let status = null;
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      status = await pdfStatus();
      if (status && status.text !== before && (status.tone === "success" || status.tone === "error")) break;
      if (status && status.text !== before && status.tone === "info" && /Çıktı oluşturulmadı/.test(status.text)) break;
      await sleep(400);
    }
    return { clicked: true, dialog, status };
  };
  /**
   * Şeritteki sayfalar: kaynak sayfa no, merkez, kutu ve Seç durumu (CSS px).
   * Seçici iki sürümde de tutar (v0.3.1'de `[role=list]` sarmalayıcısı yok).
   */
  const strip = () => eval_(`(() => {
    const list = document.querySelector('.thumbnail-list');
    const r = list?.getBoundingClientRect();
    const items = [...document.querySelectorAll('.thumbnail-list .thumbnail')].map((t) => {
      const b = t.querySelector('.thumbnail-image-button'), br = b.getBoundingClientRect(), tr = t.getBoundingClientRect();
      const page = Number((/ sayfa (\\d+) görüntüle$/.exec(b.getAttribute('aria-label') ?? '') ?? [])[1]);
      return { page, x: br.left + br.width / 2, y: br.top + br.height / 2, mid: tr.top + tr.height / 2, top: tr.top, bottom: tr.bottom,
        checked: !!t.querySelector('.thumbnail-pick input')?.checked, selected: t.classList.contains('selected') };
    });
    return { list: r ? { top: r.top, bottom: r.bottom, left: r.left, right: r.right } : null, items };
  })()`);
  const order = async () => (await strip()).items.map((t) => t.page);
  const mouse = (type, x, y, extra = {}) => send("Input.dispatchMouseEvent", {
    type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1, ...extra,
  });
  /** Gerçek işaretçi tıklaması (CDP girdi olayı; isteğe bağlı Ctrl). */
  const click = async (x, y, modifiers = 0) => {
    await mouse("mousePressed", x, y, { modifiers });
    await sleep(40);
    await mouse("mouseReleased", x, y, { modifiers });
    await sleep(250);
  };
  /** `from` sırasındaki sayfayı işaretçiyle `gap` boşluğuna sürükler (0: en başa). */
  const drag = async (from, gap) => {
    await eval_(`document.querySelector('.thumbnail-list').scrollTop = 0`);
    await sleep(200);
    const { list, items } = await strip();
    const source = items[from];
    const y = gap === 0 ? items[0].top + 6
      : gap >= items.length ? items[items.length - 1].bottom - 6
        : (items[gap - 1].mid + items[gap].mid) / 2;
    // Şerit kenarına 40 px'ten yakın nokta şeridi kendiliğinden kaydırır;
    // bırakma yeri ölçüm sırasında kayardı. En baştaki boşluk istisnadır
    // (şerit zaten başta, kaydıramaz).
    if (!list || (gap !== 0 && y < list.top + 44) || y > list.bottom - 44 || source.y < list.top || source.y > list.bottom) {
      throw new Error(`sürükleme noktası şeridin görünen alanı dışında: ${JSON.stringify({ list, from: source, y })}`);
    }
    await mouse("mousePressed", source.x, source.y);
    await sleep(60);
    await mouse("mouseMoved", source.x, source.y + 2);
    const steps = 10;
    for (let k = 1; k <= steps; k++) {
      await mouse("mouseMoved", source.x, source.y + ((y - source.y) * k) / steps);
      await sleep(35);
    }
    await mouse("mouseMoved", source.x, y);
    await sleep(250);
    const indicator = await eval_(`(() => { const d = document.querySelector('.thumbnail[data-drop]'); return d ? d.dataset.drop : null; })()`);
    await mouse("mouseReleased", source.x, y);
    await sleep(500);
    return { from: source.page, y: Math.round(y), indicator };
  };
  /** Ctrl+Z, gerçek (isTrusted) tuş olayı; odak bir metin alanında değil. */
  const ctrlZ = async () => {
    await eval_(`(() => { const a = document.activeElement; if (a && a !== document.body && a.blur) a.blur(); return true; })()`);
    await send("Input.dispatchKeyEvent", { type: "rawKeyDown", modifiers: 2, key: "Control", code: "ControlLeft", windowsVirtualKeyCode: 17 });
    await send("Input.dispatchKeyEvent", { type: "rawKeyDown", modifiers: 2, key: "z", code: "KeyZ", windowsVirtualKeyCode: 90 });
    await send("Input.dispatchKeyEvent", { type: "keyUp", modifiers: 2, key: "z", code: "KeyZ", windowsVirtualKeyCode: 90 });
    await send("Input.dispatchKeyEvent", { type: "keyUp", modifiers: 0, key: "Control", code: "ControlLeft", windowsVirtualKeyCode: 17 });
    await sleep(400);
  };
  const widthsOf = (path) => readPdf(readFileSync(path)).pages.map((p) => (p.media ? Math.round(p.media[2] - p.media[0]) : null));
  /** Kırpılmış kanıt: CDP ekran görüntüsü, öğenin kutusu (CSS px). */
  const clipShot = async (name, selector) => {
    try {
      const r = await eval_(`(() => { const e = document.querySelector(${JSON.stringify(selector)}); if (!e) return null; e.scrollIntoView({ block: 'center' }); const b = e.getBoundingClientRect(); return { x: b.left, y: b.top, width: b.width, height: b.height }; })()`);
      if (!r || r.width < 1 || r.height < 1) return null;
      const img = await send("Page.captureScreenshot", { format: "png", clip: { ...r, scale: 2 } });
      const file = join(SHOTS, `v032-${name}.png`);
      writeFileSync(file, Buffer.from(img.data, "base64"));
      return file;
    } catch (e) {
      observe("v0.3.2", `kırpılmış görüntü alınamadı: ${name}`, String(e?.message ?? e));
      return null;
    }
  };
  const norm = (p) => (p ?? "").toLocaleLowerCase("tr-TR").replace(/[\\/]+$/, "");

  // ------------------------------------------------------------ senaryolar

  /** a02 — açıklamalar önizlemede ve PNG'de (v0.3.1: dışa aktarma reddediyordu). */
  async function annotations() {
    const dir = folder("Açıklamalar");
    const path = join(dir, `v032 Açıklamalı ${RUN}.pdf`);
    writeFileSync(path, annotatedPdf());
    const bare = join(dir, `v032 Görünümsüz not ${RUN}.pdf`);
    writeFileSync(bare, annotatedPdf({ appearance: false }));

    // Önizleme: büyük sayfa görseli, sayfa içinde kanvasla örneklenir.
    await openPdf(path, 1);
    await sleep(1500);
    const preview = await eval_(`(async () => {
      const img = document.querySelector('.pdf-page-viewport .pdf-page-surface img');
      if (!img) return { error: [...document.querySelectorAll('.pdf-preview-panel [data-tone="error"]')].map((e) => e.textContent) };
      await img.decode().catch(() => {});
      const c = document.createElement('canvas'); c.width = img.naturalWidth; c.height = img.naturalHeight;
      const g = c.getContext('2d'); g.drawImage(img, 0, 0);
      const page = ${JSON.stringify(ANNOTATION_PAGE)};
      const samples = ${JSON.stringify(ANNOTATION_PROBES)}.map((p) => {
        const x = Math.floor((p.x - page[0]) / (page[2] - page[0]) * c.width), y = Math.floor((page[3] - p.y) / (page[3] - page[1]) * c.height);
        return Array.from(g.getImageData(x, y, 1, 1).data.slice(0, 3));
      });
      return { width: c.width, height: c.height, samples };
    })()`);
    shot("v032-aciklama-onizleme");
    const previewOk = Array.isArray(preview?.samples) && ANNOTATION_PROBES.every((p, k) => near(preview.samples[k], p.rgb));
    check("a02", "Açıklamalar (Highlight/Ink/FreeText, /AP) Düzenle önizlemesinde görünür", previewOk,
      { ...preview, beklenen: ANNOTATION_PROBES.map((p) => `${p.name}=${p.rgb}`) });

    // PNG dışa aktarma (150 DPI): komutun kendisi, kullanıcının "Görsel Paketi Kaydet"i.
    const out = folder(`Görsel çıktı ${RUN}`);
    const { value: exportedDir, error: exportError } = await attempt("duzenek_pdf_to_images", { path, outputDir: out, format: "png", dpi: 150, approved: false });
    const exported = { dir: exportedDir, error: exportError };
    let png = null;
    if (exported.dir) {
      const file = join(exported.dir, "sayfa-0001.png");
      if (existsSync(file)) {
        const img = decodePng(readFileSync(file));
        png = {
          file, width: img.width, height: img.height,
          beklenenBoyut: [Math.ceil((612 * 150) / 72), Math.ceil((792 * 150) / 72)],
          samples: ANNOTATION_PROBES.map((p) => { const { x, y } = pixelOf(p, ANNOTATION_PAGE, img.width, img.height); return img.at(x, y); }),
        };
      }
    }
    const pngOk = png && png.width === png.beklenenBoyut[0] && png.height === png.beklenenBoyut[1]
      && ANNOTATION_PROBES.every((p, k) => near(png.samples[k], p.rgb));
    check("a02", "PDF → PNG: açıklamalar görselde (piksel rengi dikdörtgenin içinde), sayfa boyu değişmeden", pngOk, { ...exported, png });

    // Görünümü kayıtlı olmayan açıklama: dar ret, yeni klasör yok.
    const bareOut = folder(`Görünümsüz çıktı ${RUN}`);
    const { value: refusedDir, error: refusedError } = await attempt("duzenek_pdf_to_images", { path: bare, outputDir: bareOut, format: "png", dpi: 150, approved: false });
    const refused = { dir: refusedDir, error: refusedError };
    const leftovers = readdirSync(bareOut).filter((n) => n.startsWith("GolgeDosya-Gorseller-"));
    check("a02", "/AP'siz FreeText: dışa aktarma açıklamanın görünümünü adlandırarak reddeder, klasör oluşmaz",
      !refused.dir && /görünümü belgede kayıtlı değil/.test(refused.error ?? "") && leftovers.length === 0, { ...refused, klasorler: leftovers });
  }

  /** Marka bindirmesi: MediaBox/CropBox/Rotate aynen, sayfa başına tek işaret (v0.3.1: 34 pt pay). */
  async function brandGeometry() {
    const dir = folder("Marka geometrisi");
    const source = join(dir, `v032 Kutular ${RUN}.pdf`);
    writeFileSync(source, boxesPdf());
    const first = join(dir, `v032 Kutular sırala ${RUN}.pdf`);
    const second = join(dir, `v032 Kutular sırala 2 ${RUN}.pdf`);
    const run = async (input, output) => {
      const { value, error } = await attempt("duzenek_run_pdf_tool", {
        paths: [input], operation: { kind: "reorder", pages: BOX_PAGES.map((_, i) => i + 1) }, outputPath: output, approved: false,
      });
      return value ?? { status: "failed", reason: error };
    };
    const src = readPdf(readFileSync(source)).pages;
    const compare = (output) => {
      const pages = readPdf(readFileSync(output)).pages;
      return pages.map((p, i) => ({
        sayfa: i + 1,
        media: [src[i]?.media, p.media],
        crop: [src[i]?.crop, p.crop],
        rotate: [src[i]?.rotate, p.rotate],
        marks: p.marks,
        ok: !!src[i] && sameBox(src[i].media, p.media) && sameBox(src[i].crop, p.crop) && src[i].rotate === p.rotate && p.marks === 1,
      }));
    };
    const one = await run(source, first);
    const gen1 = one?.status === "published" && existsSync(first) ? compare(first) : null;
    check("a04", "Markalı çıktı: her sayfanın MediaBox/CropBox/Rotate'i kaynağınkiyle aynı ve tam bir işaret taşıyor",
      gen1 && gen1.length === BOX_PAGES.length && gen1.every((p) => p.ok), { sonuc: one, sayfalar: gen1 });
    const two = gen1 ? await run(first, second) : null;
    const gen2 = two?.status === "published" && existsSync(second) ? compare(second) : null;
    check("a04", "Markalı çıktıdan yeniden türetme: kutular yine aynı, işaret çoğalmıyor (sayfa başına 1)",
      gen2 && gen2.length === BOX_PAGES.length && gen2.every((p) => p.ok), { sonuc: two, sayfalar: gen2 });
    // Kanıt: ikinci kuşak çıktı Düzenle'de açılır, ekran görüntüsü alınır.
    if (gen2) {
      await openPdf(second, BOX_PAGES.length).catch(() => {});
      shot("v032-marka-geometrisi");
    }
  }

  /** a03 — 1366×768 @%125 ekranda büyütülmüş pencere: Araçlar paneli açık (v0.3.1: 1280 eşiği, kapalı). */
  async function editorPanel() {
    const dir = folder("Panel");
    const path = join(dir, `v032 Altı sayfa ${RUN}.pdf`);
    writeFileSync(path, sizedPdf([595, 595, 595, 595, 595, 595], 842));
    // 1366×768 @%125 ekranda büyütülmüş pencerenin içi: 1093×614 CSS px.
    const emulate = () => send("Emulation.setDeviceMetricsOverride", { width: 1093, height: 614, deviceScaleFactor: 1.25, mobile: false, screenWidth: 1366, screenHeight: 768 });
    const measure = () => eval_(`(() => {
      const slot = document.querySelector('.inspector-slot');
      const panel = slot?.querySelector('.inspector')?.getBoundingClientRect();
      const toggle = document.querySelector('.toolbar-shell .icon-btn[aria-label]');
      const vp = document.querySelector('.pdf-page-viewport')?.getBoundingClientRect();
      return { w: innerWidth, h: innerHeight, dpr: devicePixelRatio, hidden: slot ? slot.hidden : null,
        panel: panel ? { left: Math.round(panel.left), right: Math.round(panel.right), width: Math.round(panel.width) } : null,
        toggle: toggle?.getAttribute('aria-label') ?? null,
        tools: [...document.querySelectorAll('.inspector button.tool')].map((b) => b.textContent.trim()).length,
        viewport: vp ? [Math.round(vp.width), Math.round(vp.height)] : null };
    })()`);
    const open = (st) => st.hidden === false && st.panel && st.panel.width >= 280 && st.panel.right <= st.w + 1 && st.toggle === "Araçları Gizle" && st.tools >= 7;
    // (1) Belge açıkken pencere o boya gelir (canlı yeniden boyutlandırma).
    await openPdf(path, 6);
    await emulate();
    await sleep(1000);
    const live = await measure();
    // (2) Uygulama o ekranda belgeyle açılır (ilk durum).
    await send("Page.reload");
    await ready();
    await emulate();
    await sleep(600);
    await ctx.mode("Düzenle");
    if (!(await clickText(".file-row", basename(path)))) throw new Error("son belgelerde satır yok");
    await waitFor("document.querySelectorAll('.thumbnail-list .thumbnail').length === 6", 60_000, "Düzenle: altı sayfa");
    await sleep(1000);
    const fresh = await measure();
    shot("v032-duzenle-1093x614-125");
    await clipShot("duzenle-1093x614-125", ".content");
    check("kontrol", "Emülasyon gerçekten 1093×614 CSS px, DPR 1,25",
      [live, fresh].every((st) => st.w === 1093 && st.h === 614 && st.dpr === 1.25), { canli: live, ilk: fresh });
    check("a03", "Düzenle 1093×614 (1366×768 @%125, büyütülmüş): Araçlar paneli açık, düğme 'Araçları Gizle' (canlı ve ilk açılış)",
      open(live) && open(fresh), { canli: live, ilk: fresh });
    // Kontrol: dar pencerede kural paneli kapatır (iki sürümde de).
    await send("Emulation.setDeviceMetricsOverride", { width: 900, height: 600, deviceScaleFactor: 1, mobile: false });
    await sleep(800);
    observe("a03", "900×600: panel ve düğme", await measure());
  }

  /** a05 — Birleştir: eklenen PDF açık belgenin ARDINA gelir (v0.3.1: açık belgenin yerine geçiyordu). */
  async function mergeAppend() {
    const dir = folder("Birleştir");
    const a = join(dir, `v032 A ${RUN}.pdf`);
    const b = join(dir, `v032 B ${RUN}.pdf`);
    writeFileSync(a, sizedPdf([400, 410]));
    writeFileSync(b, sizedPdf([500, 510, 520]));
    await openPdf(a, 2);
    const tool = await eval_(`[...document.querySelectorAll('button.tool[aria-pressed="true"]')].map((b) => b.textContent.trim())`);
    if (!(await clickText("button", "Birleştirilecek Belge Ekle", true))) throw new Error("'Birleştirilecek Belge Ekle' düğmesi yok");
    const dialog = nativeLive("filedialog", { Path: b });
    await waitFor("document.querySelectorAll('.source-list li').length >= 2 || document.querySelectorAll('.thumbnail-list .thumbnail').length !== 2", 30_000, "kaynak listesi değişti")
      .catch(() => {});
    await sleep(800);
    const list = await eval_(`({ sources: [...document.querySelectorAll('.source-list li > span')].map((s) => s.textContent),
      bar: [...document.querySelectorAll('.doc-chip-name')].map((e) => e.textContent),
      pages: document.querySelectorAll('.thumbnail-list .thumbnail').length })`);
    shot("v032-birlestir-ekle");
    check("a05", "Birleştir'de eklenen PDF açık belgenin ardına gelir: [A, B], 5 sayfa",
      list.sources.length === 2 && list.sources[0].startsWith(basename(a)) && list.sources[1].startsWith(basename(b)) && list.pages === 5,
      { ...list, arac: tool, pencere: dialog });
    check("a05", "Seçici başlığı açık belgeyi adlandırır ('… ile birleştirilecek PDF’ler')",
      dialog.found && (dialog.title ?? "").includes(`${basename(a)} ile birleştirilecek`), dialog);
    if (list.sources.length !== 2) return;
    const target = join(dir, `v032 Birleştirilmiş ${RUN}.pdf`);
    const saved = await saveVia("Yeni PDF Kaydet", target);
    const widths = existsSync(target) ? widthsOf(target) : null;
    check("a05", "Kaydedilen birleşik PDF: A'nın sayfaları önce (400, 410, 500, 510, 520 pt)",
      JSON.stringify(widths) === JSON.stringify([400, 410, 500, 510, 520]), { genislikler: widths, ...saved });
  }

  /** Sırala — işaretçiyle sürükleme, Ctrl ile çoklu seçim, Ctrl+Z ve taşıma düğmeleri. */
  async function reorder() {
    const dir = folder("Sırala");
    const widths = [420, 440, 460, 480];
    const path = join(dir, `v032 Sırala ${RUN}.pdf`);
    writeFileSync(path, sizedPdf(widths));
    const begin = async () => {
      await openPdf(path, 4);
      const how = await pickTool("reorder", "Sırala");
      // v0.3.1'de bu öznitelik yok: beklenir ama düşülmez, iddialar ölçer.
      await waitFor("!!document.querySelector('.thumbnail-list[data-reorder]')", 10_000, "Sırala şeridi").catch(() => {});
      await sleep(400);
      return how;
    };

    // (1) Tek sayfa: 1. sayfa 3. ile 4. arasına.
    const tool = await begin();
    const moved = await drag(0, 3);
    const after1 = await order();
    shot("v032-sirala-tek-surukle");
    const out1 = join(dir, `v032 Sırala tek ${RUN}.pdf`);
    const saved1 = JSON.stringify(after1) === JSON.stringify([2, 3, 1, 4]) ? await saveVia("Yeni PDF Kaydet", out1) : { atlandi: "sıra değişmedi" };
    const w1 = existsSync(out1) ? widthsOf(out1) : null;
    check("a06", "İşaretçiyle tek sayfa sürüklendi: şerit [2,3,1,4], kaydedilen PDF aynı sırada",
      JSON.stringify(after1) === JSON.stringify([2, 3, 1, 4]) && JSON.stringify(w1) === JSON.stringify([440, 460, 420, 480]),
      { arac: tool, surukleme: moved, sira: after1, genislikler: w1, ...saved1 });
    await ctrlZ();
    const undo1 = await order();
    // Geri alınacak bir taşıma olmadan [1,2,3,4] boş bir başarıdır: önceki sıra da ölçülür.
    check("a06", "Ctrl+Z son taşımayı geri alır: [2,3,1,4] → [1,2,3,4]",
      JSON.stringify(after1) === JSON.stringify([2, 3, 1, 4]) && JSON.stringify(undo1) === JSON.stringify([1, 2, 3, 4]), { once: after1, sonra: undo1 });

    // (2) İki bitişik olmayan sayfa: 2'ye tıkla, 4'e Ctrl+tıkla, 4'ü en başa sürükle.
    await begin();
    let s = await strip();
    await click(s.items[1].x, s.items[1].y);
    s = await strip();
    await click(s.items[3].x, s.items[3].y, 2);
    s = await strip();
    const picked = s.items.filter((t) => t.checked).map((t) => t.page);
    const moved2 = await drag(3, 0);
    const after2 = await order();
    shot("v032-sirala-iki-surukle");
    const out2 = join(dir, `v032 Sırala iki ${RUN}.pdf`);
    const saved2 = JSON.stringify(after2) === JSON.stringify([2, 4, 1, 3]) ? await saveVia("Yeni PDF Kaydet", out2) : { atlandi: "sıra değişmedi" };
    const w2 = existsSync(out2) ? widthsOf(out2) : null;
    check("a06", "Ctrl ile seçilen iki sayfa (2 ve 4) birlikte sürüklendi: [2,4,1,3], kaydedilen PDF aynı sırada",
      JSON.stringify(picked) === JSON.stringify([2, 4]) && JSON.stringify(after2) === JSON.stringify([2, 4, 1, 3])
        && JSON.stringify(w2) === JSON.stringify([440, 480, 420, 460]),
      { secili: picked, surukleme: moved2, sira: after2, genislikler: w2, ...saved2 });
    await ctrlZ();
    const undo2 = await order();
    check("a06", "Ctrl+Z iki sayfalık taşımayı geri alır: [2,4,1,3] → [1,2,3,4]",
      JSON.stringify(after2) === JSON.stringify([2, 4, 1, 3]) && JSON.stringify(undo2) === JSON.stringify([1, 2, 3, 4]), { once: after2, sonra: undo2 });

    // (3) Fare gerekmeyen yol: Seç kutusu + Başa + Geri Al.
    await begin();
    s = await strip();
    await click(s.items[2].x, s.items[2].y);
    await eval_(`(() => { const box = document.querySelectorAll('.thumbnail-list .thumbnail')[0].querySelector('.thumbnail-pick input'); if (box && !box.checked) box.click(); return !!box; })()`);
    await sleep(300);
    const chosen = (await strip()).items.filter((t) => t.checked).map((t) => t.page);
    await clickText("button", "Başa", true);
    await sleep(400);
    const toStart = await order();
    await clickText("button", "Geri Al", true);
    await sleep(400);
    const restored = await order();
    shot("v032-sirala-dugmeler");
    check("a06", "Seç kutusu + 'Başa' seçili sayfaları başa taşır ([1,3,2,4]); 'Geri Al' geri getirir",
      JSON.stringify(chosen) === JSON.stringify([1, 3]) && JSON.stringify(toStart) === JSON.stringify([1, 3, 2, 4])
        && JSON.stringify(restored) === JSON.stringify([1, 2, 3, 4]), { secili: chosen, basa: toStart, geriAl: restored });
  }

  /** a07 — Sıkıştır sonucu Türkçe sayı diliyle (v0.3.1: "13478.1 KB … %96.1"). */
  async function compression() {
    const dir = folder("Sıkıştır");
    const path = join(dir, `v032 Taranmış ${RUN}.pdf`);
    writeFileSync(path, compressiblePdf());
    await openPdf(path, 1);
    const tool = await pickTool("compress", "Sıkıştır");
    await sleep(500);
    const panel = await eval_(`({ source: [...document.querySelectorAll('.source-list li > span')].map((s) => s.textContent),
      preset: (() => { const s = document.querySelector('.inspector select'); return s ? s.options[s.selectedIndex]?.textContent : null; })() })`);
    const target = join(dir, `v032 Sıkıştırılmış ${RUN}.pdf`);
    const saved = await saveVia("Yeni PDF Kaydet", target, 180_000);
    shot("v032-sikistir-sonuc");
    await clipShot("sikistir-sonuc", ".pdf-preview-panel > .status");
    const text = saved.status?.text ?? "";
    const first = text.split("\n")[0];
    check("a07", "Sıkıştırma sonucu: virgüllü boyutlar ve 'yaklaşık %…' (ondalık nokta yok)",
      saved.status?.tone === "success" && existsSync(target)
        && /^Sıkıştırıldı: \d+(,\d)? (KB|MB) → \d+(,\d)? (KB|MB) \(yaklaşık %(\d+|1'den az) küçüldü\)\.$/.test(first)
        && !/\d\.\d/.test(text) && !/Marka dahil/.test(text),
      { arac: tool, ...panel, durum: saved.status, pencere: saved.dialog });
    check("a07", "Kaynak satırı boyutu virgüllü MB ile söyler", panel.source.length === 1 && /· \d+,\d MB$/.test(panel.source[0]), panel);
  }

  /** Büyük sayfa önizlemesi DPR 2'de: A0 ve taranmış büyük sayfa çizilir, "Bozuk" yok (v0.3.1: 40 MP reddi). */
  async function largePages() {
    const dir = folder("Büyük sayfa");
    const docs = [
      { name: `v032 A0 ${RUN}.pdf`, media: [0, 0, 2384, 3370] },
      { name: `v032 Taranmış büyük ${RUN}.pdf`, media: [0, 0, 2480, 3508] },
      { name: `v032 A1 ${RUN}.pdf`, media: [0, 0, 1684, 2384], zoom: 150 },
    ];
    const results = [];
    for (const doc of docs) {
      const path = join(dir, doc.name);
      writeFileSync(path, minimalPdf([{ media: doc.media }]));
      await invoke("remember_documents", { paths: [path] });
      await send("Page.reload");
      await ready();
      await send("Emulation.setDeviceMetricsOverride", { width: 0, height: 0, deviceScaleFactor: 2, mobile: false });
      await sleep(600);
      await ctx.mode("Düzenle");
      if (!(await clickText(".file-row", doc.name))) throw new Error(`son belgelerde satır yok: ${doc.name}`);
      await waitFor(`(() => { const i = document.querySelector('.pdf-page-viewport .pdf-page-surface img'); return (i && i.complete && i.naturalWidth > 0) || !!document.querySelector('.pdf-preview-panel .pdf-preview-stage [data-tone="error"]'); })()`, 60_000, `önizleme: ${doc.name}`)
        .catch(() => {});
      await sleep(2500);
      if (doc.zoom) {
        await eval_(`(() => { const s = document.querySelector('.zoom select'); Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(s, '${doc.zoom}'); s.dispatchEvent(new Event('change', { bubbles: true })); return s.value; })()`);
        await sleep(5000);
      }
      const state = await eval_(`(() => {
        const i = document.querySelector('.pdf-page-viewport .pdf-page-surface img');
        const errors = [...document.querySelectorAll('.pdf-preview-panel .pdf-preview-stage [data-tone="error"], .pdf-preview-panel .pdf-wait-overlay')].map((e) => e.textContent);
        return { dpr: devicePixelRatio, image: i && i.complete ? [i.naturalWidth, i.naturalHeight] : null, errors,
          bozuk: /bozuk/i.test(document.querySelector('section.pdf-root')?.textContent ?? '') };
      })()`);
      shot(`v032-buyuk-sayfa-${doc.media[2]}x${doc.media[3]}${doc.zoom ? `-${doc.zoom}` : ""}`);
      const dpi = state.image ? Math.round((state.image[0] * 72) / doc.media[2]) : null;
      results.push({ ad: doc.name, ...state, dpi });
      check("onizleme-dpi", `${doc.media[2]}×${doc.media[3]} pt sayfa DPR 2${doc.zoom ? `, %${doc.zoom}` : ""}: önizleme çizildi, hata şeridi ve "Bozuk" yok`,
        state.dpr === 2 && state.image && state.errors.length === 0 && !state.bozuk, { ...state, dpi });
      await clearEmulation();
    }
  }

  /** a09 — Karşılaştır: tutar kaynaktaki gibi, değişen sözcükte 2 px alt/üst çizgi. */
  async function compareView() {
    const dir = folder("Karşılaştır");
    const base = join(dir, `v032 Kira temel ${RUN}.docx`);
    const revised = join(dir, `v032 Kira değişik ${RUN}.docx`);
    writeFileSync(base, docx(COMPARE_BASE));
    writeFileSync(revised, docx(COMPARE_REVISED));
    await invoke("remember_documents", { paths: [revised, base] });
    await send("Page.reload");
    await ready();
    await sleep(600);
    await ctx.mode("Karşılaştır");
    await clickText(".file-row", basename(base));
    await sleep(600);
    await clickText(".file-row", basename(revised));
    await waitFor("document.querySelectorAll('.compare-root .document-row .fragment').length > 0", 60_000, "karşılaştırma satırları");
    await sleep(1500);
    const view = await eval_(`(() => {
      const pane = (label) => {
        const p = [...document.querySelectorAll('.document-scroll')].find((e) => e.getAttribute('aria-label') === label);
        if (!p) return null;
        return [...p.querySelectorAll('.document-row .row-body')].map((b) => {
          const body = b.cloneNode(true);
          body.querySelectorAll('.placeholder-copy').forEach((e) => e.remove());
          return body.textContent;
        }).filter(Boolean);
      };
      const style = (sel) => {
        const e = document.querySelector(sel);
        if (!e) return null;
        const s = getComputedStyle(e);
        return { tag: e.tagName.toLowerCase(), text: e.textContent, line: s.textDecorationLine, thickness: s.textDecorationThickness, color: s.textDecorationColor, background: s.backgroundColor };
      };
      const row = document.querySelector('.compare-root .document-row.kind-modified');
      return {
        base: pane('Temel Sürüm'), revised: pane('Değişik Sürüm'),
        added: style('.compare-root .document-row.kind-modified .fragment.added'),
        removed: style('.compare-root .document-row.kind-modified .fragment.removed'),
        rail: row ? getComputedStyle(row).borderLeftWidth : null,
      };
    })()`);
    shot("v032-karsilastir");
    await clipShot("karsilastir-temel", '.compare-root .document-scroll[aria-label="Temel Sürüm"] .document-row.kind-modified');
    await clipShot("karsilastir-degisik", '.compare-root .document-scroll[aria-label="Değişik Sürüm"] .document-row.kind-modified');
    // Hizalama paragrafları bir satırda toplayabilir; her paragraf bir satırın
    // içinde KAYNAKTAKİ yazımıyla geçmeli, tutar satırları paragrafın kendisi olmalı.
    const contains = (rows, expected) => Array.isArray(rows) && expected.every((p) => rows.some((r) => r.includes(p)));
    check("a09", "Karşılaştır: '18.500' kaynaktaki gibi ('18.500TL'dir.', '18.500.'); her paragraf belgedeki yazımıyla",
      contains(view.revised, COMPARE_REVISED) && contains(view.base, COMPARE_BASE)
        && COMPARE_AMOUNT_ROWS.every((p) => view.revised.includes(p))
        && !(view.revised ?? []).some((t) => /18\.500 [.T]|18\. 500/.test(t)),
      { temel: view.base, degisik: view.revised });
    check("a09", "Değişen sözcük: eklenen 2 px alt çizgili, çıkarılan 2 px üstü çizili; satır rayı 3 px",
      view.added && /underline/.test(view.added.line) && view.added.thickness === "2px"
        && view.removed && /line-through/.test(view.removed.line) && view.removed.thickness === "2px" && view.rail === "3px",
      { eklenen: view.added, cikarilan: view.removed, ray: view.rail });
    // İnsan gözü için: değişen paragraf gerçek seçim rengi altında (2 px çizgiler görünür kalmalı).
    await eval_(`(() => { const r = document.querySelector('.compare-root .document-scroll[aria-label="Değişik Sürüm"] .document-row.kind-modified .row-body'); if (!r) return false;
      const range = document.createRange(); range.selectNodeContents(r); const s = getSelection(); s.removeAllRanges(); s.addRange(range); return true; })()`);
    await sleep(300);
    shot("v032-karsilastir-secili");
  }

  /** a10 — Denetle: fazla boşluk ␣ ile, gerçek bir yazı tipiyle çizilir; kopya belgedeki metindir. */
  async function whitespaceGlyph() {
    const dir = folder("Denetle");
    const path = join(dir, `v032 Dilekçe boşluk ${RUN}.docx`);
    copyFileSync(join(REPO, "crates/ikincigoz-core/tests/samples/ornek-dilekce-hatali.docx"), path);
    await openFrom("Denetle", [path], basename(path));
    await waitFor("document.querySelectorAll('.findings .finding').length > 0", 90_000, "bulgular");
    await sleep(800);
    const found = await eval_(`(async () => {
      const frame = () => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 30)));
      const heads = [...document.querySelectorAll('.findings .finding > .finding-head')];
      for (const [i, head] of heads.entries()) {
        const li = head.closest('.finding');
        if (li.dataset.active !== 'true') { head.click(); await frame(); }
        const excerpt = li.querySelector('.finding-excerpt');
        if (excerpt && excerpt.querySelector('mark[data-visible-ws]') && excerpt.textContent.includes('Dava,')) return { index: i, title: head.textContent };
      }
      return null;
    })()`);
    if (!found) throw new Error("'Dava,  görevli' bulgusu bulunamadı");
    await sleep(400);
    const ws = await eval_(`(() => {
      const li = document.querySelector('.finding[data-active="true"]');
      const excerpt = li.querySelector('.finding-excerpt'), mark = excerpt.querySelector('mark[data-visible-ws]');
      const spaces = [...mark.querySelectorAll('.ws[data-ch="space"]')];
      // Karşılaştırma için aynı paragrafın düz hâli: ekran okuyucu metni çıkarılır, boşluklar düz kalır.
      const plain = excerpt.cloneNode(true); plain.querySelectorAll('.ws-spoken').forEach((e) => e.remove());
      const clone = excerpt.cloneNode(false); clone.textContent = plain.textContent;
      clone.style.visibility = 'hidden'; excerpt.after(clone);
      const heights = [excerpt.getBoundingClientRect().height, clone.getBoundingClientRect().height]; clone.remove();
      return { title: li.querySelector('.finding-title')?.textContent, spaces: spaces.length, chars: spaces.map((e) => e.textContent),
        content: spaces.map((e) => getComputedStyle(e, '::before').content), markText: mark.textContent,
        rects: mark.getClientRects().length, heights };
    })()`);
    check("a10", "Fazla boşluk: iki gerçek boşluk karakteri, her biri ␣ ile (CSS ::before), metin değişmeden",
      ws.spaces === 2 && ws.chars.every((c) => c === " ") && ws.content.every((c) => c.includes("␣")) && ws.markText === "  ", ws);
    observe("a10", "İşaret tek satırda, paragraf yüksekliği düz metninkiyle", { satir: ws.rects, yukseklik: ws.heights });

    // Gerçek yazı tipi: ::before sözde öğesinin çizildiği aile ve o ailede ␣ glifi.
    let fonts = null, glyph = null;
    try {
      await send("DOM.enable");
      await send("CSS.enable");
      const { root } = await send("DOM.getDocument", { depth: -1, pierce: false });
      const hit = [];
      const visit = (n) => {
        const a = n.attributes ?? [];
        const attr = (k) => { for (let i = 0; i + 1 < a.length; i += 2) if (a[i] === k) return a[i + 1]; return null; };
        if (/(^|\s)ws(\s|$)/.test(attr("class") ?? "") && attr("data-ch") === "space") hit.push(n);
        (n.children ?? []).forEach(visit);
      };
      visit(root);
      let before = hit[0]?.pseudoElements?.find((p) => p.pseudoType === "before");
      if (hit[0] && !before) before = (await send("DOM.describeNode", { nodeId: hit[0].nodeId, depth: 1 })).node.pseudoElements?.find((p) => p.pseudoType === "before");
      if (before && !before.nodeId) before.nodeId = (await send("DOM.pushNodesByBackendIdsToFrontend", { backendNodeIds: [before.backendNodeId] })).nodeIds[0];
      fonts = before ? (await send("CSS.getPlatformFontsForNode", { nodeId: before.nodeId })).fonts : { error: hit.length ? "::before yok" : ".ws öğesi yok" };
    } catch (e) {
      fonts = { error: String(e?.message ?? e) };
    } finally {
      await cdp.send("CSS.disable").catch(() => {});
      await cdp.send("DOM.disable").catch(() => {});
    }
    const family = Array.isArray(fonts) ? fonts.find((f) => f.glyphCount > 0)?.familyName : null;
    if (family) { try { glyph = nativeLive("glyph", { FontFamily: family, CodePoint: 0x2423 }); } catch (e) { glyph = { error: String(e?.message ?? e) }; } }
    await clipShot("denetle-bosluk", '.finding[data-active="true"] .finding-excerpt');
    shot("v032-denetle-bosluk");
    const sameFamily = (glyph?.names ?? []).some((n) => n.toLowerCase() === (family ?? "").toLowerCase());
    check("a10", "␣ gerçek bir yazı tipiyle çizilir (CSS.getPlatformFontsForNode, ::before) ve o ailede U+2423 glifi var",
      Boolean(family) && !/last ?resort/i.test(family) && sameFamily && glyph?.has === true, { fonts, glyph });

    // Kopya: önce seçim metni, sonra gerçek pano (execCommand copy, kullanıcı hareketiyle).
    const copy = await send("Runtime.evaluate", {
      expression: `(() => { const p = document.querySelector('.finding[data-active="true"] .finding-excerpt'); const range = document.createRange(); range.selectNodeContents(p);
        const s = getSelection(); s.removeAllRanges(); s.addRange(range); const text = s.toString(); return { text, copied: document.execCommand('copy') }; })()`,
      returnByValue: true, userGesture: true,
    });
    const selected = copy.result?.value ?? {};
    const faithful = (t) => typeof t === "string" && t.includes("Dava,  görevli") && !/Dava,\s*\d*\s*boşluk|␣|·/.test(t);
    let clip = null;
    try { clip = nativeLive("clip-text"); } catch (e) { clip = { error: String(e?.message ?? e) }; }
    // Kullanıcının aldığı şey panodur: okunabildiyse iddia ondan, seçim
    // metni (Selection.toString) gözlem. Pano okunamazsa iddia seçim metninden.
    if (selected.copied && typeof clip?.text === "string" && clip.text.includes("Dava,")) {
      check("a10", "Kopyalanan alıntı (gerçek pano) belgedeki gibi: 'Dava,  görevli' (iki boşluk), ␣/·/'boşluk' yok",
        faithful(clip.text), { pano: clip.text.slice(0, 200), secim: selected.text?.slice(0, 200) });
      observe("a10", "Selection.toString() metni", { sadik: faithful(selected.text), metin: selected.text?.slice(0, 200) });
    } else {
      check("a10", "Seçilen alıntının metni belgedeki gibi (pano okunamadı): 'Dava,  görevli', ␣/·/'boşluk' yok",
        faithful(selected.text), { secim: selected, pano: clip });
    }
    await eval_("getSelection().removeAllRanges(), true");
  }

  /** a14 — Düzenle'de kayıt: pencere kaynağın klasöründe açılır, "Klasörde Göster" dosyayı seçili gösterir. */
  async function revealSaved() {
    const dir = folder(`Klasörde Göster ${RUN}`);
    const path = join(dir, `v032 Gösterilecek ${RUN}.pdf`);
    writeFileSync(path, sizedPdf([595, 595], 842));
    await openPdf(path, 2);
    await pickTool("reorder", "Sırala");
    await sleep(400);
    const before = new Set(readdirSync(dir));
    const saved = await saveVia("Yeni PDF Kaydet", null);
    const created = readdirSync(dir).filter((n) => !before.has(n) && /\.pdf$/i.test(n));
    const output = created.length === 1 ? join(dir, created[0]) : null;
    check("a08", "Kaydet penceresi kaynak belgenin klasöründe açılır (önerilen adla kaydedilen kopya orada)",
      saved.status?.tone === "success" && Boolean(output), { olusan: created, pencere: saved.dialog, durum: saved.status });
    const place = await eval_(`(() => { const p = document.querySelector('.pdf-preview-panel .saved-place'); if (!p) return null;
      return { name: p.querySelector('.saved-place-name')?.textContent, dir: p.querySelector('.saved-place-name')?.getAttribute('title'),
        button: [...p.querySelectorAll('button')].map((b) => b.textContent.trim()) }; })()`);
    shot("v032-klasorde-goster-satiri");
    nativeLive("close-explorer");
    await sleep(1000);
    const clicked = place ? await clickText(".pdf-preview-panel .saved-place button", "Klasörde Göster", true) : false;
    let windows = [], target = null;
    for (let i = 0; i < 30 && clicked; i++) {
      await sleep(500);
      windows = nativeLive("explorer").windows;
      target = windows.find((w) => norm(w.path) === norm(dir));
      if (target && output && (target.selected ?? []).some((p) => norm(p) === norm(output))) break;
    }
    nativeLive("screen", { Path: join(SHOTS, "v032-klasorde-goster.png") });
    check("a08", "Düzenle 'Klasörde Göster' Gezgin'i kaydedilen dosyanın klasöründe, dosya seçili açar",
      Boolean(target) && Boolean(output) && (target.selected ?? []).some((p) => norm(p) === norm(output)),
      { satir: place, beklenen: { klasor: dir, dosya: output }, acilan: windows });
    nativeLive("close-explorer");
  }

  // [ad, süre sınırı, senaryo, pencere yüksekliği]. Sırala dört küçük resmi
  // kaydırmadan göstermek için daha uzun pencere ister.
  const scenarios = [
    ["açıklama görünümleri", 150_000, annotations],
    ["marka geometrisi", 90_000, brandGeometry],
    ["Düzenle paneli 1093×614 @1.25", 120_000, editorPanel],
    ["Birleştir: açık belgeye ekle", 150_000, mergeAppend],
    ["Sırala: sürükle, Ctrl+Z", 240_000, reorder, 1000],
    ["Sıkıştır sonuç metni", 180_000, compression],
    ["büyük sayfa önizlemesi DPR 2", 180_000, largePages],
    ["Karşılaştır tutar ve çizgiler", 120_000, compareView],
    ["Denetle ␣ ve kopya", 150_000, whitespaceGlyph],
    ["Düzenle Klasörde Göster", 150_000, revealSaved],
  ];

  const cleanup = async () => {
    try { native("dialog-close"); } catch { /* yardımcı yoksa */ }
    await clearEmulation();
  };
  // `ctx.only`: yalnız adı bu düzenli ifadeye uyan senaryolar (hata ayıklama).
  const only = ctx.only ? new RegExp(ctx.only, "u") : null;
  for (const [name, budget, scenario, height = 800] of scenarios) {
    if (only && !only.test(name)) continue;
    const token = { dead: false };
    let timer;
    const job = scope.run(token, async () => {
      await cleanup();
      native("move", { X: 20, Y: 0, Width: 1280, Height: height });
      await cdp.send("Page.reload");
      await ready();
      await sleep(500);
      await scenario();
    });
    const expired = new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`süre sınırı: ${budget / 1000} sn`)), budget); });
    const started = Date.now();
    try {
      await Promise.race([job, expired]);
    } catch (e) {
      ctx.check("v0.3.2", `${name}: senaryo tamamlanamadı`, false, String(e?.stack ?? e).slice(0, 1500));
      ctx.shot(`v032-yarida-${name.replace(/[^\p{L}\p{N}]+/gu, "-")}`);
    } finally {
      clearTimeout(timer);
      token.dead = true;
      job.catch(() => {});
      await cleanup();
      try { native("close-explorer"); } catch { /* yok */ }
      ctx.observe("v0.3.2", `${name}: süre`, `${Math.round((Date.now() - started) / 1000)} sn`);
    }
  }
  native("move", { X: 20, Y: 0, Width: 1280, Height: 800 });
}
