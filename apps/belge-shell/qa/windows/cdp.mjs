// Asgari Chrome DevTools Protocol istemcisi (Kapı 6).
//
// Bağımlılık yok: Node 22'nin yerleşik WebSocket ve fetch'i yeter. WebView2,
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=N ile başlatılınca
// sayfayı bu uçta sunar; Playwright'ın connectOverCDP'si de aynı ucu kullanır.
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function attach(port, { timeoutMs = 90_000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let last = "hiç yanıt yok";
  while (Date.now() < deadline) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = list.find((t) => t.type === "page" && /tauri\.localhost|^tauri:/.test(t.url));
      if (page) return await Session.open(page.webSocketDebuggerUrl);
      last = JSON.stringify(list.map((t) => `${t.type} ${t.url}`));
    } catch (e) {
      last = String(e?.message ?? e);
    }
    await sleep(500);
  }
  throw new Error(`CDP hedefi bulunamadı (${port}): ${last}`);
}

export class Session {
  static open(url) {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(url);
      ws.addEventListener("open", () => resolve(new Session(ws)));
      ws.addEventListener("error", (e) => reject(new Error(`WebSocket: ${e?.message ?? "hata"}`)));
    });
  }

  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.addEventListener("message", (m) => {
      const msg = JSON.parse(typeof m.data === "string" ? m.data : Buffer.from(m.data).toString("utf8"));
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(`${msg.error.message} ${msg.error.data ?? ""}`));
        else resolve(msg.result);
      }
    });
  }

  send(method, params = {}) {
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      setTimeout(() => {
        if (this.pending.delete(id)) reject(new Error(`CDP zaman aşımı: ${method}`));
      }, 60_000);
    });
  }

  /** Sayfada bir ifade değerlendirir; söz dönerse bekler, değeri JSON olarak getirir. */
  async eval(expression) {
    const r = await this.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) {
      const text = r.exceptionDetails.exception?.description ?? r.exceptionDetails.text;
      throw new Error(`sayfa istisnası: ${text}`);
    }
    return r.result.value;
  }

  /** Koşul doğru olana dek bekler. */
  async until(expression, { timeoutMs = 30_000, what = expression } = {}) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        if (await this.eval(expression)) return true;
      } catch {
        // Yeniden yükleme sırasında bağlam kısa süre yoktur.
      }
      await sleep(250);
    }
    throw new Error(`beklenen durum oluşmadı: ${what}`);
  }

  close() {
    this.ws.close();
  }
}
