/**
 * Kabuk simgeleri.
 *
 * Elle çizilmiş, 16 px ızgaraya oturan ince çizgili SVG'ler. Simge kütüphanesi
 * eklenmedi: dört simge için bir bağımlılık taşımak ve onun görsel diline
 * bağlanmak, native macOS hissinden uzaklaştırır.
 */
type P = { className?: string };
const base = {
  width: 16,
  height: 16,
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.3,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

/** Düzenle — sayfalar ve düzenleme kalemi. */
export const IconEdit = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M9.5 1.8H3.4a1 1 0 0 0-1 1v10.4a1 1 0 0 0 1 1h7.2a1 1 0 0 0 1-1V8" />
    <path d="M13.9 2.4a1.2 1.2 0 0 0-1.7-1.7L8.6 4.3l-.5 2.2 2.2-.5z" />
  </svg>
);

/** Dönüştür — iki biçim arasında gidiş geliş. */
export const IconConvert = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M2.4 5.6h9.1M9.3 3.4l2.2 2.2-2.2 2.2" />
    <path d="M13.6 10.4H4.5m2.2-2.2-2.2 2.2 2.2 2.2" />
  </svg>
);

/** Karşılaştır — yan yana iki belge. */
export const IconCompare = ({ className }: P) => (
  <svg {...base} className={className}>
    <rect x="1.7" y="2.4" width="5.2" height="11.2" rx="1" />
    <rect x="9.1" y="2.4" width="5.2" height="11.2" rx="1" />
    <path d="M3.3 5.6h2M3.3 8h2M10.7 5.6h2M10.7 8h2M10.7 10.4h2" />
  </svg>
);

/** Denetle — belge üzerinde büyüteç. */
export const IconReview = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M12.6 7.4V2.8a1 1 0 0 0-1-1H3.4a1 1 0 0 0-1 1v10.4a1 1 0 0 0 1 1h4" />
    <path d="M4.9 4.8h5.2M4.9 7.2h3.4" />
    <circle cx="11.1" cy="11.1" r="2.4" />
    <path d="m12.9 12.9 1.4 1.4" />
  </svg>
);

/** Tek belge — kenar çubuğu bağlamı ve bar çipi. */
export const IconDocument = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M9.4 1.8H4.6a1 1 0 0 0-1 1v10.4a1 1 0 0 0 1 1h6.8a1 1 0 0 0 1-1V4.8z" />
    <path d="M9.4 1.8v3h3" />
    <path d="M5.8 8h4.4M5.8 10.4h3" />
  </svg>
);

/** Sağ panel — göster/gizle. */
export const IconPanel = ({ className }: P) => (
  <svg {...base} className={className}>
    <rect x="1.8" y="2.8" width="12.4" height="10.4" rx="1.2" />
    <path d="M9.6 2.8v10.4" />
  </svg>
);

/**
 * Uygulama işareti — markanın "Kat" formu (brand/mark.svg ile aynı geometri).
 *
 * Sayfa, sağ alt köşesi kaldırılmış; altındaki pirinç katman görünüyor.
 * Kaldırılmış köşe markanın vurgu rengini taşır — bu, kenar çubuğundaki
 * "tek vurgu taşıyıcı" kuralının bilinçli istisnasıdır: kimlik bir kontrol
 * değildir ve seçili kip işareti hâlâ tek *etkileşimli* vurgudur.
 */
export const AppMark = ({ className }: P) => (
  <svg width={18} height={18} viewBox="0 0 64 64" aria-hidden className={className}>
    <defs>
      <clipPath id="gd-sidebar-mark">
        <rect x="12" y="8" width="40" height="48" rx="7" />
      </clipPath>
    </defs>
    <g clipPath="url(#gd-sidebar-mark)">
      <rect x="12" y="8" width="40" height="48" fill="var(--accent)" />
      <path d="M12 8 H52 V32 L28 56 H12 Z" fill="currentColor" />
    </g>
  </svg>
);

export const IconSettings = ({ className }: P) => (
  <svg {...base} className={className}>
    <circle cx="8" cy="8" r="2.2" />
    <path d="M8 1.6v1.8M8 12.6v1.8M14.4 8h-1.8M3.4 8H1.6M12.5 3.5l-1.3 1.3M4.8 11.2l-1.3 1.3M12.5 12.5l-1.3-1.3M4.8 4.8 3.5 3.5" />
  </svg>
);

/** Hakkında — bilgi. */
export const IconAbout = ({ className }: P) => (
  <svg {...base} className={className}>
    <circle cx="8" cy="8" r="6.2" />
    <path d="M8 7.2v4M8 4.9h.01" />
  </svg>
);

/** Geri bildirim — söz. */
export const IconFeedback = ({ className }: P) => (
  <svg {...base} className={className}>
    <path d="M13.6 9.8a1 1 0 0 1-1 1H6.2L3.4 13.2V4.2a1 1 0 0 1 1-1h8.2a1 1 0 0 1 1 1z" />
  </svg>
);
