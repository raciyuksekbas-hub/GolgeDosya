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

/** Sunucu tarafı render'da `useLayoutEffect` uyarı basar; orada etki zaten yok. */
const useIsomorphicLayoutEffect = typeof window !== "undefined" ? useLayoutEffect : useEffect;

export interface Chrome {
  toolbarSlot: HTMLElement | null;
  inspectorSlot: HTMLElement | null;
  /** Panelde içerik var mı? `InspectorPanel` bağlanınca/ayrılınca bildirir. */
  setHasInspector: (has: boolean) => void;
}

const ChromeContext = createContext<Chrome | null>(null);
export const ChromeProvider = ChromeContext.Provider;

/** Yardımcı barın sağ ucu: kipin birincil ve ikincil eylemleri. */
export function ToolbarActions({ children }: { children: ReactNode }) {
  const chrome = useContext(ChromeContext);
  if (!chrome) return <div className="toolbar-actions">{children}</div>;
  if (!chrome.toolbarSlot) return null;
  return createPortal(children, chrome.toolbarSlot);
}

/**
 * Sağ panel. Yalnız gösterecek gerçek içerik varken var olur: kip bu bileşeni
 * çizmiyorsa üçüncü kolon hiç açılmaz.
 */
export function InspectorPanel({
  title,
  scope,
  children,
}: {
  title: string;
  /** Modülün kapsam sınıfı (`pdf-root`, `compare-root`): portal dışında da
      modülün kendi CSS'i geçerli kalsın diye panel sarmalayıcısına eklenir. */
  scope?: string;
  children: ReactNode;
}) {
  const chrome = useContext(ChromeContext);
  useIsomorphicLayoutEffect(() => {
    if (!chrome) return;
    chrome.setHasInspector(true);
    return () => chrome.setHasInspector(false);
  }, [chrome]);

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

/** Panel içinde ikinci ve sonraki bölümler; ilk bölümün başlığı panelin kendisidir. */
export function InspectorSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="inspector-section">
      <h3 className="inspector-label">{title}</h3>
      {children}
    </section>
  );
}
