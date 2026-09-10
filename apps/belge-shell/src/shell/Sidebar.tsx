import type { FeatureState } from "./types";
import {
  AppMark,
  IconCompare,
  IconConvert,
  IconDocument,
  IconEdit,
  IconReview,
  IconSettings,
} from "./icons";
import { announce } from "../shared-ui/Announcer";
import { MODES, fileNameOf } from "./modes";

const ICONS = {
  duzenek: IconEdit,
  tavzih: IconConvert,
  degisikis: IconCompare,
  ikincigoz: IconReview,
} as const;

interface Props {
  features: FeatureState[];
  current: string;
  onNavigate: (route: string) => void;
  onOpenSettings: () => void;
  /** Açık belgeler. Boş bağlam geçerli bir durumdur; verilmezse boş sayılır. */
  openDocuments?: string[];
}

/**
 * Kenar çubuğu — dört çalışma kipi.
 *
 * Pencere başlığı gizli olduğundan uygulama adı burada, trafik ışıklarının
 * altında durur: ürünün kimliği kenar çubuğunun tepesindedir, pencere
 * çerçevesinde değil. Kullanılamayan bir kip macOS'ta olduğu gibi soluk ve
 * tıklanamaz görünür; gövdede açıklama metni yoktur.
 *
 * Seçili kip vurgu rengiyle boyanmaz: tonal bir yüzey ve solda ince bir vurgu
 * işareti yeter. Gruplar hairline ve küçük BÜYÜK HARF etiketle ayrılır.
 */
export function Sidebar({
  features,
  current,
  onNavigate,
  onOpenSettings,
  openDocuments = [],
}: Props) {
  return (
    <aside className="sidebar" data-tauri-drag-region>
      <div className="sidebar-head" data-tauri-drag-region>
        <AppMark className="sidebar-mark" />
        <span className="sidebar-brand">Yüksekbaş Belge</span>
      </div>

      <nav className="sidebar-nav" aria-label="Çalışma kipleri">
        {features.map((f) => {
          const Icon = ICONS[f.key];
          const active = current === f.route;
          return (
            <button
              key={f.key}
              type="button"
              className="sidebar-item"
              data-feature={f.key}
              aria-current={active ? "page" : undefined}
              disabled={!f.enabled}
              onClick={() => {
                onNavigate(f.route);
                announce(MODES[f.key].label);
              }}
            >
              <Icon className="sidebar-icon" />
              <span>{MODES[f.key].label}</span>
            </button>
          );
        })}
      </nav>

      {openDocuments.length > 0 ? (
        <div className="sidebar-group">
          <div className="sidebar-label">Açık belge</div>
          {openDocuments.map((p) => (
            <div key={p} className="sidebar-doc" title={fileNameOf(p)}>
              <IconDocument className="sidebar-icon" />
              <span>{fileNameOf(p)}</span>
            </div>
          ))}
        </div>
      ) : null}

      <div className="sidebar-spacer" data-tauri-drag-region />

      <div className="sidebar-footer">
        <button
          type="button"
          className="sidebar-item"
          onClick={onOpenSettings}
          title="Ayarlar  ⌘,"
        >
          <IconSettings className="sidebar-icon" />
          <span>Ayarlar</span>
        </button>
      </div>
    </aside>
  );
}
