import { useCallback, useRef, useState } from "react";
import type { PrefTab, Settings } from "./types";
import { useFocusTrap } from "../shared-ui/useFocusTrap";
import { AppMark } from "./icons";
import { announce } from "../shared-ui/Announcer";
import { Button, Field, Status } from "../shared-ui/primitives";
import { OutputFolderField } from "../modules/tavzih/OutputFolderField";
import { shortcutLabel } from "./platform";

interface Props {
  settings: Settings;
  /** Uygulama sürümü — yalnız Hakkında. Yapılandırma dizini asla gösterilmez. */
  version?: string | null;
  tab: PrefTab;
  onTab: (tab: PrefTab) => void;
  onChange: (next: Settings) => void;
  /** Tercih kaydedilemediyse nedeni. Sessiz kalmak yerine söylenir. */
  error?: string;
  /** Son belgeler listesini unut — kabuğun kendi eylemi. */
  onForget: () => void;
  onClose: () => void;
}

const TABS: { id: PrefTab; label: string }[] = [
  { id: "genel", label: "Genel" },
  { id: "gorunum", label: "Görünüm" },
  { id: "erisim", label: "Erişilebilirlik" },
  { id: "hakkinda", label: "Hakkında" },
  { id: "telif", label: "Telif" },
  { id: "geri", label: "Geri Bildirim" },
];

/**
 * Üçüncü taraf bildirimlerinin özeti.
 *
 * `THIRD_PARTY_NOTICES.md` gerçek bağımlılık denetiminden üretilir ve tam
 * metin ürünle birlikte dağıtılır. Burada yalnız lisans aileleri özetlenir;
 * bildirimler KALDIRILMAZ, kısaltılır.
 */
const NOTICES: { license: string; packages: string }[] = [
  { license: "MIT", packages: "react · react-dom · lopdf · zip · quick-xml · tiff · fflate" },
  { license: "Apache-2.0 / MIT", packages: "tauri · serde · image · rayon · regex · url" },
  { license: "Apache-2.0", packages: "pdfjs-dist" },
  { license: "BSD-2 / BSD-3", packages: "mammoth · lop · option · sprintf-js" },
];

/**
 * Geri bildirim yüzeyi.
 *
 * Ürünün **ağ izni yoktur**, bildirilmiş bir iletişim uç noktası yoktur ve
 * `opener` izni yalnız Dönüştür'ün çıktıyı klasöründe göstermesi
 * içindir. Bu yüzden bir gönderme uç noktası uydurulmadı: ne sahte bir
 * "gönderildi" onayı, ne de hiçbir şey yapmayan bir düğme.
 *
 * Var olan kapasiteyle çalışan tek dürüst eylem metni panoya almaktır:
 * kullanıcı yazdığını kendi kanalına taşır. Pano reddedilirse sessizce
 * geçilmez, metin seçilir ve ne yapılacağı söylenir.
 */
function FeedbackPane() {
  const [text, setText] = useState("");
  const [state, setState] = useState<"idle" | "copied" | "manual">("idle");
  const area = useRef<HTMLTextAreaElement>(null);

  const copy = useCallback(async () => {
    const value = text.trim();
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      setState("copied");
      announce("Geri bildirim metni panoya kopyalandı.");
    } catch {
      area.current?.select();
      setState("manual");
      announce(`Metin seçildi. Kopyalamak için ${shortcutLabel("C")} kullanın.`);
    }
  }, [text]);

  return (
    <div className="feedback">
      <p className="feedback-note">
        Sorun, öneri veya genel görüşünüzü yazın. Yazdığınız metin uygulamadan
        kendiliğinden çıkmaz; GölgeDosya ağ bağlantısı kurmaz ve belge
        içeriğiniz hiçbir zaman gönderilmez.
      </p>
      <label className="sr-only" htmlFor="geri-metin">
        Geri bildirim metni
      </label>
      <textarea
        id="geri-metin"
        ref={area}
        className="selectable"
        value={text}
        placeholder="Ne oldu, ne bekliyordunuz?"
        onChange={(e) => {
          setText(e.target.value);
          setState("idle");
        }}
      />
      <div className="feedback-actions">
        <Button variant="primary" disabled={text.trim().length === 0} onClick={copy}>
          Metni Kopyala
        </Button>
        <span className="feedback-limit" role="status">
          {state === "copied"
            ? "Kopyalandı. Kendi kanalınızdan iletebilirsiniz."
            : state === "manual"
              ? `Metin seçildi; ${shortcutLabel("C")} ile kopyalayın.`
              : "Bu sürümde uygulama içinden gönderim yoktur."}
        </span>
      </div>
    </div>
  );
}

/**
 * Tercihler penceresi.
 *
 * Solda sekme rayı olan gerçek bir macOS preferences yüzeyi. Ürünün kendisi —
 * hakkında, telif ve geri bildirim — gizli değil, kendi sekmelerinde.
 *
 * Form TEK grid'dir: etiket solda, kontrol sabit genişlikte sağ kolonda.
 * Yükseklik sabittir; sekme değişince pencere zıplamaz.
 *
 * Yapılandırma dizini, taşınan ayarlar, feature bayrakları ve depolama
 * anahtarları burada YOKTUR; bunlar geliştirme bilgisidir.
 */
export function PreferencesSheet({
  settings,
  version,
  tab,
  onTab,
  onChange,
  error,
  onForget,
  onClose,
}: Props) {
  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref, true, onClose);

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    onChange({ ...settings, [key]: value });

  const remembered = settings.recentDocuments.length;

  return (
    <div
      className="sheet-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="sheet prefs" ref={ref} role="dialog" aria-modal="true" aria-labelledby="ayar-baslik">
        {error ? (
          <p className="prefs-hint">
            <Status tone="error">{error}</Status>
          </p>
        ) : null}
        <nav className="prefs-tabs" aria-label="Tercih bölümleri">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className="sidebar-item"
              aria-current={tab === t.id ? "page" : undefined}
              onClick={() => onTab(t.id)}
            >
              <span>{t.label}</span>
            </button>
          ))}
        </nav>

        <div className="prefs-body">
          <h2 id="ayar-baslik" className="prefs-title">
            {TABS.find((t) => t.id === tab)?.label}
          </h2>

          {tab === "genel" ? (
            <>
              {/* Çıktı klasörü gerçek, çalışan bir tercihtir; motor yoksa
                  satır hiç çizilmez. Uydurma tercih eklenmedi. */}
              <div className="prefs-group">
                <OutputFolderField />
              </div>
              <div className="prefs-group">
                <div className="field">
                  <div className="field-label">
                    Son belgeler
                    <span>
                      {remembered > 0
                        ? `${remembered} belge hatırlanıyor`
                        : "Hatırlanan belge yok"}
                    </span>
                  </div>
                  <Button disabled={remembered === 0} onClick={onForget}>
                    Listeyi Temizle
                  </Button>
                </div>
                <p className="prefs-hint">
                  Liste yalnız bu bilgisayarda tutulur ve hiçbir yere gönderilmez.
                </p>
              </div>
            </>
          ) : null}

          {tab === "gorunum" ? (
            <>
              <Field label="Tema" htmlFor="ayar-tema">
                <select
                  id="ayar-tema"
                  value={settings.theme}
                  onChange={(e) => set("theme", e.target.value as Settings["theme"])}
                >
                  <option value="system">Sistemle aynı</option>
                  <option value="light">Açık</option>
                  <option value="dark">Koyu</option>
                </select>
              </Field>
              <Field label="Metin boyutu" htmlFor="ayar-olcek">
                <select
                  id="ayar-olcek"
                  value={String(settings.textScale)}
                  onChange={(e) => set("textScale", Number(e.target.value))}
                >
                  <option value="100">Normal</option>
                  <option value="125">Büyük</option>
                  <option value="150">Daha büyük</option>
                  <option value="175">Çok büyük</option>
                  <option value="200">En büyük</option>
                </select>
              </Field>
            </>
          ) : null}

          {tab === "erisim" ? (
            <>
              <Field
                label="Yüksek kontrast"
                hint="Çizgileri ve ikincil metni güçlendirir"
                htmlFor="ayar-kontrast"
              >
                <select
                  id="ayar-kontrast"
                  value={settings.highContrast}
                  onChange={(e) => set("highContrast", e.target.value as Settings["highContrast"])}
                >
                  <option value="system">Sistemle aynı</option>
                  <option value="on">Açık</option>
                  <option value="off">Kapalı</option>
                </select>
              </Field>
              <Field label="Hareketi azalt" hint="Geçişleri kaldırır" htmlFor="ayar-hareket">
                <select
                  id="ayar-hareket"
                  value={settings.reduceMotion}
                  onChange={(e) => set("reduceMotion", e.target.value as Settings["reduceMotion"])}
                >
                  <option value="system">Sistemle aynı</option>
                  <option value="on">Açık</option>
                  <option value="off">Kapalı</option>
                </select>
              </Field>
            </>
          ) : null}

          {tab === "hakkinda" ? (
            <div className="about">
              <AppMark className="about-mark" />
              <p className="about-name">GölgeDosya</p>
              <p className="about-version">Sürüm {version ?? "—"}</p>
              <p className="about-note">
                Belge çalışma ortamı. Dört aracın — Düzenle, Dönüştür, Karşılaştır, Denetle —
                tek pencerede birleşmiş hâli.
              </p>
              <p className="about-line">
                <span>Geliştirici</span> Raci Çetin Yüksekbaş
              </p>
              <p className="about-line">
                <span>Telif</span> © 2026 Raci Çetin Yüksekbaş
              </p>
            </div>
          ) : null}

          {tab === "telif" ? (
            <div className="legal selectable">
              <p className="legal-strong">Copyright © 2026 Raci Çetin Yüksekbaş</p>
              <p>Tüm hakları saklıdır. All Rights Reserved.</p>
              <p>
                GölgeDosya tescilli bir yazılımdır. Adı, görsel kimliği, kullanıcı arayüzü ve
                özgün bileşenleri korunmaktadır; izinsiz çoğaltılamaz, değiştirilemez veya
                yeniden yayımlanamaz.
              </p>

              <h3 className="section-head">Üçüncü taraf bildirimleri</h3>
              <p>
                Uygulama açık kaynak bileşenler kullanır; lisans bildirimleri kaldırılmamıştır.
              </p>
              <dl className="notices">
                {NOTICES.map((n) => (
                  <div key={n.license}>
                    <dt>{n.license}</dt>
                    <dd>{n.packages}</dd>
                  </div>
                ))}
              </dl>
              <p className="legal-muted">Tam liste ürünle birlikte dağıtılır.</p>
            </div>
          ) : null}

          {tab === "geri" ? <FeedbackPane /> : null}

          <div className="sheet-actions">
            <Button onClick={onClose}>Bitti</Button>
          </div>
        </div>
      </div>
    </div>
  );
}
