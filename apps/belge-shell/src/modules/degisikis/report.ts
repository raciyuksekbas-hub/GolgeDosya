import type { ComparisonChange, ComparisonSummary } from "./viewModels/comparisonViewModel";

export type ReportFormat = "html" | "docx" | "markdown" | "json";
/** Metin olarak üretilen biçimler; DOCX doğrudan bayt üretir. */
export type TextReportFormat = Exclude<ReportFormat, "docx">;

export interface ReportInput {
  baseName: string;
  revisedName: string;
  generatedAt: Date;
  summary: ComparisonSummary;
  changes: ComparisonChange[];
}

const KIND_LABEL: Record<ComparisonChange["kind"], string> = {
  added: "Eklenen",
  removed: "Silinen",
  modified: "Değiştirilen",
};

export const REPORT_FORMAT_LABEL: Record<ReportFormat, string> = {
  html: "HTML raporu",
  docx: "DOCX raporu",
  markdown: "Markdown raporu",
  json: "JSON verisi",
};

/** Menüdeki sıra: en sık kullanılan iki biçim başta. */
export const REPORT_FORMATS: ReportFormat[] = ["html", "docx", "markdown", "json"];

const EXTENSION: Record<ReportFormat, string> = { html: "html", docx: "docx", markdown: "md", json: "json" };
const MIME: Record<ReportFormat, string> = {
  html: "text/html;charset=utf-8",
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  markdown: "text/markdown;charset=utf-8",
  json: "application/json;charset=utf-8",
};

export function formatTimestamp(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.${date.getFullYear()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** Dosya adında yol ayırıcı ve kabuk için riskli karakter bırakılmaz. */
export function reportFileName(input: ReportInput, format: ReportFormat): string {
  const stamp = [
    input.generatedAt.getFullYear(),
    String(input.generatedAt.getMonth() + 1).padStart(2, "0"),
    String(input.generatedAt.getDate()).padStart(2, "0"),
    "-",
    String(input.generatedAt.getHours()).padStart(2, "0"),
    String(input.generatedAt.getMinutes()).padStart(2, "0"),
  ].join("");
  return `DegisikIs-Rapor-${stamp}.${EXTENSION[format]}`;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function buildReportHtml(input: ReportInput): string {
  const { summary, changes } = input;
  const rows = changes.map((change) => `
      <article class="change ${change.kind}">
        <header>
          <span class="no">${escapeHtml(change.displayIndex)}</span>
          <span class="kind">${KIND_LABEL[change.kind]}</span>
          <span class="loc">${escapeHtml(change.sectionLabel)}</span>
        </header>
        ${change.summary.length ? `<ul class="summary">${change.summary.map((line) => `<li>${escapeHtml(line)}</li>`).join("")}</ul>` : ""}
        <div class="texts">
          <div class="side base"><h4>Temel sürüm</h4><p>${change.leftText ? escapeHtml(change.leftText) : "<em>—</em>"}</p></div>
          <div class="side revised"><h4>Değişik sürüm</h4><p>${change.rightText ? escapeHtml(change.rightText) : "<em>—</em>"}</p></div>
        </div>
      </article>`).join("");

  return `<!doctype html>
<html lang="tr">
<head>
<meta charset="utf-8">
<title>Değişikİş — Karşılaştırma Raporu</title>
<style>
  :root { --ink:#111417; --petrol:#1e2f3d; --teal:#2a7f8e; --coral:#e06a5c; --gray:#8c9196; --line:#d4d7db; --stone:#f3f4f2; }
  * { box-sizing:border-box }
  body { margin:0; padding:32px; color:var(--ink); background:#fff; font:14px/1.6 "Manrope","Avenir Next","Segoe UI",system-ui,sans-serif; }
  .sheet { max-width:900px; margin:0 auto }
  header.doc { display:flex; align-items:flex-end; justify-content:space-between; gap:24px; padding-bottom:16px; border-bottom:2px solid var(--ink) }
  header.doc h1 { margin:0; font-size:20px; letter-spacing:-.02em }
  header.doc p { margin:4px 0 0; color:var(--gray); font-size:12px }
  .files { display:grid; gap:6px; margin:20px 0; padding:14px 16px; background:var(--stone); border-radius:6px; font-size:13px }
  .files div { display:flex; gap:10px }
  .files span { flex:0 0 110px; color:var(--gray); font-size:11px; letter-spacing:.06em }
  .totals { display:grid; grid-template-columns:repeat(4,1fr); gap:1px; margin:20px 0; background:var(--line); border:1px solid var(--line); border-radius:6px; overflow:hidden }
  .totals div { padding:12px 14px; background:#fff }
  .totals strong { display:block; font-size:22px; line-height:1.1; letter-spacing:-.02em }
  .totals span { color:var(--gray); font-size:11px }
  .totals .added strong { color:var(--teal) } .totals .removed strong { color:var(--coral) }
  .bar { display:flex; height:8px; margin:0 0 26px; border-radius:4px; overflow:hidden; background:var(--stone) }
  .bar i { display:block }
  .bar .a { background:var(--teal) } .bar .r { background:var(--coral) } .bar .m { background:var(--gray) }
  h2 { margin:26px 0 12px; font-size:13px; letter-spacing:.1em; color:var(--gray) }
  .change { padding:14px 16px; margin-bottom:10px; border:1px solid var(--line); border-left:3px solid var(--gray); border-radius:6px; break-inside:avoid }
  .change.added { border-left-color:var(--teal) } .change.removed { border-left-color:var(--coral) }
  .change header { display:flex; align-items:baseline; gap:10px; margin-bottom:8px }
  .no { font-variant-numeric:tabular-nums; font-weight:700; color:var(--gray) }
  .kind { font-size:12px; font-weight:700 }
  .change.added .kind { color:var(--teal) } .change.removed .kind { color:var(--coral) }
  .loc { margin-left:auto; color:var(--gray); font-size:12px }
  .summary { margin:0 0 10px; padding-left:18px; color:var(--petrol); font-family:ui-monospace,Menlo,Consolas,monospace; font-size:12px }
  .texts { display:grid; grid-template-columns:1fr 1fr; gap:10px }
  .side { padding:9px 11px; border-radius:4px; background:var(--stone) }
  .side h4 { margin:0 0 4px; font-size:10px; letter-spacing:.08em; color:var(--gray) }
  .side p { margin:0; font-size:12.5px; white-space:pre-wrap }
  .side.base { background:#fdf0ee } .side.revised { background:#ecf5f7 }
  footer { margin-top:28px; padding-top:14px; border-top:1px solid var(--line); color:var(--gray); font-size:11px }
  @media print { body { padding:0 } .change { border-color:#ccc } }
</style>
</head>
<body><div class="sheet">
  <header class="doc">
    <div><h1>Değişikİş — Karşılaştırma Raporu</h1><p>İki belge, bütün farklar.</p></div>
    <p>${escapeHtml(formatTimestamp(input.generatedAt))}</p>
  </header>

  <div class="files">
    <div><span>TEMEL SÜRÜM</span><strong>${escapeHtml(input.baseName)}</strong></div>
    <div><span>DEĞİŞİK SÜRÜM</span><strong>${escapeHtml(input.revisedName)}</strong></div>
  </div>

  <div class="totals">
    <div><strong>${summary.total}</strong><span>Toplam fark</span></div>
    <div class="added"><strong>${summary.added}</strong><span>Ekleme · %${summary.addedPct}</span></div>
    <div class="removed"><strong>${summary.removed}</strong><span>Silinen · %${summary.removedPct}</span></div>
    <div><strong>${summary.modified}</strong><span>Değiştirilen · %${summary.modifiedPct}</span></div>
  </div>
  <div class="bar">
    <i class="a" style="width:${summary.addedPct}%"></i>
    <i class="r" style="width:${summary.removedPct}%"></i>
    <i class="m" style="width:${summary.modifiedPct}%"></i>
  </div>

  <h2>FARK LİSTESİ</h2>
  ${rows || "<p>Fark bulunamadı.</p>"}

  <footer>Değişikİş ile yerel olarak üretilmiştir. Karşılaştırma sonuçları yardımcı niteliktedir; nihai kontrol kullanıcıya aittir.</footer>
</div></body></html>`;
}

export function buildReportMarkdown(input: ReportInput): string {
  const { summary, changes } = input;
  const lines = [
    "# Değişikİş — Karşılaştırma Raporu",
    "",
    `- **Temel sürüm:** ${input.baseName}`,
    `- **Değişik sürüm:** ${input.revisedName}`,
    `- **Tarih:** ${formatTimestamp(input.generatedAt)}`,
    "",
    "## Özet",
    "",
    "| Tür | Adet | Oran |",
    "| --- | ---: | ---: |",
    `| Toplam fark | ${summary.total} | %100 |`,
    `| Ekleme | ${summary.added} | %${summary.addedPct} |`,
    `| Silinen | ${summary.removed} | %${summary.removedPct} |`,
    `| Değiştirilen | ${summary.modified} | %${summary.modifiedPct} |`,
    "",
    "## Fark listesi",
    "",
  ];
  if (!changes.length) lines.push("Fark bulunamadı.", "");
  for (const change of changes) {
    lines.push(`### ${change.displayIndex} · ${KIND_LABEL[change.kind]} · ${change.sectionLabel}`, "");
    for (const line of change.summary) lines.push(`- ${line}`);
    if (change.summary.length) lines.push("");
    lines.push(`**Temel sürüm:** ${change.leftText ?? "—"}`, "", `**Değişik sürüm:** ${change.rightText ?? "—"}`, "");
  }
  lines.push("---", "Değişikİş ile yerel olarak üretilmiştir.");
  return lines.join("\n");
}

export function buildReportJson(input: ReportInput): string {
  return JSON.stringify({
    product: "Değişikİş",
    generatedAt: input.generatedAt.toISOString(),
    documents: { base: input.baseName, revised: input.revisedName },
    summary: input.summary,
    changes: input.changes.map((change) => ({
      index: change.index,
      displayIndex: change.displayIndex,
      kind: change.kind,
      location: change.sectionLabel,
      summary: change.summary,
      baseText: change.leftText ?? null,
      revisedText: change.rightText ?? null,
    })),
  }, null, 2);
}

export function buildReport(input: ReportInput, format: TextReportFormat): string {
  if (format === "markdown") return buildReportMarkdown(input);
  if (format === "json") return buildReportJson(input);
  return buildReportHtml(input);
}

/** Her biçim için kaydedilecek ham baytlar. */
export async function buildReportBytes(input: ReportInput, format: ReportFormat): Promise<Uint8Array> {
  if (format === "docx") {
    const { buildReportDocx } = await import("./reportDocx");
    return buildReportDocx(input);
  }
  return new TextEncoder().encode(buildReport(input, format));
}

export interface SaveResult {
  /** Masaüstünde kaydedilen dosyanın mutlak yolu; tarayıcı yedeğinde tanımsız. */
  path?: string;
}

export interface ReportOutcome {
  path?: string;
  opened: boolean;
  message: string;
}

export type ReportSaver = (fileName: string, contents: Uint8Array, format: ReportFormat) => Promise<SaveResult>;
export type ReportOpener = (path: string) => Promise<void>;

/**
 * Raporu yerel olarak yazar. Masaüstü uygulamasında Rust tarafındaki
 * `save_report` komutu kullanılır; tarayıcı bağlamında (geliştirme, test)
 * indirme bağlantısına düşülür. Hiçbir durumda ağ kullanılmaz.
 */
export async function saveReport(
  fileName: string,
  contents: Uint8Array,
  format: ReportFormat,
  invoker?: (fileName: string, contents: Uint8Array) => Promise<string>,
): Promise<SaveResult> {
  const invokeSave = invoker ?? (async (name: string, body: Uint8Array) => {
    const { invoke } = await import("@tauri-apps/api/core");
    // Komut adı birleşik kabukta modül önekli; imza ve davranış aynı.
    return invoke<string>("degisikis_save_report", { fileName: name, contents: Array.from(body) });
  });
  try {
    const path = await invokeSave(fileName, contents);
    return { path };
  } catch {
    downloadInBrowser(fileName, contents, format);
    return {};
  }
}

/**
 * Kaydedilmiş raporu işletim sisteminin varsayılan uygulamasıyla açar.
 * Yalnız `save_report` tarafından döndürülen gerçek yol için çağrılır; yol
 * Rust tarafında rapor dizini ve dosya adı bakımından yeniden doğrulanır.
 */
export async function openReport(path: string, opener?: ReportOpener): Promise<boolean> {
  const invokeOpen = opener ?? (async (target: string) => {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("degisikis_open_report", { path: target });
  });
  try {
    await invokeOpen(path);
    return true;
  } catch {
    return false;
  }
}

/**
 * Bütün biçimler için ortak akış: üret → kaydet → aç. Kaydetme başarısızsa
 * açma denenmez; kaydetme başarılı fakat açma başarısızsa dosya korunur ve
 * durum kısmi başarı olarak bildirilir. Tarayıcı bağlamında indirme yapılır ve
 * işletim sistemi uygulaması açılmaya çalışılmaz.
 */
export async function generateAndOpenReport(
  input: ReportInput,
  format: ReportFormat,
  hooks: { save?: ReportSaver; open?: ReportOpener } = {},
): Promise<ReportOutcome> {
  const fileName = reportFileName(input, format);
  const bytes = await buildReportBytes(input, format);
  const save = hooks.save ?? ((name, contents, target) => saveReport(name, contents, target));
  const saved = await save(fileName, bytes, format);

  if (!saved.path) return { opened: false, message: `Rapor indirildi: ${fileName}` };

  const opened = await openReport(saved.path, hooks.open);
  return {
    path: saved.path,
    opened,
    message: opened
      ? `Rapor oluşturuldu ve açıldı · ${fileName}`
      : `Rapor kaydedildi ancak otomatik olarak açılamadı: ${saved.path}`,
  };
}

function downloadInBrowser(fileName: string, contents: Uint8Array, format: ReportFormat): void {
  const blob = new Blob([contents as BlobPart], { type: MIME[format] });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}
