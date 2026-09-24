/**
 * Tarayıcının kendi sağ tık menüsü uygulamanın parçası değildir.
 *
 * Saha bulgusu (Windows): sayfanın boş bir yerine sağ tıklayınca WebView2'nin
 * varsayılan menüsü açılıyordu — "Yenile", "Farklı kaydet", "Paylaş", "Yazdır".
 * Kullanıcı bunları GölgeDosya komutu sanıyor: "Yenile" bütün çalışmayı
 * uyarısız siliyordu (her şey bellekte), "Farklı kaydet" belgeyi değil
 * uygulamanın HTML sayfasını kaydediyordu, "Paylaş" tauri.localhost adresini
 * paylaşıyordu. Menüdeki Türkçe etiketler Windows'un dilinden gelir; o yüzden
 * uygulamanın kendi komutları gibi görünürler.
 *
 * Kural: tarayıcı menüsü yalnız metin DÜZENLEMEK ve seçili metni KOPYALAMAK için
 * kalır — kullanıcının orada beklediği tek şey budur (Kes/Kopyala/Yapıştır).
 * Görsel önizlemelerde menü hiç açılmaz: "Resmi farklı kaydet", işaretsiz bir
 * önizleme PNG'sini dışarı verirdi.
 *
 * Klavye tarafı (F5, Ctrl+R, Ctrl+S) Windows'ta yerel katmanda kapatılır; sayfa
 * JavaScript'i tarayıcının hızlandırıcılarından SONRA çalışır (bkz.
 * src-tauri/src/browser_surface.rs).
 */

/** Olay hedefinin, DOM olmadan da test edilebilen asgari yüzü. */
interface TargetLike {
  closest?: (selector: string) => unknown;
  parentElement?: TargetLike | null;
}

interface SelectionLike {
  isCollapsed: boolean;
  containsNode: (node: never, allowPartial: boolean) => boolean;
}

const EDITABLE = 'input, textarea, select, [contenteditable=""], [contenteditable="true"]';
const MEDIA = "img, canvas, video, svg, picture";

function elementOf(target: unknown): TargetLike | null {
  const node = target as TargetLike | null;
  if (!node) return null;
  if (typeof node.closest === "function") return node;
  // Metin düğümü: menü, içinde bulunduğu öğeye göre karar verir.
  const parent = node.parentElement ?? null;
  return parent && typeof parent.closest === "function" ? parent : null;
}

/**
 * Tarayıcının kendi menüsü bu hedefte açılabilir mi?
 *
 * Evet: düzenlenebilir alan (Kes/Kopyala/Yapıştır) ya da hedef, çökmemiş bir
 * seçimin İÇİNDEyse (Kopyala). Seçimin başka bir yerde durması yetmez — yoksa
 * sayfada unutulmuş bir seçim, önizleme resmine sağ tıklayınca "Resmi farklı
 * kaydet"i geri getirirdi.
 */
export function keepsNativeMenu(target: unknown, selection: SelectionLike | null): boolean {
  const element = elementOf(target);
  if (!element?.closest) return false;
  if (element.closest(EDITABLE)) return true;
  if (element.closest(MEDIA)) return false;
  return Boolean(selection && !selection.isCollapsed && selection.containsNode(element as never, true));
}

interface DocumentLike {
  addEventListener: (type: "contextmenu", listener: (event: MouseEvent) => void, options: { capture: boolean }) => void;
  removeEventListener: (type: "contextmenu", listener: (event: MouseEvent) => void, options: { capture: boolean }) => void;
  getSelection: () => SelectionLike | null;
}

/** Üretim girişinde bir kez kurulur; geri dönen fonksiyon kaldırır. */
export function installBrowserSurfaceGuard(doc: DocumentLike = document as unknown as DocumentLike): () => void {
  const onContextMenu = (event: MouseEvent) => {
    if (!keepsNativeMenu(event.target, doc.getSelection())) event.preventDefault();
  };
  // Yakalama evresinde: bir bileşenin stopPropagation'ı korumayı delemez.
  doc.addEventListener("contextmenu", onContextMenu, { capture: true });
  return () => doc.removeEventListener("contextmenu", onContextMenu, { capture: true });
}
