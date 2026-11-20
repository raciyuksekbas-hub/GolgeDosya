import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it, vi } from "vitest";
import { answerComparison, computeComparison, type ComparisonReply, type ComparisonRequest } from "./comparisonCompute";
import { ComparisonCancelled, startComparison, type ComparisonWorker } from "./comparisonJob";
import { compareDocuments } from "./core/compare";
import { makeBlocks } from "./core/normalize";
import { buildComparisonViewModel } from "./viewModels/comparisonViewModel";

const here = dirname(fileURLToPath(import.meta.url));
const read = (name: string) => readFileSync(resolve(here, name), "utf8");

const base = makeBlocks([
  { text: "MADDE 1 – KONU" },
  { text: "1.1. Kira bedeli aylık 100.000 TL olup her ayın beşinde ödenir." },
  { text: "1.2. Taraflar uyuşmazlıkta İstanbul mahkemelerini yetkili kılar." },
], "base");
const revised = makeBlocks([
  { text: "MADDE 1 – KONU" },
  { text: "1.1. Kira bedeli aylık 118.000 TL olup her ayın beşinde ödenir." },
  { text: "1.2. Taraflar uyuşmazlıkta Ankara mahkemelerini yetkili kılar." },
  { text: "1.3. Ek protokol imzalanmıştır." },
], "revised");

/**
 * Tarayıcıdaki worker'ın davranışı: ileti yapılandırılmış kopyayla gider ve
 * yanıt bir sonraki görevde döner.
 */
function fakeWorker(respond: (request: ComparisonRequest) => ComparisonReply | "error" | "silent" = answerComparison) {
  const worker: ComparisonWorker & { posted: number; terminated: number } = {
    onmessage: null,
    onerror: null,
    onmessageerror: null,
    posted: 0,
    terminated: 0,
    postMessage(message) {
      worker.posted++;
      const request = structuredClone(message);
      setTimeout(() => {
        const reply = respond(request);
        if (reply === "silent") return;
        if (reply === "error") worker.onerror?.({ message: "worker yüklenemedi", preventDefault() {} } as ErrorEvent);
        else worker.onmessage?.({ data: structuredClone(reply) } as MessageEvent<ComparisonReply>);
      }, 0);
    },
    terminate() {
      worker.terminated++;
    },
  };
  return worker;
}

describe("karşılaştırma işi arayüz iş parçacığının dışında koşar", () => {
  it("worker'dan dönen sonuç motorun eşzamanlı çıktısıyla aynıdır", async () => {
    const worker = fakeWorker();
    const outcome = await startComparison(base, revised, () => worker).result;
    const direct = compareDocuments(base, revised);
    expect(outcome.comparison).toEqual(direct);
    expect(outcome.model).toEqual(buildComparisonViewModel(direct));
    expect(worker.posted).toBe(1);
    expect(worker.terminated).toBe(1);
  });

  it("Worker olmayan ortamda (test, eski motor) aynı kod eşzamanlı yedekte koşar", async () => {
    const outcome = await startComparison(base, revised, undefined).result;
    expect(outcome).toEqual(computeComparison(base, revised));
    expect(outcome.model.summary.total).toBeGreaterThan(0);
  });

  it("worker kurulamazsa eşzamanlı yedeğe düşer", async () => {
    const outcome = await startComparison(base, revised, () => {
      throw new Error("Worker desteklenmiyor");
    }).result;
    expect(outcome).toEqual(computeComparison(base, revised));
  });

  it("iptal worker'ı sonlandırır; geç gelen sonuç yok sayılır", async () => {
    const worker = fakeWorker();
    const job = startComparison(base, revised, () => worker);
    job.cancel();
    job.cancel();
    await expect(job.result).rejects.toBeInstanceOf(ComparisonCancelled);
    expect(worker.terminated).toBe(1);
    // Worker'ın geç yanıtı sözü yeniden çözemez.
    await new Promise((resolveLater) => setTimeout(resolveLater, 5));
    await expect(job.result).rejects.toBeInstanceOf(ComparisonCancelled);
  });

  it("eşzamanlı yedekte de iptal edilen iş hesaplanmaz", async () => {
    const touched = vi.fn();
    const watched = new Proxy(base, {
      get(target, key, receiver) {
        touched(key);
        return Reflect.get(target, key, receiver);
      },
    });
    const job = startComparison(watched, revised, undefined);
    job.cancel();
    await expect(job.result).rejects.toBeInstanceOf(ComparisonCancelled);
    await new Promise((resolveLater) => setTimeout(resolveLater, 5));
    expect(touched).not.toHaveBeenCalled();
  });

  it("worker hatası açık bir hata olarak döner, sessiz kalmaz", async () => {
    const worker = fakeWorker(() => "error");
    await expect(startComparison(base, revised, () => worker).result).rejects.toThrow("worker yüklenemedi");
    expect(worker.terminated).toBe(1);
  });

  it("motor istisnası worker'ı öldürmez; mesajıyla yanıt olur", async () => {
    const worker = fakeWorker(() => ({ ok: false, message: "beklenmeyen blok" }));
    await expect(startComparison(base, revised, () => worker).result).rejects.toThrow("beklenmeyen blok");
    expect(answerComparison({ base: null as never, revised })).toMatchObject({ ok: false });
  });
});

describe("Karşılaştır ekranı sonucu worker'dan bekler", () => {
  const workspace = read("CompareWorkspace.tsx");
  const job = read("comparisonJob.ts");
  const worker = read("comparison.worker.ts");

  it("hizalama render sırasında koşmaz", () => {
    expect(workspace).not.toContain("compareDocuments(");
    expect(workspace).not.toContain("buildComparisonViewModel(");
    expect(workspace).toContain("startComparison(docs[0].doc.blocks, docs[1].doc.blocks)");
  });

  it("sonuç gelene kadar meşgul durumu kalır; belge değişince ya da ekran kapanınca iş iptal edilir", () => {
    expect(workspace).toContain("if (busy || comparing)");
    expect(workspace).toMatch(/return \(\) => \{\s+cancelled = true;\s+job\.cancel\(\);\s+\};\s+\}, \[docs\]\);/);
    expect(workspace).toContain("outcome.docs === docs");
    expect(workspace).toContain('logFailure("degisikis compare"');
    expect(workspace).toContain("Belgeler karşılaştırılamadı.");
  });

  it("worker paketlenmiş, aynı kökenden bir modül dosyasıdır (CSP: worker-src 'self')", () => {
    expect(job).toContain('new Worker(new URL("./comparison.worker.ts", import.meta.url), { type: "module" })');
    expect(job + worker).not.toMatch(/["'`]blob:|URL\.createObjectURL|\beval\(|new Function\(/);
    // Worker yalnız motoru içe aktarır; kendini kuran modülü değil.
    expect(worker).toContain('from "./comparisonCompute"');
    expect(worker).not.toContain("comparisonJob\"");
  });
});
