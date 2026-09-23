// PDF önizleme yüzeyinin sözleşmesi.
//
// Saha bulgusu (Windows): önizleme hatası küçük resmin dışına taşıp komşu
// sayfaları ve araçları örtüyordu; ayrıca etkin sayfanın büyük önizlemesi,
// görünen bütün küçük resimler bitene kadar tek bir kuyrukta bekliyordu. Bu
// testler iki kararı kaynak düzeyinde kilitler. Platform ayrımı YOKTUR: arayüz
// macOS ile Windows'u bilmez, render farkı arka uçtadır.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const workspace = readFileSync("src/modules/duzenek/PdfWorkspace.tsx", "utf8");
const css = readFileSync("src/modules/duzenek/pdf.css", "utf8");

test("etkin sayfa önizlemesi küçük resim kuyruğunda beklemez", () => {
  assert.match(workspace, /let queue: Promise<unknown>/, "küçük resim kuyruğu");
  assert.match(workspace, /let activeQueue: Promise<unknown>/, "etkin sayfanın ayrı kuyruğu");
  assert.match(
    workspace,
    /if \(large\) activeQueue = activeQueue[\s\S]*?else queue = queue/,
    "büyük önizleme etkin kuyruğa, küçük resim kendi kuyruğuna gitmeli",
  );
  // Bayat sonuç yeni sayfanın üstüne yazılmaz: sıradaki iş başlamadan ve
  // sonuç geldiğinde bileşenin hâlâ aynı sayfayı gösterdiği denetlenir.
  assert.ok((workspace.match(/if \(!alive\) return;/g) || []).length >= 2, "bayat sonuç koruması");
  // Platform ayrımı arayüze sızmamalı.
  assert.doesNotMatch(workspace, /navigator\.platform|userAgent|isWindows|isMac/i);
});

test("küçük resimdeki hata cümlesi kutusunun içinde kalır", () => {
  assert.match(
    css,
    /\.pdf-root \.pdf-thumbnail-stage \{[^}]*overflow: hidden;/,
    "küçük resim sahnesi taşmayı kesmeli",
  );
  assert.match(
    css,
    /\.pdf-thumbnail-stage \.pdf-wait\[data-tone="error"\] \{[^}]*-webkit-line-clamp: 4;[^}]*overflow: hidden;/,
    "hata cümlesi satır sınırıyla kutuda kalmalı",
  );
  // Kısaltılan cümle kaybolmaz: ekran okuyucuya ve fareyle üzerine gelene açık.
  assert.match(workspace, /data-tone="error" role="status" title=\{/, "tam cümle title ve status ile erişilebilir");
});
