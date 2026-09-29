// Kapı 6 — önizleme kararlılığı.
//
// Saha: "PDF belgem ekranda tir tir titriyor, hiç durmuyor; Yakınlaştır'ı
// Genişliğe Sığdır yapınca düzeldi." İnvaryant: PDF açılıp kutu ölçüsü
// oturduktan sonra kullanıcı hiçbir şey yapmazken önizlemenin ölçeği ve
// yerleşimi DEĞİŞMEZ ve yeni render istenmez.
//
// Gerçek WebView2, gerçek renderer, gerçek (yer kaplayan) Windows kaydırma
// çubuğu. Her PDF için sığdırma kiplerinde, sayfa oranının kutu oranına denk
// geldiği bölge (ölçek sınırlayıcısının genişlikten yüksekliğe geçtiği yer)
// pencere genişliği taranarak ölçülür; titreme tam bu bölgede doğar.
import { renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/** Asgari PDF: sayfa başına MediaBox, isteğe bağlı CropBox; içerikte şerit + çerçeve. */
export function minimalPdf(pages) {
  const bodies = [null, null];
  const add = (body) => { bodies.push(body); return bodies.length; };
  const kids = [];
  for (const p of pages) {
    const [x0, y0, x1, y1] = p.crop ?? p.media;
    const w = x1 - x0, h = y1 - y0;
    const stream = `q 0.20 0.30 0.40 rg ${(x0 + w * 0.1).toFixed(2)} ${(y0 + h * 0.8).toFixed(2)} ${(w * 0.8).toFixed(2)} ${(h * 0.06).toFixed(2)} re f 0 0 0 RG 2 w ${x0 + 4} ${y0 + 4} ${w - 8} ${h - 8} re S Q`;
    const content = add(`<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`);
    kids.push(add(`<< /Type /Page /Parent 2 0 R /MediaBox [${p.media.join(" ")}]${p.crop ? ` /CropBox [${p.crop.join(" ")}]` : ""} /Resources << >> /Contents ${content} 0 R >>`));
  }
  bodies[0] = "<< /Type /Catalog /Pages 2 0 R >>";
  bodies[1] = `<< /Type /Pages /Kids [${kids.map((k) => `${k} 0 R`).join(" ")}] /Count ${kids.length} >>`;
  let out = "%PDF-1.4\n";
  const offsets = [];
  bodies.forEach((body, i) => { offsets.push(Buffer.byteLength(out, "latin1")); out += `${i + 1} 0 obj\n${body}\nendobj\n`; });
  const xref = Buffer.byteLength(out, "latin1");
  out += `xref\n0 ${bodies.length + 1}\n0000000000 65535 f \n${offsets.map((o) => `${String(o).padStart(10, "0")} 00000 n \n`).join("")}`;
  out += `trailer\n<< /Size ${bodies.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(out, "latin1");
}

const A4 = [0, 0, 595, 842], A4L = [0, 0, 842, 595], LETTER = [0, 0, 612, 792];
const repeat = (n, page) => Array.from({ length: n }, () => page);
/** Test korpusu: farklı en-boy oranları, çok sayfa, CropBox ≠ MediaBox. */
export const CORPUS = [
  { key: "a4-portre", name: "A4 portre.pdf", pages: repeat(3, { media: A4 }), ratio: 595 / 842 },
  { key: "a4-yatay", name: "A4 yatay.pdf", pages: repeat(2, { media: A4L }), ratio: 842 / 595 },
  { key: "buyuk-tarama", name: "Büyük taranmış sayfa.pdf", pages: [{ media: [0, 0, 2480, 3508] }], ratio: 2480 / 3508 },
  { key: "cok-sayfa", name: "Çok sayfalı (12).pdf", pages: repeat(12, { media: A4 }), ratio: 595 / 842 },
  { key: "cropbox", name: "CropBox MediaBox farklı.pdf", pages: repeat(2, { media: LETTER, crop: [100, 80, 500, 690] }), ratio: 400 / 610 },
  { key: "a6", name: "Küçük sayfa A6.pdf", pages: repeat(2, { media: [0, 0, 298, 420] }), ratio: 298 / 420 },
  // Ölçeği 1'in üstünde kalan küçük sayfa: pencere boyu değiştikçe daha keskin
  // render istenir (dpi 96 tabanına takılmaz). Render hatası senaryosu bunu kullanır.
  { key: "kart", name: "Küçük kart.pdf", pages: repeat(2, { media: [0, 0, 150, 210] }), ratio: 150 / 210 },
];

const SAMPLE = `window.__probe = async (ms) => {
  const vp = document.querySelector('.pdf-page-viewport');
  if (!vp) return { error: 'önizleme kutusu yok' };
  const frame = () => new Promise((r) => requestAnimationFrame(() => r()));
  const states = new Map(); const c0 = window.__previewCalls || 0; const t0 = performance.now(); let frames = 0; const history = [];
  while (performance.now() - t0 < ms) {
    await frame(); frames++;
    const stage = vp.querySelector('.pdf-preview-stage'), surf = vp.querySelector('.pdf-page-surface');
    const s = [vp.clientWidth, vp.clientHeight, stage ? Math.round(stage.getBoundingClientRect().width) + 'x' + Math.round(stage.getBoundingClientRect().height) : '-', surf?.style.width, surf?.style.height, vp.scrollHeight > vp.clientHeight ? 'V' : '-', vp.scrollWidth > vp.clientWidth ? 'H' : '-', vp.querySelector('[data-tone="error"]') ? 'HATA' : 'ok'].join(' ');
    states.set(s, (states.get(s) || 0) + 1);
    if (history.length < 8) history.push(s);
  }
  return { frames, states: states.size, renders: (window.__previewCalls || 0) - c0, history: states.size > 1 ? history : history.slice(0, 1) };
};
(() => {
  const T = window.__TAURI_INTERNALS__;
  if (!T.__counted) {
    const original = T.invoke;
    T.invoke = (cmd, args, options) => { if (cmd === 'duzenek_preview_pdf_page') window.__previewCalls = (window.__previewCalls || 0) + 1; return original(cmd, args, options); };
    T.__counted = true;
  }
  return true;
})()`;

/**
 * @param ctx { cdp, invoke, native, mode, clickText, check, observe, sleep, dir, shot }
 */
export async function previewStability(ctx) {
  const { cdp, invoke, native, mode, clickText, check, observe, sleep, dir, shot } = ctx;
  const ready = () => cdp.until("document.readyState === 'complete' && !!document.querySelector('.sidebar-nav')", { timeoutMs: 30_000 });
  const setZoom = (value) => cdp.eval(`(() => { const s = document.querySelector('.zoom select'); if (!s) return null;
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(s, ${JSON.stringify(String(value))});
    s.dispatchEvent(new Event('change', { bubbles: true })); return s.value; })()`);
  const probe = async () => { await sleep(1000); return cdp.eval("window.__probe(1000)"); };
  const move = (width, height) => native("move", { X: 20, Y: 0, Width: width, Height: height });
  /**
   * Ölçek sınırlayıcısının genişlikten yüksekliğe geçtiği pencere bölgesi.
   * Pencerenin en küçük boyu 900×600; dar sayfalarda bölgeye genişlikle
   * ulaşılamaz, o zaman sabit genişlikte yükseklik taranır.
   */
  const zone = async (ratio) => {
    move(1100, 800);
    await sleep(700);
    const box = await cdp.eval("(() => { const v = document.querySelector('.pdf-page-viewport'); return { w: v.clientWidth, h: v.clientHeight }; })()");
    const offW = 1100 - box.w, offH = 800 - box.h;
    const byWidth = Math.round(offW + 16 + (box.h - 16) * ratio);
    if (byWidth - 36 >= 900 && byWidth + 36 <= 1270) return { axis: "genişlik", fixed: 800, center: byWidth };
    for (const width of [1000, 900]) {
      const byHeight = Math.round(offH + 16 + (width - offW - 16) / ratio);
      if (byHeight - 36 >= 620 && byHeight + 36 <= 1080) return { axis: "yükseklik", fixed: width, center: byHeight };
    }
    return null;
  };
  const sweep = async (z, onEach) => {
    const bad = []; let windows = 0;
    for (let x = z.center - 36; x <= z.center + 36; x += 4) {
      const [w, h] = z.axis === "genişlik" ? [x, z.fixed] : [z.fixed, x];
      move(w, h); windows++;
      const r = await probe();
      onEach?.(r);
      if (r.states !== 1 || r.renders !== 0) bad.push({ pencere: `${w}x${h}`, ...r });
    }
    return { bad, windows };
  };

  const open = async (doc) => {
    await invoke("remember_documents", { paths: [doc.path] });
    await cdp.send("Page.reload");
    await ready();
    await sleep(600);
    await cdp.eval(SAMPLE);
    await mode("Düzenle");
    await clickText(".file-row", doc.name);
    await cdp.until("!!document.querySelector('.pdf-page-viewport .pdf-page-surface img')", { timeoutMs: 60_000, what: `önizleme: ${doc.name}` });
    await sleep(800);
  };

  for (const spec of CORPUS) {
    const doc = { ...spec, path: join(dir, spec.name) };
    writeFileSync(doc.path, minimalPdf(spec.pages));
    move(1280, 800);
    await open(doc);
    shot(`kararlilik-${spec.key}`);

    // Sayısal kiplerin ölçeği kutuya bağlı değil: üç pencerede ölçülür.
    for (const zoom of [75, 100, 125]) {
      await setZoom(zoom);
      const bad = [];
      for (const [w, h] of [[1280, 800], [1100, 760], [900, 860]]) {
        move(w, h);
        const r = await probe();
        if (r.states !== 1 || r.renders !== 0) bad.push({ pencere: `${w}x${h}`, ...r });
      }
      check("önizleme", `${spec.name} · %${zoom}: 1 sn boyunca ölçek/yerleşim sabit, render yok`, bad.length === 0, bad.length ? bad : "3 pencere");
    }

    // Sığdırma kiplerinde: sınırlayıcının değiştiği bölge ±36 px taranır.
    for (const zoom of ["fit-page", "fit-width"]) {
      await setZoom(zoom);
      const z = await zone(spec.ratio);
      const label = zoom === "fit-page" ? "Sayfaya sığdır" : "Genişliğe sığdır";
      if (!z) { observe("önizleme", `${spec.name} · ${label}: sınır bölgesine bu ekranda ulaşılamıyor`, spec.ratio); continue; }
      const { bad, windows } = await sweep(z);
      check("önizleme", `${spec.name} · ${label}: sınır bölgesinde (${z.axis}, ${windows} pencere) titreme yok, render yok`, bad.length === 0,
        bad.length ? { titreyen: bad.length, ornek: bad.slice(0, 3) } : { pencere: windows, bolge: z });
    }
  }

  // Yeniden render hatası: sayfa çizilmişken dosyaya artık ulaşılamaz (başka
  // uygulama kilitledi, eşitleme taşıdı...). Pencere boyu değişince daha keskin
  // render istenir ve başarısız olur; hata yerleşimi titretmemeli.
  const spec = CORPUS.find((c) => c.key === "kart");
  const doc = { ...spec, path: join(dir, spec.name) };
  move(1100, 800);
  await open(doc);
  await setZoom("fit-page");
  await sleep(600);
  const z = await zone(spec.ratio);
  const moved = doc.path + ".tasindi";
  renameSync(doc.path, moved);
  try {
    let withError = 0;
    const { bad, windows } = z ? await sweep(z, (r) => { if (r.history.some((line) => line.endsWith("HATA"))) withError++; }) : { bad: [], windows: 0 };
    shot("kararlilik-render-hatasi");
    // Titreme ölçütü burada yalnız yerleşimdir: dosyaya ulaşılamadığı için
    // her pencere değişikliği bir render isteği doğurur ve başarısız olur.
    const unstable = bad.filter((b) => b.states !== 1);
    observe("önizleme", "Yeniden render hatası görünen pencere", `${withError}/${windows}`);
    check("önizleme", `Sayfa çizilmişken yeniden render başarısız: Sayfaya sığdır'da titreme yok (${windows} pencere)`,
      z && withError > 0 && unstable.length === 0, { bolge: z, hataGorulen: withError, titreyen: unstable.length, ornek: unstable.slice(0, 3) });
  } finally {
    renameSync(moved, doc.path);
  }
  move(1280, 800);
}
