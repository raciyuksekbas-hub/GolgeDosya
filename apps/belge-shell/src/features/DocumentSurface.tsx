import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { FeatureState, RecentDocument } from "../shell/types";
import { IconDocumentLarge } from "../shell/icons";
import { announce } from "../shared-ui/Announcer";
import { Button, Pill, Status } from "../shared-ui/primitives";
import { MODES, extensionOf, fileNameOf, type ContextOutcome } from "../shell/modes";
import { relativeTime } from "./relativeTime";

interface Props {
  feature: FeatureState;
  recents: RecentDocument[];
  /** Kip değişince belgeye ne olduğu. Uyuşmazlık sessizce geçilmez. */
  outcome: ContextOutcome | null;
  /** ⌘O işareti. Her artışta belge seçici açılır; düğmeyle aynı yol. */
  openRequest?: number;
  onDocuments: (paths: string[]) => void;
  onForget: () => void;
}

/** Aşağı bölgede en fazla bu kadar satır: 900×600'de sütun kaymaz. */
const RECENT_ROWS = 5;

/**
 * Belge yüzeyi — uygulamanın açılış görüntüsü.
 *
 * Mod başlığı yok: kip kenar çubuğunda zaten seçili. Ekranın başlığı boş
 * durumun kendisidir ("Belge açın"), optik olarak üst-orta bölgede; altında bu
 * kipin gerçekten açtığı türler. Tek birincil eylem. Son kullanılanlar aşağı
 * bölgede liste olarak — kart galerisi değil; tam dosya yolu gösterilmez.
 */
export function DocumentSurface({ feature, recents, outcome, openRequest, onDocuments, onForget }: Props) {
  const mode = MODES[feature.key];
  const [over, setOver] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const accepted = useRef(mode.extensions);
  accepted.current = mode.extensions;

  const accept = useCallback(
    (paths: string[]) => {
      const usable = paths.filter((p) => accepted.current.includes(extensionOf(p)));
      if (usable.length === 0) {
        setRefused("Bu kip seçilen belge türünü açamıyor.");
        announce("Bu kip seçilen belge türünü açamıyor.");
        return;
      }
      setRefused(null);
      onDocuments(mode.needs === 2 ? usable : usable.slice(0, 1));
    },
    [mode.needs, onDocuments],
  );

  // Native sürükle-bırak. HTML5 drop olayı Tauri'de dosya yolunu vermez.
  useEffect(() => {
    let dispose: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") setOver(true);
        else if (event.payload.type === "leave") setOver(false);
        else if (event.payload.type === "drop") {
          setOver(false);
          accept(event.payload.paths);
        }
      })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else dispose = unlisten;
      })
      .catch(() => {
        /* Sürükle-bırak yoksa düğme tek yoldur; hata gösterilmez. */
      });
    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [accept]);

  const browse = useCallback(async () => {
    // Seçici başarısız olabilir. Yakalanmazsa söz reddi sessizce düşer ve
    // kullanıcı düğmeye bastığında HİÇBİR ŞEY olmaz — ne pencere ne açıklama.
    // Canlı klavye denemesinde ⌘O tam olarak bunu ortaya çıkardı.
    try {
      const picked = await open({
        multiple: mode.needs === 2,
        directory: false,
        filters: [{ name: mode.pickerLabel, extensions: mode.extensions }],
      });
      if (!picked) return;
      setRefused(null);
      accept(Array.isArray(picked) ? picked : [picked]);
    } catch {
      setRefused("Belge seçici açılamadı. Lütfen yeniden deneyin.");
      announce("Belge seçici açılamadı.");
    }
  }, [mode, accept]);

  // ⌘O düğmenin yaptığı işi yapar; ayrı bir yol açılmadı.
  //
  // "İlk render mı?" bayrağı yerine SON İŞLENEN DEĞER tutulur. Bayrak
  // yaklaşımı StrictMode'un etkiyi iki kez çalıştırmasında kırılıyordu:
  // ilk çalışma bayrağı düşürüyor, ikincisi seçiciyi açıyordu — uygulama
  // açılır açılmaz. Değer karşılaştırması bu sıraya bağlı değildir.
  const handledOpen = useRef(openRequest);
  useEffect(() => {
    if (openRequest === undefined || openRequest === handledOpen.current) return;
    handledOpen.current = openRequest;
    void browse();
  }, [openRequest, browse]);

  const shown = recents.slice(0, RECENT_ROWS);

  return (
    <div className="surface home">
      {/* Kip değişiminde belge taşınamadıysa, sebebi sade biçimde söylenir. */}
      {outcome?.kind === "mismatch" ? (
        <Status tone="info">
          {outcome.rejected.map(fileNameOf).join(", ")} bu kipte açılamıyor.{" "}
          {mode.label} {mode.extensions.slice(0, 4).join(", ").toLocaleUpperCase("tr-TR")} belgeleriyle çalışır.
        </Status>
      ) : null}
      {outcome?.kind === "needsMore" ? (
        <Status tone="info">Karşılaştırmak için bir belge daha açın.</Status>
      ) : null}
      {refused ? <Status tone="error">{refused}</Status> : null}

      <div
        className="dropzone"
        data-over={over}
        onDoubleClick={browse}
        role="group"
        aria-label={mode.prompt}
      >
        <div className="empty">
          <div className="empty-icon">
            <IconDocumentLarge />
          </div>
          <h1 className="empty-primary">Belge açın</h1>
          <p className="empty-hint">{mode.hint}</p>
          <div className="empty-actions">
            <Button variant="primary" onClick={browse} title="Belge Aç  ⌘O">
              Belge Aç
            </Button>
          </div>
        </div>
      </div>

      {shown.length > 0 ? (
        <section className="recents" aria-labelledby="son-baslik">
          <h2 className="section-head" id="son-baslik">
            Son kullanılanlar
          </h2>
          <ul className="file-list">
            {shown.map((r) => (
              <li key={r.path}>
                <button type="button" className="file-row" onClick={() => accept([r.path])}>
                  <span className="file-name">{fileNameOf(r.path)}</span>
                  <Pill>{extensionOf(r.path)}</Pill>
                  <span className="file-time">{relativeTime(r.openedAt)}</span>
                </button>
              </li>
            ))}
          </ul>
          <div className="row-end">
            <Button variant="quiet" onClick={onForget}>
              Listeyi temizle
            </Button>
          </div>
        </section>
      ) : null}
    </div>
  );
}
