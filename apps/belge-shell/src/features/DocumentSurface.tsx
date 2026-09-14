import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { FeatureState, RecentDocument } from "../shell/types";
import { announce } from "../shared-ui/Announcer";
import { Button, Status } from "../shared-ui/primitives";
import { IconDocument } from "../shell/icons";
import {
  MODES,
  extensionOf,
  fileNameOf,
  formatList,
  type ContextOutcome,
} from "../shell/modes";
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
 * Dört kip bu yüzeyi paylaşır ama AYNI ekranı göstermez. Kipe ait olanlar:
 * başlık, açıklama, birincil eylemin adı, son kullanılanlar başlığı ve — iki
 * belge isteyen kipte — belgelerin yuvaları.
 *
 * Ekranın %80'i boş kalmaz: karşılama bloğu çalışma sütununun üst bandında ve
 * sola hizalıdır, hemen altında gerçek içerik durur. O içerik **bu kipin
 * açabildiği** son belgelerdir; açamayacağı bir satır çizilmez (tıklanınca
 * reddedilen satır bilgi değil tuzaktır). Hiç kayıt yoksa yerine kipin kabul
 * ettiği türler tek künye satırı olarak durur.
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

  /**
   * İki belgelik kipte ilk belge seçilmiş olabilir.
   *
   * Bu, kabuğun `needsMore` durumudur: belge açıldı ama kip iki tane istiyor.
   * Eskiden yalnız tek satırlık bir uyarıya dönüşüyordu; artık yuvanın
   * kendisi doluyor ve model ekranda kalıyor.
   */
  const first = outcome?.kind === "needsMore" ? outcome.paths[0] ?? null : null;
  const firstRef = useRef(first);
  firstRef.current = first;

  const accept = useCallback(
    (paths: string[]) => {
      const usable = paths.filter((p) => accepted.current.includes(extensionOf(p)));
      if (usable.length === 0) {
        setRefused("Bu kip seçilen belge türünü açamıyor.");
        announce("Bu kip seçilen belge türünü açamıyor.");
        return;
      }
      setRefused(null);
      if (mode.needs !== 2) {
        onDocuments(usable.slice(0, 1));
        return;
      }
      // İkinci belge tek başına gelirse birincinin YERİNİ ALMAZ, yanına
      // eklenir: kullanıcı A'yı seçtikten sonra B'yi sürüklediğinde
      // karşılaştırma başlamalı, A kaybolmamalı.
      const carried = firstRef.current;
      if (carried && usable.length === 1 && usable[0] !== carried) {
        onDocuments([carried, usable[0]]);
        return;
      }
      onDocuments(usable.slice(0, 2));
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

  /**
   * Belge seçici.
   *
   * `slot` yalnız iki belgelik kipte anlamlıdır: ikinci yuva seçildiğinde
   * birinci belge korunur ve ikisi birlikte açılır.
   */
  const browse = useCallback(
    async (slot: 0 | 1 = 0) => {
      // Seçici başarısız olabilir. Yakalanmazsa söz reddi sessizce düşer ve
      // kullanıcı düğmeye bastığında HİÇBİR ŞEY olmaz — ne pencere ne açıklama.
      // Canlı klavye denemesinde ⌘O tam olarak bunu ortaya çıkardı.
      try {
        const picked = await open({
          // İlk yuva çoklu seçime açıktır: iki belgeyi tek pencerede seçmek
          // isteyen kullanıcı iki adımı beklemek zorunda kalmaz.
          multiple: mode.needs === 2 && slot === 0,
          directory: false,
          filters: [{ name: mode.pickerLabel, extensions: mode.extensions }],
        });
        if (!picked) return;
        setRefused(null);
        const paths = Array.isArray(picked) ? picked : [picked];
        if (slot === 1 && first && paths.length === 1) {
          accept([first, paths[0]]);
          return;
        }
        accept(paths);
      } catch {
        setRefused("Belge seçici açılamadı. Lütfen yeniden deneyin.");
        announce("Belge seçici açılamadı.");
      }
    },
    [mode, accept, first],
  );

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
    void browse(first ? 1 : 0);
  }, [openRequest, browse, first]);

  // Liste kipe göre süzülür: bu kipin açamayacağı kayıt gösterilmez.
  const shown = useMemo(
    () =>
      recents
        .filter((r) => mode.extensions.includes(extensionOf(r.path)))
        .filter((r) => r.path !== first)
        .slice(0, RECENT_ROWS),
    [recents, mode.extensions, first],
  );

  const pair = mode.needs === 2 && mode.slots;

  return (
    <div className="surface home" data-over={over}>
      {/* Kip değişiminde belge taşınamadıysa, sebebi sade biçimde söylenir. */}
      {outcome?.kind === "mismatch" ? (
        <Status tone="info">
          {outcome.rejected.map(fileNameOf).join(", ")} bu kipte açılamıyor.{" "}
          {mode.label} {formatList(mode, 4)} belgeleriyle çalışır.
        </Status>
      ) : null}
      {refused ? <Status tone="error">{refused}</Status> : null}

      {/* Karşılama: kipin GÖREVİ. Ekranın matematiksel merkezinde değil,
          çalışma sütununun üst bandında ve sola hizalı. */}
      <div className="welcome">
        <h1 className="welcome-title">{mode.emptyTitle}</h1>
        <p className="welcome-note">{mode.hint}</p>

        {pair ? (
          /* İki belgelik model boş durumda da görünür: kullanıcı ilk bakışta
             "buraya iki belge koyacağım" anlar. Yuvalar kart değildir —
             tonal yüzey, çerçeve yok. */
          <div className="slots" role="group" aria-label="Karşılaştırılacak belgeler">
            <button
              type="button"
              className="slot"
              data-filled={first ? "true" : undefined}
              onClick={() => void browse(0)}
            >
              <span className="slot-label">{mode.slots![0]}</span>
              {first ? (
                <>
                  <span className="slot-name">{fileNameOf(first)}</span>
                  <span className="slot-meta">
                    {extensionOf(first).toLocaleUpperCase("tr-TR")} · değiştirmek için seçin
                  </span>
                </>
              ) : (
                <span className="slot-invite">İlk belgeyi seçin</span>
              )}
            </button>

            <span className="slot-join" aria-hidden="true">
              ↔
            </span>

            <button
              type="button"
              className="slot"
              data-next={first ? "true" : undefined}
              disabled={!first}
              onClick={() => void browse(1)}
            >
              <span className="slot-label">{mode.slots![1]}</span>
              <span className="slot-invite">İkinci belgeyi seçin</span>
            </button>
          </div>
        ) : (
          <div className="welcome-action">
            <Button variant="primary" onClick={() => void browse(0)} title={`${mode.openLabel}  ⌘O`}>
              {mode.openLabel}
            </Button>
            <span className="welcome-hint">veya belgeyi buraya sürükleyin</span>
          </div>
        )}

        {shown.length > 0 ? (
          <section className="recents" aria-labelledby="son-baslik">
            <div className="recents-head">
              <h2 className="section-head" id="son-baslik">
                {mode.recentTitle}
              </h2>
              <Button className="btn-sm" variant="quiet" onClick={onForget}>
                Listeyi temizle
              </Button>
            </div>
            <ul className="file-list">
              {shown.map((r) => (
                <li key={r.path}>
                  <button
                    type="button"
                    className="file-row"
                    onClick={() => accept(first ? [first, r.path] : [r.path])}
                  >
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
        ) : (
          /* Kayıt yokken "henüz yok" yazmak bilgi üretmez; ekran bunun yerine
             ne kabul ettiğini söyler. */
          <p className="welcome-formats">Açılabilen türler: {formatList(mode)}</p>
        )}
      </div>
    </div>
  );
}
