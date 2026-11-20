import type { DocumentBlock } from "./core/types";
import { answerComparison, type ComparisonOutcome, type ComparisonReply, type ComparisonRequest } from "./comparisonCompute";

export type { ComparisonOutcome } from "./comparisonCompute";

/**
 * Karşılaştırma arayüz iş parçacığının DIŞINDA koşar.
 *
 * `compareDocuments` eskiden `useMemo` içinde, render sırasında eşzamanlı
 * koşuyordu. Çıkarma bitince `setBusy(false)` düşüyor, sonra render hizalamayı
 * hesaplıyordu: büyük ve çok değişmiş belgede pencere saniyelerce donuyor,
 * döner simge de çoktan kalkmış oluyordu. Artık iş bir modül worker'ında
 * yapılır; sonuç gelene kadar "karşılaştırılıyor" durumu ekranda kalır.
 *
 * Worker yoksa (vitest/node, eski ortam) aynı `computeComparison` eşzamanlı
 * yedekte koşar: testler üretimdeki kodun aynısını sınar.
 */

/** İptal edilen işin sözü bu hatayla reddedilir; çağıran onu yok sayar. */
export class ComparisonCancelled extends Error {
  constructor() {
    super("Karşılaştırma iptal edildi.");
    this.name = "ComparisonCancelled";
  }
}

export interface ComparisonJob {
  result: Promise<ComparisonOutcome>;
  /** Worker'ı sonlandırır; sonuç artık hiç gelmez. Birden çok kez çağrılabilir. */
  cancel(): void;
}

/** Worker'ın görebildiği en dar arayüz; testler sahte bir worker verebilir. */
export interface ComparisonWorker {
  onmessage: ((event: MessageEvent<ComparisonReply>) => void) | null;
  onerror: ((event: ErrorEvent) => void) | null;
  onmessageerror: ((event: MessageEvent) => void) | null;
  postMessage(message: ComparisonRequest): void;
  terminate(): void;
}

export type ComparisonWorkerFactory = () => ComparisonWorker;

/**
 * Paketlenmiş modül worker'ı. Vite `new Worker(new URL(...))` kalıbını
 * yalnız bu biçimde yazıldığında tanır; worker aynı kökenden ayrı bir dosya
 * olarak gelir (CSP: `worker-src 'self'`; blob: ya da eval yok).
 */
function bundledWorker(): ComparisonWorker {
  return new Worker(new URL("./comparison.worker.ts", import.meta.url), { type: "module" });
}

/** Ortamda Worker yoksa `undefined`: iş eşzamanlı yedekte koşar. */
export function defaultWorkerFactory(): ComparisonWorkerFactory | undefined {
  return typeof Worker === "undefined" ? undefined : bundledWorker;
}

/**
 * Bir karşılaştırma başlatır. Her iş kendi worker'ını açar ve bitince
 * kapatır; iptal `terminate()` demektir, yarım kalan hesap da durur. Worker
 * kurulamazsa (ör. kurucu hata atarsa) iş eşzamanlı yedeğe düşer.
 */
export function startComparison(
  base: DocumentBlock[],
  revised: DocumentBlock[],
  factory: ComparisonWorkerFactory | undefined = defaultWorkerFactory(),
): ComparisonJob {
  let settled = false;
  let reject: (reason: unknown) => void = () => undefined;
  let worker: ComparisonWorker | undefined;
  const result = new Promise<ComparisonOutcome>((resolveResult, rejectResult) => {
    reject = rejectResult;
    const finish = (action: () => void) => {
      if (settled) return;
      settled = true;
      worker?.terminate();
      action();
    };
    try {
      worker = factory?.();
    } catch {
      worker = undefined;
    }
    if (!worker) {
      // Eşzamanlı yedek: bir sonraki görevde koşar, böylece çağıran önce
      // "karşılaştırılıyor" durumunu çizebilir ve iptal şansı olur.
      setTimeout(() => {
        if (settled) return;
        const reply = answerComparison({ base, revised });
        finish(() => (reply.ok ? resolveResult(reply.outcome) : rejectResult(new Error(reply.message))));
      }, 0);
      return;
    }
    worker.onmessage = (event) => {
      const reply = event.data;
      finish(() => (reply.ok ? resolveResult(reply.outcome) : rejectResult(new Error(reply.message))));
    };
    worker.onerror = (event) => {
      event.preventDefault?.();
      finish(() => rejectResult(new Error(event.message || "Karşılaştırma iş parçacığı çalışamadı.")));
    };
    worker.onmessageerror = () => {
      finish(() => rejectResult(new Error("Karşılaştırma sonucu okunamadı.")));
    };
    worker.postMessage({ base, revised });
  });
  return {
    result,
    cancel() {
      if (settled) return;
      settled = true;
      worker?.terminate();
      reject(new ComparisonCancelled());
    },
  };
}
