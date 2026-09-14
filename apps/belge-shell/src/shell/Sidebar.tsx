import type { FeatureState, PrefTab } from "./types";
import {
  AppMark,
  IconCompare,
  IconConvert,
  IconEdit,
  IconReview,
  IconSettings,
  IconAbout,
  IconFeedback,
} from "./icons";
import { announce } from "../shared-ui/Announcer";
import { MODES } from "./modes";

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
  onOpenSettings: (tab: PrefTab) => void;
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
 * işareti yeter.
 *
 * Açık belge burada TEKRARLANMAZ: yardımcı bar her kipte belgenin adını ve
 * türünü zaten taşıyor. İkisi aynı ekranda yan yana durunca aynı bilgi iki kez
 * yazılmış oluyordu — gerçek pencerede görüldü.
 */
export function Sidebar({ features, current, onNavigate, onOpenSettings }: Props) {
  return (
    <aside className="sidebar" data-tauri-drag-region>
      <div className="sidebar-head" data-tauri-drag-region>
        <AppMark className="sidebar-mark" />
        <span className="sidebar-brand" title="GölgeDosya">GölgeDosya</span>
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

      <div className="sidebar-spacer" data-tauri-drag-region />

      {/* Ürün yüzeyleri: gizli değil, kiplerin altında kendi grubunda. Üçü de
          tercihler penceresini kendi sekmesinde açar. */}
      <div className="sidebar-footer">
        <button type="button" className="sidebar-item" onClick={() => onOpenSettings("genel")} title="Ayarlar  ⌘,">
          <IconSettings className="sidebar-icon" />
          <span>Ayarlar</span>
        </button>
        <button type="button" className="sidebar-item" onClick={() => onOpenSettings("hakkinda")}>
          <IconAbout className="sidebar-icon" />
          <span>Hakkında</span>
        </button>
        <button type="button" className="sidebar-item" onClick={() => onOpenSettings("geri")}>
          <IconFeedback className="sidebar-icon" />
          <span>Geri Bildirim</span>
        </button>
      </div>
    </aside>
  );
}
