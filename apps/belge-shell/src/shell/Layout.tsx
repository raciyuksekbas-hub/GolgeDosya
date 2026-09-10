import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { FeatureState } from "./types";
import { Sidebar } from "./Sidebar";
import { Announcer } from "../shared-ui/Announcer";
import { IconButton } from "../shared-ui/primitives";
import { IconPanel } from "./icons";
import { ChromeProvider } from "./chrome";
import { useFullscreen } from "./useFullscreen";

interface Props {
  features: FeatureState[];
  current: string;
  onNavigate: (route: string) => void;
  onOpenSettings: () => void;
  /** Yardımcı barın sol ucu: belge bağlamı. Belge yokken boş ve sakin. */
  context?: ReactNode;
  /** Kabuğun kendi eylemleri (Kapat, Belge Aç). Kip eylemleri portalla gelir. */
  actions?: ReactNode;
  children: ReactNode;
}

/** Panel bu genişliğin altında varsayılan olarak kapalı gelir. */
const WIDE = "(min-width: 1280px)";

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
  context,
  actions,
  children,
}: Props) {
  const [toolbarSlot, setToolbarSlot] = useState<HTMLElement | null>(null);
  const [inspectorSlot, setInspectorSlot] = useState<HTMLElement | null>(null);
  const [hasInspector, setHasInspector] = useState(false);
  const [hasActions, setHasActions] = useState(false);
  const [wide, setWide] = useState(() =>
    typeof window === "undefined" ? true : window.matchMedia(WIDE).matches,
  );
  const [inspectorOpen, setInspectorOpen] = useState(wide);
  const fullscreen = useFullscreen();

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

  const chrome = useMemo(
    () => ({ toolbarSlot, inspectorSlot, setHasInspector, setHasActions }),
    [toolbarSlot, inspectorSlot],
  );

  const showInspector = hasInspector && inspectorOpen;
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
        />
        <section className="content">
          {/* Başlık çubuğu bandı: trafik ışıklarının satırı pencerenin
              tamamında ayrılır, yalnız kenar çubuğunda değil. Yardımcı bar
              onun altında başlar ve kimlik bloğuyla hizalanır. */}
          <div className="titlebar-band" data-tauri-drag-region />
          <div className="toolbar" data-empty={barEmpty} data-tauri-drag-region>
            <div className="toolbar-context">{context}</div>
            <div className="toolbar-spacer" data-tauri-drag-region />
            <div className="toolbar-actions" ref={setToolbarSlot} />
            <div className="toolbar-shell">
              {hasInspector ? (
                <IconButton
                  label={inspectorOpen ? "Ayrıntıları gizle" : "Ayrıntıları göster"}
                  pressed={inspectorOpen}
                  onClick={() => setInspectorOpen((v) => !v)}
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
          <div className="inspector-slot" ref={setInspectorSlot} hidden={!showInspector} />
        </section>
        <Announcer />
      </div>
    </ChromeProvider>
  );
}
