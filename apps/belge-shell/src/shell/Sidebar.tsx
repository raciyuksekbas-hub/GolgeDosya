import type { FeatureState } from "./types";
import { IconCompare, IconConvert, IconEdit, IconReview, IconSettings } from "./icons";
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
 * Kullanılamayan bir kip macOS'ta olduğu gibi soluk ve tıklanamaz görünür;
 * gövdede açıklama metni yoktur. Neden kullanılamadığı son kullanıcıyı
 * ilgilendirmez.
 *
 * Altta açık belge durur: kullanıcı kipler arasında gezerken hangi belge
 * üzerinde çalıştığını görmeyi bırakmaz. Tam dosya yolu gösterilmez.
 */
export function Sidebar({
  features,
  current,
  onNavigate,
  onOpenSettings,
  openDocuments = [],
}: Props) {
  return (
    <aside className="sidebar">
      <div className="sidebar-title">Yüksekbaş Belge</div>

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

      <div className="sidebar-spacer" />

      {openDocuments.length > 0 ? (
        <div className="sidebar-context">
          <div className="sidebar-context-label">Açık belge</div>
          {openDocuments.map((p) => (
            <div key={p} className="sidebar-context-name" title={fileNameOf(p)}>
              {fileNameOf(p)}
            </div>
          ))}
        </div>
      ) : null}

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
