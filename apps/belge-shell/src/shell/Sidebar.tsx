import type { FeatureState } from "./types";
import { IconCompare, IconConvert, IconEdit, IconReview, IconSettings } from "./icons";
import { announce } from "../shared-ui/Announcer";

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
}

/**
 * Kenar çubuğu.
 *
 * Kullanılamayan bir bölüm macOS'ta olduğu gibi **soluk ve tıklanamaz** görünür;
 * gövdede hiçbir açıklama metni yoktur. Neden kullanılamadığı son kullanıcıyı
 * ilgilendiren bir bilgi değildir; geliştirici tarafı için `console.debug`'a
 * yazılır (bkz. App.tsx).
 */
export function Sidebar({ features, current, onNavigate, onOpenSettings }: Props) {
  return (
    <aside className="sidebar">
      <div className="sidebar-title">Yüksekbaş Belge</div>
      <nav className="sidebar-nav" aria-label="Bölümler">
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
                announce(`${f.label}`);
              }}
            >
              <Icon className="sidebar-icon" />
              <span>{f.label}</span>
            </button>
          );
        })}
      </nav>
      <div className="sidebar-spacer" />
      <div className="sidebar-footer">
        <button type="button" className="sidebar-item" onClick={onOpenSettings}>
          <IconSettings className="sidebar-icon" />
          <span>Ayarlar</span>
        </button>
      </div>
    </aside>
  );
}
