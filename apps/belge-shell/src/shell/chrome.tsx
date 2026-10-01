/**
 * Kabuk çerçevesi — kiplerin yardımcı bara ve sağ panele çizim yapması için
 * iki sunum adaptörü.
 *
 * Kipler kendi durumunu, komutlarını ve motor çağrılarını tutmaya devam eder;
 * yalnız birincil eylemleri ve ikincil bağlamı **çizdikleri yer** değişir:
 * eylem yardımcı barın sağ ucuna, bağlam sağdaki panele gider. Böylece dört
 * kip tek pencere düzenini paylaşır ve hiçbir motor UI'ye uydurulmak için
 * değiştirilmez.
 *
 * Kabuk yokken (sunucu tarafı render, testler) içerik olduğu yerde satır içi
 * çizilir; portal yalnız gerçek pencerede kurulur.
 */
import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import type { InspectorKind } from "./inspectorRule";

/** Sunucu tarafı render'da `useLayoutEffect` uyarı basar; orada etki zaten yok. */
const useIsomorphicLayoutEffect = typeof window !== "undefined" ? useLayoutEffect : useEffect;

export interface Chrome {
  toolbarSlot: HTMLElement | null;
  statusSlot: HTMLElement | null;
  inspectorSlot: HTMLElement | null;
  /** Panelde içerik var mı, varsa hangi tür? `InspectorPanel` bağlanınca/ayrılınca bildirir. */
  setInspectorKind: (kind: InspectorKind | null) => void;
  /** Paneli açar: kaydetmeyi bekleten bir onay yalnız paneldeyse. */
  revealInspector: () => void;
  /** Barda kipe ait eylem var mı? Portalla geldiği için kabuk kendi göremez. */
  setHasActions: (has: boolean) => void;
}

const ChromeContext = createContext<Chrome | null>(null);
export const ChromeProvider = ChromeContext.Provider;

/** Yardımcı barın sağ ucu: kipin birincil ve ikincil eylemleri. */
export function ToolbarActions({ children }: { children: ReactNode }) {
  const chrome = useContext(ChromeContext);
  useIsomorphicLayoutEffect(() => {
    if (!chrome) return;
    // Bar, portalla gelen içeriği kendi göremez; boş mu dolu mu olduğunu
    // buradan öğrenir. Boş bar bir bant değildir (çizgisiz, kendi rengi yok).
    chrome.setHasActions(true);
    return () => chrome.setHasActions(false);
  }, [chrome]);
  if (!chrome) return <div className="toolbar-actions">{children}</div>;
  if (!chrome.toolbarSlot) return null;
  return createPortal(children, chrome.toolbarSlot);
}

/**
 * Yardımcı barın ORTASI: açık bağlamın ölçüsü.
 *
 * Toolbar boş bir bant olmasın diye eklenmedi — belge adının yanında o
 * belgenin **büyüklüğü** durur: kaç sayfa, hangi yöne dönüşecek, kaç fark,
 * kaç bulgu. Kullanıcının "şu an neye bakıyorum" sorusunun ikinci yarısı
 * budur ve her kipte aynı yerde okunur.
 *
 * Sayı kipin kendi durumundan gelir; kabuk onu üretemez, yalnız yerini verir.
 */
export function ToolbarStatus({ children }: { children: ReactNode }) {
  const chrome = useContext(ChromeContext);
  if (!chrome) return <span className="toolbar-status">{children}</span>;
  if (!chrome.statusSlot) return null;
  return createPortal(children, chrome.statusSlot);
}

/**
 * Sağ panel. Yalnız gösterecek gerçek içerik varken var olur: kip bu bileşeni
 * çizmiyorsa üçüncü kolon hiç açılmaz.
 */
export function InspectorPanel({
  title,
  scope,
  kind = "details",
  children,
}: {
  title: string;
  /** Modülün kapsam sınıfı (`pdf-root`, `compare-root`): portal dışında da
      modülün kendi CSS'i geçerli kalsın diye panel sarmalayıcısına eklenir. */
  scope?: string;
  /** "details": ikincil bağlam, 1280 px altında kapalı gelir. "tools": kipin
      araçlarının tek yeri; workspace'e yer kaldıkça açık gelir
      (`inspectorRule.ts`). */
  kind?: InspectorKind;
  children: ReactNode;
}) {
  const chrome = useContext(ChromeContext);
  useIsomorphicLayoutEffect(() => {
    if (!chrome) return;
    chrome.setInspectorKind(kind);
    return () => chrome.setInspectorKind(null);
  }, [chrome, kind]);

  const panel = (
    <aside className={`inspector${scope ? ` ${scope}` : ""}`} aria-label={title}>
      <div className="inspector-head">
        <h2 className="inspector-title">{title}</h2>
      </div>
      <div className="inspector-body">{children}</div>
    </aside>
  );
  if (!chrome) return panel;
  if (!chrome.inspectorSlot) return null;
  return createPortal(panel, chrome.inspectorSlot);
}

/** Paneli açan işlev; kabuk yokken (testler) hiçbir şey yapmaz. */
export function useRevealInspector(): () => void {
  const chrome = useContext(ChromeContext);
  return chrome ? chrome.revealInspector : noop;
}
const noop = () => {};

/** Panel içinde ikinci ve sonraki bölümler; ilk bölümün başlığı panelin kendisidir. */
export function InspectorSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="inspector-section">
      <h3 className="inspector-label">{title}</h3>
      {children}
    </section>
  );
}
