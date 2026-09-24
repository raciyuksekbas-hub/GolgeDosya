/**
 * Hangi masaüstündeyiz? Yalnız iki şeyi değiştirir: pencere başlığının
 * içeriğin üstüne binip binmediği ve kısayol simgesi.
 *
 * Arayüzün hiç platform bilgisi yoktu. Sonuç (Windows saha bulguları):
 *  * macOS'un trafik ışıkları için ayrılan 40 px'lik bant Windows'ta da
 *    duruyordu. Windows başlık çubuğunu kendisi çizer; bant onun ALTINDA boş
 *    bir şerit olarak kalıyor, ürün işareti ve bütün arayüz 40 px aşağı
 *    itiliyordu ("Simge biraz aşağıda kalmış", madde 1).
 *  * İpuçları ve duyurular Windows'ta "⌘O", "⌘," ve "⌘C" diyordu; kısayolun
 *    kendisi Ctrl ile çalışıyordu (useShortcuts).
 *
 * Tespit kullanıcı aracısından yapılır: WebView2 "Windows NT", WKWebView
 * "Macintosh" taşır. Eklenti ya da yeni izin gerekmez, ilk çizimden önce
 * eşzamanlı çalışır.
 */
export type Platform = "mac" | "windows" | "other";

export function detectPlatform(userAgent: string): Platform {
  if (/Windows NT/.test(userAgent)) return "windows";
  if (/Macintosh|Mac OS X/.test(userAgent)) return "mac";
  return "other";
}

let current: Platform | null = null;

export function platform(): Platform {
  current ??= detectPlatform(typeof navigator === "undefined" ? "" : navigator.userAgent);
  return current;
}

/** Kısayolun kullanıcıya gösterilen adı: macOS'ta ⌘O, diğerlerinde Ctrl+O. */
export function shortcutLabel(key: string, on: Platform = platform()): string {
  return on === "mac" ? `⌘${key}` : `Ctrl+${key}`;
}

/**
 * Kökü işaretler. Görsel kararlar CSS'te kalır: `data-titlebar="overlay"`
 * yalnız macOS'ta (tauri.conf.json: titleBarStyle Overlay — bu ayar yalnız
 * macOS'ta derlenir), `native` başka her yerde.
 */
export function applyPlatform(root: HTMLElement, on: Platform = platform()): void {
  root.dataset.platform = on;
  root.dataset.titlebar = on === "mac" ? "overlay" : "native";
}
