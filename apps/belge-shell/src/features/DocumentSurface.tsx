import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { FeatureState, RecentDocument } from "../shell/types";
import { announce } from "../shared-ui/Announcer";
import { Button, Status } from "../shared-ui/primitives";
import { IconDocument } from "../shell/icons";
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

/** Depo on kayıt tutar; hepsi gösterilir, alan yetmezse liste kendi içinde kayar. */
const RECENT_ROWS = 10;

/**
 * Belge yüzeyi — uygulamanın açılış görüntüsü.
 *
 * Mod başlığı yok: kip kenar çubuğunda zaten seçili.
 *
 * Dört kip bu yüzeyi paylaşır ama AYNI ekranı göstermez: başlık, açıklama ve
 * birincil eylem kipin görevinden gelir (`MODES`). Kullanıcı yalnız ekrana
 * bakarak hangi kipte olduğunu anlar.
 *
 * Karşılama bloğu ekranın matematiksel merkezinde değil, çalışma sütununun
 * üst bandında ve sola hizalıdır; hemen altında gerçek içerik durur: son
 * kullanılan belgeler. Kayıt yoksa o blok hiç çizilmez.
 *
 * Sürükle-bırak hedefi tüm yüzeydir; kesikli çerçeve yalnız sürükleme
 * sırasında belirir.
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
    <div className="surface home" data-over={over}>
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

      {/* Karşılama: kipin GÖREVİ. Dört kip aynı şablonu paylaşmaz — başlık,
          açıklama ve eylem kipe aittir. Ekranın matematiksel merkezinde değil,
          çalışma sütununun üst bandında ve sola hizalı durur. */}
      <div className="welcome">
        <h1 className="welcome-title">{mode.emptyTitle}</h1>
        <p className="welcome-note">{mode.hint}</p>
        <div className="welcome-action">
          <Button variant="primary" onClick={browse} title={`${mode.openLabel}  ⌘O`}>
            {mode.openLabel}
          </Button>
          <span className="welcome-hint">veya belgeyi buraya sürükleyin</span>
        </div>

        {shown.length > 0 ? (
          <section className="recents" aria-labelledby="son-baslik">
            <div className="recents-head">
              <h2 className="section-head" id="son-baslik">
                Son Kullanılanlar
              </h2>
              <Button className="btn-sm" variant="quiet" onClick={onForget}>
                Listeyi temizle
              </Button>
            </div>
            <ul className="file-list">
              {shown.map((r) => (
                <li key={r.path}>
                  <button type="button" className="file-row" onClick={() => accept([r.path])}>
                    <IconDocument className="file-icon" />
                    <span className="file-text">
                      <span className="file-name">{fileNameOf(r.path)}</span>
                      <span className="file-meta">
                        {extensionOf(r.path).toLocaleUpperCase("tr-TR")} · {relativeTime(r.openedAt)}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
      </div>
    </div>
  );
}
