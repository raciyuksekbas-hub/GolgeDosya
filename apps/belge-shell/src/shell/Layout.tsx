import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { FeatureState, PrefTab } from "./types";
import { Sidebar } from "./Sidebar";
import { Announcer } from "../shared-ui/Announcer";
import { IconButton } from "../shared-ui/primitives";
import { IconPanel } from "./icons";
import { ChromeProvider } from "./chrome";
import { useFullscreen } from "./useFullscreen";
import {
  activeToolsChoice,
  inspectorShown,
  inspectorToggleLabel,
  type InspectorKind,
  type ToolsChoice,
} from "./inspectorRule";

interface Props {
  features: FeatureState[];
  current: string;
  onNavigate: (route: string) => void;
  onOpenSettings: (tab: PrefTab) => void;
  /** Ürün işaretine tıklamak: kipin başlangıç yüzeyi. Yoksa işaret düğme değildir. */
  onHome?: () => void;
  /** Yardımcı barın sol ucu: belge bağlamı. Belge yokken boş ve sakin. */
  context?: ReactNode;
  /** Kabuğun kendi eylemleri (Kapat, Belge Aç). Kip eylemleri portalla gelir. */
  actions?: ReactNode;
  children: ReactNode;
}

/** Ayrıntılar paneli bu genişliğin altında varsayılan olarak kapalı gelir.
    Düzenle'nin Araçlar paneli bu eşiğe bağlı değildir (`inspectorRule.ts`). */
const WIDE = "(min-width: 1280px)";

/** Sunucu tarafı render'da `useLayoutEffect` uyarı basar; orada etki zaten yok. */
const useIsomorphicLayoutEffect = typeof window !== "undefined" ? useLayoutEffect : useEffect;

/**
 * Pencere iskeleti — dört yüzey.
 *
 *   kenar çubuğu · yardımcı bar · workspace · sağ panel
 *
 * Başlık çubuğu örtüşük: trafik ışıkları kenar çubuğunun üstünde durur, kenar
 * çubuğu pencerenin tepesine kadar çıkar ve içerik sütununun üstünde yalnız
 * tek bir bar vardır. O bar sayfa başlığı taşımaz; sol ucunda belge bağlamı,
 * sağ ucunda kipin eylemi durur. Sağ panel yalnız içerik varken çizilir.
 */
export function Layout({
  features,
  current,
  onNavigate,
  onOpenSettings,
  onHome,
  context,
  actions,
  children,
}: Props) {
  const [toolbarSlot, setToolbarSlot] = useState<HTMLElement | null>(null);
  const [statusSlot, setStatusSlot] = useState<HTMLElement | null>(null);
  const [inspectorSlot, setInspectorSlot] = useState<HTMLElement | null>(null);
  const [inspectorKind, setInspectorKind] = useState<InspectorKind | null>(null);
  const [hasActions, setHasActions] = useState(false);
  const [wide, setWide] = useState(() =>
    typeof window === "undefined" ? true : window.matchMedia(WIDE).matches,
  );
  const [inspectorOpen, setInspectorOpen] = useState(wide);
  const fullscreen = useFullscreen();
  // Araçlar paneli: kullanıcının bu kipte düğmeyle verdiği karar. Ayrıntılar
  // panelinin durumundan AYRI tutulur; tek `Layout` bütün kiplere hizmet
  // ettiği için Düzenle'de açılan panel Karşılaştır'a taşınmamalı.
  const [toolsChoice, setToolsChoice] = useState<ToolsChoice | null>(null);
  // Kipten ayrılınca elle verilen karar düşer; geri dönüldüğünde kural işler.
  if (toolsChoice && toolsChoice.route !== current) setToolsChoice(null);
  // İçerik sütununun (pencere − kenar çubuğu) ölçüsü. Panel açılıp kapanınca
  // değişmez; kenar çubuğu metin ölçeğiyle genişlediğinde değişir.
  const [content, setContent] = useState<HTMLElement | null>(null);
  const [contentWidth, setContentWidth] = useState<number | null>(null);
  useIsomorphicLayoutEffect(() => {
    if (!content) return;
    const measure = () => setContentWidth(content.getBoundingClientRect().width);
    measure();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measure);
      return () => window.removeEventListener("resize", measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(content);
    return () => observer.disconnect();
  }, [content]);

  // Dar pencerede panel kendiliğinden kapanır, genişleyince geri gelir.
  // Kullanıcının elle açması/kapatması bir sonraki eşik geçişine kadar geçerli.
  useEffect(() => {
    if (typeof window === "undefined") return;
    const query = window.matchMedia(WIDE);
    const onChange = () => {
      setWide(query.matches);
      setInspectorOpen(query.matches);
    };
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const kindRef = useRef(inspectorKind);
  const routeRef = useRef(current);
  useIsomorphicLayoutEffect(() => {
    kindRef.current = inspectorKind;
    routeRef.current = current;
  });
  const revealInspector = useCallback(() => {
    if (kindRef.current === "tools") setToolsChoice({ route: routeRef.current, open: true });
    else setInspectorOpen(true);
  }, []);

  const chrome = useMemo(
    () => ({ toolbarSlot, statusSlot, inspectorSlot, setInspectorKind, setHasActions, revealInspector }),
    [toolbarSlot, statusSlot, inspectorSlot, revealInspector],
  );

  const hasInspector = inspectorKind !== null;
  const panelOpen = inspectorShown({
    kind: inspectorKind,
    detailsOpen: inspectorOpen,
    toolsChoice: activeToolsChoice(toolsChoice, current),
    contentWidth,
  });
  const togglePanel = () => {
    if (inspectorKind === "tools") setToolsChoice({ route: current, open: !panelOpen });
    else setInspectorOpen((v) => !v);
  };
  /**
   * Bar gerçekten boş mu?
   *
   * Home'da bağlam da eylem de yoktur (§6: "boşsa sakin bırak"). Ama boş bir
   * bandı yine de çizip altına çizgi koymak, reddedilen "ikinci başlık şeridi"
   * hissini geri getiriyordu — gerçek pencerede görüldü. Boşken bar bir bant
   * değil, workspace'in üst boşluğudur: çizgi yok, kendi rengi yok. Trafik
   * ışıklarının yüksekliği ve sürükleme alanı korunur.
   */
  const barEmpty = !context && !actions && !hasInspector && !hasActions;

  return (
    <ChromeProvider value={chrome}>
      <div className="shell" data-wide={wide} data-fullscreen={fullscreen}>
        <a className="skip-link" href="#icerik">
          İçeriğe geç
        </a>
        <Sidebar
          features={features}
          current={current}
          onNavigate={onNavigate}
          onOpenSettings={onOpenSettings}
          onHome={onHome}
        />
        <section className="content" ref={setContent}>
          {/* Başlık çubuğu bandı: trafik ışıklarının satırı pencerenin
              tamamında ayrılır, yalnız kenar çubuğunda değil. Yardımcı bar
              onun altında başlar ve kimlik bloğuyla hizalanır. */}
          <div className="titlebar-band" data-tauri-drag-region />
          <div className="toolbar" data-empty={barEmpty} data-tauri-drag-region>
            <div className="toolbar-context">{context}</div>
            {/* Bağlamın ÖLÇÜSÜ: kaç sayfa, kaç fark, kaç bulgu. Kip doldurur;
                doldurmazsa yuva hiç görünmez (`:empty`). */}
            <div className="toolbar-status" ref={setStatusSlot} />
            <div className="toolbar-spacer" data-tauri-drag-region />
            <div className="toolbar-actions" ref={setToolbarSlot} />
            <div className="toolbar-shell">
              {hasInspector ? (
                <IconButton
                  label={inspectorToggleLabel(inspectorKind, panelOpen)}
                  pressed={panelOpen}
                  onClick={togglePanel}
                >
                  <IconPanel />
                </IconButton>
              ) : null}
              {actions}
            </div>
          </div>
          <main id="icerik" className="content-body" tabIndex={-1}>
            {children}
          </main>
          <div className="inspector-slot" ref={setInspectorSlot} hidden={!panelOpen} />
        </section>
        <Announcer />
      </div>
    </ChromeProvider>
  );
}
